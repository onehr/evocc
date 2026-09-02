use evocc_cpp::{FileId, Lexer, Preprocessor, TokenKind, TokenStream, TokenText};

const PREFIX: &str = concat!(
    "#define smp() Z\n",
    "#define G smp\n",
    "#define H(x) x smp\n",
    "#define K(x) x\n",
);

fn preprocess_tail(tail: &str) -> Vec<String> {
    let source = format!("{PREFIX}{tail}\n");
    let file = FileId::new(92_016);
    let mut lexer = Lexer::new(file, &source);
    let mut input = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        input.push(token);
    }
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(input), &source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "unexpected diagnostics: {:?}",
        preprocessor.errors()
    );
    output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| match &token.text {
            TokenText::Decoded(text) => text.clone(),
            TokenText::Borrowed => {
                source[token.span.byte_start as usize..token.span.byte_end as usize].to_owned()
            }
        })
        .collect()
}

#[test]
fn object_macro_rescans_with_following_parentheses() {
    assert_eq!(preprocess_tail("A G() B"), ["A", "Z", "B"]);
}

#[test]
fn object_macro_rescans_across_a_line_break() {
    assert_eq!(preprocess_tail("C G\n() D"), ["C", "Z", "D"]);
}

#[test]
fn replacement_tail_rescans_with_outer_parentheses() {
    assert_eq!(preprocess_tail("E H(1)() F"), ["E", "1", "Z", "F"]);
}

#[test]
fn substituted_object_macro_rescans_with_outer_parentheses() {
    assert_eq!(preprocess_tail("I K(G)() J"), ["I", "Z", "J"]);
}

#[test]
fn nested_argument_rescans_across_lines() {
    assert_eq!(preprocess_tail("L K(G\n)() M"), ["L", "Z", "M"]);
}
