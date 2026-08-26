use evocc_common::FileId;
use evocc_lex::{Lexer, TokenKind};

fn kinds(source: &str) -> Vec<TokenKind> {
    let mut lexer = Lexer::new(FileId::new(9_811), source);
    let mut kinds = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        kinds.push(token.kind);
    }
    kinds
}

#[test]
fn preprocessing_hash_operators_are_single_tokens() {
    assert_eq!(kinds("#"), vec![TokenKind::Hash, TokenKind::Eof]);
    assert_eq!(kinds("##"), vec![TokenKind::HashHash, TokenKind::Eof]);
    assert_eq!(
        kinds("#a##b"),
        vec![
            TokenKind::Hash,
            TokenKind::Identifier,
            TokenKind::HashHash,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]
    );
}
