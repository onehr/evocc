//! Checked-arithmetic builtin contract (specification 10.1.15).

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_601),
        "checked-arithmetic.c".to_owned(),
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
fn checked_arithmetic_calls_parse_and_have_type_bool() {
    accepts("int f(int a, int b) { int r; return __builtin_mul_overflow(a, b, &r); }\n");
    accepts(
        "int f(long a, unsigned b) { unsigned char c; return __builtin_add_overflow(a, b, &c); }\n",
    );
    accepts("int f(unsigned a, int b) { long long r; return __builtin_sub_overflow(a, b, &r); }\n");
    accepts(
        "int f(int a) { int r; return _Generic(__builtin_mul_overflow(a, a, &r), _Bool: 1); }\n",
    );
}

#[test]
fn checked_arithmetic_rejects_bad_operands() {
    rejects(
        "int f(int a) { int r; return __builtin_mul_overflow(a, &r); }\n",
        "E2909",
    );
    rejects(
        "int f(double a) { int r; return __builtin_mul_overflow(a, a, &r); }\n",
        "E2913",
    );
    rejects(
        "int f(int a) { _Bool r; return __builtin_mul_overflow(a, a, &r); }\n",
        "E2913",
    );
    rejects(
        "int f(int a) { int r; return __builtin_mul_overflow(a, a, r); }\n",
        "E2913",
    );
    rejects(
        "int r; _Static_assert(__builtin_mul_overflow(70000, 70000, &r) == 1, \"x\");\n",
        "E2912",
    );
}

#[test]
fn the_kernel_overflow_line_parses() {
    accepts(
        "int f(unsigned long factor1, unsigned long factor2) { unsigned long bytes; \
         if (__builtin_mul_overflow(factor1, factor2, &bytes)) return 1; return (int)bytes; }\n",
    );
}

#[test]
fn checked_arithmetic_is_visible_in_the_chir_summary() {
    let arena = Arena::default();
    let unit = parse(
        "int f(int a, int b) { int r; return __builtin_mul_overflow(a, b, &r); }\n",
        &arena,
    )
    .expect("fixture must parse");
    let summary = evocc_chir::render_unit_summary(&unit);
    assert!(
        summary.contains("checked mul("),
        "checked operation missing from:\n{summary}"
    );
    assert!(
        summary.contains(" -> *"),
        "checked destination missing from:\n{summary}"
    );
}
