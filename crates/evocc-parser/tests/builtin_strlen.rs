use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 100_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_145),
        "builtin-strlen.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
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

fn run(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena)?;
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .map_err(|error| format!("value: {error}"))
}

#[test]
fn string_literal_length_is_a_size_t_integer_constant_expression() {
    assert_eq!(
        run(concat!(
            "typedef unsigned long size_t; ",
            "_Static_assert(__builtin_strlen(\"abc\") == 3, \"literal folds\"); ",
            "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_strlen(\"x\")), size_t), \"type\"); ",
            "int main(void) { return (int)__builtin_strlen(\"a\\n\"); }"
        ))
        .unwrap(),
        2
    );
}

#[test]
fn literal_length_stops_at_the_first_nul_without_changing_array_extent() {
    assert_eq!(
        run(concat!(
            "_Static_assert(__builtin_strlen(\"a\\0tail\") == 1, \"first nul\"); ",
            "_Static_assert(__builtin_strlen(\"\\0tail\") == 0, \"empty prefix\"); ",
            "_Static_assert(__builtin_strlen(\"a\\0\" \"tail\") == 1, \"fragments\"); ",
            "_Static_assert(sizeof(\"a\\0tail\") == 7, \"array extent\"); ",
            "int main(void) { return __builtin_strlen(\"a\\0tail\"); }"
        ))
        .unwrap(),
        1
    );
}

#[test]
fn literal_folding_does_not_bypass_arity_or_operand_types() {
    for (source, code) in [
        ("int f(void){return __builtin_strcmp(\"a\");}", "E2909"),
        (
            "int f(void){return __builtin_strcmp(\"a\",\"b\",\"c\");}",
            "E2909",
        ),
        (
            "int f(void){return __builtin_strlen(\"a\",\"b\");}",
            "E2909",
        ),
        ("int f(void){return __builtin_strlen(L\"a\");}", "E2913"),
        (
            "int f(void){return __builtin_strcmp(L\"a\",L\"b\");}",
            "E2913",
        ),
    ] {
        let arena = Arena::default();
        let error = parse(source, &arena).unwrap_err();
        assert!(error.starts_with(code), "{source}: {error}");
    }
}

#[test]
fn non_literal_length_is_an_ordinary_call_to_visible_strlen() {
    assert_eq!(
        run(concat!(
            "typedef unsigned long size_t; ",
            "size_t strlen(const char *s) { return s[0] ? 1005 : 1000; } ",
            "int main(void) { char text[8] = \"hello\"; return (int)__builtin_strlen(text); }"
        ))
        .unwrap(),
        1005
    );
}

#[test]
fn digit_builtin_is_not_replaced_by_a_visible_library_body() {
    assert_eq!(run("int isdigit(int value) { return value + 7; } int main(void) { return __builtin_isdigit(42); }").unwrap(), 0);
}

#[test]
fn non_literal_length_uses_the_builtin_prototype_without_a_visible_declaration() {
    let arena = Arena::default();
    parse(
        "typedef unsigned long size_t; size_t f(char *s) { return __builtin_strlen(s); }",
        &arena,
    )
    .expect("the builtin supplies the external call prototype");
}

#[test]
fn non_literal_length_rejects_an_incompatible_visible_declaration() {
    let arena = Arena::default();
    let error = parse(
        "typedef unsigned long size_t; int strlen(int); size_t f(char *s) { return __builtin_strlen(s); }",
        &arena,
    )
    .expect_err("an incompatible visible declaration must fail");
    assert_eq!(error, "E2927: builtin-library-declaration-incompatible");
}

#[test]
fn initialized_file_scope_strings_are_constants_in_every_integer_context() {
    for (declarations, expression, length) in [
        ("static const char a[] = \"abc\";", "a", 3),
        ("const char a[] = \"abc\";", "a + 1", 2),
        ("const char a[] = \"abc\";", "(const char *)&a + 1", 2),
        ("static const char a[] = \"abc\";", "1 + a", 2),
        ("static const char a[] = \"abc\";", "&a[3]", 0),
        ("static const char a[] = \"ab\\0cd\";", "a", 2),
        ("static const char a[8] = {'a', 'b'};", "a", 2),
        (
            "static const unsigned char a[] = {'a', 255, 0};",
            "(const char *)a",
            2,
        ),
        ("static const char a[8] = {[3] = 'x'};", "a + 3", 1),
        ("static const char a[8] = {[3] = 'x'};", "a + 4", 0),
        ("static const char a[2][4] = {\"ab\", \"cde\"};", "a[1]", 3),
        (
            "static const struct { int n; char s[8]; } a = {1, \"abc\"};",
            "a.s",
            3,
        ),
        (
            "static const union { char s[8]; int n; } a = {.s = \"abc\"};",
            "a.s",
            3,
        ),
        ("static const char *const p = \"abc\" + 1;", "p", 2),
        ("static char *const p = \"abc\";", "p", 3),
        (
            "static const char a[] = \"abc\", *const p = a + 1;",
            "p + 1",
            1,
        ),
        ("static const char a[] = \"abc\";", "1 ? a : \"other\"", 3),
        ("", "&\"abc\"[1]", 2),
        ("", "(const char *)\"abc\" + 1", 2),
    ] {
        let source = format!(
            "{declarations} \
             _Static_assert(__builtin_strlen({expression}) == {length}, \"length\"); \
             enum {{ E = __builtin_strlen({expression}) }}; \
             static char buffer[__builtin_strlen({expression}) + 1]; \
             struct bits {{ unsigned x : __builtin_strlen({expression}) + 1; }}; \
             static unsigned long value = __builtin_strlen({expression}); \
             int main(void) {{ \
               switch (value) {{ case __builtin_strlen({expression}): break; default: return 1; }} \
               return (E != {length}) || sizeof buffer != {length} + 1 || \
                 __builtin_choose_expr(__builtin_strlen({expression}) == {length}, 0, 1); \
             }}"
        );
        assert_eq!(
            run(&source).unwrap_or_else(|e| panic!("{expression}: {e}")),
            0
        );
    }
}

