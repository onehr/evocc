//! Front-end preservation and fail-closed execution for GNU computed goto.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::TargetDataLayout;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_305);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn parse_valid<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    parse(source, arena).unwrap_or_else(|diagnostic| {
        panic!(
            "fixture must parse, got {}: {}\nsource: {source}",
            diagnostic.code, diagnostic.message
        )
    })
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture defines {name}"))
}

fn assert_execution_and_lowering_refuse(source: &str) {
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let interpreter = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(100_000),
    )
    .expect_err("C-HIR execution must fail closed");
    assert_eq!(interpreter.code(), "E3029");

    let lowering = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect_err("computed goto must not reach EvoIR");
    assert_eq!(lowering.code(), "E3029");
    assert_eq!(lowering.to_string(), "not-lowered: computed-goto");
}

#[test]
fn label_address_is_preserved_but_execution_and_lowering_fail_closed() {
    assert_execution_and_lowering_refuse(concat!(
        "int main(void) { void *target = &&done; ",
        "done: return target != (void *)0; }\n",
    ));
}

#[test]
fn computed_goto_is_preserved_but_execution_and_lowering_fail_closed() {
    assert_execution_and_lowering_refuse(concat!(
        "int main(void) { void *target = &&done; goto *target; ",
        "done: return 0; }\n",
    ));
}

#[test]
fn cohort_label_address_form_fails_closed_without_an_evoir_form() {
    assert_execution_and_lowering_refuse(concat!(
        "unsigned long main(void) { return ({ __label__ __here; ",
        "__here: (unsigned long)&&__here; }); }\n",
    ));
}

#[test]
fn static_label_address_initializer_keeps_the_typed_diagnostic() {
    let arena = Arena::default();
    let diagnostic = parse(
        "int main(void) { static void *target = &&done; done: return target != 0; }\n",
        &arena,
    )
    .expect_err("a label address is not a static initializer");
    assert_eq!(diagnostic.code, "E2208");
}

#[test]
fn undeclared_ordinary_goto_keeps_the_typed_diagnostic() {
    let arena = Arena::default();
    let diagnostic = parse("int main(void) { goto nowhere; }\n", &arena)
        .expect_err("an undeclared label must be rejected");
    assert_eq!(diagnostic.code, "E2920");
}
