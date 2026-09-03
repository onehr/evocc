use evocc_cpp::{
    pragma_pack_action, FileId, Lexer, PragmaPackAction, PreprocessError, Preprocessor, Token,
    TokenKind, TokenStream, TokenText,
};

fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(FileId::new(92_030), source);
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
            .expect("token spelling belongs to the fixture")
            .to_owned(),
    }
}

fn preprocess(source: &str) -> (Vec<Token>, Vec<PreprocessError>, u64) {
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(lex(source)), source)
        .into_vec();
    (
        output,
        preprocessor.errors().to_vec(),
        preprocessor.ignored_pragma_count(),
    )
}

#[test]
fn substituted_argument_whitespace_matches_the_measured_stringizations() {
    let source = concat!(
        "#define STR1(s) #s\n",
        "#define STR(s) STR1(s)\n",
        "#define D(s) STR(GCC diagnostic s)\n",
        "#define E(s) STR(a s b)\n",
        "#define G(s) STR(s.h)\n",
        "D(push)\n",
        "E(x)\n",
        "G(name)\n",
    );
    let (output, errors, _) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    let spellings = output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect::<Vec<_>>();
    assert_eq!(
        spellings,
        ["\"GCC diagnostic push\"", "\"a x b\"", "\"name.h\""]
    );
}

#[test]
fn expanded_diagnostic_pragma_operators_are_consumed_and_counted() {
    let source = concat!(
        "#define STR1(s) #s\n",
        "#define STR(s) STR1(s)\n",
        "#define __diag(s) _Pragma(STR(GCC diagnostic s))\n",
        "#define __diag_push() __diag(push)\n",
        "#define __diag_ignore(option) __diag(ignored option)\n",
        "__diag_push();\n",
        "__diag_ignore(\"-Wmissing-prototypes\");\n",
        "int after;\n",
    );
    let (output, errors, ignored) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(ignored, 2, "both ignored-safe pragmas must be counted");
    let spellings = output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect::<Vec<_>>();
    assert_eq!(spellings, [";", ";", "int", "after", ";"]);
}

#[test]
fn expanded_pack_pragma_emits_the_existing_layout_marker() {
    let source = concat!(
        "#define APPLY_PRAGMA(text) _Pragma(#text)\n",
        "APPLY_PRAGMA(pack(1))\n",
        "struct packed_record { char byte; int word; };\n",
    );
    let (output, errors, ignored) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(ignored, 0);
    let actions = output
        .iter()
        .filter_map(pragma_pack_action)
        .collect::<Vec<_>>();
    assert_eq!(actions, [PragmaPackAction::Set(1)]);
}

#[test]
fn pragma_operator_requires_exactly_one_string_literal() {
    for source in ["_Pragma(1)\n", "_Pragma(\"one\" \"two\")\n"] {
        let (_, errors, _) = preprocess(source);
        assert_eq!(errors.len(), 1, "source: {source}\nerrors: {errors:?}");
        assert_eq!(errors[0].code, "E0112", "source: {source}");
        assert_eq!(errors[0].message, "stray-token", "source: {source}");
    }
}
