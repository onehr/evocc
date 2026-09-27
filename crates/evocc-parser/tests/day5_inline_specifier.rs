use evocc_chir::{CHirBlockItem, CHirDecl, CHirStmt, StorageClass};
use evocc_common::{Arena, FileId, SourceFile, SourceSpan};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::{IntRank, TypeKind};

fn parse<'arena>(
    arena: &'arena Arena,
    source: &SourceFile,
) -> (
    evocc_chir::CHirUnit<'arena>,
    Option<evocc_parser::ParseDiagnostic>,
) {
    let mut lexer = Lexer::new(source.id, &source.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, source.id, &source.contents);
    let unit = parser.parse_translation_unit_in(arena);
    (unit, parser.diagnostic().cloned())
}

fn inline_spans(source: &SourceFile) -> Vec<SourceSpan> {
    source
        .contents
        .match_indices("inline")
        .map(|(start, spelling)| {
            let (line, column) = source.line_col(start);
            SourceSpan::new(
                source.id,
                start as u32,
                (start + spelling.len()) as u32,
                line,
                column,
            )
        })
        .collect()
}

#[test]
fn day5_inline_syntax_and_source_facts_are_preserved() {
    let source = SourceFile::new(
        FileId::new(805),
        "/virtual/day5/inline.c".into(),
        concat!(
            "static inline int before_type(void);\n",
            "inline static int before_storage(void) { return 1; }\n",
            "int inline after_type(void);\n",
            "unsigned inline long between_type_words(void);\n",
            "const inline unsigned long qualified(void) { return 2; }\n",
            "inline inline int repeated(void) { return 3; }\n",
            "inline int pair(void);\n",
            "inline int pair(void) { return 4; }\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);

    assert!(
        diagnostic.is_none(),
        "unexpected diagnostic: {diagnostic:?}"
    );
    assert_eq!(unit.declarations().len(), 8);

    let functions = unit
        .declarations()
        .iter()
        .map(|declaration| match declaration {
            CHirDecl::Function(function) => function,
            other => panic!("expected function declaration, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(functions[0].name.spelling, "before_type");
    assert_eq!(functions[0].storage, StorageClass::Static);
    assert!(functions[0].body.is_none());
    assert_eq!(functions[1].name.spelling, "before_storage");
    assert_eq!(functions[1].storage, StorageClass::Static);
    assert!(functions[1].body.is_some());
    assert_eq!(functions[2].name.spelling, "after_type");
    assert_eq!(functions[2].storage, StorageClass::None);
    assert_eq!(functions[3].name.spelling, "between_type_words");
    assert_eq!(functions[4].name.spelling, "qualified");
    assert!(functions[4].body.is_some());
    let between_type = unit
        .types()
        .get(functions[3].return_specifier.type_id())
        .expect("captured return type belongs to the unit");
    assert_eq!(between_type.kind, TypeKind::Int(IntRank::ULong));
    assert_eq!(between_type.qualifiers, Default::default());
    let qualified_type = unit
        .types()
        .get(functions[4].return_specifier.type_id())
        .expect("captured return type belongs to the unit");
    assert_eq!(qualified_type.kind, TypeKind::Int(IntRank::ULong));
    assert!(qualified_type.qualifiers.is_const);
    assert_eq!(functions[5].name.spelling, "repeated");
    assert!(functions[5].body.is_some());
    assert_eq!(functions[6].name.spelling, "pair");
    assert!(functions[6].body.is_none());
    assert_eq!(functions[7].name.spelling, "pair");
    assert!(functions[7].body.is_some());

    let expected = inline_spans(&source);
    let captured = functions
        .iter()
        .flat_map(|function| function.inline_spans.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(
        captured, expected,
        "inline source facts changed or reordered"
    );
    assert_eq!(
        functions
            .iter()
            .map(|function| function.inline_spans.len())
            .collect::<Vec<_>>(),
        vec![1, 1, 1, 1, 1, 2, 1, 1],
        "each declaration must retain every written inline occurrence",
    );
    for span in captured {
        assert_eq!(
            &source.contents[span.byte_start as usize..span.byte_end as usize],
            "inline",
        );
    }
}

#[test]
fn inline_on_non_function_declarations_is_transactional() {
    let cases = [
        ("inline int object; int after;\n", "object"),
        ("inline typedef int Alias; int after;\n", "Alias"),
        ("inline typedef int AliasFn(void); int after;\n", "AliasFn"),
        ("typedef inline int OtherFn(void); int after;\n", "OtherFn"),
        ("int outer(inline int parameter); int after;\n", "outer"),
    ];

    for (index, (contents, rejected_name)) in cases.into_iter().enumerate() {
        let source = SourceFile::new(
            FileId::new(806 + index as u32),
            format!("/virtual/day5/inline-invalid-{index}.c"),
            contents.into(),
        );
        let arena = Arena::default();
        let (unit, diagnostic) = parse(&arena, &source);
        let diagnostic = diagnostic.expect("invalid inline placement must be diagnosed");
        assert_eq!(diagnostic.code, "E1401", "source: {contents:?}");
        assert_eq!(
            &source.contents
                [diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            "inline",
            "diagnostic must point at the offending function specifier",
        );
        assert_eq!(unit.declarations().len(), 1, "source: {contents:?}");
        let CHirDecl::Object(after) = unit.declarations()[0] else {
            panic!("the declaration after the rejected transaction must survive")
        };
        assert_eq!(after.name.spelling, "after");

        let symbols = unit.symbols();
        let file_scope = symbols.file_scope();
        assert!(
            symbols
                .resolve(file_scope, rejected_name)
                .expect("file scope is live")
                .is_none(),
            "rejected declaration leaked an ordinary binding: {rejected_name}",
        );
    }
}

#[test]
fn bare_inline_starts_a_block_scope_function_declaration() {
    let source = SourceFile::new(
        FileId::new(812),
        "/virtual/day5/inline-block.c".into(),
        "int outer(void) { inline int nested(void); return 0; }\n".into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    assert!(
        diagnostic.is_none(),
        "unexpected diagnostic: {diagnostic:?}"
    );

    let CHirDecl::Function(outer) = unit.declarations()[0] else {
        panic!("expected outer function definition")
    };
    let CHirStmt::Compound(items, _, _) = outer.body.expect("outer function body") else {
        panic!("expected compound function body")
    };
    let CHirBlockItem::Declaration(CHirDecl::Function(nested)) = items[0] else {
        panic!("expected block-scope function declaration")
    };
    assert_eq!(nested.name.spelling, "nested");
    assert_eq!(nested.inline_spans.len(), 1);
    let span = nested.inline_spans[0];
    assert_eq!(
        &source.contents[span.byte_start as usize..span.byte_end as usize],
        "inline"
    );
}

#[test]
fn function_declarators_are_rejected_in_for_initializers() {
    let declarations = [
        "int nested(void)",
        "int inline nested(void)",
        "const inline int nested(void)",
        "extern inline int nested(void)",
        "static inline int nested(void)",
    ];

    for (index, declaration) in declarations.into_iter().enumerate() {
        let contents = format!("int outer(void) {{ for ({declaration};;) ; }}\n");
        let source = SourceFile::new(
            FileId::new(813 + index as u32),
            format!("/virtual/day5/inline-for-invalid-{index}.c"),
            contents,
        );
        let arena = Arena::default();
        let (unit, diagnostic) = parse(&arena, &source);
        let diagnostic = diagnostic.expect("for initializer function must be diagnosed");
        assert_eq!(diagnostic.code, "E1206", "source: {declaration}");
        assert_eq!(
            &source.contents
                [diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            "nested",
            "diagnostic must identify the forbidden function declarator",
        );
        assert!(
            unit.declarations().is_empty(),
            "the failed outer declaration must roll back transactionally"
        );
    }
}
