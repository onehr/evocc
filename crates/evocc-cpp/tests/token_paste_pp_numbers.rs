use evocc_cpp::{FileId, Lexer, PreprocessError, Preprocessor, Token, TokenKind, TokenStream};

fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(FileId::new(9_813), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    tokens
}

fn preprocess(source: &str) -> (Vec<Token>, Vec<PreprocessError>) {
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(lex(source)), source)
        .into_vec();
    (output, preprocessor.errors().to_vec())
}

fn spellings(tokens: &[Token], source: &str) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| match &token.text {
            evocc_cpp::TokenText::Decoded(text) => text.clone(),
            evocc_cpp::TokenText::Borrowed => source
                .get(token.span.byte_start as usize..token.span.byte_end as usize)
                .expect("token span belongs to fixture")
                .to_owned(),
        })
        .collect()
}

#[test]
fn pasted_preprocessing_numbers_survive_until_their_consuming_context() {
    let source = concat!(
        "#define CAT(a, b) a##b\n",
        "CAT(1, _MODULE) CAT(1, x) CAT(1, .5)\n",
    );
    let (tokens, errors) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(spellings(&tokens, source), ["1_MODULE", "1x", "1.5"]);
}

#[test]
fn an_identifier_universal_character_name_is_valid_inside_a_pp_number() {
    let source = "#define CAT(a, b) a##b\nCAT(1, \\u0041)\n";
    let (tokens, errors) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(spellings(&tokens, source), ["1\\u0041"]);
}

#[test]
fn the_kernel_module_shape_can_repaste_a_preprocessing_number() {
    let source = concat!(
        "#define CAT_RAW(a, b) a##b\n",
        "#define MODULE_VALUE(x) CAT_RAW(x, _MODULE)\n",
        "#define PREFIX_RAW(x) __ARG_PLACEHOLDER_##x\n",
        "#define PREFIX(x) PREFIX_RAW(x)\n",
        "#define CONFIG_X 1\n",
        "PREFIX(MODULE_VALUE(CONFIG_X))\n",
    );
    let (tokens, errors) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(spellings(&tokens, source), ["__ARG_PLACEHOLDER_1_MODULE"]);
}

#[test]
fn an_invalid_paste_has_the_frozen_diagnostic_at_the_operator() {
    let source = "#define CAT(a, b) a##b\nCAT(1, +)\n";
    let (_, errors) = preprocess(source);
    assert_eq!(errors.len(), 1, "diagnostics: {errors:?}");
    assert_eq!(errors[0].code, "E0110");
    assert_eq!(errors[0].message, "invalid-token-paste");
    let operator = source.find("##").expect("fixture has a paste operator") as u32;
    assert_eq!(errors[0].span.byte_start, operator);
    assert_eq!(errors[0].span.byte_end, operator + 2);
}

#[test]
fn a_noninteger_preprocessing_number_is_rejected_only_when_if_consumes_it() {
    for (pasted, spelling) in [
        ("CAT(1, x)", "1x"),
        ("CAT(1, .5)", "1.5"),
        ("CAT(0, 8)", "08"),
        ("CAT(1, lul)", "1lul"),
    ] {
        let source = format!("#define CAT(a, b) a##b\n{pasted}\n#if {pasted}\ny\n#endif\n");
        let (tokens, errors) = preprocess(&source);
        assert_eq!(errors.len(), 1, "{pasted}: {errors:?}");
        assert_eq!(errors[0].code, "E0111", "{pasted}: {errors:?}");
        assert_eq!(errors[0].message, "conditional-operand-not-integer");
        assert!(
            spellings(&tokens, &source)
                .iter()
                .any(|token| token == spelling),
            "the translation-unit occurrence must survive: {tokens:?}"
        );
    }
}

#[test]
fn pasted_integer_constants_remain_valid_if_operands() {
    let source = concat!(
        "#define CAT(a, b) a##b\n",
        "#if CAT(1, UL) == 1 && CAT(0x, 10) == 16\n",
        "yes\n",
        "#endif\n",
    );
    let (tokens, errors) = preprocess(source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(spellings(&tokens, source), ["yes"]);
}
