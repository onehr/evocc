//! GNU local labels, label addresses, and computed goto diagnostics.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(10_302);
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

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\nsource: {source}");
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\nsource: {source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\nsource: {source}"
        ),
    }
}

#[test]
fn ordinary_goto_requires_a_declared_function_label() {
    rejects(
        "int f(int condition) { if (condition) goto nowhere; return 0; }\n",
        "E2920",
    );
    rejects(
        concat!(
            "int f(int condition) { { __label__ local; if (condition) goto local; ",
            "local: return 1; } goto local; }\n",
        ),
        "E2920",
    );
}

#[test]
fn label_addresses_are_void_pointers_with_function_scope() {
    accepts(concat!(
        "unsigned long f(void) { void *pointer = &&target; goto *pointer; ",
        "target: return (unsigned long)&&target; }\n",
    ));
    rejects(
        "static void *pointer = &&outside; int f(void) { outside: return pointer != 0; }\n",
        "E2920",
    );
}

#[test]
fn computed_goto_composes_with_local_labels_and_loops() {
    accepts(concat!(
        "int f(int count) { __label__ again; __label__ done; void *next = &&again; ",
        "goto *next; again: if (--count > 0) goto *next; next = &&done; ",
        "goto *next; done: return count; }\n",
    ));
}

#[test]
fn label_addresses_are_static_address_constants_in_range_designators() {
    accepts(concat!(
        "int f(int index) { static const void *targets[4] = { ",
        "[0 ... 3] = &&fallback, [1] = &&one }; goto *targets[index & 3]; ",
        "one: return 1; fallback: return 0; }\n",
    ));
}

#[test]
fn nonconstant_case_range_bound_keeps_the_typed_diagnostic() {
    rejects(
        "int f(int value, int bound) { switch (value) { case bound ... 3: return 1; } return 0; }\n",
        "E2916",
    );
}
