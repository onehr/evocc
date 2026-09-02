use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_140);
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
fn function_name_has_the_measured_array_size_and_decay_type() {
    accepts(concat!(
        "void alpha(void) { ",
        "_Static_assert(sizeof __func__ == 6, \"size\"); ",
        "_Static_assert(_Generic(__func__, const char *: 1, default: 0), \"decay\"); ",
        "}\n",
    ));
}

#[test]
fn alternate_function_name_spelling_has_the_same_object_type() {
    accepts(concat!(
        "void beta(void) { ",
        "_Static_assert(sizeof __FUNCTION__ == 5, \"size\"); ",
        "_Static_assert(_Generic(&__FUNCTION__, const char (*)[5]: 1, default: 0), \"address\"); ",
        "}\n",
    ));
}

#[test]
fn function_name_is_one_unevaluated_address_constant_in_nested_scopes() {
    accepts(concat!(
        "static const char *kept; ",
        "void inner(void) { static const char *local = __func__; kept = local; ",
        "{ const char *same = __func__; kept = same; } }\n",
    ));
}

#[test]
fn pretty_function_uses_the_conflict_diagnostic() {
    let arena = Arena::default();
    let error = parse("void pretty(void) { (void)__PRETTY_FUNCTION__; }\n", &arena)
        .expect_err("the oracle-conflicting spelling must be refused");
    assert_eq!(error.code, "E2921");
    assert_eq!(error.message, "pretty-function-unsupported");
}
