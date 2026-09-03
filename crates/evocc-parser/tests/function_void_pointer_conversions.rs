use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_203);
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

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!(
            "function and void pointer conversion must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

const PREFIX: &str = "typedef int (*fn_t)(int); int impl(int x) { return x; } ";

#[test]
fn initialization_converts_both_pointer_directions() {
    accepts(&format!(
        "{PREFIX} void *erased = impl; fn_t restored = (void *)0;\n"
    ));
}

#[test]
fn assignment_argument_and_return_share_the_conversion_rule() {
    accepts(&format!(
        "{PREFIX} void take(void *); fn_t restore(void *value) {{ fn_t callback; callback = value; take(callback); return value; }}\n"
    ));
}

#[test]
fn casts_and_conditional_arms_preserve_function_pointer_type() {
    accepts(&format!(
        "{PREFIX} fn_t choose(int condition, void *value) {{ fn_t casted = (fn_t)value; void *erased = (void *)impl; return condition ? casted : (void *)0; }}\n"
    ));
}

#[test]
fn arithmetic_void_pointer_result_converts_to_function_pointer() {
    accepts(&format!(
        "{PREFIX} fn_t adjust(void *value) {{ fn_t callback = value + 4; return callback; }}\n"
    ));
}
