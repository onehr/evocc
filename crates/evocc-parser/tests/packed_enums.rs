use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_601),
        "packed-enums.c".to_owned(),
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

fn layout_of(source: &str, typedef_name: &str) -> (u64, u32) {
    let arena = Arena::default();
    let unit = parse(source, &arena)
        .unwrap_or_else(|error| panic!("must parse, got {error}\n  source: {source}"));
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), typedef_name)
        .expect("file scope remains live")
        .unwrap_or_else(|| panic!("{typedef_name} is declared"));
    let ty = unit.symbols().symbol(symbol).expect("symbol is live").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .expect("complete enum has a layout");
    (layout.size_bytes, layout.align_bytes)
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\n  source: {source}");
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n  source: {source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\n  source: {source}"
        ),
    }
}

#[test]
fn packed_enums_use_the_smallest_unsigned_mode() {
    let cases = [
        (
            "typedef enum { A0 = 0, A1 = 255 } __attribute__((packed)) t;\n",
            (1, 1),
        ),
        (
            "typedef enum { A0 = 0, A1 = 65535 } __attribute__((packed)) t;\n",
            (2, 2),
        ),
        (
            "typedef enum { A0 = 0, A1 = 65536 } __attribute__((packed)) t;\n",
            (4, 4),
        ),
        (
            "typedef enum { A0 = 0, A1 = 4294967295u } __attribute__((packed)) t;\n",
            (4, 4),
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(layout_of(source, "t"), expected, "source: {source}");
    }
}

#[test]
fn packed_enums_with_negative_values_use_the_smallest_signed_mode() {
    assert_eq!(
        layout_of(
            "typedef enum { N0 = -1, N1 = 6 } __attribute__((packed)) t;\n",
            "t"
        ),
        (1, 1)
    );
    assert_eq!(
        layout_of(
            "typedef enum { N0 = -1, N1 = 200 } __attribute__((packed)) t;\n",
            "t"
        ),
        (2, 2)
    );
}

#[test]
fn packed_applies_before_the_tag_and_after_the_closing_brace() {
    assert_eq!(
        layout_of(
            "typedef enum __attribute__((packed)) before { B0, B1 = 6 } before_t;\n",
            "before_t"
        ),
        (1, 1)
    );
    assert_eq!(
        layout_of(
            "typedef enum after { C0, C1 = 6 } __attribute__((packed)) after_t;\n",
            "after_t"
        ),
        (1, 1)
    );
}

#[test]
fn enum_constants_remain_int_and_negative_packed_enums_are_signed() {
    accepts(
        "enum e { A0 = -1, A1 = 6 } __attribute__((packed));\n\
         _Static_assert(sizeof(A1) == 4, \"enumeration constants stay int\");\n\
         _Static_assert((enum e)-1 < 0, \"the underlying mode is signed\");\n",
    );
}

#[test]
fn plain_enums_keep_the_target_default_layout() {
    assert_eq!(
        layout_of("typedef enum { P0, P1 = 6 } plain_t;\n", "plain_t"),
        (4, 4)
    );
}

#[test]
fn packed_and_aligned_on_one_enum_is_refused() {
    rejects(
        "enum e { A0, A1 = 6 } __attribute__((packed, aligned(4)));\n",
        "E2903",
    );
    rejects(
        "enum __attribute__((aligned(4), packed)) e { A0, A1 = 6 };\n",
        "E2903",
    );
}

#[test]
fn kernel_rw_hint_size_assertion_holds() {
    accepts(
        "enum rw_hint {\n\
             WRITE_LIFE_NOT_SET = 0,\n\
             WRITE_LIFE_NONE = 1,\n\
             WRITE_LIFE_SHORT = 2,\n\
             WRITE_LIFE_MEDIUM = 3,\n\
             WRITE_LIFE_LONG = 4,\n\
             WRITE_LIFE_EXTREME = 5\n\
         } __attribute__((packed));\n\
         _Static_assert(sizeof(enum rw_hint) == 1, \"enum rw_hint must fit in one byte\");\n",
    );
}
