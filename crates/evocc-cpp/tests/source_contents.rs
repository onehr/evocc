use evocc_common::FileId;
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::Lexer;

#[test]
fn diagnostic_source_uses_the_bytes_that_were_preprocessed() {
    let directory = std::env::temp_dir().join(format!("evocc-source-bytes-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let header = directory.join("input.h");
    let bytes = b"/* raw byte: \xff */\nint n;\n";
    std::fs::write(&header, bytes).unwrap();
    let header = std::fs::canonicalize(header).unwrap();
    let source = format!("#include \"{}\"\n", header.display());
    let mut lexer = Lexer::new(FileId::new(1), &source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut pp = Preprocessor::new();
    pp.current_file = "main.c".into();
    pp.process(TokenStream::new(tokens), &source);
    assert!(!pp.has_errors());
    std::fs::remove_file(&header).unwrap();
    std::fs::remove_dir(directory).unwrap();
    assert_eq!(
        pp.source_contents(header.to_str().unwrap()),
        Some(bytes.as_slice())
    );
    assert_eq!(pp.source_contents("main.c"), Some(source.as_bytes()));
    let mut lexer = Lexer::new(FileId::new(1), "int m;");
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    pp.process(TokenStream::new(tokens), "int m;");
    assert_eq!(pp.source_contents(header.to_str().unwrap()), None);
}
