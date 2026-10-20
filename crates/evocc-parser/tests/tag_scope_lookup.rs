//! Tag lookup must use the current lexical scope without scanning every tag.

use std::time::{Duration, Instant};

use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse(source: &str) -> Duration {
    let file = FileId::new(10_317);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error);
        tokens.push(token);
    }
    let arena = Arena::default();
    let start = Instant::now();
    let mut parser = Parser::new(tokens, file, source);
    let _ = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    start.elapsed()
}

fn tags(count: usize) -> Duration {
    let source = (0..count)
        .map(|index| format!("struct record_{index} {{ int value; }};\n"))
        .collect::<String>();
    (0..3).map(|_| parse(&source)).min().unwrap()
}

#[test]
fn unrelated_tag_declarations_do_not_multiply_two_global_scans() {
    let _ = tags(16);
    let small = tags(128);
    let large = tags(512);
    eprintln!("parser tag lookup: 128={small:?}, 512={large:?}");
    assert!(
        large <= small.saturating_mul(12),
        "quadrupling unrelated tags took {large:?} versus {small:?}"
    );
}

#[test]
fn definitions_and_forward_declarations_shadow_only_in_their_scope() {
    parse(
        "struct record { char value; };\n\
         _Static_assert(sizeof(struct record)==1, \"outer\");\n\
         int main(void) {\n\
           struct record; struct record *p;\n\
           struct record { long value; };\n\
           _Static_assert(sizeof(*p)==8, \"completed inner\");\n\
           { struct record { short value; };\n\
             _Static_assert(sizeof(struct record)==2, \"nested\"); }\n\
           _Static_assert(sizeof(struct record)==8, \"restored inner\");\n\
           return 0;\n\
         }\n\
         _Static_assert(sizeof(struct record)==1, \"restored outer\");",
    );
}
