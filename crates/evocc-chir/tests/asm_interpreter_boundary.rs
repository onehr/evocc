//! Interpreter boundary for front-end-complete GNU asm.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 20_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = SourceFile::new(
        FileId::new(9_605),
        "asm-interpreter-boundary.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected fixture");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser rejected fixture: {:?}",
        parser.diagnostic()
    );
    unit
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

#[test]
fn reaching_asm_fails_with_the_frozen_diagnostic() {
    let arena = Arena::default();
    let unit = parse(
        "int main(void) { asm volatile(\"nop\" ::: \"memory\"); return 0; }\n",
        &arena,
    );
    let error = interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
        .expect_err("reaching asm must fail closed");
    assert_eq!(error.code(), "E3021");
    assert_eq!(error.message(), "not-lowered: asm");
}

#[test]
fn an_unreached_asm_does_not_fail_interpretation() {
    let arena = Arena::default();
    let unit = parse(
        "int main(void) { if (0) asm volatile(\"nop\"); return 7; }\n",
        &arena,
    );
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .expect("unreached asm has no runtime effect");
    let value = outcome.return_value.expect("main returns a value");
    assert_eq!(value.mathematical_value(unit.types()).unwrap(), 7);
}
