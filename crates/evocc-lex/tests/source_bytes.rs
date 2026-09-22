use evocc_common::FileId;
use evocc_lex::{Lexer, TokenKind, TokenText};

#[test]
fn raw_spelling_and_original_offsets_survive_line_splicing() {
    let source = b"/*\xff*/ \"\xc0\x80\\\n\xff\" name";
    let mut lexer = Lexer::from_bytes(FileId::new(781), source);
    let string = lexer.next().unwrap();
    assert_eq!(string.kind, TokenKind::StringLiteral);
    assert_eq!((string.span.byte_start, string.span.byte_end), (6, 13));
    assert_eq!((string.span.line_start, string.span.column_start), (1, 7));
    let TokenText::RawBytes { spelling, escaped } = string.text else {
        panic!("raw bytes lost")
    };
    assert_eq!(spelling, b"\"\xc0\x80\xff\"");
    assert_eq!(escaped, "\"\\300\\200\\377\"");
    let name = lexer.next().unwrap();
    assert_eq!(
        (
            name.span.byte_start,
            name.span.line_start,
            name.span.column_start
        ),
        (14, 2, 4)
    );
}

#[test]
fn unicode_source_and_raw_bytes_are_not_the_same_literal() {
    let mut lexer = Lexer::from_bytes(FileId::new(782), b"\"\xc3\xbf\" \"\xff\"");
    assert_eq!(
        lexer.next().unwrap().text,
        TokenText::Decoded("\"ÿ\"".into())
    );
    assert!(lexer.next().unwrap().text.has_raw_bytes());
}

#[test]
fn raw_escape_serialization_keeps_the_original_spelling_and_backslash_parity() {
    for (source, expected) in [
        (&b"\"\\\xff\""[..], "\"\\377\""),
        (&b"\"\\\\\xff\""[..], "\"\\\\\\377\""),
        (&b"\"\\\\\\\xff\""[..], "\"\\\\\\377\""),
    ] {
        let token = Lexer::from_bytes(FileId::new(782), source).next().unwrap();
        assert_eq!(token.kind, TokenKind::StringLiteral);
        let TokenText::RawBytes { spelling, escaped } = token.text else {
            panic!("raw spelling lost")
        };
        assert_eq!(spelling, source);
        assert_eq!(escaped, expected);
        assert_eq!(token.span.byte_end as usize, source.len());
    }
}
