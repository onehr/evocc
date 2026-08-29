//! Every diagnostic a translation unit produces, not only the first.
//!
//! The compiler reports one diagnostic per unit so a failure has a single
//! unambiguous first-error site. That is the right answer for a build, and the
//! wrong answer for a survey: enumerating what a body of code needs takes as
//! many rounds as it has distinct unsupported constructs. The parser already
//! recovers past an error; these tests require it to keep what it found.

use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use std::num::NonZeroUsize;

fn lex(source: &str) -> Vec<evocc_lex::Token> {
    let mut lexer = Lexer::new(FileId::new(77), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    tokens
}

fn survey(source: &str) -> Vec<(&'static str, u32)> {
    let arena = Arena::default();
    let mut parser = Parser::new(lex(source), FileId::new(77), source);
    let _ = parser.parse_translation_unit_in(&arena);
    parser
        .diagnostics()
        .iter()
        .map(|d| (d.code, d.span.line_start))
        .collect()
}

/// Three unsupported constructs in one unit are three findings, in source
/// order, so one pass enumerates the work instead of one pass per construct.
#[test]
fn a_survey_reports_every_diagnostic_in_source_order() {
    let source = concat!("int a[-1];\n", "int b[-2];\n", "struct s { int c[-3]; };\n",);
    let found = survey(source);
    assert_eq!(
        &found[..3],
        &[("E2923", 1), ("E2923", 2), ("E2923", 3)][..],
        "{found:?}"
    );
}

/// Recovery is sound at the external-declaration boundary. It is **not** yet
/// sound after a declaration fails part way through a function body: the
/// symbol table rolls back the scope the parser is still standing in, and the
/// next scope it opens panics. Two of the sixteen kernel cohort units hit that
/// path, so a survey reports them as unavailable rather than as clean. This
/// test pins the boundary that does work, so a fix can extend it.
#[test]
fn recovery_is_sound_across_external_declarations() {
    let found = survey("int a[-1];\nint ok = 1;\nint b[-2];\nint fine(void) { return ok; }\n");
    assert_eq!(found, vec![("E2923", 1), ("E2923", 3)], "{found:?}");
}

/// A refused declaration leaves its name undeclared, so later uses report too.
/// The parser records the cascade rather than hiding it; ranking the distinct
/// constructs is the surveying tool's job, not the parser's.
#[test]
fn a_refused_declaration_cascades_into_its_uses() {
    let found = survey("int g(long l) { __auto_type *p = &l; return (int)*p; }\n");
    assert_eq!(found.first(), Some(&("E2915", 1)), "{found:?}");
    assert!(
        found.iter().any(|(code, _)| *code == "E2001"),
        "the refused name is undeclared afterwards: {found:?}"
    );
}

/// The build path is unchanged: one diagnostic, the first one.
#[test]
fn the_first_diagnostic_is_still_the_reported_one() {
    let source = "int a[-1];\nvoid f(void) { goto nowhere; }\n";
    let arena = Arena::default();
    let mut parser = Parser::new(lex(source), FileId::new(77), source);
    let _ = parser.parse_translation_unit_in(&arena);
    let first = parser.diagnostic().expect("a diagnostic");
    assert_eq!((first.code, first.span.line_start), ("E2923", 1));
    assert_eq!(parser.diagnostics().first().map(|d| d.code), Some("E2923"));
}

/// An error limit still stops the parse, and the survey holds exactly what was
/// found before it stopped.
#[test]
fn an_error_limit_bounds_the_survey() {
    let source = "int a[-1];\nvoid f(void) { goto nowhere; }\n";
    let arena = Arena::default();
    let mut parser = Parser::new(lex(source), FileId::new(77), source);
    parser.set_error_limit(NonZeroUsize::MIN);
    let _ = parser.parse_translation_unit_in(&arena);
    assert_eq!(parser.diagnostics().len(), 1);
    assert_eq!(parser.diagnostics()[0].code, "E2923");
}

/// A clean unit surveys empty.
#[test]
fn a_clean_unit_has_no_findings() {
    assert!(survey("int a = 1;\nint f(void) { return a; }\n").is_empty());
}
