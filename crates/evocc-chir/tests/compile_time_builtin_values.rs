//! Interpreter semantics for compile-time builtins and small GNU forms.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_118);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser rejected {source:?}: {:?}",
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

fn evaluate(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let outcome = interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(200_000))
        .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a scalar")
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
}

#[test]
fn bit_builtins_match_the_frozen_oracle_values_at_runtime() {
    let cases = [
        ("int x = 8; return __builtin_ffs(x);", 4),
        ("long long x = 0; return __builtin_ffsll(x);", 0),
        ("unsigned x = 1; return __builtin_clz(x);", 31),
        ("unsigned long long x = 1; return __builtin_clzll(x);", 63),
        ("unsigned x = 8; return __builtin_ctz(x);", 3),
        ("unsigned x = 7; return __builtin_popcount(x);", 3),
        (
            "unsigned long long x = ~0ull; return __builtin_popcountll(x);",
            64,
        ),
    ];
    for (body, expected) in cases {
        assert_eq!(
            evaluate(&format!("int main(void) {{ {body} }}\n")).unwrap(),
            expected,
            "{body}"
        );
    }
}

#[test]
fn zero_clz_or_ctz_reports_the_typed_runtime_diagnostic() {
    for name in ["__builtin_clz", "__builtin_ctz"] {
        let error = evaluate(&format!(
            "int main(void) {{ unsigned value = 0; return {name}(value); }}\n"
        ))
        .expect_err("zero operand must fail");
        assert_eq!(error, "E3022: bit-builtin-zero-operand");
    }
}

#[test]
fn choose_expr_preserves_an_lvalue_and_never_evaluates_the_other_arm() {
    assert_eq!(
        evaluate(concat!(
            "int main(void) { int selected = 1; int other = 0; ",
            "__builtin_choose_expr(1, selected, other++) = 7; ",
            "return selected * 10 + other; }\n"
        ))
        .unwrap(),
        70
    );
}

#[test]
fn elvis_evaluates_its_left_operand_exactly_once() {
    assert_eq!(
        evaluate("int main(void) { int value = 6; int result = value++ ?: 9; return result * 10 + value; }\n")
            .unwrap(),
        67
    );
}

#[test]
fn case_range_dispatches_every_value_in_the_inclusive_interval() {
    for value in [1, 2, 3] {
        assert_eq!(
            evaluate(&format!(
                "int main(void) {{ int value = {value}; switch (value) {{ case 1 ... 3: return 7; default: return 0; }} }}\n"
            ))
            .unwrap(),
            7
        );
    }
    assert_eq!(
        evaluate("int main(void) { int value = -1; switch (value) { case -1 ... 1: return 9; default: return 0; } }\n")
            .unwrap(),
        9
    );
}

#[test]
fn reaching_builtin_unreachable_reports_the_typed_runtime_diagnostic() {
    let error = evaluate("int main(void) { __builtin_unreachable(); return 0; }\n")
        .expect_err("reaching unreachable must fail");
    assert_eq!(error, "E3024: unreachable-reached");
}
