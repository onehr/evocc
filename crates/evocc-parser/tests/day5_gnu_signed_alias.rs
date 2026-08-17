use evocc_chir::CHirDecl;
use evocc_common::{Arena, FileId, SourceFile, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{IntRank, TypeKind};

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

fn span_of(source: &SourceFile, spelling: &str, occurrence: usize) -> SourceSpan {
    let (start, _) = source
        .contents
        .match_indices(spelling)
        .nth(occurrence)
        .expect("spelling occurrence exists");
    let (line, column) = source.line_col(start);
    SourceSpan::new(
        source.id,
        start as u32,
        (start + spelling.len()) as u32,
        line,
        column,
    )
}

#[test]
fn gnu_signed_alias_uses_the_standard_signed_canonical_types() {
    let source = SourceFile::new(
        FileId::new(841),
        "/virtual/day5/gnu-signed.c".into(),
        concat!(
            "typedef __signed__ char __s8;\n",
            "typedef __signed__ long __slong;\n",
        )
        .into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    assert!(
        diagnostic.is_none(),
        "the frozen RED was E1001 at the first __signed__: {diagnostic:?}",
    );
    assert_eq!(unit.declarations().len(), 2);

    let expected = [
        ("__s8", IntRank::SChar, "__signed__ char"),
        ("__slong", IntRank::Long, "__signed__ long"),
    ];
    for (declaration, (name, rank, written_specifier)) in unit.declarations().iter().zip(expected) {
        let CHirDecl::Typedef(declaration) = declaration else {
            panic!("expected typedef declaration, got {declaration:?}")
        };
        assert_eq!(declaration.name.spelling, name);
        assert_eq!(
            unit.types()
                .get(declaration.ty)
                .expect("typedef type belongs to unit")
                .kind,
            TypeKind::Int(rank),
        );
        let span = declaration.specifier.span();
        assert_eq!(
            &source.contents[span.byte_start as usize..span.byte_end as usize],
            written_specifier,
            "the alias must retain its exact source extent",
        );
    }

    assert_eq!(
        span_of(&source, "__signed__", 0),
        SourceSpan::new(source.id, 8, 18, 1, 9),
    );
}

#[test]
fn duplicate_and_contradictory_alias_combinations_fail_like_signed() {
    let cases = [
        (
            "typedef __signed__ __signed__ int bad;",
            "__signed__ __signed__ int",
        ),
        (
            "typedef signed __signed__ int bad;",
            "signed __signed__ int",
        ),
        (
            "typedef __signed__ unsigned int bad;",
            "__signed__ unsigned int",
        ),
        ("typedef __signed__ double bad;", "__signed__ double"),
    ];

    for (index, (contents, rejected_specifier)) in cases.into_iter().enumerate() {
        let source = SourceFile::new(
            FileId::new(842 + index as u32),
            format!("/virtual/day5/gnu-signed-invalid-{index}.c"),
            contents.into(),
        );
        let arena = Arena::default();
        let (unit, diagnostic) = parse(&arena, &source);
        let diagnostic = diagnostic.expect("invalid type combination must be diagnosed");
        assert_eq!(diagnostic.code, "E1007", "source: {contents:?}");
        assert_eq!(
            diagnostic.message, "invalid-type-specifier-combination",
            "source: {contents:?}",
        );
        assert_eq!(
            &source.contents
                [diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            rejected_specifier,
            "source: {contents:?}",
        );
        assert!(
            unit.declarations().is_empty(),
            "the rejected typedef must not be committed: {contents:?}",
        );
    }
}

#[test]
fn surviving_gnu_signed_spelling_is_reserved_at_the_parser_boundary() {
    let source = SourceFile::new(
        FileId::new(846),
        "/virtual/day5/gnu-signed-reserved.c".into(),
        "typedef int __signed__; int after;\n".into(),
    );
    let arena = Arena::default();
    let (unit, diagnostic) = parse(&arena, &source);
    let diagnostic = diagnostic.expect("GNU keyword spelling cannot be a typedef name");
    assert_eq!(diagnostic.code, "E1001");
    assert_eq!(
        &source.contents[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
        ";",
        "after parser-boundary normalization, `int __signed__` is the valid type-specifier\
         sequence `int signed` followed by a missing declarator",
    );
    assert!(
        unit.declarations()
            .iter()
            .all(|declaration| match declaration {
                CHirDecl::Object(object) => object.name.spelling == "after",
                _ => false,
            }),
        "the rejected declaration must not create a GNU-keyword binding",
    );
}