#[test]
fn other_string_operands_keep_their_ordinary_call_classification() {
    for (declarations, local, expression) in [
        ("static char a[] = \"abc\";", "", "a"),
        ("static const char *p = \"abc\";", "", "p"),
        (
            "static const volatile char a[] = \"abc\";",
            "",
            "(const char *)a",
        ),
        ("static const char a[3] = \"abc\";", "", "a"),
        ("static const char a[] = \"abc\";", "", "a + 4"),
        ("static const char a[] = \"abc\";", "", "a - 1"),
        ("static const char a[] = \"abc\";", "", "(a + 5) - 5"),
        (
            "static const char a[] = \"abc\";",
            "",
            "(const char *)(&a + 1)",
        ),
        ("", "", "(const char *)(&\"abc\" + 1)"),
        ("static const char a[4];", "", "a"),
        ("extern const char a[];", "", "a"),
        ("__attribute__((weak)) const char a[] = \"abc\";", "", "a"),
        ("static const char *const a[] = {\"abc\"};", "", "a[0]"),
        (
            "static const struct { const char *const s; } a = {\"abc\"};",
            "",
            "a.s",
        ),
        ("static const int a = 0x6162;", "", "(const char *)&a"),
        ("static const char a[] = \"abc\";", "", "(0, a)"),
        ("static const char a[] = \"abc\";", "", "({a;})"),
        (
            "static const char a[] = \"abc\"; static const int k = 1;",
            "",
            "a + k",
        ),
        (
            "static const int k = 1; static const char a[] = {k, 0};",
            "",
            "a",
        ),
        (
            "static const union { int n; char s[8]; } a = {.n = 0};",
            "",
            "a.s",
        ),
        ("static const char *const p = (const char *)&p;", "", "p"),
        ("", "const char a[] = \"abc\";", "a"),
        ("", "static const char a[] = \"abc\";", "a"),
        ("", "static const char *const p = \"abc\";", "p"),
        ("", "const char *const p = \"abc\";", "p"),
    ] {
        let source = format!(
            "{declarations} int main(void) {{ {local} \
             _Static_assert(!__builtin_constant_p(__builtin_strlen({expression})), \"ordinary call\"); \
             return 0; }}"
        );
        assert_eq!(run(&source).unwrap_or_else(|e| panic!("{source}: {e}")), 0);
    }
}

#[test]
fn string_folding_keeps_call_and_operand_effects() {
    assert_eq!(
        run(concat!(
            "static const char a[] = \"abc\"; static const char *const p = a; ",
            "static const char *mutable = a; static int calls, operands; ",
            "unsigned long strlen(const char *s) { calls++; return s[0] == 'a' ? 91 : 92; } ",
            "const char *choose(void) { operands++; return a; } ",
            "int main(void) { ",
            "unsigned long x = __builtin_strlen(a), y = __builtin_strlen(p); ",
            "unsigned long z = __builtin_strlen(mutable), w = __builtin_strlen(choose()); ",
            "unsigned long ordinary = strlen(a); ",
            "return x != 3 || y != 3 || z != 91 || w != 91 || ordinary != 91 ",
            "|| calls != 3 || operands != 1; }"
        ))
        .unwrap(),
        0
    );
}

#[test]
fn sparse_string_queries_do_not_materialize_the_declared_extent() {
    let arena = Arena::default();
    parse(
        concat!(
            "static const char a[1000000000000UL] = {[999999999998UL] = 'x'}; ",
            "_Static_assert(__builtin_strlen(a) == 0, \"zero fill\"); ",
            "_Static_assert(__builtin_strlen(a + 999999999998UL) == 1, \"tail\");"
        ),
        &arena,
    )
    .expect("work must depend on initializer entries, not array extent");
}

#[test]
fn later_interposition_cannot_leave_a_stale_string_constant() {
    for declaration in [
        "extern const char a[] __attribute__((weak));",
        "extern const char a[] __asm__(\"other_array\");",
    ] {
        let arena = Arena::default();
        let source =
            format!("const char a[] = \"abc\"; enum {{ N = __builtin_strlen(a) }}; {declaration}");
        let error =
            parse(&source, &arena).expect_err("binding changes must refuse the old constant");
        assert!(error.starts_with("E2954:"), "{source}: {error}");
    }
}
