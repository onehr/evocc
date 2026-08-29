//! Constant-expression folding values fixed by specification 10.1.19.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

const STEP_BUDGET: u64 = 200_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_119);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!(
            "must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

fn diagnostic(source: &str) -> ParseDiagnostic {
    let arena = Arena::default();
    parse(source, &arena).expect_err("fixture must be rejected")
}

fn entry(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.body.is_some() && function.name.spelling == "main" =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main")
}

fn evaluate(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit =
        parse(source, &arena).map_err(|error| format!("{}: {}", error.code, error.message))?;
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a scalar")
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
}

#[test]
fn sizeof_and_alignof_expression_match_oracle_values_without_evaluation() {
    let source = concat!(
        "int main(void) { int i = 0; ",
        "_Static_assert(sizeof(*(int *)8) == 4, \"sizeof dereference\"); ",
        "_Static_assert(__alignof__(*(int *)8) == 4, \"alignof dereference\"); ",
        "unsigned long s = sizeof(i++); unsigned long a = __alignof__(i++); ",
        "return (int)(s * 100 + a * 10 + i); }\n",
    );
    assert_eq!(evaluate(source).unwrap(), 440);
}

#[test]
fn sizeof_void_and_function_type_are_one() {
    accepts(concat!(
        "typedef int function_type(void); ",
        "_Static_assert(sizeof(void) == 1, \"void\"); ",
        "_Static_assert(sizeof(function_type) == 1, \"function\");\n",
    ));
}

#[test]
fn a_pointer_typed_null_pointer_constant_preserves_the_other_pointer_type() {
    accepts(concat!(
        "_Static_assert(__builtin_types_compatible_p(",
        "typeof(8 ? (void *)((long)1 * 0l) : (int *)8), int *), ",
        "\"null pointer constant\");\n",
    ));
}

#[test]
fn the_kernel_constexpr_shape_has_the_measured_constant_operand_value() {
    accepts(concat!(
        "_Static_assert(sizeof(*(8 ? ((void *)((long)(1) * 0l)) : (int *)8)) == 4, ",
        "\"constant operand\");\n",
    ));
    let error = diagnostic(concat!(
        "_Static_assert(sizeof(*(8 ? ((void *)((long)(1) * 0l)) : (int *)8)) == 1, ",
        "\"opposite answer must fail\");\n",
    ));
    assert_eq!(error.code, "E2911");
}

#[test]
fn the_kernel_constexpr_shape_has_the_measured_nonconstant_operand_value() {
    accepts(concat!(
        "int probe(int value) { ",
        "_Static_assert(sizeof(*(8 ? ((void *)((long)(value) * 0l)) : (int *)8)) == 1, ",
        "\"nonconstant operand\"); return value; }\n",
    ));
}
