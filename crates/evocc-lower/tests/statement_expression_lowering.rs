use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as ChirLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, render_module, validate_module, Inst, InterpreterLimits as EvoIrLimits,
    Module, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, TargetDataLayout};

fn parse<'arena>(source: &str, arena: &'arena Arena, ordinal: u32) -> CHirUnit<'arena> {
    let file = FileId::new(10_120 + ordinal);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser rejected {source:?}: {:?}",
        parser.diagnostic()
    );
    unit
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function {name}"))
}

fn lower(source: &str, ordinal: u32) -> Module {
    let arena = Arena::default();
    let unit = parse(source, &arena, ordinal);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("lowering rejected {source:?}: {error}"));
    validate_module(&module)
        .unwrap_or_else(|error| panic!("lowered module failed validation: {error}"));
    module
}

fn entry_arguments(module: &Module) -> Vec<RuntimeValue> {
    module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("lowering produced @main")
        .params
        .iter()
        .map(|parameter| match &parameter.ty {
            EvoIRType::Mem(_) | EvoIRType::World => RuntimeValue::new(parameter.ty.clone(), 0),
            other => panic!("unsupported @main entry parameter {other:?}"),
        })
        .collect()
}

fn observe(source: &str, ordinal: u32) -> (i128, i128) {
    let arena = Arena::default();
    let unit = parse(source, &arena, ordinal);
    let chir = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        ChirLimits::new(100_000),
    )
    .unwrap_or_else(|error| panic!("C-HIR execution failed: {error:?}"))
    .return_value
    .expect("C-HIR main returned a value")
    .mathematical_value(unit.types())
    .expect("C-HIR integer result");

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("lowering failed: {error}"));
    validate_module(&module).expect("lowered module validates");
    let evoir = interpret_module(
        &module,
        "main",
        &entry_arguments(&module),
        EvoIrLimits::new(100_000),
    )
    .unwrap_or_else(|error| panic!("EvoIR execution failed: {error:?}"));
    let value = evoir.return_values.first().expect("EvoIR main result");
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        panic!("non-integer result type: {:?}", value.ty);
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let raw = value.raw_bits & mask;
    let sign = 1_u64 << (width - 1);
    let mathematical = if raw & sign == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    };
    (chir, mathematical)
}

#[test]
fn straight_line_expression_adds_no_control_flow_block() {
    let module = lower("int main(void) { return ({ int t = 1; t + 1; }); }\n", 0);
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap();
    assert_eq!(
        main.blocks.len(),
        1,
        "straight-line expression stays in entry"
    );
}

#[test]
fn conditional_expression_has_one_value_join_block() {
    let module = lower(
        "int main(void) { int c = 1; return ({ if (c) c = 2; c; }); }\n",
        1,
    );
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap();
    let mut predecessor_counts = std::collections::BTreeMap::new();
    for block in &main.blocks {
        match &block.terminator {
            Inst::Br(target, _) => *predecessor_counts.entry(*target).or_insert(0_usize) += 1,
            Inst::CondBr(_, true_target, _, false_target, _) => {
                *predecessor_counts.entry(*true_target).or_insert(0_usize) += 1;
                *predecessor_counts.entry(*false_target).or_insert(0_usize) += 1;
            }
            _ => {}
        }
    }
    let value_joins = main
        .blocks
        .iter()
        .filter(|block| {
            predecessor_counts.get(&block.id).copied().unwrap_or(0) > 1 && block.args.len() == 1
        })
        .count();
    assert_eq!(value_joins, 1, "one block argument transports the value");
}

#[test]
fn both_interpreters_agree_on_statement_and_initializer_forms() {
    let cases = [
        ("int main(void) { return ({ int t = 1; t + 1; }); }\n", 2),
        (
            "int main(void) { int sum = 0; for (int i = 0; i < 3; ++i) { sum += ({ if (i == 1) break; 10; }); } return sum; }\n",
            10,
        ),
        ("int main(void) { return ({ ({ 1; }) + ({ 2; }); }); }\n", 3),
        (
            "struct pair { int a; int b; }; int main(void) { struct pair p = { .b = 2, .a = 1 }; return p.a + p.b; }\n",
            3,
        ),
        (
            "int read(const int *p) { return *p; } int main(void) { return read(&(int){ 4 }); }\n",
            4,
        ),
    ];
    for (ordinal, (source, expected)) in cases.into_iter().enumerate() {
        let (chir, evoir) = observe(source, 10 + ordinal as u32);
        assert_eq!(chir, expected, "C-HIR case {ordinal}");
        assert_eq!(evoir, expected, "EvoIR case {ordinal}");
    }
}

#[test]
fn a_void_pointer_spelled_null_executes_as_a_null_function_pointer() {
    let source = concat!(
        "struct c { void (*fn)(int); }; ",
        "int main(void) { struct c call; call.fn = ((void *)0); ",
        "return call.fn ? 0 : 1; }\n",
    );
    assert_eq!(observe(source, 32), (1, 1));
}

#[test]
fn null_function_pointer_conversions_execute_in_every_conversion_context() {
    let source = concat!(
        "typedef void callback(int); typedef callback *callback_ptr; ",
        "struct holder { callback *fn; }; ",
        "int is_null(callback *fn) { return fn == (void *)0; } ",
        "callback_ptr return_null(void) { return (void *)0; } ",
        "int main(void) { struct holder holder; callback *local = (void *)0; ",
        "holder.fn = (void *)0; return is_null((void *)0) ",
        "+ 2 * (local == (void *)0) + 4 * (holder.fn == (void *)0) ",
        "+ 8 * (return_null() == (void *)0); }\n",
    );
    assert_eq!(observe(source, 34), (15, 15));
}

#[test]
fn designated_and_positional_forms_lower_to_identical_bytes() {
    let designated = render_module(&lower(
        "struct pair { int a; int b; }; int main(void) { struct pair p = { .a = 1, .b = 2 }; return p.a + p.b; }\n",
        30,
    ));
    let positional = render_module(&lower(
        "struct pair { int a; int b; }; int main(void) { struct pair p = { 1, 2 }; return p.a + p.b; }\n",
        31,
    ));
    assert_eq!(designated, positional);
}

#[test]
fn file_scope_compound_pointer_fails_closed_without_a_global_identity() {
    let arena = Arena::default();
    let unit = parse("int *values = (int[]){ 1, 2 };\n", &arena, 32);
    let error = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect_err("an unspecified relocation identity must not produce dangling global_addr");
    assert_eq!(error.code(), "E3026");
    assert_eq!(error.to_string(), "not-lowered: static-relocation");
}

#[test]
fn an_integer_round_trip_address_stops_at_the_static_relocation_boundary() {
    let arena = Arena::default();
    let unit = parse(
        "typedef unsigned long uintptr_t; int k(char *s); static void *p = (void *)(uintptr_t)&k;\n",
        &arena,
        33,
    );
    let error = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect_err("the deferred relocation must fail closed");
    assert_eq!(error.code(), "E3026");
    assert_eq!(error.to_string(), "not-lowered: static-relocation");
}
