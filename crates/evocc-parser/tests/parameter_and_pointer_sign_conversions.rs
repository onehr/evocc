use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_142);
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
fn adjusted_array_parameter_initializes_the_corresponding_pointer() {
    accepts(concat!(
        "int copy(const char * const names[]) { ",
        "const char * const *pointer = names; return pointer == names; }\n",
    ));
}

#[test]
fn static_and_qualified_array_parameter_assigns_as_a_pointer() {
    accepts(concat!(
        "int copy(unsigned values[const static 4]) { ",
        "unsigned *pointer; pointer = values; return pointer == values; }\n",
    ));
}

#[test]
fn multidimensional_array_parameter_returns_the_adjusted_pointer() {
    accepts("int (*rows(int values[][3]))[3] { return values; }\n");
}

#[test]
fn signedness_only_pointer_initialization_is_accepted() {
    accepts("void init(int *input) { unsigned int *output = input; (void)output; }\n");
}

#[test]
fn signedness_only_pointer_assignment_is_accepted() {
    accepts("void assign(int *input) { unsigned int *output; output = input; (void)output; }\n");
}

#[test]
fn signedness_only_pointer_argument_is_accepted() {
    accepts("void take(unsigned int *); void pass(int *input) { take(input); }\n");
}

#[test]
fn signedness_only_pointer_return_is_accepted() {
    accepts("unsigned int *pass(int *input) { return input; }\n");
}

#[test]
fn structurally_incompatible_pointers_remain_e2202() {
    let arena = Arena::default();
    let error = parse(
        "struct left { int x; }; struct right { int x; }; void f(struct left *p) { struct right *q = p; }\n",
        &arena,
    )
    .expect_err("distinct record pointers stay incompatible");
    assert_eq!(error.code, "E2202");
    assert_eq!(error.message, "invalid-initializer-type");
}
