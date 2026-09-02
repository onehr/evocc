use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_143);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
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
fn pointee_qualifiers_do_not_make_the_pointer_object_const() {
    accepts(concat!(
        "void assign(const char * const *source) { ",
        "const char * const *target; target = source; (void)target; }\n",
    ));
}

#[test]
fn a_pointer_typed_member_is_modifiable_through_arrow() {
    accepts(concat!(
        "struct item { const char * const *names; }; ",
        "void assign(struct item *item, const char * const *names) { item->names = names; }\n",
    ));
}

#[test]
fn the_same_member_is_modifiable_through_nested_projections() {
    accepts(concat!(
        "struct item { const char * const *names; }; ",
        "void assign(struct item items[2], const char * const *names) { ",
        "(*items).names = names; items[1].names = names; }\n",
    ));
}

#[test]
fn a_const_pointer_object_and_const_containing_record_stay_nonmodifiable() {
    let arena = Arena::default();
    let pointer = parse(
        "void f(int *value) { int * const fixed = value; fixed = value; }\n",
        &arena,
    )
    .expect_err("a top-level const pointer is not modifiable");
    assert_eq!(pointer.code, "E2101");

    let arena = Arena::default();
    let record = parse(
        "struct fixed { const int value; }; void f(struct fixed *p) { struct fixed q = *p; q = *p; }\n",
        &arena,
    )
    .expect_err("a record containing a const member is not modifiable");
    assert_eq!(record.code, "E2101");
}
