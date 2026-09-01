//! Both C-HIR execution paths refuse deferred variable-argument operations.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_122);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "fixture must parse: {:?}\nsource: {source}",
        parser.diagnostic()
    );
    unit
}

fn main_symbol(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.name.spelling == "main" && function.body.is_some() =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main")
}

fn assert_varargs_error(source: &str) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let error =
        interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000))
            .expect_err("varargs execution must fail closed");
    assert_eq!(error.code(), "E3030");
    assert_eq!(error.message(), "not-lowered: varargs");
}

#[test]
fn direct_expression_interpreter_reports_e3030() {
    assert_varargs_error(
        "int main(void) { __builtin_va_list ap; __builtin_va_end(ap); return 0; }\n",
    );
}

#[test]
fn call_capable_interpreter_reports_e3030() {
    assert_varargs_error(concat!(
        "int helper(void) { return 0; } ",
        "int main(void) { __builtin_va_list ap; helper(); ",
        "__builtin_va_end(ap); return 0; }\n",
    ));
}
