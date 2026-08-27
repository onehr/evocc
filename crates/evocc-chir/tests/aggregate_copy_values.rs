//! C-HIR execution semantics for record and union object copies.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(10_202);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lexer rejected aggregate-copy fixture".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().filter(|_| parser.had_error()) {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None if parser.had_error() => Err("parser failed without a diagnostic".to_owned()),
        None => Ok(unit),
    }
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

fn evaluate(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena)?;
    let outcome =
        interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(300_000))
            .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a scalar")
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
}

#[test]
fn assignment_copy_initialization_and_expression_values_match() {
    let source = concat!(
        "struct S { int x; long y; }; int main(void) { ",
        "struct S a = { 3, 5 }; struct S b = { 7, 11 }; struct S c = b; ",
        "return (a = c).x * 100 + a.y; }\n",
    );
    assert_eq!(evaluate(source).unwrap(), 711);
}

#[test]
fn record_arguments_and_returns_are_copied_by_value() {
    let source = concat!(
        "struct S { int x; int y; }; ",
        "struct S advance(struct S value) { value.x += 2; return value; } ",
        "int main(void) { struct S input = { 4, 9 }; struct S output = advance(input); ",
        "return input.x * 100 + output.x * 10 + output.y; }\n",
    );
    assert_eq!(evaluate(source).unwrap(), 469);
}

#[test]
fn self_assignment_and_identical_pointer_ranges_are_defined() {
    let source = concat!(
        "struct S { int x; int y; }; int main(void) { struct S value = { 8, 13 }; ",
        "value = value; struct S *left = &value; struct S *right = left; ",
        "*left = *right; return value.x * 100 + value.y; }\n",
    );
    assert_eq!(evaluate(source).unwrap(), 813);
}

#[test]
fn copied_padding_stays_uninitialized_but_members_are_readable() {
    let member_source = concat!(
        "struct P { char c; int value; }; int main(void) { ",
        "struct P source = { 2, 7 }; struct P destination = source; ",
        "return destination.c * 10 + destination.value; }\n",
    );
    assert_eq!(evaluate(member_source).unwrap(), 27);

    let padding_source = concat!(
        "struct P { char c; int value; }; int main(void) { ",
        "struct P source = { 2, 7 }; struct P destination = source; ",
        "unsigned char *bytes = (unsigned char *)&destination; return bytes[1]; }\n",
    );
    assert_eq!(
        evaluate(padding_source).unwrap_err(),
        "E3001: read-of-uninitialized-object"
    );
}
