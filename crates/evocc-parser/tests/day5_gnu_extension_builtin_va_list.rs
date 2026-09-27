use evocc_chir::{CHirBlockItem, CHirDecl, CHirStmt};
use evocc_common::{Arena, FileId, SourceFile, SourceSpan};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::{Lexer, Token, TokenKind, TokenText};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{IntRank, TargetDataLayout, TypeKind, TypeLayout};

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

fn written(source: &SourceFile, span: SourceSpan) -> &str {
    &source.contents[span.byte_start as usize..span.byte_end as usize]
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

fn token_spelling<'a>(source: &'a SourceFile, token: &'a Token) -> &'a str {
    match &token.text {
        TokenText::Borrowed => written(source, token.span),
        TokenText::Decoded(spelling)
        | TokenText::RawBytes {
            escaped: spelling, ..
        } => spelling,
    }
}

#[test]
fn gnu_extension_prefix_preserves_the_frozen_kernel_typedef_and_exact_span() {
    let source = SourceFile::new(
        FileId::new(5_601),
        "/virtual/day5/gnu-extension-kernel.c".into(),
        concat!(
            "__extension__ typedef __signed__ long long __s64;\n",
            "int after;\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);

    assert!(
        diagnostic.is_none(),
        "frozen kernel typedef failed: {diagnostic:?}"
    );
    assert_eq!(unit.declarations().len(), 2);
    let CHirDecl::Typedef(alias) = unit.declarations()[0] else {
        panic!("expected the frozen typedef")
    };
    assert_eq!(alias.name.spelling, "__s64");
    assert_eq!(
        unit.types()
            .get(alias.ty)
            .expect("typedef type exists")
            .kind,
        TypeKind::Int(IntRank::LongLong),
    );
    assert_eq!(alias.gnu_extension_spans.len(), 1);
    assert_eq!(
        alias.gnu_extension_spans[0],
        SourceSpan::new(source.id, 0, 13, 1, 1),
    );
    assert_eq!(
        written(&source, alias.gnu_extension_spans[0]),
        "__extension__"
    );
    assert_eq!(
        written(&source, alias.specifier.span()),
        "__signed__ long long"
    );
    assert_eq!(
        written(&source, alias.span),
        "__extension__ typedef __signed__ long long __s64;",
    );
    let CHirDecl::Object(after) = unit.declarations()[1] else {
        panic!("the following declaration was lost")
    };
    assert_eq!(after.name.spelling, "after");
    assert!(after.gnu_extension_spans.is_empty());
}

#[test]
fn repeated_gnu_extension_prefixes_are_trivia_on_file_and_block_declarations() {
    let source = SourceFile::new(
        FileId::new(5_602),
        "/virtual/day5/gnu-extension-contexts.c".into(),
        concat!(
            "__extension__ __extension__ static int object;\n",
            "__extension__ int function(void) {\n",
            "  __extension__ int local;\n",
            "  __extension__ __extension__ typedef int local_type;\n",
            "  local_type copy;\n",
            "  return local;\n",
            "}\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);

    assert!(
        diagnostic.is_none(),
        "valid declaration prefixes failed: {diagnostic:?}"
    );
    let CHirDecl::Object(object) = unit.declarations()[0] else {
        panic!("expected file object")
    };
    assert_eq!(object.gnu_extension_spans.len(), 2);
    let CHirDecl::Function(function) = unit.declarations()[1] else {
        panic!("expected function")
    };
    assert_eq!(function.gnu_extension_spans.len(), 1);
    let CHirStmt::Compound(items, _, _) = function.body.expect("function body") else {
        panic!("expected compound body")
    };
    let CHirBlockItem::Declaration(CHirDecl::Object(local)) = items[0] else {
        panic!("expected prefixed local")
    };
    assert_eq!(local.gnu_extension_spans.len(), 1);
    let CHirBlockItem::Declaration(CHirDecl::Typedef(local_type)) = items[1] else {
        panic!("expected prefixed block typedef")
    };
    assert_eq!(local_type.gnu_extension_spans.len(), 2);

    for span in object
        .gnu_extension_spans
        .iter()
        .chain(&function.gnu_extension_spans)
        .chain(&local.gnu_extension_spans)
        .chain(&local_type.gnu_extension_spans)
    {
        assert_eq!(written(&source, *span), "__extension__");
    }
}

#[test]
fn gnu_extension_rejects_a_missing_declaration_or_expression() {
    let cases = [
        ("int __extension__; int after;\n", "E1001", "__extension__"),
        ("__extension__ value = 1; int after;\n", "E1001", "value"),
        (
            "int f(void) { return __extension__ ; } int after;\n",
            "E1003",
            ";",
        ),
    ];

    for (index, (contents, code, rejected)) in cases.into_iter().enumerate() {
        let source = SourceFile::new(
            FileId::new(5_603 + index as u32),
            format!("/virtual/day5/gnu-extension-invalid-{index}.c"),
            contents.into(),
        );
        let arena = Arena::default();
        let (unit, diagnostic) = parse(&arena, &source);
        let diagnostic = diagnostic.expect("invalid marker placement must be diagnosed");
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

#[test]
fn builtin_va_list_has_one_canonical_type_and_source_faithful_specifiers() {
    let source = SourceFile::new(
        FileId::new(5_606),
        "/virtual/day5/builtin-va-list.c".into(),
        concat!(
            "typedef __builtin_va_list va_list;\n",
            "typedef __builtin_va_list other_va_list;\n",
            "typedef const __builtin_va_list const_va_list;\n",
            "va_list *pointer;\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);

    assert!(
        diagnostic.is_none(),
        "builtin type name failed: {diagnostic:?}"
    );
    let aliases = unit
        .declarations()
        .iter()
        .take(3)
        .map(|declaration| match declaration {
            CHirDecl::Typedef(alias) => alias,
            other => panic!("expected typedef, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        aliases[0].ty, aliases[1].ty,
        "identical builtin uses must intern once"
    );
    assert_ne!(
        aliases[0].ty, aliases[2].ty,
        "qualifiers are part of canonical identity"
    );
    for alias in &aliases[..2] {
        let canonical = unit
            .types()
            .get(alias.ty)
            .expect("builtin type is interned");
        assert_eq!(canonical.kind, TypeKind::BuiltinVaList);
        assert_eq!(
            written(&source, alias.specifier.span()),
            "__builtin_va_list"
        );
    }
    assert_eq!(
        aliases[0].specifier.span(),
        SourceSpan::new(source.id, 8, 25, 1, 9),
    );
    let qualified = unit
        .types()
        .get(aliases[2].ty)
        .expect("qualified builtin type exists");
    assert_eq!(qualified.kind, TypeKind::BuiltinVaList);
    assert!(qualified.qualifiers.is_const);
    assert_eq!(
        written(&source, aliases[2].specifier.span()),
        "const __builtin_va_list"
    );
    assert_eq!(
        TargetDataLayout::aarch64_unknown_linux_gnu().layout_of(unit.types(), aliases[0].ty),
        Ok(TypeLayout::new(32, 8)),
        "the target profile gives va_list its measured object layout",
    );
}

#[test]
fn builtin_va_list_rejects_invalid_combinations_and_non_type_positions() {
    let cases = [
        (
            "typedef unsigned __builtin_va_list bad; int after;\n",
            "E1007",
            "unsigned __builtin_va_list",
        ),
        (
            "typedef __builtin_va_list __builtin_va_list bad; int after;\n",
            "E1007",
            "__builtin_va_list __builtin_va_list",
        ),
        (
            "int __builtin_va_list; int after;\n",
            "E1007",
            "int __builtin_va_list",
        ),
        (
            "int f(void) { return __builtin_va_list; } int after;\n",
            "E1003",
            "__builtin_va_list",
        ),
    ];

    for (index, (contents, code, rejected)) in cases.into_iter().enumerate() {
        let source = SourceFile::new(
            FileId::new(5_607 + index as u32),
            format!("/virtual/day5/builtin-va-list-invalid-{index}.c"),
            contents.into(),
        );
        let arena = Arena::default();
        let (unit, diagnostic) = parse(&arena, &source);
        let diagnostic = diagnostic.expect("invalid builtin use must be diagnosed");
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

#[test]
fn contextual_gnu_spellings_remain_identifiers_in_the_raw_lexer() {
    let source = "__extension__ __builtin_va_list";
    let mut lexer = Lexer::new(FileId::new(5_611), source);
    assert_eq!(
        lexer.next().expect("extension token").kind,
        TokenKind::Identifier
    );
    assert_eq!(
        lexer.next().expect("builtin token").kind,
        TokenKind::Identifier
    );
}

#[test]
fn preprocessing_expands_contextual_spellings_before_parser_promotion() {
    let source = SourceFile::new(
        FileId::new(5_612),
        "/virtual/day5/contextual-macro-away.c".into(),
        concat!(
            "#define __extension__\n",
            "#define __builtin_va_list int\n",
            "__extension__ typedef __builtin_va_list expanded;\n",
        )
        .into(),
    );
    let tokens = preprocess(&source);
    assert!(tokens.iter().all(|token| {
        !matches!(
            token_spelling(&source, token),
            "__extension__" | "__builtin_va_list"
        )
    }));

    let arena = Arena::default();
    let mut parser = Parser::new(tokens, source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.diagnostic().is_none());
    let CHirDecl::Typedef(alias) = unit.declarations()[0] else {
        panic!("expected expanded typedef")
    };
    assert!(alias.gnu_extension_spans.is_empty());
    assert_eq!(
        unit.types()
            .get(alias.ty)
            .expect("expanded type exists")
            .kind,
        TypeKind::Int(IntRank::Int),
    );
}

#[test]
fn pasted_decoded_contextual_spellings_promote_only_at_the_parser_boundary() {
    let source = SourceFile::new(
        FileId::new(5_613),
        "/virtual/day5/contextual-paste.c".into(),
        concat!(
            "#define CAT_RAW(a, b) a ## b\n",
            "CAT_RAW(__extension, __) typedef CAT_RAW(__builtin_va_, list) pasted;\n",
        )
        .into(),
    );
    let tokens = preprocess(&source);
    let extension = tokens
        .iter()
        .find(|token| token_spelling(&source, token) == "__extension__")
        .expect("paste produced extension spelling");
    let builtin = tokens
        .iter()
        .find(|token| token_spelling(&source, token) == "__builtin_va_list")
        .expect("paste produced builtin spelling");
    assert_eq!(extension.kind, TokenKind::Identifier);
    assert_eq!(builtin.kind, TokenKind::Identifier);
    assert!(matches!(extension.text, TokenText::Decoded(_)));
    assert!(matches!(builtin.text, TokenText::Decoded(_)));
    let extension_span = extension.span;
    let builtin_span = builtin.span;

    let arena = Arena::default();
    let mut parser = Parser::new(tokens, source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.diagnostic().is_none());
    let CHirDecl::Typedef(alias) = unit.declarations()[0] else {
        panic!("expected pasted typedef")
    };
    assert_eq!(alias.gnu_extension_spans, [extension_span]);
    assert_eq!(alias.specifier.span(), builtin_span);
    assert_eq!(
        unit.types().get(alias.ty).expect("pasted type exists").kind,
        TypeKind::BuiltinVaList,
    );
}
