use evocc_chir::interp::{interpret_scalar_function, InterpreterError, InterpreterLimits};
use evocc_chir::{CHirBlockItem, CHirDecl, CHirStmt};
use evocc_common::{Arena, FileId, SourceFile, SourceSpan};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::{Lexer, Token, TokenKind, TokenText};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{
    GnuInt128Signedness, TagDefinition, TagState, TargetDataLayout, TypeKind, TypeLayout,
};

fn parse<'arena>(
    arena: &'arena Arena,
    source: &SourceFile,
) -> (evocc_chir::CHirUnit<'arena>, Option<ParseDiagnostic>) {
    let mut lexer = Lexer::new(source.id, &source.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected test input");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(arena);
    (unit, parser.diagnostic().cloned())
}

fn preprocess(source: &SourceFile) -> Vec<Token> {
    let mut lexer = Lexer::new(source.id, &source.contents);
    let mut input = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected test input");
        input.push(token);
    }
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(input), &source.contents)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "preprocessing failed: {:?}",
        preprocessor.errors(),
    );
    output
}

fn written(source: &SourceFile, span: SourceSpan) -> &str {
    &source.contents[span.byte_start as usize..span.byte_end as usize]
}

fn token_spelling<'a>(source: &'a SourceFile, token: &'a Token) -> &'a str {
    match &token.text {
        TokenText::Borrowed => written(source, token.span),
        TokenText::Decoded(spelling) => spelling,
    }
}

