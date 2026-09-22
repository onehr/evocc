use evocc_cpp::{FileId, Lexer, Preprocessor, Token, TokenKind, TokenStream, TokenText};

fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(FileId::new(9_812), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    tokens
}

fn spelling(token: &Token, source: &str) -> String {
    match &token.text {
        TokenText::Decoded(text) | TokenText::RawBytes { escaped: text, .. } => text.clone(),
        TokenText::Borrowed => source
            .get(token.span.byte_start as usize..token.span.byte_end as usize)
            .expect("token span belongs to fixture")
            .to_owned(),
    }
}

fn preprocess(source: &str) -> Result<String, Vec<String>> {
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(lex(source)), source)
        .into_vec();
    if !preprocessor.errors().is_empty() {
        return Err(preprocessor
            .errors()
            .iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect());
    }
    Ok(output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect::<Vec<_>>()
        .join(" "))
}

fn selected(source: &str) -> String {
    preprocess(source)
        .unwrap_or_else(|errors| panic!("unexpected preprocessing errors: {errors:?}"))
        .trim()
        .to_owned()
}

#[test]
fn character_constants_use_their_decoded_value_in_conditions() {
    for expression in [
        r"L'\400' == 256",
        r"'\101' == 65",
        r"'\x41' == 'A'",
        r"'\e' == 27",
        r"L'\u0100' == 256",
        r"'\v' == 11 && '\a' == 7 && '\b' == 8 && '\f' == 12",
    ] {
        let source = format!("#if {expression}\ny\n#else\nn\n#endif\n");
        assert_eq!(selected(&source), "y", "{expression}");
    }
    let source = "#define C L'\\400'\n#if C\n#define zero (!C)\n#else\n#define zero C\n#endif\n#if zero == 0\ny\n#else\nn\n#endif\n";
    assert_eq!(selected(source), "y");
}

#[test]
fn malformed_character_constants_are_not_placeholder_zero() {
    assert!(preprocess("#if '\\x'\ny\n#endif\n").is_err());
}

#[test]
fn direct_pp_numbers_keep_their_integer_values_and_suffixes() {
    for (expression, expected) in [
        ("300 > 100", "y"),
        ("1UL == 1", "y"),
        ("1U > 100", "n"),
        ("0x10 > 3", "y"),
    ] {
        let source = format!("#if {expression}\ny\n#else\nn\n#endif\n");
        assert_eq!(selected(&source), expected, "expression: {expression}");
    }
}

#[test]
fn pasted_pp_numbers_are_relexed_in_if_arithmetic() {
    for (expression, expected) in [
        ("CAT(300,) > 100", "y"),
        ("CAT(0x1,00) == 256", "y"),
        ("CAT(1,UL) == 1 && CAT(1,ULL) == 1 && CAT(7,u) == 7", "y"),
    ] {
        let source = format!("#define CAT(a,b) a##b\n#if {expression}\ny\n#else\nn\n#endif\n");
        assert_eq!(selected(&source), expected, "expression: {expression}");
    }
}

#[test]
fn the_kernel_bit_macro_uses_the_pasted_literal_value() {
    let source = concat!(
        "#define __AC(X,Y) (X##Y)\n",
        "#define _AC(X,Y) __AC(X,Y)\n",
        "#define UL(x) (_AC(x, UL))\n",
        "#define BIT(nr) (UL(1) << (nr))\n",
        "#if 100 >= BIT(15)\n",
        "over\n",
        "#else\n",
        "ok\n",
        "#endif\n",
    );
    assert_eq!(selected(source), "ok");
}

#[test]
fn a_paste_that_cannot_form_one_token_is_rejected() {
    let source = "#define CAT(a,b) a##b\n#if CAT(1,+)\n#endif\n";
    assert!(preprocess(source).is_err());
}
