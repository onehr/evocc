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

// Section 10.1.3's external execution boundary supersedes the older
// interpreter table requirement; the sealed oracle fixture remains unchanged.
#[test]
fn ctype_external_execution_requires_a_definition() {
    for name in [
        "__builtin_isdigit",
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
