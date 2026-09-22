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
        TokenText::Decoded(text) | TokenText::RawBytes { escaped: text, .. } => text.clone(),
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
    assert_expands("#define I(x) M\n#define M I\nM(7)\n", "I");
    assert_expands("#define F() G\n#define G(x) F()\nF()(0)\n", "G");
    assert_expands("#define F(x) x\nF(F(7))\n", "7");
    assert_expands("#define X Y\n#define Y X\nX\n", "X");
}

#[test]
fn c_keywords_are_identifiers_during_macro_expansion() {
    for (source, expansion) in [
        (
            "#define inline inline __attribute__((__gnu_inline__))\ninline int f(int x){return x;}\n",
            "inline __attribute__((__gnu_inline__)) int f(int x){return x;}",
        ),
        ("#define const volatile\nconst int x;\n", "volatile int x;"),
        ("#define int(x) x\nint(7) int keep;\n", "7 int keep;"),
        (
            "#define int(x) x\n#define NEXT int\nNEXT\n(7)\n",
            "7",
        ),
        (
            "#define const 3\n#define ID(x) x\nID(const)\n#if defined(const) && const == 3\nyes\n#endif\n",
            "3 yes",
        ),
        ("#define int long\n#define long int\nint long\n", "int long"),
    ] {
        assert_expands(source, expansion);
    }
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

#[test]
fn macro_stacks_restore_nested_and_undefined_states() {
    // The sealed pushpop.c prints 2 1 1 0 11. Retain those five observations,
    // with the final function macro still expressed as preprocessing tokens.
    assert_expands(
        concat!(
            "#define A 1\n#pragma push_macro(\"A\")\n#undef A\n#define A 2\n",
            "#pragma push_macro(\"A\")\n#undef A\n#pragma pop_macro(\"A\")\nA\n",
            "#pragma pop_macro(\"A\")\nA\n#pragma pop_macro(\"A\")\nA\n",
            "#pragma push_macro(\"U\")\n#define U 7\n#pragma pop_macro(\"U\")\n",
            "#ifdef U\n1\n#else\n0\n#endif\n",
            "#define F(x) (x+10)\n#pragma push_macro(\"F\")\n",
            "#undef F\n#define F(x) (x+20)\n#pragma pop_macro(\"F\")\nF(1)\n",
        ),
        "2 1 1 0 (1+10)",
    );
}

#[test]
fn macro_stacks_preserve_definition_kind_parameters_and_pasting() {
    assert_expands(
        concat!(
            "#define F() 4\n#pragma push_macro(\"F\")\n#undef F\n#define F 8\n",
            "F\n#pragma pop_macro(\"F\")\nF()\n",
            "#define V(name, args...) name##suffix #args args\n",
            "#pragma push_macro(\"V\")\n#undef V\n#define V(x) x\n",
            "#pragma pop_macro(\"V\")\nV(a, 1, 2)\n",
            "#define E\n#pragma push_macro(\"E\")\n#undef E\n#define E 9\n",
            "#pragma pop_macro(\"E\")\nE end\n",
        ),
        "8 4 asuffix \"1, 2\" 1, 2 end",
    );
}

#[test]
fn macro_stacks_are_per_name_and_do_not_restore_definition_dependencies() {
    assert_expands(
        concat!(
            "#define A B\n#define B 1\n#pragma push_macro(\"A\")\n",
            "#pragma push_macro(\"B\")\n#undef A\n#define A 3\n",
            "#undef B\n#define B 2\n#pragma pop_macro(\"A\")\nA\n",
            "#pragma pop_macro(\"B\")\nA\n",
        ),
        "2 1",
    );
}

#[test]
fn expanded_pragma_operators_change_macro_state_before_following_uses() {
    for operator in ["_Pragma(TEXT)", "APPLY(pop_macro(\"A\"))", "P(TEXT)"] {
        let source = format!(
            concat!(
                "#define A 1\n#pragma push_macro(\"A\")\n#undef A\n#define A 2\n",
                "#define TEXT \"pop_macro(\\\"A\\\")\"\n",
                "#define APPLY(x) _Pragma(#x)\n#define P _Pragma\n",
                "{operator} A\nA\n",
            ),
            operator = operator,
        );
        assert_expands(&source, "1 1");
    }
}

#[test]
fn restored_feature_query_keeps_its_builtin_implementation() {
    assert_expands(
        concat!(
            "#pragma push_macro(\"__has_attribute\")\n#undef __has_attribute\n",
            "#define __has_attribute(x) 0\n__has_attribute(aligned)\n",
            "#pragma pop_macro(\"__has_attribute\")\n__has_attribute(aligned)\n",
        ),
        "0 1",
    );
}

#[test]
fn macro_pragmas_in_inactive_branches_have_no_effect() {
    assert_expands(
        concat!(
            "#define A 1\n#if 0\n#pragma push_macro(\"A\")\n#endif\n",
            "#undef A\n#define A 2\n#pragma pop_macro(\"A\")\nA\n",
        ),
        "2",
    );
}

#[test]
fn macro_stack_observations_distinguish_empty_pops_from_ignored_pragmas() {
    let source = concat!(
        "#define A 1\n#pragma push_macro(\"A\")\n#pragma pop_macro(\"A\")\n",
        "#pragma pop_macro(\"A\")\n#pragma push_macro(A)\n",
        "#pragma pop_macro(\"A\", \"B\")\n",
        "#pragma pack(push, 1)\n#pragma pack(pop)\n",
    );
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor.process(TokenStream::new(lex(FileId::DUMMY, source)), source);
    assert!(preprocessor.errors().is_empty());
    assert_eq!(preprocessor.empty_macro_pop_count(), 1);
    assert_eq!(preprocessor.ignored_pragma_count(), 2);
    assert_eq!(preprocessor.ignored_pragma_counts()["push_macro"], 1);
    assert_eq!(preprocessor.ignored_pragma_counts()["pop_macro"], 1);
    assert_eq!(
        output
            .into_vec()
            .iter()
            .filter_map(evocc_cpp::pragma_pack_action)
            .collect::<Vec<_>>(),
        [
            evocc_cpp::PragmaPackAction::Push(Some(1)),
            evocc_cpp::PragmaPackAction::Pop
        ]
    );
}

#[test]
fn macro_stacks_do_not_leak_between_translation_units() {
    let mut preprocessor = Preprocessor::new();
    let first = "#define A 1\n#pragma push_macro(\"A\")\n#undef A\n#define A 2\n";
    preprocessor.process(TokenStream::new(lex(FileId::DUMMY, first)), first);
    let next = "#pragma pop_macro(\"A\")\nA\n";
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::DUMMY, next)), next)
        .into_vec();
    assert!(preprocessor.errors().is_empty());
    assert_eq!(preprocessor.empty_macro_pop_count(), 1);
    assert_eq!(
        output
            .iter()
            .filter(|token| token.kind != TokenKind::Eof)
            .map(|token| spelling(token, next))
            .collect::<Vec<_>>(),
        ["2"]
    );
}

#[test]
fn a_definition_saved_in_a_header_keeps_its_spelling_after_restoration() {
    let source = "#include \"saved-macro.h\"\n#pragma pop_macro(\"VALUE\")\nVALUE(2)\n";
    let mut preprocessor = Preprocessor::new();
    preprocessor.add_include_path(format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR")));
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::new(93_032), source)), source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "{:?}",
        preprocessor.errors()
    );
    assert_eq!(
        output
            .iter()
            .filter(|token| token.kind != TokenKind::Eof)
            .map(|token| spelling(token, source))
            .collect::<Vec<_>>(),
        expected("(2 + 7)")
    );
}
