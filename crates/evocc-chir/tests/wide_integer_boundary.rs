//! Execution boundary for front-end-complete 128-bit integer values.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_131);
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

#[test]
fn chir_refuses_executed_wide_arithmetic_with_e3033() {
    let arena = Arena::default();
    let unit = parse(
        concat!(
            "int main(void) { ",
            "__uint128_t u = ((__uint128_t)1 << 100) + 7; ",
            "unsigned long high = (unsigned long)(u >> 64); ",
            "unsigned long multiplied = (unsigned long)((u * 3) >> 100); ",
            "return high == 68719476736UL && multiplied == 3; }\n",
        ),
        &arena,
    );
    let error =
        interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000))
            .expect_err("128-bit execution must fail closed until its value model exists");
    assert_eq!(error.code(), "E3033", "{error:?}");
    assert_eq!(error.message(), "not-executed: wide-integer", "{error:?}");
}
