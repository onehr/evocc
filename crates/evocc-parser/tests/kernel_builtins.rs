//! Kernel-facing GNU builtins resolved by the front end (specification 10.1.14).

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{render_unit_summary, CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

const STEP_BUDGET: u64 = 200_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_502),
        "kernel-builtins.c".to_owned(),
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
        .map_err(|error| error.to_string())
}

fn object_layout(source: &str, name: &str) -> (u64, u32) {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| panic!("must parse: {error}"));
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), name)
        .expect("symbol table is live")
        .unwrap_or_else(|| panic!("{name} is declared"));
    let ty = unit.symbols().symbol(symbol).expect("declared symbol").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .unwrap_or_else(|error| panic!("layout of {name}: {error:?}"));
    (layout.size_bytes, layout.align_bytes)
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    let error = parse(source, &arena).expect_err("fixture must be rejected");
    assert!(
        error.starts_with(&format!("{code}:")),
        "expected {code}, got {error}"
    );
}

#[test]
fn gnu_alignof_accepts_types_and_unevaluated_expressions() {
    assert_eq!(
        run("int main(void) { int i = 0; unsigned long a = __alignof__(i++); return i * 10 + a; }\n")
            .unwrap(),
        4
    );
    assert_eq!(object_layout("char a[__alignof__(long)];\n", "a"), (8, 1));
    assert_eq!(
        run("int main(void) { int a[3]; return __alignof__(a); }\n").unwrap(),
        4
    );
}

#[test]
fn builtin_expect_distinguishes_runtime_evaluation_from_source_constant_projection() {
    assert_eq!(
        run(concat!(
            "long main(void) { int first = 3; int second = 9; ",
            "long value = __builtin_expect(first++, second++); ",
            "return value * 100 + first * 10 + second; }\n"
        ))
        .unwrap(),
        350
    );
    assert_eq!(
        object_layout(
            "int expected; char a[__builtin_expect(4, expected)];\n",
            "a"
        ),
        (4, 1)
    );
    assert_eq!(
        object_layout(
            "char a[sizeof(__builtin_expect(1, 0)) == sizeof(long) ? 1 : -1];\n",
            "a"
        ),
        (1, 1)
    );
}

#[test]
fn builtin_expect_constant_projection_still_checks_the_unevaluated_operand() {
    let source = concat!(
        "int side; volatile int input; int call(void); ",
        "_Static_assert(__builtin_expect(1, side++), \"increment is unevaluated\"); ",
        "enum { VALUE = __builtin_expect(4, input) }; ",
        "static long initial = __builtin_expect(1, call()); ",
        "char bound[__builtin_expect(VALUE, side++)];"
    );
    assert_eq!(object_layout(source, "bound"), (4, 1));
    rejects(
        "long f(void){return __builtin_expect(1, missing);}",
        "E2001",
    );
    rejects(
        "long f(void){struct S { int a; } s; return __builtin_expect(1, s);}",
        "E2913",
    );
    rejects(
        "void f(void){const long k=1; _Static_assert(__builtin_expect(k,0), \"not source ICE\");}",
        "E2912",
    );
}

#[test]
fn builtin_constant_p_uses_proved_local_values_without_evaluation() {
    assert_eq!(
        run(concat!(
            "int global_value; enum { ENUM_VALUE = 3 }; ",
            "int probe(int parameter) { int local = 2; const int constant_local = 7; int side = 0; ",
            "return (__builtin_constant_p(7) << 0) | ",
            "(__builtin_constant_p(ENUM_VALUE) << 1) | ",
            "(__builtin_constant_p(sizeof(int)) << 2) | ",
            "(__builtin_constant_p(\"x\") << 3) | ",
            "(__builtin_constant_p(&global_value) << 4) | ",
            "(__builtin_constant_p(local) << 5) | ",
            "(__builtin_constant_p(parameter) << 6) | ",
            "(__builtin_constant_p(global_value) << 7) | ",
            "(__builtin_constant_p(constant_local) << 8) | ",
            "(__builtin_constant_p(side++) << 9) | (side << 10); } ",
            "int main(void) { return probe(5); }\n"
        ))
        .unwrap(),
        // Literals and immutable local initializers are known; the externally
        // callable parameter, global object, address and update remain unknown.
        303
    );
}

#[test]
fn builtin_constant_p_is_an_integer_constant_expression_in_all_frozen_contexts() {
    let source = concat!(
        "enum { YES = __builtin_constant_p(1), NO = __builtin_constant_p(*(int *)0) }; ",
        "char bound[YES + 1]; static int initial = __builtin_constant_p(sizeof(long)); ",
        "_Static_assert(__builtin_constant_p(3), \"literal is constant\");\n"
    );
    assert_eq!(object_layout(source, "bound"), (2, 1));
    let arena = Arena::default();
    let unit = parse(source, &arena).expect("all constant-expression contexts parse");
    let initial = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), "initial")
        .expect("symbol table is live")
        .expect("static initializer is declared");
    assert!(unit.symbols().symbol(initial).is_ok());
}

#[test]
fn exact_kernel_builtin_lines_parse_and_leave_source_fidelity_records() {
    let source = concat!(
        "typedef _Bool bool; ",
        "static inline __attribute__((__gnu_inline__)) __attribute__((__unused__)) ",
        "__attribute__((__no_instrument_function__)) bool __attribute__((__warn_unused_result__)) ",
        "__must_check_overflow(bool overflow) { return __builtin_expect(!!(overflow), 0); } ",
        "bool cpucap_is_possible(const unsigned int cap) { ",
        "return __builtin_constant_p(cap) ? cap < 119 : 0; }\n"
    );
    let arena = Arena::default();
    let unit = parse(source, &arena).expect("kernel builtin lines parse");
    let output = render_unit_summary(&unit);
    assert!(output.contains("builtin-expect"), "{output}");
    assert!(output.contains("builtin-constant-p"), "{output}");
}

#[test]
fn builtin_wrong_arity_has_one_diagnostic_authority() {
    rejects("int f(void) { return __builtin_expect(1); }\n", "E2909");
    rejects(
        "int f(void) { return __builtin_constant_p(1, 2); }\n",
        "E2909",
    );
    rejects("int f(void) { return __alignof__(); }\n", "E2909");
}

#[test]
fn builtin_names_are_not_ordinary_values() {
    rejects("long f(void) { return __builtin_expect; }\n", "E2910");
    rejects("int f(void) { return __builtin_constant_p; }\n", "E2910");
    rejects("unsigned long f(void) { return __alignof__; }\n", "E2910");
}
