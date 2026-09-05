//! Scalar argument lists execute; invalid list reads remain typed failures.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_122);
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

fn main_symbol(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.name.spelling == "main" && function.body.is_some() =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main")
}

fn assert_result(source: &str, expected: Result<u128, &str>) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let result =
        interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000));
    match expected {
        Ok(value) => assert_eq!(result.unwrap().return_value.unwrap().raw_bits, value),
        Err(code) => assert_eq!(result.unwrap_err().code(), code),
    }
}

#[test]
fn a_read_before_start_reports_a_list_mismatch() {
    assert_result(
        "int main(void) { __builtin_va_list ap; return __builtin_va_arg(ap, int); }\n",
        Err("E3038"),
    );
}

#[test]
fn unused_argument_list_objects_are_ordinary_storage() {
    assert_result(
        "int main(void) { __builtin_va_list ap; return 0; }\n",
        Ok(0),
    );
}

#[test]
fn calls_preserve_argument_list_storage() {
    assert_result(
        concat!(
            "int helper(void) { return 0; } ",
            "int main(void) { __builtin_va_list ap; helper(); ",
            "__builtin_va_end(ap); return 0; }\n",
        ),
        Ok(0),
    );
}

#[test]
fn a_promoted_scalar_argument_is_executed() {
    assert_result(
        concat!(
            "int take(int count, ...) { __builtin_va_list ap; ",
            "__builtin_va_start(ap, count); return __builtin_va_arg(ap, int); } ",
            "int main(void) { return take(1, 7); }\n",
        ),
        Ok(7),
    );
}
