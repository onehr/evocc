//! Array-bound diagnostics fixed by specification 10.1.19.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{TargetDataLayout, TypeLayout};

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

fn object_layout(source: &str, object: &str) -> TypeLayout {
    let arena = Arena::default();
    let unit = parse(source, &arena).expect("layout fixture must parse");
    let declaration = unit
        .declarations()
        .iter()
        .find_map(|declaration| match declaration {
            evocc_chir::CHirDecl::Object(declaration) if declaration.name.spelling == object => {
                Some(declaration)
            }
            _ => None,
        })
        .expect("named object must be present");
    TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), declaration.ty)
        .expect("object layout must be available")
}

#[test]
fn negative_file_scope_array_bound_is_rejected() {
    rejects("int negative[-1];\n", "E2923", "invalid-array-bound");
}

#[test]
fn zero_length_arrays_have_the_sealed_oracle_layouts() {
    let source = concat!(
        "struct mid { int a; char d[0]; int b; };\n",
        "struct last { int a; char d[0]; };\n",
        "struct comp { char pad[20 - 2 * sizeof(long) - sizeof(unsigned)]; int x; };\n",
        "struct mid mid; struct last last; struct comp comp; int g[0];\n",
        "_Static_assert(sizeof(struct mid) == 8, \"mid\");\n",
        "_Static_assert(sizeof(struct last) == 4, \"last\");\n",
        "_Static_assert(sizeof(struct comp) == 4, \"comp\");\n",
        "_Static_assert(__builtin_offsetof(struct comp, x) == 0, \"padoff\");\n",
        "_Static_assert(sizeof(g) == 0, \"global\");\n",
    );
    assert_eq!(object_layout(source, "mid"), TypeLayout::new(8, 4));
    assert_eq!(object_layout(source, "last"), TypeLayout::new(4, 4));
    assert_eq!(object_layout(source, "comp"), TypeLayout::new(4, 4));
    assert_eq!(object_layout(source, "g"), TypeLayout::new(0, 4));
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
fn zero_length_arrays_are_valid_in_local_parameter_and_abstract_positions() {
    let arena = Arena::default();
    parse(
        "int parameter(int value[0]);\n\
         int local(void) { int value[0]; return sizeof(int[0]); }\n",
        &arena,
    )
    .expect("a zero bound is valid wherever an array bound is written");
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
fn block_scope_variable_length_array_is_rejected_without_an_initializer() {
    rejects(
        "int local(int bound) { int object[bound]; return 0; }\n",
        "E2203",
        "variable-length-object",
    );
}

#[test]
fn captured_typedef_sizes_do_not_fabricate_dynamic_object_storage() {
    rejects(
        "int local(int bound) { typedef int A[bound]; A object; return 0; }",
        "E2203",
        "variable-length-object",
    );
}

#[test]
fn initialized_block_scope_variable_length_array_uses_the_canonical_message() {
    rejects(
        "int local(int bound) { int object[bound] = { 1 }; return 0; }\n",
        "E2203",
        "variable-length-object",
    );
}

#[test]
fn parameter_variable_bound_is_adjusted_to_a_pointer() {
    let arena = Arena::default();
    let unit = parse(
        "int global_bound; int parameter(int value[global_bound]);\n",
        &arena,
    )
    .expect("an array parameter creates a pointer, not a variable-length object");
    let parameter = unit
        .iter()
        .find_map(|decl| match decl {
            evocc_chir::CHirDecl::Function(function) => function.parameters.parameters().first(),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        unit.types().get(parameter.ty).unwrap().kind,
        evocc_types::TypeKind::Pointer(_)
    ));
}

#[test]
fn parameter_adjustment_does_not_admit_noninteger_bounds() {
    for source in [
        "void f(int a[1.5]);",
        "void f(int a[(int *)0]);",
        "void side(void); void f(int a[side()]);",
    ] {
        rejects(source, "E2923", "invalid-array-bound");
    }
}

#[test]
fn inner_parameter_bounds_must_be_integer_even_in_prototypes() {
    for source in [
        "void f(int a[2][1.5]);",
        "void f(int (*a)[(int *)0]);",
        "void side(void); void f(int a[2][side()]);",
    ] {
        rejects(source, "E2923", "invalid-array-bound");
    }
}

#[test]
fn positive_constant_bounds_remain_valid() {
    let arena = Arena::default();
    parse(
        "_Static_assert(sizeof(int[1 + 2]) == 12, \"bound\");\n\
         int object[3];\n\
         struct record { int member[2]; };\n",
        &arena,
    )
    .expect("positive constant bounds remain accepted");
}
