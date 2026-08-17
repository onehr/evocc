use evocc_chir::{CHirDecl, CHirExpr, TagMembers};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{TagDefinition, TagState};

fn tokens(source: &str) -> Vec<evocc_lex::Token> {
    let mut lexer = Lexer::new(FileId::new(55), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    tokens
}

fn first_diagnostic(source: &str) -> ParseDiagnostic {
    let arena = Arena::default();
    let mut parser = Parser::new(tokens(source), FileId::new(55), source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error(), "expected parser error for {source:?}");
    parser
        .diagnostic()
        .cloned()
        .expect("an erroneous parse must carry a diagnostic")
}

fn assert_diagnostic(source: &str, code: &'static str, message: &str, highlighted_source: &str) {
    let diagnostic = first_diagnostic(source);
    let byte_start = source
        .find(highlighted_source)
        .unwrap_or_else(|| panic!("missing highlight {highlighted_source:?}"));
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.message, message);
    assert_eq!(
        diagnostic.span,
        SourceSpan::new(
            FileId::new(55),
            byte_start as u32,
            (byte_start + highlighted_source.len()) as u32,
            1,
            byte_start as u32 + 1,
        )
    );
}

#[test]
fn bit_field_constraint_failures_have_stable_codes_messages_and_spans() {
    let cases = [
        (
            "struct S { unsigned int x : 33; };",
            "E1301",
            "bit-field-width-exceeds-type: 33 > 32",
            "33",
        ),
        (
            "struct S { unsigned int x : -1; };",
            "E1302",
            "bit-field-width-is-negative: -1",
            "-1",
        ),
        (
            "struct S { unsigned int x : limit; };",
            "E1303",
            "bit-field-width-is-not-an-integer-constant",
            "limit",
        ),
        (
            "struct S { unsigned int x : 1.5; };",
            "E1303",
            "bit-field-width-is-not-an-integer-constant",
            "1.5",
        ),
        (
            "struct S { unsigned int x : 0; };",
            "E1304",
            "named-bit-field-has-zero-width",
            "0",
        ),
        (
            "struct S { float x : 1; };",
            "E1305",
            "bit-field-has-non-integer-type",
            "float",
        ),
        (
            "enum { A = 1U }; struct S { unsigned int x : ((A - 2) % 32); };",
            "E1302",
            "bit-field-width-is-negative: -1",
            "((A - 2) % 32)",
        ),
        (
            "struct S { unsigned int x : (int)1e100; };",
            "E1303",
            "bit-field-width-is-not-an-integer-constant",
            "(int)1e100",
        ),
        (
            concat!(
                "enum Wide { BIG = 4294967296 }; ",
                "struct S { unsigned int x : ((enum Wide)-1 < 0); };"
            ),
            "E1304",
            "named-bit-field-has-zero-width",
            "((enum Wide)-1 < 0)",
        ),
    ];

    for (source, code, message, highlight) in cases {
        assert_diagnostic(source, code, message, highlight);
    }
}

#[test]
fn legal_bit_fields_preserve_written_chir_and_canonical_widths() {
    let source = concat!(
        "struct Bits { ",
        "unsigned int named : 1u + 2; ",
        "unsigned int : 5; ",
        "unsigned int : 0; ",
        "};"
    );
    let arena = Arena::default();
    let mut parser = Parser::new(tokens(source), FileId::new(55), source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());

    let CHirDecl::Tag(tag) = unit
        .declarations()
        .first()
        .expect("record definition declaration")
    else {
        panic!("expected record definition")
    };
    let TagMembers::Struct(written_fields) = &tag.members else {
        panic!("expected written struct members")
    };
    assert_eq!(written_fields.len(), 3);
    assert!(matches!(
        written_fields[0].bit_width,
        Some(CHirExpr::Binary(..))
    ));
    assert!(written_fields[1].name.is_none());
    assert!(written_fields[2].name.is_none());

    let tag_state = &unit.types().tag(tag.tag).expect("canonical tag").state;
    let TagState::Complete(TagDefinition::Struct(canonical_fields)) = tag_state else {
        panic!("expected complete canonical struct")
    };
    assert_eq!(canonical_fields.len(), 3);
    let widths: Vec<_> = canonical_fields
        .iter()
        .map(|field| {
            field
                .bit_field
                .expect("every written bit-field has canonical metadata")
                .width_bits
        })
        .collect();
    assert_eq!(widths, [3, 5, 0]);
    assert_eq!(canonical_fields[0].name.as_deref(), Some("named"));
    assert_eq!(canonical_fields[1].name, None);
    assert_eq!(canonical_fields[2].name, None);
}

