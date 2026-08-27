//! Empty declarations are accepted wherever the GNU C profile permits them.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(10_301);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected input at {:?}", token.span));
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().filter(|_| parser.had_error()) {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None if parser.had_error() => Err("parser failed without a diagnostic".to_owned()),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\nsource: {source}");
    }
}

#[test]
fn file_scope_empty_declarations_have_no_semantic_nodes() {
    for source in [
        ";\n",
        "int first;; int second;\n",
        "struct S { int value; };; int after;\n",
        "void function(void) { };; int after;\n",
    ] {
        accepts(source);
    }

    let arena = Arena::default();
    let unit = parse("; int object;;\n", &arena).expect("empty declarations parse");
    assert_eq!(
        unit.len(),
        1,
        "empty declarations must not create C-HIR nodes"
    );
}

#[test]
fn empty_record_member_declarations_are_ignored() {
    accepts("struct S { ; int first;; long second; ; };\n");
}

#[test]
fn macro_expanded_inline_function_tail_is_accepted() {
    accepts(concat!(
        "static inline void __cmpwait_case_8(volatile void *ptr, ",
        "unsigned long value) { (void)ptr; (void)value; };\n",
    ));
}
