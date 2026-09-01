use evocc_common::FileId;
use evocc_lex::{Lexer, TokenKind};

fn kinds(source: &str) -> Vec<TokenKind> {
    let mut lexer = Lexer::new(FileId::new(10_331), source);
    let mut kinds = Vec::new();
    while let Some(token) = lexer.next() {
        kinds.push(token.kind);
    }
    kinds
}

#[test]
fn every_gnu_typeof_spelling_uses_the_same_token() {
    for spelling in ["typeof", "__typeof__", "__typeof"] {
        assert_eq!(
            kinds(spelling),
            vec![TokenKind::Typeof, TokenKind::Eof],
            "{spelling} must use the shared typeof spelling table",
        );
    }
}
