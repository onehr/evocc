use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_202);
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
            "designated bound must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

#[test]
fn sparse_designator_sets_an_unbounded_array_size() {
    accepts(concat!(
        "int values[] = { [3] = 1 }; ",
        "_Static_assert(sizeof values == 4 * sizeof(int), \"sparse bound\");\n",
    ));
}

#[test]
fn range_designator_sets_an_unbounded_array_size() {
    accepts(concat!(
        "int values[] = { [0 ... 4] = 1 }; ",
        "_Static_assert(sizeof values == 5 * sizeof(int), \"range bound\");\n",
    ));
}

#[test]
fn largest_designator_wins_over_positional_count() {
    accepts(concat!(
        "int values[] = { 1, 2, [7] = 3, 4 }; ",
        "_Static_assert(sizeof values == 9 * sizeof(int), \"mixed bound\");\n",
    ));
}

#[test]
fn nested_designators_infer_the_outer_bound() {
    accepts(concat!(
        "int values[][2] = { [2 ... 4] = { 1, 2 } }; ",
        "_Static_assert(sizeof values == 5 * 2 * sizeof(int), \"nested bound\");\n",
    ));
}
