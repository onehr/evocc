use evocc_cpp::{FileId, Lexer, Preprocessor, TokenKind, TokenStream};

#[test]
fn each_triple_selects_its_macro_and_builtin_files() {
    for (triple, apple) in [
        ("aarch64-unknown-linux-gnu", false),
        ("aarch64-apple-darwin", true),
    ] {
        let mut preprocessor = Preprocessor::for_target(triple).unwrap();
        assert_eq!(preprocessor.macros().contains("__APPLE__"), apple);
        assert_eq!(preprocessor.macros().contains("__linux__"), !apple);
        let expected = u8::from(apple);
        let source = format!(
            "#if __has_builtin(__c11_atomic_load) != {expected}\n#error atomic list\n#endif\n\
             #if __has_builtin(__builtin_os_log_format) != {expected}\n#error log list\n#endif\n"
        );
        let mut lexer = Lexer::new(FileId::new(1), &source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            assert_ne!(token.kind, TokenKind::Error);
            tokens.push(token);
        }
        preprocessor.process(TokenStream::new(tokens), &source);
        assert!(!preprocessor.has_errors(), "{:?}", preprocessor.errors());
    }
}

#[test]
fn an_unknown_triple_does_not_fall_back_to_an_admitted_profile() {
    assert!(Preprocessor::for_target("unlisted-target").is_err());
}
