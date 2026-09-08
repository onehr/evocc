use evocc_cpp::{FileId, Lexer, Preprocessor, TokenKind, TokenStream, TokenText};

fn process(pp: &mut Preprocessor, source: &str) -> Vec<String> {
    let mut lexer = Lexer::new(FileId::new(94100), source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    pp.process(TokenStream::new(tokens), source)
        .into_vec()
        .into_iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| match token.text {
            TokenText::Decoded(text) => text,
            TokenText::Borrowed => panic!("lexer must own spelling"),
        })
        .collect()
}

#[test]
fn expanded_line_numbers_apply_to_macro_uses_without_rewriting_physical_spans() {
    let source = "#define N 1000\n#line N\n__LINE__\n#define M __LINE__\nM\n";
    let mut pp = Preprocessor::new();
    assert_eq!(process(&mut pp, source), ["1000", "1002"]);
    assert!(pp.errors().is_empty());
    let mut lexer = Lexer::new(FileId::new(94100), source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let result = pp.process(TokenStream::new(tokens), source).into_vec();
    let number = result
        .iter()
        .find(|token| token.kind == TokenKind::IntLiteral)
        .unwrap();
    assert_eq!(number.span.line_start, 3);
    assert_eq!(number.span.file, FileId::new(94100));
}

#[test]
fn line_control_keeps_filename_on_number_only_reset_and_ignores_inactive_directives() {
    let source = "#line 010 \"virtual.c\"\n__LINE__ __FILE__\n#line 25\n\n__LINE__ __FILE__\n#if 0\n#line 200 \"inactive.c\"\n#endif\n__LINE__ __FILE__\n#line 0\n__LINE__\n";
    let mut pp = Preprocessor::new();
    assert_eq!(
        process(&mut pp, source),
        [
            "10",
            "\"virtual.c\"",
            "26",
            "\"virtual.c\"",
            "30",
            "\"virtual.c\"",
            "0"
        ]
    );
    assert!(pp.errors().is_empty());
}

#[test]
fn includes_keep_physical_lookup_and_restore_the_includers_presumed_location() {
    let directory = std::env::temp_dir().join(format!("evocc-line-control-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let header = directory.join("header.h");
    std::fs::write(
        &header,
        "#line 400 \"logical-header.h\"\n__LINE__ __FILE__\n",
    )
    .unwrap();
    let mut pp = Preprocessor::new();
    pp.current_file = directory.join("main.c").to_string_lossy().into_owned();
    let result = process(&mut pp, "#line 50 \"logical-main.c\"\n__LINE__ __FILE__\n#include \"header.h\"\n__LINE__ __FILE__\n");
    std::fs::remove_file(header).unwrap();
    std::fs::remove_dir(directory).unwrap();
    assert!(pp.errors().is_empty(), "{:?}", pp.errors());
    assert_eq!(
        result,
        [
            "50",
            "\"logical-main.c\"",
            "400",
            "\"logical-header.h\"",
            "52",
            "\"logical-main.c\""
        ]
    );
}

#[test]
fn malformed_line_controls_are_not_silently_discarded() {
    for source in [
        "#line bad\nint x;",
        "#line 1 2\nint x;",
        "#line 4294967296\nint x;",
    ] {
        let mut pp = Preprocessor::new();
        process(&mut pp, source);
        assert_eq!(
            pp.errors()
                .first()
                .map(|error| (error.code, error.message.as_str())),
            Some(("E2936", "invalid-line-directive")),
            "{source}"
        );
    }
}
