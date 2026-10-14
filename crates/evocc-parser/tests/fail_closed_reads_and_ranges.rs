//! T03j (audit findings A06 and A07): two front-end refusals of EvoIR Section
//! 10.1 that the parser did not make. A read of an `_Atomic` object is
//! `E2702` at the read, as an update already is, so that no read becomes a
//! plain load. The initializer of a range designator must be a constant,
//! because one written expression serves every element; a nonconstant one is
//! `E2917` at the expression.

use evocc_common::{Arena, FileId, SourceFile, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn first_diagnostic(source: &SourceFile) -> Option<ParseDiagnostic> {
    let arena = Arena::default();
    let mut lexer = Lexer::new(source.id, &source.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected test input");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, source.id, &source.contents);
    let _ = parser.parse_translation_unit_in(&arena);
    parser.diagnostic().cloned().filter(|_| parser.had_error())
}

fn file(text: &str) -> SourceFile {
    SourceFile::new(FileId::new(10_317), "/virtual/t03j.c".into(), text.into())
}

/// The span of the `occurrence`th `spelling` in `source`.
fn span_of(source: &SourceFile, spelling: &str, occurrence: usize) -> SourceSpan {
    let (start, _) = source
        .contents
        .match_indices(spelling)
        .nth(occurrence)
        .expect("spelling occurrence exists");
    let (line, column) = source.line_col(start);
    SourceSpan::new(
        source.id,
        start as u32,
        (start + spelling.len()) as u32,
        line,
        column,
    )
}

fn refused_at(text: &str, code: &str, spelling: &str, occurrence: usize) {
    let source = file(text);
    let diagnostic =
        first_diagnostic(&source).unwrap_or_else(|| panic!("{code} expected, but parsed: {text}"));
    assert_eq!(diagnostic.code, code, "{text}: {diagnostic:?}");
    let expected = span_of(&source, spelling, occurrence);
    assert_eq!(
        (diagnostic.span.byte_start, diagnostic.span.line_start),
        (expected.byte_start, expected.line_start),
        "{text}: {diagnostic:?}"
    );
}

fn accepted(text: &str) {
    let source = file(text);
    if let Some(diagnostic) = first_diagnostic(&source) {
        panic!("must parse, got {diagnostic:?}\n  source: {text}");
    }
}

#[test]
fn a_read_of_an_atomic_object_is_refused_at_the_read() {
    // The probe of the audit: before, the read became `ldr` with no order.
    refused_at(
        "_Atomic int g; int ld(void) { return g; }\n",
        "E2702",
        "g",
        1,
    );
    refused_at(
        "int ld(_Atomic int *p) { return *p + 1; }\n",
        "E2702",
        "*p",
        1,
    );
    refused_at(
        "struct s { _Atomic int a; int b; }; int ld(struct s *q) { return q->a; }\n",
        "E2702",
        "q->a",
        0,
    );
    refused_at(
        "_Atomic int table[4]; int ld(void) { return table[1]; }\n",
        "E2702",
        "table[1]",
        0,
    );
    refused_at(
        "int f(void) { _Atomic long x = 0; long y = x; return (int)y; }\n",
        "E2702",
        "x;",
        0,
    );
    refused_at(
        "_Atomic long g; void f(void) { (void)g; }\n",
        "E2702",
        "g;",
        1,
    );
    // An update stays refused at its target.
    refused_at("_Atomic int g; void f(void) { g = 3; }\n", "E2702", "g", 1);
}

#[test]
fn an_atomic_object_that_is_not_read_stays_accepted() {
    accepted("_Atomic int g; unsigned long f(void) { return sizeof g + sizeof(g + 1); }\n");
    accepted("_Atomic int g; _Atomic int *f(void) { return &g; }\n");
    accepted("_Atomic int g = 5; int f(void) { return 0; }\n");
}

#[test]
fn a_nonconstant_range_initializer_is_refused_at_the_expression() {
    // The probe of the audit: before, `f` ran once per element and the
    // program exited 33.
    refused_at(
        "int n; int f(void) { return ++n; }\nint g(void) { int b[4] = {[0 ... 2] = f()}; return n * 10 + b[2]; }\n",
        "E2917",
        "f()",
        0,
    );
    refused_at(
        "int g(int x) { int b[4] = {[1 ... 3] = x}; return b[2]; }\n",
        "E2917",
        "x}",
        0,
    );
    refused_at(
        "struct p { int a, b; }; int g(int x) { struct p b[3] = {[0 ... 2] = {1, x}}; return b[1].b; }\n",
        "E2917",
        "x}",
        0,
    );
}

#[test]
fn a_constant_range_initializer_stays_accepted() {
    accepted("int g(void) { int b[4] = {[0 ... 2] = 7}; return b[2]; }\n");
    accepted("int b[] = {[2 ... 4] = 3 * 5, 9};\n");
    accepted("double d[3] = {[0 ... 2] = 1.5};\n");
    accepted("int x; int *p[3] = {[0 ... 2] = &x};\n");
    accepted("int g(void) { int b[4] = {[0] = 1, [1 ... 3] = sizeof(long)}; return b[3]; }\n");
}
