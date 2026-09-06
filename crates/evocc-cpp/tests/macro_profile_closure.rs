use evocc_cpp::{FileId, Lexer, Preprocessor, Token, TokenKind, TokenStream, TokenText};

struct Run {
    spellings: Vec<String>,
    errors: Vec<String>,
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
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::new(93_031), source)), source)
        .into_vec();
    Run {
        spellings: output
            .iter()
            .filter(|token| token.kind != TokenKind::Eof)
            .map(|token| spelling(token, source))
            .collect(),
        errors: preprocessor
            .errors()
            .iter()
            .map(|error| format!("{}: {}", error.code, error.message))
            .collect(),
    }
}

fn expected(source: &str) -> Vec<String> {
    lex(FileId::DUMMY, source)
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect()
}

fn assert_expands(source: &str, expansion: &str) {
    let actual = run(source);
    assert!(
        actual.errors.is_empty(),
        "unexpected errors: {:?}",
        actual.errors
    );
    assert_eq!(actual.spellings, expected(expansion));
}

#[test]
fn null_directives_do_not_consume_the_next_logical_line() {
    assert_expands("#\nint keep;\n#\n", "int keep;");
    assert_expands("#\n#if 0\nwrong\n#else\nright\n#endif\n", "right");
    assert_expands("# /* ignored */\n#define X 7\nX\n", "7");
}

#[test]
fn form_feed_and_vertical_tab_are_preprocessing_whitespace() {
    assert_expands("\x0c#define\x0bX\x0c7\nint\x0bx=X;\n", "int x=7;");
}

#[test]
fn expanded_macro_names_do_not_suppress_expansion_in_their_arguments() {
    assert_expands("#define I(x) x\n#define M I\nM(M(7))\n", "7");
    assert_expands("#define I(x) x\n#define M I\nM(I(7))\n", "7");
}

#[test]
fn conditional_state_crosses_the_macro_argument_boundary() {
    assert_expands(
        concat!(
            "#define WARN(x) warn x\n",
            "#define ERR(x) error x\n",
            "#ifdef NEVER\n",
            "WARN(\n",
            "#else\n",
            "ERR(\n",
            "#endif\n",
            "7)\n",
        ),
        "error 7",
    );
}

#[test]
fn an_empty_paste_operand_preserves_the_literal_operand() {
    assert_expands(
        concat!(
            "#define _DEVINC(mod, field) 0; mod##SNMP_INC(field)\n",
            "#define SNMP_INC(field) field\n",
            "#define INC(field) _DEVINC(, field)\n",
            "INC(3)\n",
        ),
        "0; 3",
    );
    assert_expands(
        concat!(
            "#define PASTE(left, right) left##right\n",
            "PASTE(, right) PASTE(left, ) PASTE(, )\n",
        ),
        "right left",
    );
}

#[test]
fn a_multiline_comment_in_a_continued_definition_is_whitespace() {
    assert_expands(
        concat!(
            "#define RES_MAXSZ (4 + \\\n",
            "  /* change, size,\n",
            "   * mtime */ \\\n",
            "  (2 + 2) * 4)\n",
            "RES_MAXSZ\n",
        ),
        "(4 + (2 + 2) * 4)",
    );
}

#[test]
fn va_opt_tracks_whether_variadic_tokens_are_present() {
    assert_expands(
        concat!(
            "#define OPT(a, ...) a __VA_OPT__(+) __VA_ARGS__\n",
            "OPT(1) OPT(1, 2)\n",
        ),
        "1 1 + 2",
    );
}

#[test]
fn macro_parameter_names_may_use_c_keyword_spellings() {
    assert_expands(
        concat!(
            "#define read_roothub(hc, register, mask) ((hc)[register] & (mask)), \"register\"\n",
            "read_roothub(base, index, 15)\n",
        ),
        "((base)[index] & (15)), \"register\"",
    );
}
