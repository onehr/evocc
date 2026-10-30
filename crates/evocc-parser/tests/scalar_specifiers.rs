use evocc_chir::CHirDecl;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{ComplexBase, FloatKind, IntRank, TypeKind};

#[derive(Clone, Copy)]
enum ExpectedKind {
    Int(IntRank),
    Float(FloatKind),
    Complex(ComplexBase),
}

fn tokens(source: &str) -> Vec<evocc_lex::Token> {
    let mut lexer = Lexer::new(FileId::new(81), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    tokens
}

#[test]
fn canonical_builtin_specifier_combinations_parse_generally() {
    let cases = [
        (
            "typedef signed char value_t;",
            ExpectedKind::Int(IntRank::SChar),
        ),
        (
            "typedef long long value_t;",
            ExpectedKind::Int(IntRank::LongLong),
        ),
        (
            "typedef unsigned long long value_t;",
            ExpectedKind::Int(IntRank::ULongLong),
        ),
        (
            "typedef long double value_t;",
            ExpectedKind::Float(FloatKind::F128),
        ),
        (
            "typedef double _Complex value_t;",
            ExpectedKind::Complex(ComplexBase::CF64),
        ),
        (
            "typedef long double _Complex value_t;",
            ExpectedKind::Complex(ComplexBase::CF128),
        ),
        (
            "typedef unsigned const long int value_t;",
            ExpectedKind::Int(IntRank::ULong),
        ),
    ];

    for (source, expected) in cases {
        let arena = Arena::default();
        let mut parser = Parser::new(tokens(source), FileId::new(81), source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source:?}: {:?}", parser.diagnostic());
        let CHirDecl::Typedef(declaration) = unit.declarations()[0] else {
            panic!("{source:?}: expected typedef")
        };
        let canonical = unit
            .types()
            .get(declaration.ty)
            .expect("typedef type belongs to unit");
        let matches = match expected {
            ExpectedKind::Int(rank) => canonical.kind == TypeKind::Int(rank),
            ExpectedKind::Float(kind) => canonical.kind == TypeKind::Float(kind),
            ExpectedKind::Complex(base) => canonical.kind == TypeKind::Complex(base),
        };
        assert!(matches, "{source:?}: actual={:?}", canonical.kind);
    }
}

#[test]
fn contradictory_builtin_specifiers_fail_closed() {
    let source = "typedef unsigned double impossible_t;";
    let arena = Arena::default();
    let mut parser = Parser::new(tokens(source), FileId::new(81), source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error());
    let diagnostic = parser.diagnostic().expect("controlled diagnostic");
    assert_eq!(diagnostic.code, "E1007");
    assert_eq!(diagnostic.message, "invalid-type-specifier-combination");
}
