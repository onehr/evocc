use evocc_cpp::{FileId, Lexer, Preprocessor, TokenKind, TokenStream, TokenText};

const SOURCE: &str = concat!(
    "#define __stringify_1(x...) #x\n",
    "#define __stringify(x...) __stringify_1(x)\n",
    "#define TRACE_INCLUDE_PATH .\n",
    "#define TRACE_INCLUDE_FILE trace_arm\n",
    "#define __TRACE_INCLUDE(system) __stringify(TRACE_INCLUDE_PATH/system.h)\n",
    "#define TRACE_INCLUDE(system) __TRACE_INCLUDE(system)\n",
    "#define A a\n",
    "#define B b\n",
    "#define E\n",
    "#define F(x) x\n",
    "TRACE_INCLUDE(TRACE_INCLUDE_FILE)\n",
    "__stringify(A/B)\n",
    "__stringify(A / B)\n",
    "__stringify(  A  B  )\n",
    "__stringify(A E B)\n",
    "__stringify(A E/B)\n",
    "__stringify(F(A)B)\n",
    "__stringify(F( A )/B)\n",
    "__stringify_1(A/B)\n",
    "__stringify(A.h)\n",
);

fn output_spellings() -> Vec<String> {
    let file = FileId::new(92_015);
    let mut lexer = Lexer::new(file, SOURCE);
    let mut input = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        input.push(token);
    }
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(input), SOURCE)
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
            TokenText::Decoded(text) | TokenText::RawBytes { escaped: text, .. } => text.clone(),
            TokenText::Borrowed => {
                SOURCE[token.span.byte_start as usize..token.span.byte_end as usize].to_owned()
            }
        })
        .collect()
}

fn assert_row(index: usize, expected: &str) {
    let actual = output_spellings();
    assert_eq!(
        actual.len(),
        10,
        "all measured rows must survive: {actual:?}"
    );
    assert_eq!(actual[index], expected, "measured row {index}: {actual:?}");
}

#[test]
fn computed_trace_include_has_no_inserted_spaces() {
    assert_row(0, "\"./trace_arm.h\"");
}

#[test]
fn adjacent_expanded_tokens_stay_adjacent() {
    assert_row(1, "\"a/b\"");
}

#[test]
fn written_spaces_collapse_to_one_space() {
    assert_row(2, "\"a / b\"");
}

#[test]
fn leading_and_trailing_argument_space_is_dropped() {
    assert_row(3, "\"a b\"");
}

#[test]
fn an_empty_expansion_preserves_separation_between_neighbors() {
    assert_row(4, "\"a b\"");
}

#[test]
fn an_empty_expansion_transfers_its_leading_space() {
    assert_row(5, "\"a /b\"");
}

#[test]
fn a_function_expansion_does_not_invent_trailing_space() {
    assert_row(6, "\"ab\"");
}

#[test]
fn spaces_inside_a_function_invocation_do_not_escape_it() {
    assert_row(7, "\"a/b\"");
}

#[test]
fn raw_stringization_keeps_unexpanded_spellings() {
    assert_row(8, "\"A/B\"");
}

#[test]
fn member_punctuation_stays_adjacent() {
    assert_row(9, "\"a.h\"");
}
