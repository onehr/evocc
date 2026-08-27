//! Volatile aggregate copies stop where no portable access sequence exists.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(10_304);
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
        Ok(_) => panic!("must reject with E2922, but parsed\nsource: {source}"),
        Err(error) => assert!(
            error.starts_with("E2922:"),
            "expected E2922, got {error}\nsource: {source}"
        ),
    }
}

#[test]
fn volatile_union_copy_is_refused() {
    rejects(concat!(
        "union U { int narrow; long wide; }; ",
        "void f(volatile union U *to, volatile union U *from) { *to = *from; }\n",
    ));
}

#[test]
fn volatile_record_with_a_bit_field_is_refused() {
    rejects(concat!(
        "struct B { unsigned int bits : 3; int value; }; ",
        "void f(volatile struct B *to, volatile struct B *from) { *to = *from; }\n",
    ));
}

#[test]
fn volatile_record_without_bit_fields_remains_accepted() {
    let arena = Arena::default();
    parse(
        concat!(
            "struct S { int first; long second; }; ",
            "void f(volatile struct S *to, volatile struct S *from) { *to = *from; }\n",
        ),
        &arena,
    )
    .expect("plain volatile records have a declaration-order copy");
}