#[test]
fn gnu_int128_forms_have_canonical_signedness_identity_spans_and_layout() {
    let source = SourceFile::new(
        FileId::new(5_701),
        "/virtual/day5/gnu-int128-forms.c".into(),
        concat!(
            "typedef __int128 bare_signed;\n",
            "typedef signed __int128 explicit_signed;\n",
            "typedef __signed__ __int128 gnu_signed;\n",
            "typedef __int128 signed reordered_signed;\n",
            "typedef unsigned __int128 unsigned_type;\n",
            "typedef __int128 unsigned reordered_unsigned;\n",
            "typedef const __int128 const_signed;\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    assert!(
        diagnostic.is_none(),
        "valid GNU integer forms failed: {diagnostic:?}"
    );

    let aliases = unit
        .declarations()
        .iter()
        .map(|declaration| match declaration {
            CHirDecl::Typedef(alias) => alias,
            other => panic!("expected typedef, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 7);
    assert_eq!(aliases[0].ty, aliases[1].ty);
    assert_eq!(aliases[0].ty, aliases[2].ty);
    assert_eq!(aliases[0].ty, aliases[3].ty);
    assert_eq!(aliases[4].ty, aliases[5].ty);
    assert_ne!(aliases[0].ty, aliases[4].ty);
    assert_ne!(aliases[0].ty, aliases[6].ty);

    assert_eq!(
        unit.types().get(aliases[0].ty).expect("signed type").kind,
        TypeKind::GnuInt128(GnuInt128Signedness::Signed),
    );
    assert_eq!(
        unit.types().get(aliases[4].ty).expect("unsigned type").kind,
        TypeKind::GnuInt128(GnuInt128Signedness::Unsigned),
    );
    assert_eq!(written(&source, aliases[0].specifier.span()), "__int128");
    assert_eq!(
        aliases[0].specifier.span(),
        SourceSpan::new(source.id, 8, 16, 1, 9),
    );
    assert_eq!(
        written(&source, aliases[2].specifier.span()),
        "__signed__ __int128",
    );
    assert_eq!(
        written(&source, aliases[5].specifier.span()),
        "__int128 unsigned",
    );
    assert!(
        unit.types()
            .get(aliases[6].ty)
            .expect("qualified type")
            .qualifiers
            .is_const,
    );

    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    for alias in aliases {
        assert_eq!(
            layout.layout_of(unit.types(), alias.ty),
            Ok(TypeLayout::new(16, 16)),
        );
    }
}

#[test]
fn gnu_int128_sizeof_and_alignof_are_exact_layout_only_integer_constants() {
    let source = SourceFile::new(
        FileId::new(5_714),
        "/virtual/day5/gnu-int128-layout-ice.c".into(),
        concat!(
            "enum layout_observations {\n",
            "  signed_size = sizeof(__int128),\n",
            "  unsigned_align = _Alignof(unsigned __int128)\n",
            "};\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    assert!(
        diagnostic.is_none(),
        "layout-only integer constants failed: {diagnostic:?}",
    );
    let CHirDecl::Tag(declaration) = unit.declarations()[0] else {
        panic!("expected enum declaration")
    };
    let tag = unit
        .types()
        .tag(declaration.tag)
        .expect("enum tag is canonical");
    let TagState::Complete(TagDefinition::Enum(constants)) = &tag.state else {
        panic!("enum definition was not completed")
    };
    assert_eq!(
        constants
            .iter()
            .map(|constant| (constant.name.as_str(), constant.value))
            .collect::<Vec<_>>(),
        [("signed_size", 16), ("unsigned_align", 16)],
    );
}

#[test]
fn gnu_int128_is_contextual_after_preprocessing_including_macro_away_and_paste() {
    let raw = "__int128";
    let mut lexer = Lexer::new(FileId::new(5_702), raw);
    assert_eq!(
        lexer.next().expect("raw GNU spelling").kind,
        TokenKind::Identifier,
    );

    let source = SourceFile::new(
        FileId::new(5_703),
        "/virtual/day5/gnu-int128-cpp.c".into(),
        concat!(
            "#define __int128 long long\n",
            "typedef __int128 expanded;\n",
            "#undef __int128\n",
            "#define CAT_RAW(a, b) a ## b\n",
            "typedef CAT_RAW(__int, 128) pasted;\n",
        )
        .into(),
    );
    let tokens = preprocess(&source);
    let pasted = tokens
        .iter()
        .find(|token| token_spelling(&source, token) == "__int128")
        .expect("paste produced the contextual spelling");
    assert_eq!(pasted.kind, TokenKind::Identifier);
    assert!(matches!(pasted.text, TokenText::Decoded(_)));
    let pasted_span = pasted.span;

    let arena = Arena::default();
    let mut parser = Parser::new(tokens, source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.diagnostic().is_none());
    let CHirDecl::Typedef(expanded) = unit.declarations()[0] else {
        panic!("expected expanded typedef")
    };
    assert_eq!(
        unit.types().get(expanded.ty).expect("expanded type").kind,
        TypeKind::Int(evocc_types::IntRank::LongLong),
    );
    let CHirDecl::Typedef(pasted) = unit.declarations()[1] else {
        panic!("expected pasted typedef")
    };
    assert_eq!(pasted.specifier.span(), pasted_span);
    assert_eq!(
        unit.types().get(pasted.ty).expect("pasted type").kind,
        TypeKind::GnuInt128(GnuInt128Signedness::Signed),
    );
}

#[test]
fn gnu_int128_rejects_duplicate_contradictory_and_non_type_uses() {
    let cases = [
        (
            "typedef __int128 __int128 bad; int after;\n",
            "E1007",
            "__int128 __int128",
        ),
        (
            "typedef signed unsigned __int128 bad; int after;\n",
            "E1007",
            "signed unsigned __int128",
        ),
        (
            "typedef long __int128 bad; int after;\n",
            "E1007",
            "long __int128",
        ),
        (
            "typedef int __int128 bad; int after;\n",
            "E1007",
            "int __int128",
        ),
        (
            "typedef short __int128 bad; int after;\n",
            "E1007",
            "short __int128",
        ),
        (
            "typedef char __int128 bad; int after;\n",
            "E1007",
            "char __int128",
        ),
        (
            "typedef _Bool __int128 bad; int after;\n",
            "E1007",
            "_Bool __int128",
        ),
        (
            "typedef double __int128 bad; int after;\n",
            "E1007",
            "double __int128",
        ),
        (
            "int f(void) { return __int128; } int after;\n",
            "E1003",
            "__int128",
        ),
    ];

    for (index, (contents, code, rejected)) in cases.into_iter().enumerate() {
        let source = SourceFile::new(
            FileId::new(5_704 + index as u32),
            format!("/virtual/day5/gnu-int128-invalid-{index}.c"),
            contents.into(),
        );
        let arena = Arena::default();
        let (unit, diagnostic) = parse(&arena, &source);
        let diagnostic = diagnostic.expect("invalid GNU integer use must be diagnosed");
        assert_eq!(diagnostic.code, code, "source: {contents:?}");
        assert_eq!(
            written(&source, diagnostic.span),
            rejected,
            "source: {contents:?}"
        );
        assert!(
            unit.declarations().iter().any(|declaration| matches!(
                declaration,
                CHirDecl::Object(object) if object.name.spelling == "after"
            )),
            "the recovery tail was lost: {contents:?}",
        );
    }
}

/// The `types.h` line that stopped every parse lane in the kernel cohort. It
/// used to pin the boundary at `__attribute__`; the boundary has moved, so the
/// test now pins what replaced it: the line parses, the typedef exists, and
/// `aligned(16)` reaches the layout service. A boundary test that only asserted
/// "still rejected" would have had to be deleted here, which is why it did not.
#[test]
fn exact_kernel_typedef_parses_and_applies_its_alignment() {
    let source = SourceFile::new(
        FileId::new(5_708),
        "/virtual/day5/linux-types-line-17.c".into(),
        "typedef __signed__ __int128 __s128 __attribute__((aligned(16)));\n".into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    assert!(
        diagnostic.is_none(),
        "kernel typedef must parse, got {diagnostic:?}"
    );
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), "__s128")
        .expect("symbol table is live")
        .expect("__s128 is declared");
    let ty = unit.symbols().symbol(symbol).expect("declared").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .expect("__s128 has a layout");
    assert_eq!((layout.size_bytes, layout.align_bytes), (16, 16));
}

#[test]
fn gnu_int128_front_end_values_stop_at_the_execution_boundary() {
    let accepted = SourceFile::new(
        FileId::new(5_709),
        "/virtual/day5/gnu-int128-front-end.c".into(),
        concat!(
            "__int128 object = 1;\n",
            "enum E { value = (__int128) 1 };\n",
            "int update(void) { __int128 x = 0; ++x; return (int)x; }\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (_, diagnostic) = parse(&arena, &accepted);
    assert!(
        diagnostic.is_none(),
        "front-end-complete wide values must parse: {diagnostic:?}",
    );

    let refused_bit_field = SourceFile::new(
        FileId::new(5_710),
        "/virtual/day5/gnu-int128-bit-field.c".into(),
        "struct S { __int128 field : 1; }; int after;\n".into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &refused_bit_field);
    let diagnostic = diagnostic.expect("wide bit fields remain outside the profile");
    assert_eq!(diagnostic.code, "E1305");
    assert_eq!(diagnostic.message, "bit-field-has-non-integer-type");
    assert_eq!(written(&refused_bit_field, diagnostic.span), "__int128");
    assert!(unit.declarations().iter().any(|declaration| matches!(
        declaration,
        CHirDecl::Object(object) if object.name.spelling == "after"
    )));

    // Wide execution is admitted. Reading an uninitialized wide object must
    // still report its memory fault rather than inventing a narrow value.
    let source = SourceFile::new(
        FileId::new(5_713),
        "/virtual/day5/gnu-int128-interpreter-boundary.c".into(),
        "int f(void) { __int128 wide; return (int)(wide + 1); }\n".into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    assert!(
        diagnostic.is_none(),
        "parse-only syntax must remain accepted: {diagnostic:?}",
    );
    let CHirDecl::Function(function) = unit.declarations()[0] else {
        panic!("expected function")
    };
    let CHirStmt::Compound(items, _) = function.body.expect("function body") else {
        panic!("expected compound body")
    };
    let CHirBlockItem::Declaration(CHirDecl::Object(wide)) = items[0] else {
        panic!("expected wide local")
    };
    assert_eq!(written(&source, wide.specifier.span()), "__int128");
    let error = interpret_scalar_function(&unit, function.symbol, InterpreterLimits::default())
        .unwrap_err();
    assert_eq!(error.code(), "E3001");
    assert!(matches!(error, InterpreterError::Memory { .. }));
}
