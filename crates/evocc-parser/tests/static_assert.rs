//! `_Static_assert` diagnostic contract (specification 10.1.14).

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = SourceFile::new(
        FileId::new(9_603),
        "static-assert.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected fixture");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
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
            "must parse, got {}: {}\n  source: {source}",
            error.code, error.message
        );
    }
}

fn diagnostic(source: &str) -> ParseDiagnostic {
    let arena = Arena::default();
    parse(source, &arena).expect_err("fixture must be rejected")
}

#[test]
fn zero_static_assert_uses_the_frozen_failure_code() {
    let error = diagnostic("_Static_assert(0, \"m\");\n");
    assert_eq!(error.code, "E2911");
    assert_eq!((error.span.line_start, error.span.column_start), (1, 1));
}

#[test]
fn nonconstant_static_assert_uses_the_condition_span() {
    let error = diagnostic("int n; _Static_assert(n, \"m\");\n");
    assert_eq!(error.code, "E2912");
    assert_eq!(
        (error.span.line_start, error.span.column_start),
        (1, 23),
        "E2912 is at the condition, not the message"
    );
}

#[test]
fn static_assert_accepts_frozen_constant_forms() {
    accepts("_Static_assert(__builtin_constant_p(1), \"m\");\n");
    accepts("_Static_assert(1);\n");
}