#[test]
fn short_circuit_and_conditional_constant_expressions_do_not_evaluate_dead_arms() {
    let source = concat!(
        "struct Constants { ",
        "unsigned int : 0 && (1 / 0); ",
        "unsigned int b : 1 ? 7 : limit; ",
        "};"
    );
    let arena = Arena::default();
    let mut parser = Parser::new(tokens(source), FileId::new(55), source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());

    let CHirDecl::Tag(tag) = unit.declarations()[0] else {
        panic!("expected tag")
    };
    let TagState::Complete(TagDefinition::Struct(fields)) =
        &unit.types().tag(tag.tag).expect("canonical tag").state
    else {
        panic!("expected complete struct")
    };
    assert_eq!(
        fields
            .iter()
            .map(|field| field.bit_field.expect("bit-field").width_bits)
            .collect::<Vec<_>>(),
        [0, 7]
    );
}

#[test]
fn conditional_ice_applies_usual_arithmetic_conversions_without_evaluating_dead_arms() {
    let cases = [
        "struct S { unsigned int x : ((1 ? -1 : 0U) % 32); };",
        "struct S { unsigned int x : ((0 ? 0U : -1) % 32); };",
        "struct S { unsigned int x : ((1 ? -1 : (1 / 0U)) % 32); };",
        "struct S { unsigned int x : ((0 ? (1 / 0U) : -1) % 32); };",
    ];

    for source in cases {
        let arena = Arena::default();
        let mut parser = Parser::new(tokens(source), FileId::new(55), source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source:?}: {:?}", parser.diagnostic());
        let CHirDecl::Tag(tag) = unit.declarations()[0] else {
            panic!("expected struct definition")
        };
        let TagState::Complete(TagDefinition::Struct(fields)) =
            &unit.types().tag(tag.tag).expect("canonical tag").state
        else {
            panic!("expected complete struct")
        };
        assert_eq!(
            fields[0].bit_field.expect("bit-field").width_bits,
            31,
            "{source:?}"
        );
    }
}

#[test]
fn enum_bit_fields_use_the_target_selected_underlying_mode() {
    let source = concat!(
        "enum Wide { BIG = 4294967296 }; ",
        "struct Bits { enum Wide value : 40; };"
    );
    let arena = Arena::default();
    let mut parser = Parser::new(tokens(source), FileId::new(55), source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());

    let CHirDecl::Tag(bits) = unit.declarations()[1] else {
        panic!("expected bit-field record")
    };
    let TagState::Complete(TagDefinition::Struct(fields)) =
        &unit.types().tag(bits.tag).expect("canonical tag").state
    else {
        panic!("expected complete bit-field record")
    };
    assert_eq!(fields[0].bit_field.expect("bit-field").width_bits, 40);
}

#[test]
fn bit_field_ice_uses_enum_layout_sizeof_and_unsigned_wraparound() {
    let cases = [
        ("enum { W = 3 }; struct S { unsigned int x : W; };", 3),
        (
            "enum { BASE = 2, W = BASE + 1 }; struct S { unsigned int x : W; };",
            3,
        ),
        ("struct S { unsigned int x : sizeof(int) * 8; };", 32),
        ("struct S { unsigned int x : (0xffffffffU + 2U); };", 1),
        ("struct S { unsigned int x : (0xffffffff + 2); };", 1),
        ("struct S { unsigned int x : (unsigned char)257; };", 1),
        ("struct S { unsigned int x : ((long)-1 < 1U); };", 1),
        ("struct S { unsigned int x : _Alignof(long); };", 8),
        ("struct S { unsigned int x : sizeof(int[3]); };", 12),
        ("struct S { unsigned int x : sizeof(int *); };", 8),
        ("struct S { unsigned int x : _Alignof(int[3]); };", 4),
        ("struct S { unsigned int x : 'c' - 'a'; };", 2),
        ("struct S { unsigned int x : '\\n' - '\\t'; };", 1),
        ("struct S { unsigned int x : '\\x43' - '\\101'; };", 2),
        ("struct S { unsigned int x : sizeof 1 * 8; };", 32),
        ("struct S { unsigned long x : sizeof((long)1) * 8; };", 64),
        ("struct S { unsigned int x : (int)1.5; };", 1),
        (
            concat!(
                "enum Wide { LOW = -4294967297, BIG = 4294967296 }; ",
                "struct S { unsigned int x : ((enum Wide)-1 < 0); };"
            ),
            1,
        ),
        (
            concat!(
                "enum Wide { BIG = 4294967296 }; ",
                "struct S { unsigned long x : sizeof(enum Wide) * 8; };"
            ),
            64,
        ),
    ];

    for (source, expected_width) in cases {
        let arena = Arena::default();
        let mut parser = Parser::new(tokens(source), FileId::new(55), source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source:?}: {:?}", parser.diagnostic());
        let fields = unit
            .declarations()
            .iter()
            .rev()
            .find_map(|declaration| {
                let CHirDecl::Tag(tag) = declaration else {
                    return None;
                };
                let TagState::Complete(TagDefinition::Struct(fields)) =
                    &unit.types().tag(tag.tag)?.state
                else {
                    return None;
                };
                Some(fields)
            })
            .expect("struct definition");
        assert_eq!(
            fields[0].bit_field.expect("bit-field").width_bits,
            expected_width,
            "{source:?}"
        );
    }
}
