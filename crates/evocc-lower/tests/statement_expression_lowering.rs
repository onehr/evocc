use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as ChirLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, render_module, validate_module, InterpreterLimits as EvoIrLimits, Module,
    RuntimeValue,
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
    let value_joins = main
        .blocks
        .iter()
        .filter(|block| block.args.len() == 1)
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
