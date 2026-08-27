//! Observable semantics and conservative CFG edges for GNU computed goto.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as CHirLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, render_module, validate_module, BlockId, Inst,
    InterpreterLimits as EvoIrLimits, Module, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_305);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "fixture must parse: {:?}\nsource: {source}",
        parser.diagnostic()
    );
    unit
}

fn lower(source: &str) -> Module {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("computed-goto fixture lowers");
    validate_module(&module).expect("computed-goto module validates");
    module
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture defines {name}"))
}

fn entry_arguments(module: &Module, name: &str) -> Vec<RuntimeValue> {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("module defines {name}"));
    function
        .params
        .iter()
        .map(|parameter| match parameter.ty {
            EvoIRType::Mem(Region::All) | EvoIRType::World => {
                RuntimeValue::new(parameter.ty.clone(), 0)
            }
            _ => panic!("unexpected entry parameter: {parameter:?}"),
        })
        .collect()
}

fn signed(value: &RuntimeValue) -> i128 {
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        panic!("unexpected result type: {:?}", value.ty);
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let raw = value.raw_bits & mask;
    let sign = 1_u64 << (width - 1);
    if raw & sign == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    }
}

fn block_returning(function: &evocc_evoir::Function, raw: u64) -> BlockId {
    function
        .blocks
        .iter()
        .find_map(|block| {
            let value = block
                .insts
                .iter()
                .find_map(|instruction| match instruction {
                    Inst::Const(value, observed) if *observed == raw => Some(value.id),
                    _ => None,
                })?;
            match &block.terminator {
                Inst::Ret(results) if results.iter().any(|result| result.id == value) => {
                    Some(block.id)
                }
                _ => None,
            }
        })
        .unwrap_or_else(|| panic!("function has a block returning {raw}"))
}

#[test]
fn computed_goto_lists_exactly_the_address_taken_labels_as_successors() {
    let module = lower(concat!(
        "int f(int selector) { void *target = selector ? &&first : &&second; goto *target; ",
        "first: return 11; second: return 22; unused: return 33; }\n",
    ));
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("lowering emits f");
    let first = block_returning(function, 11);
    let second = block_returning(function, 22);
    let unused = block_returning(function, 33);
    let text = render_module(&module);
    let first_name = format!("^b{first}(");
    let second_name = format!("^b{second}(");
    let unused_name = format!("^b{unused}(");
    let dispatch = text
        .lines()
        .find(|line| line.contains(&first_name) && line.contains(&second_name))
        .unwrap_or_else(|| panic!("no terminator lists both address-taken labels:\n{text}"));
    assert!(!dispatch.contains(&unused_name), "{dispatch}");
}

#[test]
fn dispatch_loop_executes_identically_in_both_interpreters() {
    let source = concat!(
        "int main(void) { int visits = 0; void *target = &&again; goto *target; ",
        "again: if (visits++ == 0) goto *target; return visits; }\n",
    );
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let chir = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        CHirLimits::new(100_000),
    )
    .expect("C-HIR dispatch loop executes")
    .return_value
    .expect("main returns")
    .mathematical_value(unit.types())
    .expect("main returns an integer");

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("dispatch loop lowers");
    validate_module(&module).expect("dispatch loop validates");
    let evoir = interpret_module(
        &module,
        "main",
        &entry_arguments(&module, "main"),
        EvoIrLimits::new(100_000),
    )
    .expect("EvoIR dispatch loop executes");
    assert_eq!(chir, 2);
    assert_eq!(signed(&evoir.return_values[0]), chir);
}

#[test]
fn invalid_computed_target_is_the_same_typed_runtime_failure() {
    let source = "int main(void) { void *target = (void *)0; goto *target; }\n";
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let chir = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        CHirLimits::new(10_000),
    )
    .expect_err("invalid C-HIR target must fail");
    assert_eq!(chir.code(), "E3028");

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("invalid-target program still lowers");
    validate_module(&module).expect("invalid-target module validates");
    let evoir = interpret_module(
        &module,
        "main",
        &entry_arguments(&module, "main"),
        EvoIrLimits::new(10_000),
    )
    .expect_err("invalid EvoIR target must fail");
    assert_eq!(evoir.code(), "E3028");
}

#[test]
fn functions_without_computed_goto_do_not_gain_dispatch_edges() {
    let module = lower("int f(int value) { return value + 1; }\n");
    let function = module
        .functions
        .iter()
        .find(|function| function.name == "f")
        .expect("lowering emits f");
    assert!(
        function
            .blocks
            .iter()
            .all(|block| matches!(&block.terminator, Inst::Ret(_) | Inst::Trap(_))),
        "{}",
        render_module(&module)
    );
}
