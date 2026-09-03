use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_205);
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
            "static address constant must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

#[test]
fn integer_constant_cast_to_pointer_is_an_address_constant() {
    accepts(concat!(
        "struct entry { void *next; }; ",
        "static struct entry value = { .next = (void *)2 };\n",
    ));
}

#[test]
fn static_array_member_decay_is_an_address_constant() {
    accepts(concat!(
        "struct storage { int values[4]; }; static struct storage object; ",
        "static int *pointer = object.values;\n",
    ));
}

#[test]
fn static_multidimensional_element_decay_is_an_address_constant() {
    accepts("static int matrix[2][3]; static int *row = matrix[1];\n");
}

#[test]
fn nested_static_array_decay_is_valid_inside_an_aggregate() {
    accepts(concat!(
        "struct storage { unsigned long bits[2]; }; static struct storage object; ",
        "struct descriptor { unsigned long *bits; }; ",
        "static struct descriptor descriptor = { .bits = object.bits };\n",
    ));
}
