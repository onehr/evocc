use evocc_cpp::{FileId, Lexer, Preprocessor, Token, TokenKind, TokenStream, TokenText};

fn preprocess(source: &str) -> Vec<(TokenKind, String)> {
    let mut lexer = Lexer::new(FileId::new(5_201), source);
    let mut input = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "phase-2 source must lex");
        input.push(token);
    }

    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(input), source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "unexpected preprocessing errors: {:?}",
        preprocessor.errors(),
    );
    output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| (token.kind, spelling(token, source)))
        .collect()
}

fn spelling(token: &Token, source: &str) -> String {
    match &token.text {
        TokenText::Decoded(text) => text.clone(),
        TokenText::Borrowed => {
            source[token.span.byte_start as usize..token.span.byte_end as usize].to_owned()
        }
    }
}

#[test]
fn continued_object_macro_body_belongs_to_the_directive_logical_line() {
    let output = preprocess("#define VALUE \\\n42\nVALUE\nVALUE\n");
    assert_eq!(
        output,
        vec![
            (TokenKind::IntLiteral, "42".to_owned()),
            (TokenKind::IntLiteral, "42".to_owned()),
        ],
    );
}

#[test]
fn continued_function_macro_reproduces_kernel_define_shape() {
    let output = preprocess(concat!(
        "#define DECLARE(name, value) \\\n",
        "int name = \\\n",
        "value;\n",
        "DECLARE(answer, 7)\n",
    ));
    assert_eq!(
        output,
        vec![
            (TokenKind::Int, "int".to_owned()),
            (TokenKind::Identifier, "answer".to_owned()),
            (TokenKind::Assign, "=".to_owned()),
            (TokenKind::IntLiteral, "7".to_owned()),
            (TokenKind::Semicolon, ";".to_owned()),
        ],
    );
}

#[test]
fn splice_does_not_turn_a_midline_hash_into_a_directive() {
    let output = preprocess(concat!("int marker; \\\n", "#define VALUE 1\n", "VALUE\n",));
    assert_eq!(
        output,
        vec![
            (TokenKind::Int, "int".to_owned()),
            (TokenKind::Identifier, "marker".to_owned()),
            (TokenKind::Semicolon, ";".to_owned()),
            (TokenKind::Hash, "#".to_owned()),
            (TokenKind::Identifier, "define".to_owned()),
            (TokenKind::Identifier, "VALUE".to_owned()),
            (TokenKind::IntLiteral, "1".to_owned()),
            (TokenKind::Identifier, "VALUE".to_owned()),
        ],
    );
}

#[test]
fn crlf_continued_macro_body_is_reused_for_every_expansion() {
    let output = preprocess("#define VALUE \\\r\n9\r\nVALUE\r\nVALUE\r\n");
    assert_eq!(
        output,
        vec![
            (TokenKind::IntLiteral, "9".to_owned()),
            (TokenKind::IntLiteral, "9".to_owned()),
        ],
    );
}
