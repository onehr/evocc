use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_205);
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
            "static address constant must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

#[test]
fn string_literal_address_has_pointer_to_complete_array_type() {
    for source in [
        "_Static_assert(sizeof(*(&\"abc\"))==4,\"array extent\");",
        "_Static_assert(__builtin_types_compatible_p(typeof(&\"abc\"), char (*)[4]),\"array pointer\");",
        "_Static_assert(sizeof(*(&u\"xy\"))==6,\"wide array\");",
        "static char (*end)[4]=&\"abc\"+1;",
    ] {
        accepts(source);
    }
}

#[test]
fn dereferencing_a_literal_address_does_not_make_the_whole_array_assignable() {
    let arena = Arena::default();
    let error = parse("void f(void){*(&\"abc\")=1;}", &arena).unwrap_err();
    assert_eq!(error.code, "E2101");
}

#[test]
fn integer_constant_cast_to_pointer_is_an_address_constant() {
    accepts(concat!(
        "struct entry { void *next; }; ",
        "static struct entry value = { .next = (void *)2 };\n",
    ));
}

#[test]
fn static_array_member_decay_is_an_address_constant() {
    accepts(concat!(
        "struct storage { int values[4]; }; static struct storage object; ",
        "static int *pointer = object.values;\n",
    ));
}

#[test]
fn static_multidimensional_element_decay_is_an_address_constant() {
    accepts("static int matrix[2][3]; static int *row = matrix[1];\n");
}

#[test]
fn nested_static_array_decay_is_valid_inside_an_aggregate() {
    accepts(concat!(
        "struct storage { unsigned long bits[2]; }; static struct storage object; ",
        "struct descriptor { unsigned long *bits; }; ",
        "static struct descriptor descriptor = { .bits = object.bits };\n",
    ));
}

const FUNCTION_ADDRESS_PREFIX: &str = concat!(
    "typedef int (*fn_t)(int); int impl(int value) { return value; } ",
    "struct table_entry { const void *data; }; ",
);

#[test]
fn pointer_comparison_condition_preserves_the_selected_address_constant() {
    accepts(&format!(
        "{FUNCTION_ADDRESS_PREFIX} static const struct table_entry entry = {{ .data = (impl == (fn_t)0) ? impl : impl }};\n"
    ));
}

#[test]
fn integer_condition_preserves_the_selected_address_constant() {
    accepts(&format!(
        "{FUNCTION_ADDRESS_PREFIX} static const struct table_entry entry = {{ .data = 1 ? impl : (void *)0 }};\n"
    ));
}

#[test]
fn nonnull_function_address_condition_selects_an_address_constant() {
    accepts(&format!(
        "{FUNCTION_ADDRESS_PREFIX} static const struct table_entry entry = {{ .data = impl ? impl : 0 }};\n"
    ));
}
