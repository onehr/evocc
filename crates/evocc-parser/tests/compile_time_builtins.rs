//! GNU compile-time builtins and small expression/statement forms.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(10_117),
        "compile-time-builtins.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected input at {:?}", token.span));
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\n  source: {source}");
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n  source: {source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\n  source: {source}"
        ),
    }
}

#[test]
fn choose_expr_preserves_selected_type_value_category_and_constant_status() {
    let source = concat!(
        "long f(int i, long l) { __builtin_choose_expr(1, i, l) = 7; return i; } ",
        "_Static_assert(__builtin_choose_expr(1, 3, 4) == 3, \"selected\");\n"
    );
    accepts(source);

    let arena = Arena::default();
    let unit = parse(
        "int f(void) { int i = 0; return __builtin_choose_expr(1, 3, i++); }\n",
        &arena,
    )
    .expect("choose expression parses");
    let retained = format!("{unit:#?}");
    assert!(!retained.contains("Update"), "{retained}");
    rejects(
        "int f(int c) { return __builtin_choose_expr(c, 1, 2); }\n",
        "E2912",
    );
}

#[test]
fn types_compatible_p_uses_compatible_unqualified_types() {
    accepts(concat!(
        "_Static_assert(__builtin_types_compatible_p(int, signed int), \"signed\"); ",
        "_Static_assert(__builtin_types_compatible_p(const int, int), \"qualifier\"); ",
        "_Static_assert(__builtin_types_compatible_p(int[4], int[]), \"array\");\n"
    ));
}

#[test]
fn builtin_offsetof_accepts_member_index_and_nested_paths() {
    accepts(concat!(
        "struct s { char c; int i; int arr[4]; struct { short x; } in; }; ",
        "_Static_assert(__builtin_offsetof(struct s, i) == 4, \"i\"); ",
        "_Static_assert(__builtin_offsetof(struct s, arr[2]) == 16, \"arr\"); ",
        "_Static_assert(__builtin_offsetof(struct s, in.x) == 24, \"nested\");\n"
    ));
    rejects(
        "struct s { int i; }; unsigned long n = __builtin_offsetof(struct s, missing);\n",
        "E2906",
    );
}

#[test]
fn bit_builtins_fold_integer_constant_expressions_to_int() {
    accepts(concat!(
        "_Static_assert(__builtin_ffs(8) == 4, \"ffs\"); ",
        "_Static_assert(__builtin_ffsll(0) == 0, \"ffsll\"); ",
        "_Static_assert(__builtin_clz(1u) == 31, \"clz\"); ",
        "_Static_assert(__builtin_clzll(1ull) == 63, \"clzll\"); ",
        "_Static_assert(__builtin_ctz(8u) == 3, \"ctz\"); ",
        "_Static_assert(__builtin_popcount(7u) == 3, \"popcount\"); ",
        "_Static_assert(__builtin_popcountll(~0ull) == 64, \"popcountll\"); ",
        "char result_is_int[sizeof(__builtin_ffs(8)) == sizeof(int) ? 1 : -1];\n"
    ));
}

#[test]
fn builtin_unreachable_is_a_void_expression_statement() {
    accepts("void f(void) { __builtin_unreachable(); }\n");
}

#[test]
fn builtin_return_address_accepts_only_constant_zero() {
    accepts("void *f(void) { return __builtin_return_address(0); }\n");
    rejects(
        "void *f(void) { return __builtin_return_address(1); }\n",
        "E2913",
    );
}

#[test]
fn auto_type_infers_one_initialized_non_array_declarator() {
    accepts(concat!(
        "long f(void) { const long c = 1; __auto_type a = c; return a; } ",
        "int g(void) { int values[3]; __auto_type p = values; return sizeof(p) == 8; }\n"
    ));
    rejects("void f(void) { __auto_type a, b = 1; }\n", "E2915");
    rejects("void f(void) { __auto_type a; }\n", "E2915");
    rejects("void f(void) { __auto_type a[2] = { 1, 2 }; }\n", "E2915");
}

#[test]
fn elvis_operator_uses_its_left_operand_as_the_true_arm() {
    accepts("int f(int value) { return value ?: 9; }\n");
}

#[test]
fn case_ranges_are_inclusive_nonoverlapping_integer_ranges() {
    accepts("int f(int value) { switch (value) { case 1 ... 3: return 7; default: return 0; } }\n");
    accepts(
        "int f(int value) { switch (value) { case -1 ... 1: switch (value) { case -1 ... 1: return 7; } default: return 0; } }\n",
    );
    rejects(
        "int f(int value) { switch (value) { case 3 ... 1: return 7; } return 0; }\n",
        "E2916",
    );
    rejects(
        "int f(int value) { switch (value) { case 1 ... 3: return 7; case 3 ... 5: return 8; } return 0; }\n",
        "E2916",
    );
}

#[test]
fn local_label_declarations_bind_gotos_in_their_block() {
    accepts("int f(int value) { __label__ out; if (value) goto out; return 0; out: return 1; }\n");
    accepts(
        "int f(int value) { out: { __label__ out; if (value) goto out; out: return 1; } return 0; }\n",
    );
}
