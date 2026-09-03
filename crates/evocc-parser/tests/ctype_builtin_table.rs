use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const ORACLE_TABLE: &str = include_str!("fixtures/p28-table.out");

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

fn render_subject_table() -> Result<String, String> {
    let mut output = String::new();
    for name in [
        "__builtin_isdigit",
        "__builtin_isalpha",
        "__builtin_isspace",
    ] {
        for input in 0..=255 {
            output.push(if evaluate(name, &input.to_string())? == 0 {
                '0'
            } else {
                '1'
            });
        }
        output.push('\n');
    }
    for name in ["__builtin_toupper", "__builtin_tolower"] {
        for input in 0..=255 {
            let value = evaluate(name, &input.to_string())?;
            output.push_str(&format!("{value:02x}"));
        }
        output.push('\n');
    }
    output.push_str(&format!(
        "{} {}\n",
        evaluate("__builtin_isdigit", "-1")?,
        evaluate("__builtin_isdigit", "0x130")?
    ));
    Ok(output)
}

#[test]
fn ctype_builtins_byte_match_the_sealed_p28_table() {
    let actual = render_subject_table().expect("ctype builtins must execute");
    assert_eq!(actual.as_bytes(), ORACLE_TABLE.as_bytes());
}
