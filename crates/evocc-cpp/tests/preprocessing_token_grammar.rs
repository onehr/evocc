use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use evocc_cpp::{FileId, Lexer, PreprocessError, Preprocessor, Token, TokenKind, TokenStream};

fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(FileId::new(19_812), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
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

fn token_text(token: &Token, source: &str) -> String {
    match &token.text {
        evocc_cpp::TokenText::Decoded(text) => text.clone(),
        evocc_cpp::TokenText::Borrowed => source
            .get(token.span.byte_start as usize..token.span.byte_end as usize)
            .expect("token span belongs to fixture")
            .to_owned(),
    }
}

fn spellings(tokens: &[Token], source: &str) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| token_text(token, source))
        .collect()
}

fn assert_lexes_without_error(source: &str) {
    let tokens = lex(source);
    assert!(
        tokens.iter().all(|token| token.kind != TokenKind::Error),
        "preprocessing spelling did not lex: {source:?}: {tokens:?}"
    );
}

fn non_eof_kinds(source: &str) -> Vec<String> {
    lex(source)
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| format!("{:?}", token.kind))
        .collect()
}

fn assert_expands_pastes_and_stringifies(expression: &str, expected: &str) {
    let source = format!(
        "#define CAT(a, b) a##b\n#define STRINGIFY_RAW(x) #x\n#define STRINGIFY(x) STRINGIFY_RAW(x)\nSTRINGIFY({expression})\n"
    );
    let (tokens, errors) = preprocess(&source);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");
    assert_eq!(spellings(&tokens, &source), [expected]);
}

#[test]
fn header_name_spelling_survives_expansion_pasting_and_stringification() {
    assert_lexes_without_error("<fixture.h>");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "evocc-preprocessing-header-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).expect("create isolated include directory");
    let header = directory.join("fixture.h");
    fs::write(&header, "header_token_marker\n").expect("write isolated header");

    let source = concat!(
        "#define CAT(a, b) a##b\n",
        "#define HEADER CAT(, <fixture.h>)\n",
        "#define STRINGIFY_RAW(x) #x\n",
        "#define STRINGIFY(x) STRINGIFY_RAW(x)\n",
        "#include <fixture.h>\n",
        "#include HEADER\n",
        "STRINGIFY(HEADER)\n",
    );
    let mut preprocessor = Preprocessor::new();
    preprocessor.current_file = directory.join("main.c").to_string_lossy().into_owned();
    preprocessor.add_include_path(directory.to_string_lossy().into_owned());
    let output = preprocessor
        .process(TokenStream::new(lex(source)), source)
        .into_vec();

    fs::remove_file(&header).expect("remove isolated header");
    fs::remove_dir(&directory).expect("remove isolated include directory");

    assert!(
        preprocessor.errors().is_empty(),
        "unexpected diagnostics: {:?}",
        preprocessor.errors()
    );
    assert_eq!(
        spellings(&output, source),
        [
            "header_token_marker",
            "header_token_marker",
            "\"<fixture.h>\""
        ]
    );
}

#[test]
fn identifier_survives_expansion_pasting_and_stringification() {
    assert_eq!(non_eof_kinds("kernel_id"), ["Identifier"]);
    assert_expands_pastes_and_stringifies("CAT(kernel, _id)", "\"kernel_id\"");
}

#[test]
fn leading_dot_preprocessing_number_survives_expansion_pasting_and_stringification() {
    let spelling = ".5foo";
    assert_eq!(
        non_eof_kinds(spelling),
        ["PpNumber"],
        "preprocessing number must be one token: {spelling:?}: {:?}",
        lex(spelling)
    );
    assert_expands_pastes_and_stringifies("CAT(.5, foo)", "\".5foo\"");
}

#[test]
fn multi_dot_exponent_preprocessing_number_survives_pasting_and_stringification() {
    let spelling = "1.2.3e+x";
    assert_eq!(
        non_eof_kinds(spelling),
        ["PpNumber"],
        "preprocessing number must be one token: {spelling:?}: {:?}",
        lex(spelling)
    );
    assert_expands_pastes_and_stringifies("CAT(1.2.3e+, x)", "\"1.2.3e+x\"");
}

#[test]
fn character_and_string_literals_survive_expansion_pasting_and_stringification() {
    assert_eq!(
        non_eof_kinds("'x' \"text\""),
        ["CharLiteral", "StringLiteral"]
    );
    assert_expands_pastes_and_stringifies("CAT(L, 'x')", "\"L'x'\"");
    assert_expands_pastes_and_stringifies("CAT(L, \"text\")", "\"L\\\"text\\\"\"");
}

#[test]
fn punctuator_survives_expansion_pasting_and_stringification() {
    assert_eq!(non_eof_kinds(">>"), ["ShiftRight"]);
    assert_expands_pastes_and_stringifies("CAT(>, >)", "\">>\"");
}

#[test]
fn other_character_tokens_survive_expansion_pasting_and_stringification() {
    assert_eq!(non_eof_kinds("\\sreg"), ["PpOther", "Identifier"]);
    assert_eq!(non_eof_kinds("@"), ["PpOther"]);
    assert_eq!(non_eof_kinds("`"), ["PpOther"]);
    assert_expands_pastes_and_stringifies("CAT(, \\sreg)", "\"\\sreg\"");
    assert_expands_pastes_and_stringifies(".L__gpr_num_\\rt", "\".L__gpr_num_\\rt\"");
    assert_expands_pastes_and_stringifies("CAT(, @)", "\"@\"");
    assert_expands_pastes_and_stringifies("CAT(, `)", "\"`\"");
}
