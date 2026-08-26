use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_cpp::{Lexer, Preprocessor, TokenKind, TokenStream};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> Result<CHirUnit<'a>, String> {
    let file = SourceFile::new(
        FileId::new(9_813),
        "pragma-pack.c".to_owned(),
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

    let mut preprocessor = Preprocessor::new();
    let tokens = preprocessor
        .process(TokenStream::new(tokens), &file.contents)
        .into_vec();
    if let Some(error) = preprocessor.errors().first() {
        return Err(format!("{}: {}", error.code, error.message));
    }

    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
}

fn layout_of(source: &str, name: &str) -> (u64, u32) {
    let arena = Arena::default();
    let unit =
        parse(source, &arena).unwrap_or_else(|error| panic!("must parse, got {error}\n{source}"));
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), name)
        .expect("symbol table is live")
        .unwrap_or_else(|| panic!("{name} is not declared"));
    let ty = unit.symbols().symbol(symbol).expect("declared").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .unwrap_or_else(|error| panic!("layout of {name}: {error:?}"));
    (layout.size_bytes, layout.align_bytes)
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n{source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\n{source}"
        ),
    }
}

#[test]
fn pragma_pack_caps_records_defined_while_active() {
    let source = concat!(
        "#pragma pack(1)\n",
        "struct p1 { char c; int i; } a;\n",
        "#pragma pack()\n",
        "struct p0 { char c; int i; } b;\n",
        "#pragma pack(2)\n",
        "struct p2 { char c; int i; long l; } d;\n",
        "#pragma pack()\n",
    );
    assert_eq!(layout_of(source, "a"), (5, 1));
    assert_eq!(layout_of(source, "b"), (8, 4));
    assert_eq!(layout_of(source, "d"), (14, 2));
}

#[test]
fn pragma_pack_push_pop_is_a_stack_and_caps_aligned_members() {
    let source = concat!(
        "#pragma pack(push, 1)\n",
        "struct q1 { char c; int i; } a;\n",
        "#pragma pack(push, 4)\n",
        "struct q4 { char c; long l; } b;\n",
        "#pragma pack(pop)\n",
        "struct q1b { char c; int i; } c;\n",
        "#pragma pack(pop)\n",
        "struct q0 { char c; int i; } d;\n",
        "#pragma pack(1)\n",
        "struct r { char c; int __attribute__((aligned(4))) i; } e;\n",
        "#pragma pack()\n",
    );
    assert_eq!(layout_of(source, "a"), (5, 1));
    assert_eq!(layout_of(source, "b"), (12, 4));
    assert_eq!(layout_of(source, "c"), (5, 1));
    assert_eq!(layout_of(source, "d"), (8, 4));
    assert_eq!(layout_of(source, "e"), (5, 1));
}

#[test]
fn unsupported_pack_forms_are_e0107() {
    rejects("#pragma pack(show)\n", "E0107");
    rejects("#pragma pack(3)\n", "E0107");
}
