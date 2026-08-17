use std::num::NonZeroUsize;

use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;

fn lex(source: &str) -> Vec<evocc_lex::Token> {
    let mut lexer = Lexer::new(FileId::new(5_500), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    tokens
}

#[test]
fn default_parser_recovery_keeps_later_declarations() {
    let source = concat!("int +;\n", "int after_error;\n",);
    let arena = Arena::default();
    let mut parser = Parser::new(lex(source), FileId::new(5_500), source);
    let unit = parser.parse_translation_unit_in(&arena);

    let diagnostic = parser.diagnostic().expect("first error is recorded");
    assert_eq!(diagnostic.code, "E1001");
    assert_eq!(diagnostic.message, "expected-declaration");
    assert_eq!(diagnostic.span.line_start, 1);
    assert_eq!(diagnostic.span.column_start, 5);
    assert!(unit.declarations().iter().any(|declaration| matches!(
        declaration,
        evocc_chir::CHirDecl::Object(declaration)
            if declaration.name.spelling == "after_error"
    )));
}

#[test]
fn configured_error_limit_stops_before_later_declarations() {
    let source = concat!("int +;\n", "int after_error;\n",);
    let arena = Arena::default();
    let mut parser = Parser::new(lex(source), FileId::new(5_500), source);
    parser.set_error_limit(NonZeroUsize::MIN);
    let unit = parser.parse_translation_unit_in(&arena);

    let diagnostic = parser.diagnostic().expect("first error is recorded");
    assert_eq!(diagnostic.code, "E1001");
    assert_eq!(diagnostic.message, "expected-declaration");
    assert_eq!(diagnostic.span.line_start, 1);
    assert_eq!(diagnostic.span.column_start, 5);
    assert!(
        unit.is_empty(),
        "the valid recovery tail must not be parsed"
    );
}
