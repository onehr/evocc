//! Pointer-declarator GNU attributes (specification 10.1.9).
//!
//! These attributes belong to the pointer layer formed by the immediately
//! preceding `*`.  This profile retains them for source fidelity and never
//! applies them, including `aligned`.

use evocc_chir::{render_unit_summary, CHirUnit};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_501),
        "gnu-pointer-attributes.c".to_owned(),
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

fn dump(source: &str) -> String {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| panic!("must parse: {error}"));
    render_unit_summary(&unit)
}

#[test]
fn exact_err_ptr_declaration_retains_the_pointer_attribute() {
    let output = dump("void * __attribute__((__warn_unused_result__)) ERR_PTR(long error);\n");
    assert!(output.contains("function ERR_PTR"), "{output}");
    assert!(
        output.contains(
            "attribute warn_unused_result() placement=pointer-declarator preserved-not-applied"
        ),
        "{output}"
    );
}

#[test]
fn nested_declarator_retains_one_unambiguous_pointer_attachment() {
    let output = dump("int * __attribute__((unused)) (*p)(void);\n");
    assert_eq!(
        output
            .matches("attribute unused() placement=pointer-declarator preserved-not-applied")
            .count(),
        1,
        "{output}"
    );
}

#[test]
fn aligned_on_a_pointer_declarator_is_preserved_but_never_applied() {
    let output = dump("void * __attribute__((aligned(64))) f(void);\n");
    assert!(
        output.contains("attribute aligned(64) placement=pointer-declarator preserved-not-applied"),
        "{output}"
    );
    assert!(
        output.contains("preserved-not-applied: aligned x1"),
        "{output}"
    );
    assert!(!output.contains("applied: aligned"), "{output}");
}

#[test]
fn malformed_pointer_attribute_reports_the_attribute_diagnostic() {
    let arena = Arena::default();
    let error = parse("void * __attribute__((unused),) f(void);\n", &arena)
        .expect_err("malformed pointer attribute must be rejected");
    assert!(error.starts_with("E2903:"), "expected E2903, got {error}");
}
