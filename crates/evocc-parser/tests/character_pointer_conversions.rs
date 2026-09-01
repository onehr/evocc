//! GNU character-pointer signedness conversions from specification 10.1.3.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_130);
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

#[test]
fn character_pointer_initializers_accept_signedness_changes() {
    accepts(concat!(
        "struct qstr { const unsigned char *name; }; ",
        "struct qstr make(char *value, signed char *signed_value, unsigned char *unsigned_value) { ",
        "const unsigned char *from_plain = value; ",
        "const char *from_signed = signed_value; ",
        "const signed char *from_unsigned = unsigned_value; ",
        "return (struct qstr){ .name = from_plain }; }\n",
    ));
}

#[test]
fn character_pointer_assignments_accept_signedness_changes() {
    accepts(concat!(
        "void assign(char *plain, signed char *signed_value, unsigned char *unsigned_value) { ",
        "const unsigned char *a; const char *b; const signed char *c; ",
        "a = plain; b = signed_value; c = unsigned_value; }\n",
    ));
}

#[test]
fn character_pointer_arguments_and_returns_use_the_same_rule() {
    accepts(concat!(
        "void consume(const unsigned char *value) { (void)value; } ",
        "const unsigned char *forward(char *value) { consume(value); return value; }\n",
    ));
}

#[test]
fn unrelated_object_pointer_types_remain_incompatible() {
    let arena = Arena::default();
    let error = parse(
        "void f(int *value) { unsigned char *bytes = value; }\n",
        &arena,
    )
    .expect_err("unrelated object pointers must remain rejected");
    assert_eq!(error.code, "E2202");
    assert_eq!(error.message, "invalid-initializer-type");
}
