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

#[test]
fn qualified_void_pointers_convert_at_initialization_sites() {
    accepts(&format!(
        "{PREFIX} int probe(const void *constant, volatile void *changing, const volatile void *both) {{ fn_t a = constant; fn_t b = changing; fn_t c = both; return a != 0 && b != 0 && c != 0; }}\n"
    ));
}

#[test]
fn qualified_void_pointers_convert_at_assignment_sites() {
    accepts(&format!(
        "{PREFIX} int probe(const void *constant, volatile void *changing, const volatile void *both) {{ fn_t callback; callback = constant; callback = changing; callback = both; constant = callback; changing = callback; both = callback; return callback != 0; }}\n"
    ));
}

#[test]
fn qualified_void_pointers_convert_at_argument_sites() {
    accepts(&format!(
        "{PREFIX} void take_callback(fn_t); void take_erased(const volatile void *); void pass(const volatile void *value) {{ take_callback(value); take_erased(impl); }}\n"
    ));
}

#[test]
fn qualified_void_pointers_convert_at_return_sites() {
    accepts(&format!(
        "{PREFIX} fn_t restore(const volatile void *value) {{ return value; }} const void *erase(fn_t callback) {{ return callback; }}\n"
    ));
}

#[test]
fn qualified_void_pointers_convert_at_cast_sites() {
    accepts(&format!(
        "{PREFIX} fn_t restore(const volatile void *value) {{ return (fn_t)value; }} const void *erase(fn_t callback) {{ return (const void *)callback; }}\n"
    ));
}

#[test]
fn qualified_void_pointers_convert_in_conditional_arms() {
    accepts(&format!(
        "{PREFIX} fn_t choose(int condition, const volatile void *value) {{ fn_t left = condition ? value : impl; fn_t right = condition ? impl : value; return condition ? left : right; }}\n"
    ));
}
