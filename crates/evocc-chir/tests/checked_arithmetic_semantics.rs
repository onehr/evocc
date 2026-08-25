//! Interpreter semantics for checked arithmetic (specification 10.1.15).

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 200_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_604),
        "checked-arithmetic-semantics.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
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

fn run(body: &str) -> i128 {
    let source = format!("int main(void) {{ {body} }}\n");
    let arena = Arena::default();
    let unit = parse(&source, &arena).unwrap_or_else(|error| panic!("must parse: {error}"));
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .unwrap_or_else(|error| panic!("{}: {}", error.code(), error.message()));
    outcome
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .expect("main result is an integer")
}

#[test]
fn multiplying_into_int_wraps_and_reports_overflow() {
    assert_eq!(
        run("int r; return __builtin_mul_overflow(70000, 70000, &r) * 10 + (r == 605032704);"),
        11
    );
}

#[test]
fn multiplying_into_long_is_representable() {
    assert_eq!(
        run("long r; return __builtin_mul_overflow(70000, 70000, &r) * 10 + (r == 4900000000L);"),
        1
    );
}

#[test]
fn adding_into_unsigned_char_wraps_and_reports_overflow() {
    assert_eq!(
        run("unsigned char c; return __builtin_add_overflow(200, 100, &c) * 10 + (c == 44);"),
        11
    );
}

#[test]
fn mixed_signed_subtraction_uses_infinite_precision() {
    assert_eq!(
        run("long long r; return __builtin_sub_overflow(4000000000u, -1, &r) * 10 + (r == 4000000001LL);"),
        1
    );
}

#[test]
fn negative_to_unsigned_reports_overflow() {
    assert_eq!(
        run("unsigned u; return __builtin_add_overflow(-1, 0, &u);"),
        1
    );
}

#[test]
fn checked_arithmetic_evaluates_operands_once_left_to_right() {
    assert_eq!(
        run("int r, s = 0; __builtin_mul_overflow(s++, 1, &r); return s;"),
        1
    );
}
