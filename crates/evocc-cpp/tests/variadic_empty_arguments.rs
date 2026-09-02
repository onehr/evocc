use evocc_cpp::{FileId, Lexer, Preprocessor, Token, TokenKind, TokenStream, TokenText};

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

fn preprocess(source: &str) -> Vec<String> {
    let mut preprocessor = Preprocessor::new();
    let output = preprocessor
        .process(TokenStream::new(lex(FileId::new(93_018), source)), source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "unexpected errors: {:?}",
        preprocessor.errors(),
    );
    output
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect()
}

fn expected(source: &str) -> Vec<String> {
    lex(FileId::DUMMY, source)
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| spelling(token, source))
        .collect()
}

#[test]
fn empty_fixed_arguments_keep_their_positions() {
    let source = "#define E(a, b) [a|b]\nE(,) E(1,) E(,2) E( , )\n";
    assert_eq!(preprocess(source), expected("[|] [1|] [|2] [|]"));
}

#[test]
fn empty_standard_variadic_arguments_keep_their_positions() {
    let source = concat!(
        "#define V(a, ...) [a|__VA_ARGS__]\n",
        "V() V(1) V(1,) V(,2) V(1,2,3)\n",
    );
    assert_eq!(preprocess(source), expected("[|] [1|] [1|] [|2] [1|2,3]"),);
}

#[test]
fn empty_named_variadic_arguments_keep_their_positions() {
    let source = concat!("#define N(a, rest...) [a|rest]\n", "N() N(1) N(1,2,3)\n",);
    assert_eq!(preprocess(source), expected("[|] [1|] [1|2,3]"));
}

#[test]
fn empty_arguments_survive_nested_struct_group_forwarding() {
    let source = concat!(
        "#define __struct_group_tag(TAG) TAG\n",
        "#define __struct_group(TAG, NAME, ATTRS, MEMBERS...) union { struct { MEMBERS } ATTRS; struct __struct_group_tag(TAG) { MEMBERS } ATTRS NAME; } ATTRS\n",
        "#define struct_group(NAME, MEMBERS...) __struct_group(, NAME, , MEMBERS)\n",
        "struct sk { int a; struct_group(headers, int b; unsigned c:3; ); int d; };\n",
    );
    assert_eq!(
        preprocess(source),
        expected(
            "struct sk { int a; union { struct { int b; unsigned c:3; }; struct { int b; unsigned c:3; } headers; }; int d; };",
        ),
    );
}

#[test]
fn kernel_map_chain_preserves_every_empty_argument_and_comma() {
    let source = concat!(
        "#define __MAP0(m,...)\n",
        "#define __MAP1(m,t,a,...) m(t,a)\n",
        "#define __MAP2(m,t,a,...) m(t,a), __MAP1(m,__VA_ARGS__)\n",
        "#define __MAP3(m,t,a,...) m(t,a), __MAP2(m,__VA_ARGS__)\n",
        "#define __MAP(n,...) __MAP##n(__VA_ARGS__)\n",
        "#define __SC_ARGS(t, a) , a\n",
        "#define REGS(x, ...) __MAP(x,__SC_ARGS,,regs->regs[0],,regs->regs[1],,regs->regs[2],,regs->regs[3])\n",
        "long g(void) { return h(REGS(3)); }\n",
    );
    assert_eq!(
        preprocess(source),
        expected("long g(void) { return h(, regs->regs[0], , regs->regs[1], , regs->regs[2]); }",),
    );
}
