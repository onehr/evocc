//! C11 declaration specifiers may be written in any permitted order.

use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TypeKind;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_306);
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

fn object_type<'arena>(unit: &'arena CHirUnit<'arena>, name: &str) -> evocc_types::TypeId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == name => Some(object.ty),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture declares object {name}"))
}

#[test]
fn qualifier_after_record_specifier_qualifies_the_pointee() {
    let arena = Arena::default();
    let unit = parse(
        "struct midr_range { unsigned long model; }; struct midr_range const *ranges;\n",
        &arena,
    );
    let pointer = unit.types().get(object_type(&unit, "ranges")).unwrap();
    let TypeKind::Pointer(pointee) = pointer.kind else {
        panic!("ranges must be a pointer");
    };
    assert!(unit.types().get(pointee).unwrap().qualifiers.is_const);
}

#[test]
fn qualifier_after_enum_specifier_is_preserved() {
    let arena = Arena::default();
    let unit = parse(
        "enum state { READY }; enum state const current = READY;\n",
        &arena,
    );
    assert!(
        unit.types()
            .get(object_type(&unit, "current"))
            .unwrap()
            .qualifiers
            .is_const
    );
}

#[test]
fn qualifier_after_record_specifier_is_preserved_on_an_object() {
    let arena = Arena::default();
    let unit = parse(
        "struct slot { int value; }; struct slot volatile active;\n",
        &arena,
    );
    assert!(
        unit.types()
            .get(object_type(&unit, "active"))
            .unwrap()
            .qualifiers
            .is_volatile
    );
}
