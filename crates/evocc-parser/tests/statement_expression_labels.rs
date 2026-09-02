use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_141);
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
fn a_single_label_preserves_the_final_scalar_value() {
    accepts(
        "long value(long input) { long result = ({ long q = input; out: q; }); return result; }\n",
    );
}

#[test]
fn recursively_prefixed_labels_preserve_the_final_pointer_type() {
    accepts(concat!(
        "int value(int *input) { ",
        "int *result = ({ first: second: input; }); ",
        "return result == input; }\n",
    ));
}

#[test]
fn a_local_label_declaration_does_not_change_the_final_value() {
    accepts(concat!(
        "long value(long input) { long result = ({ __label__ out; long q = input; ",
        "if (q) goto out; q = 2; out: q; }); return result; }\n",
    ));
}
