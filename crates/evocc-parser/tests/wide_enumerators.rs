use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(10_128),
        "wide-enumerators.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
}

#[test]
fn wide_enumerations_use_the_smallest_representing_type_for_every_constant() {
    let source = concat!(
        "enum big { BIG = 0xffffffff00000000ULL };\n",
        "_Static_assert(sizeof(enum big) == 8, \"wide unsigned layout\");\n",
        "_Static_assert(sizeof(BIG) == 8, \"wide unsigned constant\");\n",
        "_Static_assert(__builtin_types_compatible_p(typeof(BIG), unsigned long), \"wide unsigned type\");\n",
        "enum big2 { SHIFTED = (0xffffffffULL << 32), SMALL = 1 };\n",
        "_Static_assert(sizeof(enum big2) == 8, \"shifted layout\");\n",
        "_Static_assert(__builtin_types_compatible_p(typeof(SHIFTED), unsigned long), \"shifted type\");\n",
        "_Static_assert(__builtin_types_compatible_p(typeof(SMALL), unsigned long), \"completed-list type\");\n",
        "enum negative { NEGATIVE = -1, LARGE = 0x100000000 };\n",
        "_Static_assert(sizeof(enum negative) == 8, \"wide signed layout\");\n",
        "_Static_assert(__builtin_types_compatible_p(typeof(NEGATIVE), long), \"negative type\");\n",
        "_Static_assert(__builtin_types_compatible_p(typeof(LARGE), long), \"large signed type\");\n",
    );
    let arena = Arena::default();
    parse(source, &arena).unwrap_or_else(|error| panic!("must parse: {error}"));
}

#[test]
fn an_enumerator_outside_every_target_integer_type_is_e1303() {
    let arena = Arena::default();
    let error = parse(
        "enum overflow { LAST = 0xffffffffffffffffULL, TOO_MANY };\n",
        &arena,
    )
    .expect_err("implicit successor of ULONG_MAX must be rejected");
    assert_eq!(error, "E1303: enumerator-value-is-not-representable");
}

#[test]
fn signed_constant_arithmetic_wraps_in_the_operand_width() {
    let source = concat!(
        "enum signed_shift { SIGN_BIT = 1 << 31, AFTER_SIGN_BIT = 2 };\n",
        "_Static_assert(sizeof(enum signed_shift) == 4, \"enum layout\");\n",
        "_Static_assert(__builtin_types_compatible_p(typeof(SIGN_BIT), int), \"enum type\");\n",
        "_Static_assert(SIGN_BIT == (-2147483647 - 1), \"shift wraps\");\n",
        "_Static_assert((1 << 31) < 0, \"shift is negative\");\n",
        "_Static_assert((2147483647 + 1) == (-2147483647 - 1), \"add wraps\");\n",
        "_Static_assert((-2147483647 - 2) == 2147483647, \"subtract wraps\");\n",
        "_Static_assert((1073741824 * 4) == 0, \"multiply wraps\");\n",
        "_Static_assert(-(-2147483647 - 1) == (-2147483647 - 1), \"negate wraps\");\n",
    );
    let arena = Arena::default();
    parse(source, &arena).unwrap_or_else(|error| panic!("must parse: {error}"));
}
