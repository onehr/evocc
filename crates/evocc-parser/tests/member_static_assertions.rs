//! C11 static assertions are permitted among record member declarations.

use evocc_chir::{CHirDecl, CHirUnit, TagMembers};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_308);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "fixture must parse: {:?}\nsource: {source}",
        parser.diagnostic()
    );
    unit
}

#[test]
fn static_assertion_declares_no_record_member() {
    let arena = Arena::default();
    let unit = parse(
        "struct measured { int value; _Static_assert(sizeof(int) == 4, \"int width\"); };\n",
        &arena,
    );
    let members = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Tag(tag)
                if tag
                    .name
                    .as_ref()
                    .is_some_and(|name| name.spelling == "measured") =>
            {
                Some(&tag.members)
            }
            _ => None,
        })
        .expect("fixture defines measured");
    let TagMembers::Struct(members) = members else {
        panic!("measured must be a struct");
    };
    assert_eq!(members.len(), 1, "_Static_assert contributes no member");
}

#[test]
fn static_assertion_is_accepted_in_an_anonymous_record() {
    let arena = Arena::default();
    parse(
        concat!(
            "struct outer { union { int value; ",
            "_Static_assert(sizeof(int) == 4, \"int width\"); }; };\n",
        ),
        &arena,
    );
}

#[test]
fn static_assertion_in_sizeof_anonymous_record_matches_the_cohort_form() {
    let arena = Arena::default();
    parse(
        concat!(
            "unsigned long f(void) { return sizeof(struct { ",
            "_Static_assert(sizeof(unsigned long) == 8, \"long width\"); }); }\n",
        ),
        &arena,
    );
}
