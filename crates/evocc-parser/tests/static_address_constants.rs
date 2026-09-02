use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_144);
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
fn a_constant_array_subscript_address_survives_a_pointer_cast() {
    accepts("int values[3]; static const void *second = (const void *)&values[1];\n");
}

#[test]
fn a_static_member_address_survives_a_wide_integer_cast() {
    accepts(concat!(
        "struct pair { int first; int second; }; static struct pair pair; ",
        "static unsigned long member = (unsigned long)&pair.second;\n",
    ));
}

#[test]
fn address_constants_compose_inside_nested_aggregate_initializers() {
    accepts(concat!(
        "int values[3]; int target(void); ",
        "struct inner { const void *element; unsigned long function; }; ",
        "struct outer { int head; struct inner inner; }; ",
        "static struct outer table = { .head = 1, .inner = { ",
        ".element = (const void *)&values[2], .function = (unsigned long)&target } };\n",
    ));
}
