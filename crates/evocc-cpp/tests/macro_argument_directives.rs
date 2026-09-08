use evocc_cpp::{
    FileId, Lexer, PreprocessError, Preprocessor, Token, TokenKind, TokenStream, TokenText,
};

struct Run {
    spellings: Vec<String>,
    errors: Vec<PreprocessError>,
}

fn lex(file: FileId, source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    tokens
}

fn spelling(token: &Token, source: &str) -> String {
    match &token.text {
        TokenText::Decoded(text) => text.clone(),
        TokenText::Borrowed => source
            .get(token.spelling_span.byte_start as usize..token.spelling_span.byte_end as usize)
            .expect("token spelling span belongs to fixture")
            .to_owned(),
    }
}

fn run(source: &str) -> Run {
    let mut preprocessor = Preprocessor::new();
    preprocessor.add_include_path(format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR")));
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::new(93_020), source)), source)
        .into_vec();
    Run {
        spellings: output
            .iter()
            .filter(|token| token.kind != TokenKind::Eof)
            .map(|token| spelling(token, source))
            .collect(),
        errors: preprocessor.errors().to_vec(),
    }
}

fn expected(source: &str) -> Vec<String> {
    lex(FileId::DUMMY, source)
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect()
}

#[test]
fn ordinary_function_parentheses_do_not_open_macro_argument_contexts() {
    for (source, output) in [
        ("ordinary(\n#line 40\n1);", "ordinary(1);"),
        (
            "void function(int a,\n#line 40\nint b);",
            "void function(int a, int b);",
        ),
        ("ordinary(\n#line 40\n__LINE__);", "ordinary(40);"),
        (
            "ordinary(\n#define ordinary(x) x + 5\n1); ordinary(2);",
            "ordinary(1); 2 + 5;",
        ),
    ] {
        let actual = run(source);
        assert!(actual.errors.is_empty(), "{source}: {:?}", actual.errors);
        assert_eq!(actual.spellings, expected(output), "{source}");
    }
}

#[test]
fn includes_between_ordinary_call_arguments_preserve_tokens() {
    for prefix in ["", "#define cstr_cat function\n"] {
        let source = format!(
            "{prefix}cstr_cat(cs,\n#if 1\n#include \"call-strings.h\"\n#else\n\"unselected\"\n#endif\n, -1);"
        );
        let actual = run(&source);
        assert!(actual.errors.is_empty(), "{source}: {:?}", actual.errors);
        let callee = if prefix.is_empty() {
            "cstr_cat"
        } else {
            "function"
        };
        assert_eq!(
            actual.spellings,
            expected(&format!("{callee}(cs, \"first\" \"second\", -1);"))
        );
    }
}

#[test]
fn conditional_directives_select_macro_argument_members() {
    let source = concat!(
        "#define ID(x...) x\n",
        "#define C 1\n",
        "struct t { ID(int a;\n",
        "#ifdef C\n",
        "int b;\n",
        "#else\n",
        "int nb;\n",
        "#endif\n",
        "#if C == 1\n",
        "int c1;\n",
        "#elif C == 2\n",
        "int c2;\n",
        "#endif\n",
        "#ifndef D\n",
        "int d;\n",
        "#endif\n",
        "int e;) };\n",
    );
    let actual = run(source);
    assert!(actual.errors.is_empty(), "{:?}", actual.errors);
    assert_eq!(
        actual.spellings,
        expected("struct t { int a; int b; int c1; int d; int e; };"),
    );
}

#[test]
fn directives_inside_a_later_argument_preserve_the_comma_boundary() {
    let source = concat!(
        "#define TWO(a, b) a b\n",
        "#define C 1\n",
        "struct u { TWO(int a;,\n",
        "#ifdef C\n",
        "int b;\n",
        "#endif\n",
        ") };\n",
    );
    let actual = run(source);
    assert!(actual.errors.is_empty(), "{:?}", actual.errors);
    assert_eq!(actual.spellings, expected("struct u { int a; int b; };"),);
}

#[test]
fn definitions_and_undefinitions_inside_arguments_take_effect() {
    let source = concat!(
        "#define ID(x...) x\n",
        "ID(int a;\n",
        "#define LATER 7\n",
        "int b;)\n",
        "int later = LATER;\n",
        "ID(int c;\n",
        "#undef LATER\n",
        "int d;)\n",
        "#ifdef LATER\n",
        "bad\n",
        "#else\n",
        "int gone;\n",
        "#endif\n",
    );
    let actual = run(source);
    assert!(actual.errors.is_empty(), "{:?}", actual.errors);
    assert_eq!(
        actual.spellings,
        expected("int a; int b; int later = 7; int c; int d; int gone;"),
    );
}

#[test]
fn pragma_inside_a_macro_argument_is_e0113() {
    let source = concat!(
        "#define ID(x...) x\n",
        "ID(int a;\n",
        "#pragma GCC diagnostic push\n",
        "int b;)\n",
    );
    let actual = run(source);
    assert_eq!(actual.errors.len(), 1, "{:?}", actual.errors);
    assert_eq!(actual.errors[0].code, "E0113");
    assert_eq!(actual.errors[0].message, "directive-in-macro-argument");
}

#[test]
fn every_other_directive_inside_a_macro_argument_is_e0113() {
    let source = concat!(
        "#define ID(x...) x\n",
        "ID(int a;\n",
        "#line 40\n",
        "int b;)\n",
    );
    let actual = run(source);
    assert_eq!(actual.errors.len(), 1, "{:?}", actual.errors);
    assert_eq!(actual.errors[0].code, "E0113");
    assert_eq!(actual.errors[0].message, "directive-in-macro-argument");
}
