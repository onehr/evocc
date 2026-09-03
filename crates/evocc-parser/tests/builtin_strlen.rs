use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 100_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_145),
        "builtin-strlen.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
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

fn run(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena)?;
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .map_err(|error| format!("value: {error}"))
}

#[test]
fn string_literal_length_is_a_size_t_integer_constant_expression() {
    assert_eq!(
        run(concat!(
            "typedef unsigned long size_t; ",
            "_Static_assert(__builtin_strlen(\"abc\") == 3, \"literal folds\"); ",
            "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_strlen(\"x\")), size_t), \"type\"); ",
            "int main(void) { return (int)__builtin_strlen(\"a\\n\"); }"
        ))
        .unwrap(),
        2
    );
}

#[test]
fn non_literal_length_is_an_ordinary_call_to_visible_strlen() {
    assert_eq!(
        run(concat!(
            "typedef unsigned long size_t; ",
            "size_t strlen(const char *s) { return s[0] ? 1005 : 1000; } ",
            "int main(void) { char text[8] = \"hello\"; return (int)__builtin_strlen(text); }"
        ))
        .unwrap(),
        1005
    );
}

#[test]
fn non_literal_length_uses_the_builtin_prototype_without_a_visible_declaration() {
    let arena = Arena::default();
    parse(
        "typedef unsigned long size_t; size_t f(char *s) { return __builtin_strlen(s); }",
        &arena,
    )
    .expect("the builtin supplies the external call prototype");
}

#[test]
fn non_literal_length_rejects_an_incompatible_visible_declaration() {
    let arena = Arena::default();
    let error = parse(
        "typedef unsigned long size_t; int strlen(int); size_t f(char *s) { return __builtin_strlen(s); }",
        &arena,
    )
    .expect_err("an incompatible visible declaration must fail");
    assert_eq!(error, "E2927: builtin-library-declaration-incompatible");
}
