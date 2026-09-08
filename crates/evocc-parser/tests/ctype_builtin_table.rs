use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = FileId::new(10_207);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
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

fn evaluate(name: &str, input: &str) -> Result<i128, String> {
    let source = format!("int main(void) {{ int value = {input}; return {name}(value); }}\n");
    let arena = Arena::default();
    let unit = parse(&source, &arena)?;
    let outcome = interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(100_000))
        .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
}

#[test]
fn digit_builtin_matches_the_sealed_byte_table_and_integer_boundaries() {
    let table = include_str!("../../../tests/fixtures/ctype-oracle-table.txt");
    let digits = table.lines().next().expect("sealed digit table").as_bytes();
    assert_eq!(digits.len(), 256);
    for (input, expected) in digits.iter().enumerate() {
        assert_eq!(
            evaluate("__builtin_isdigit", &input.to_string()).unwrap(),
            i128::from(expected - b'0'),
            "input {input}"
        );
    }
    for input in ["-1", "0x130", "(-2147483647-1)", "2147483647"] {
        assert_eq!(
            evaluate("__builtin_isdigit", input).unwrap(),
            0,
            "input {input}"
        );
    }
}

// The other ctype builtins remain calls, not fabricated external executors.
#[test]
fn ctype_external_execution_requires_a_definition() {
    for name in [
        "__builtin_isalpha",
        "__builtin_isspace",
        "__builtin_toupper",
        "__builtin_tolower",
    ] {
        for input in ["48", "-1", "0x130"] {
            let error = evaluate(name, input)
                .expect_err("undeclared external implementation must not be fabricated");
            assert!(error.starts_with("E3039: "), "{name}: {error}");
        }
    }
}
