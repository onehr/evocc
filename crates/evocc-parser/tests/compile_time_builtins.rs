//! GNU compile-time builtins and small expression/statement forms.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

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

fn first_diagnostic(source: &str) -> ParseDiagnostic {
    let arena = Arena::default();
    let file = SourceFile::new(
        FileId::new(10_118),
        "compile-time-builtin-diagnostic.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    parser.parse_translation_unit_in(&arena);
    parser
        .diagnostic()
        .cloned()
        .expect("fixture must produce a diagnostic")
}

fn entry(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.body.is_some() && function.name.spelling == "main" =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main")
}

fn evaluate(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena)?;
    let outcome = interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(200_000))
        .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a scalar")
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
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
fn classify_type_is_an_integer_constant_without_evaluating_its_operand() {
    assert_eq!(
        evaluate(include_str!(
            "../../../tests/fixtures/type-classification.c"
        ))
        .unwrap(),
        0,
    );
}

#[test]
fn classify_type_refuses_conflicting_or_malformed_forms() {
    rejects(
        "int f(void){return __builtin_classify_type((_Bool)1);}",
        "E2941",
    );
    rejects(
        "int f(void){_Bool b=0;return __builtin_classify_type(b);}",
        "E2941",
    );
    rejects(
        "int f(void){return __builtin_classify_type((void)0);}",
        "E2913",
    );
    rejects("int f(void){return __builtin_classify_type();}", "E2909");
    rejects("int f(void){return __builtin_classify_type(1,2);}", "E2909");
    rejects("int f(void){return __builtin_classify_type(int);}", "E1003");
    rejects("int f(void){return __builtin_classify_type;}", "E2910");
    rejects(
        "int f(void){return __builtin_classify_type(missing);}",
        "E2001",
    );
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
fn builtin_offsetof_constant_indexes_are_unchecked_arithmetic() {
    accepts(concat!(
        "struct s { int head; int arr[4]; }; ",
        "_Static_assert(__builtin_offsetof(struct s, arr[-1]) == 0, \"negative\"); ",
        "_Static_assert(__builtin_offsetof(struct s, arr[9]) == 40, \"past end\");\n",
    ));
}

#[test]
fn builtin_offsetof_dynamic_index_is_evaluated_once() {
    assert_eq!(
        evaluate(concat!(
            "struct s { int head; int arr[4]; }; ",
            "int main(void) { int index = 2; ",
            "unsigned long offset = __builtin_offsetof(struct s, arr[index++]); ",
            "return (int)(offset + 100 * index); }\n",
        ))
        .unwrap(),
        312,
    );
    assert_eq!(
        evaluate(concat!(
            "struct s { int head; int arr[4]; }; ",
            "int main(void) { int index = 2; ",
            "unsigned long offset = __builtin_offsetof(struct s, arr[index++ + 0]); ",
            "return (int)(offset + 100 * index); }\n",
        ))
        .unwrap(),
        312,
    );
    assert_eq!(
        evaluate(concat!(
            "struct s { int head; int arr[4]; }; ",
            "int main(void) { int memory[1] = { 0 }; int index = 2; ",
            "unsigned long offset = __builtin_offsetof(struct s, arr[index++ + 0]); ",
            "return (int)(offset + 100 * index + memory[0]); }\n",
        ))
        .unwrap(),
        312,
    );
}

#[test]
fn builtin_offsetof_dynamic_index_is_not_an_integer_constant_expression() {
    rejects(
        concat!(
            "struct s { int head; int arr[4]; }; int index; ",
            "_Static_assert(__builtin_offsetof(struct s, arr[index]) == 4, \"dynamic\");\n",
        ),
        "E2912",
    );
}

#[test]
fn builtin_offsetof_dynamic_index_is_not_a_file_scope_array_bound() {
    rejects(
        concat!(
            "struct s { int head; int arr[4]; }; int index; ",
            "int values[__builtin_offsetof(struct s, arr[index])];\n",
        ),
        "E2203",
    );
}

#[test]
fn builtin_object_size_returns_proven_array_extents() {
    accepts(concat!(
        "int known[4]; ",
        "_Static_assert(__builtin_object_size(\"abc\", 1) == 4, \"literal extent\"); ",
        "_Static_assert(__builtin_object_size(known, 0) == 16, \"mode 0\"); ",
        "_Static_assert(__builtin_object_size(known, 1) == 16, \"mode 1\"); ",
        "_Static_assert(__builtin_object_size(known, 2) == 16, \"mode 2\"); ",
        "_Static_assert(__builtin_object_size(known, 3) == 16, \"mode 3\");\n",
    ));
}

#[test]
fn builtin_object_size_never_evaluates_its_operand() {
    assert_eq!(
        evaluate(concat!(
            "int main(void) { int side = 0; int known[4]; ",
            "unsigned long size = __builtin_object_size((side++, known), 0); ",
            "return side * 10 + (size == (unsigned long)-1); }\n",
        ))
        .unwrap(),
        1,
    );
}

#[test]
fn builtin_object_size_rejects_invalid_modes_at_the_mode_operand() {
    for (source, mode) in [
        (
            "unsigned long size = __builtin_object_size((void *)0, 4);\n",
            "4",
        ),
        (
            "unsigned long f(int mode) { return __builtin_object_size((void *)0, mode); }\n",
            "mode",
        ),
    ] {
        let diagnostic = first_diagnostic(source);
        assert_eq!(diagnostic.code, "E2925", "source: {source}");
        assert_eq!(
            diagnostic.message, "builtin-mode-out-of-range",
            "source: {source}"
        );
        assert_eq!(
            &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            mode,
            "diagnostic must point at the mode operand"
        );
    }
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
fn activation_address_depths_convert_to_unsigned_int_without_becoming_constants() {
    for builtin in ["__builtin_return_address", "__builtin_frame_address"] {
        for depth in ["0", "1", "65535", "0x100000001UL", "-4294967295L"] {
            accepts(&format!("void *f(void) {{ return {builtin}({depth}); }}"));
        }
        for depth in ["-1", "65536", "0x1ffffUL", "n", "1.0"] {
            rejects(
                &format!("void *f(int n) {{ return {builtin}({depth}); }}"),
                "E2913",
            );
        }
        // A runtime null pointer value is not a C null pointer constant.
        accepts(&format!("int x; _Static_assert(__builtin_types_compatible_p(typeof(1 ? &x : {builtin}(1)), void *), \"pointer type\");"));
        rejects(
            &format!("_Static_assert({builtin}(1) == 0, \"not an ICE\");"),
            "E2912",
        );
    }
}

#[test]
fn builtin_frame_address_converts_its_constant_depth_and_checks_the_target_range() {
    accepts(concat!(
        "void *zero(void) { return __builtin_frame_address(0); } ",
        "void *one(void) { return __builtin_frame_address(1); } ",
        "_Static_assert(__builtin_types_compatible_p(",
        "typeof(__builtin_frame_address(1)), void *), \"frame address type\");\n",
    ));
    rejects(
        "void *runtime_level(int level) { return __builtin_frame_address(level); }\n",
        "E2913",
    );
    accepts("void *f(void){return __builtin_return_address(0x100000000UL);}");
    accepts("void *f(void){return __builtin_frame_address(0x100000001UL);}");
    for argument in ["-1", "65536"] {
        rejects(
            &format!("void *f(void){{return __builtin_frame_address({argument});}}"),
            "E2913",
        );
    }
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
