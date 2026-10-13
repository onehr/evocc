//! `__has_builtin` answers from the preprocessor's table, which does not
//! depend on the parser. The table must list exactly the builtin spellings
//! that the front end resolves, so a header never selects a builtin that the
//! compiler lacks and never misses one that it has.

use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const TABLE: &str = include_str!("../../evocc-cpp/predefined/has-builtin.txt");

fn has_builtin(name: &str) -> bool {
    let source = format!("#if __has_builtin({name})\nyes\n#else\nno\n#endif\n");
    let mut lexer = Lexer::new(FileId::new(1), &source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut preprocessor = evocc_cpp::Preprocessor::new();
    let output = preprocessor
        .process(evocc_cpp::TokenStream::new(tokens), &source)
        .into_vec();
    assert!(preprocessor.errors().is_empty(), "{name}");
    let words: Vec<_> = output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| &source[token.span.byte_start as usize..token.span.byte_end as usize])
        .collect();
    match words.as_slice() {
        ["yes"] => true,
        ["no"] => false,
        other => panic!("{name}: {other:?}"),
    }
}

fn lex(source: &str) -> Vec<evocc_lex::Token> {
    let mut lexer = Lexer::new(FileId::new(1), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    tokens
}

#[test]
fn the_has_builtin_table_lists_exactly_the_resolved_builtins() {
    let names = evocc_parser::builtins::names();
    let expected: String = names.iter().map(|name| format!("{name}\n")).collect();
    assert!(
        TABLE == expected,
        "crates/evocc-cpp/predefined/has-builtin.txt must be exactly:\n{expected}"
    );
    for name in &names {
        assert!(has_builtin(name), "{name}");
    }
    for name in [
        "__builtin_alloca_uninitialized",
        "__builtin___memccpy_chk",
        "__builtin___strlcpy_chk",
        "__builtin___printf_chk",
        "__builtin_aarch64_absdi",
        "__builtin_aarch64_ld2v16qi",
        "__builtin_puts",
        "__builtin_exit",
        "__builtin__Exit",
        "memcpy",
    ] {
        assert!(!has_builtin(name), "{name}");
    }
}

#[test]
fn each_listed_builtin_is_resolved_by_the_front_end() {
    // A listed spelling never falls back to an implicit declaration, which
    // would be E2949 for a `__builtin_` name or W0003 otherwise.
    for name in evocc_parser::builtins::names() {
        if name == "__builtin_va_list" {
            continue;
        }
        let source = format!("int probe(void) {{ return sizeof {name}; }}\n");
        let arena = Arena::default();
        let mut parser = Parser::new(lex(&source), FileId::new(1), &source);
        parser.enable_lints();
        parser.parse_translation_unit_in(&arena);
        assert!(
            parser
                .diagnostics()
                .iter()
                .all(|diagnostic| diagnostic.code != "E2949" && diagnostic.code != "E2001"),
            "{name}: {:?}",
            parser.diagnostics()
        );
        // Lint 2 is the implicit function declaration.
        assert!(
            parser
                .lint_findings()
                .iter()
                .all(|finding| finding.lint != 2),
            "{name}"
        );
    }
}
