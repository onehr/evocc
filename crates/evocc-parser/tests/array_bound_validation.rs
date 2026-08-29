//! Array-bound diagnostics fixed by specification 10.1.19.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_120);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn rejects(source: &str, code: &str, message: &str) {
    let arena = Arena::default();
    let error = parse(source, &arena).expect_err("fixture must be rejected");
    assert_eq!(error.code, code, "source: {source}");
    assert_eq!(error.message, message, "source: {source}");
}

#[test]
fn negative_file_scope_array_bound_is_rejected() {
    rejects("int negative[-1];\n", "E2923", "invalid-array-bound");
}

#[test]
fn zero_file_scope_array_bound_is_rejected() {
    rejects("int zero[0];\n", "E2923", "invalid-array-bound");
}

#[test]
fn negative_record_member_array_bound_is_rejected() {
    rejects(
        "struct record { int member[-1]; };\n",
        "E2923",
        "invalid-array-bound",
    );
}

#[test]
fn zero_local_array_bound_is_rejected() {
    rejects(
        "int local(void) { int value[0]; return 0; }\n",
        "E2923",
        "invalid-array-bound",
    );
}

#[test]
fn zero_parameter_array_bound_is_rejected() {
    rejects(
        "int parameter(int value[0]);\n",
        "E2923",
        "invalid-array-bound",
    );
}

#[test]
fn file_scope_variable_length_array_is_rejected_without_an_initializer() {
    rejects(
        "int bound; int object[bound];\n",
        "E2203",
        "variable-length-object",
    );
}

#[test]
fn record_member_variable_length_array_is_rejected() {
    rejects(
        "int bound; struct record { int member[bound]; };\n",
        "E2203",
        "variable-length-object",
    );
}

#[test]
fn a_later_file_declarator_cannot_hide_a_variable_bound() {
    rejects(
        "int bound; int fixed[2], variable[bound];\n",
        "E2203",
        "variable-length-object",
    );
}

#[test]
fn abstract_array_bounds_are_evaluated_where_written() {
    rejects(
        "int value(void) { return sizeof(int[0]); }\n",
        "E2923",
        "invalid-array-bound",
    );
}

#[test]
fn positive_constant_bounds_and_permitted_vlas_remain_valid() {
    let arena = Arena::default();
    parse(
        "_Static_assert(sizeof(int[1 + 2]) == 12, \"bound\");\n\
         int object[3];\n\
         struct record { int member[2]; };\n\
         int global_bound;\n\
         int prototype(int parameter[global_bound]);\n\
         int local(int bound) {\n\
             int variable[bound];\n\
             return 0;\n\
         }\n",
        &arena,
    )
    .expect("positive constant bounds and block or parameter VLAs remain accepted");
}
