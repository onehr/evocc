use evocc_chir::integer::IntegerError;
use evocc_chir::interp::{
    interpret_scalar_function, InterpreterError as ChirError, InterpreterLimits as ChirLimits,
};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, validate_module, InterpreterError as EvoIrError,
    InterpreterLimits as EvoIrLimits, Module, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, TargetDataLayout};

const FILE_ID: u32 = 6_003;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(FILE_ID);
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

fn entry_arguments(module: &Module) -> Vec<RuntimeValue> {
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .unwrap_or_else(|| panic!("lowering produced no @main"));
    main.params
        .iter()
        .map(|parameter| match &parameter.ty {
            EvoIRType::Mem(_) | EvoIRType::World => RuntimeValue::new(parameter.ty.clone(), 0),
            other => panic!("@main has unsupported ordinary entry parameter {other:?}"),
        })
        .collect()
}

fn assert_typed_runtime_diagnostic(
    source: &str,
    expression: &str,
    expected_integer_error: IntegerError,
    expected_code: &'static str,
    expected_message: &'static str,
) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let chir_error = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        ChirLimits::new(100_000),
    )
    .expect_err("the C-HIR interpreter accepted undefined integer behavior");
    assert_eq!(chir_error.code(), expected_code);
    assert_eq!(chir_error.message(), expected_message);
    let ChirError::Integer { error, span } = chir_error else {
        panic!("C-HIR returned an untyped error: {chir_error:?}");
    };
    assert_eq!(error, expected_integer_error);
    let byte_start = source
        .find(expression)
        .unwrap_or_else(|| panic!("missing diagnostic expression {expression:?}"));
    assert_eq!(span.file.raw(), FILE_ID);
    assert_eq!(span.byte_start, byte_start as u32);
    assert_eq!(span.byte_end, (byte_start + expression.len()) as u32);
    assert_eq!(span.line_start, 1);
    assert_eq!(span.column_start, byte_start as u32 + 1);

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("lowering failed: {error}"));
    validate_module(&module)
        .unwrap_or_else(|error| panic!("lowered EvoIR failed validation: {error}"));
    let evoir_error = interpret_module(
        &module,
        "main",
        &entry_arguments(&module),
        EvoIrLimits::new(100_000),
    )
    .expect_err("the EvoIR interpreter accepted undefined integer behavior");
    assert_eq!(evoir_error.code(), expected_code);
    assert_eq!(evoir_error.message(), expected_message);
    let EvoIrError::UndefinedBehavior {
        block,
        diagnostic,
        message,
    } = evoir_error
    else {
        panic!("EvoIR returned an untyped error: {evoir_error:?}");
    };
    assert_eq!(block, 0);
    assert_eq!(diagnostic, expected_code);
    assert_eq!(message, expected_message);
}

#[test]
fn division_by_zero_matches_typed_runtime_diagnostic() {
    assert_typed_runtime_diagnostic(
        "int main(void) { int numerator = 7; int denominator = 0; return numerator / denominator; }\n",
        "numerator / denominator",
        IntegerError::DivisionByZero,
        "E3002",
        "integer-division-by-zero",
    );
}

#[test]
fn invalid_shift_count_matches_typed_runtime_diagnostic() {
    assert_typed_runtime_diagnostic(
        "int main(void) { int value = 1; int count = 32; return value << count; }\n",
        "value << count",
        IntegerError::InvalidShiftCount,
        "E3005",
        "invalid-shift-count",
    );
}

#[test]
fn signed_overflow_matches_typed_runtime_diagnostic() {
    assert_typed_runtime_diagnostic(
        "int main(void) { int value = 2147483647; int one = 1; return value + one; }\n",
        "value + one",
        IntegerError::SignedOverflow,
        "E3004",
        "signed-integer-overflow",
    );
}

#[test]
fn runtime_signed_left_shift_keeps_its_undefined_behavior_obligation() {
    assert_typed_runtime_diagnostic(
        "int main(void) { int one = 1; int count = 31; return one << count; }\n",
        "one << count",
        IntegerError::InvalidSignedLeftShift,
        "E3006",
        "invalid-signed-left-shift",
    );
}
