//! Refusals that keep `__auto_type` to one initialized plain identifier.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(10_303);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected input at {:?}", token.span));
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

fn rejects(source: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with E2915, but parsed\nsource: {source}"),
        Err(error) => assert!(
            error.starts_with("E2915:"),
            "expected E2915, got {error}\nsource: {source}"
        ),
    }
}

#[test]
fn inferred_type_requires_a_plain_identifier_declarator() {
    rejects("int f(long value) { __auto_type *pointer = &value; return (int)*pointer; }\n");
    rejects("int f(long value) { __auto_type array[2] = { value, value }; return 0; }\n");
}

#[test]
fn inferred_type_object_cannot_appear_in_its_initializer() {
    rejects("int f(void) { __auto_type value = value; return value; }\n");
}

#[test]
fn missing_initializer_and_multiple_declarators_remain_refused() {
    rejects("void f(void) { __auto_type value; }\n");
    rejects("void f(void) { __auto_type first = 1, second = 2; }\n");
}
