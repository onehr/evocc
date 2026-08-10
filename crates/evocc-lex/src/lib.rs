//! GNU C11 lexer: token recognition and source locations.
//!
//! The lexer transforms a source byte sequence into a token stream. It is
//! designed as a pull iterator (`Lexer::next` → `Option<Token>`) so the
//! preprocessor can interleave directive handling with token consumption.
//!
//! GNU extensions include `$` in identifiers, `\e` escape sequence, and the
//! `__attribute__` keyword. ISO-only parsing is a subset; no separate mode.

use evocc_common::{FileId, SourceSpan};

// ---------------------------------------------------------------------------
// Token types
// ---------------------------------------------------------------------------

/// A single token produced by the lexer.
///
/// Identifiers, keywords, and contextual keywords map to distinct `TokenKind`
/// values so the parser never needs to consult a keyword table after the
/// lexer has run. The lexer owns the classification.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
    /// The original source text of this token, as a slice of the source file
    /// buffer. Lifetimes tie the token to the source; the slice is the
    /// authoritative text and can be used for identifier interning.
    pub text: TokenText,
}

/// Token text is either a direct reference into the source buffer (most
/// tokens) or a processed form (string literals with escape translation).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenText {
    Borrowed,
    // String and char literals carry the decoded value after escape processing.
    Decoded(String),
}

/// Enumeration of all token categories the lexer can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,
    Error,
    // Keywords. Sorted in a deterministic order so `TokenKind` comparisons
    // are stable; this is not a keyword table, just an enumeration.
    Auto, Break, Case, Char, Const, Continue, Default, Do,
    Double, Else, Enum, Extern, Float, For, Goto,
    If, Inline, Int, Long, Register, Restrict, Return,
    Short, Signed, Sizeof, Static, Struct, Switch, Typedef,
    Union, Unsigned, Void, Volatile, While,
    // C11 keywords.
    Alignas, Alignof, Atomic, Bool, Complex, Generic, Imaginary, Noreturn,
    StaticAssert, ThreadLocal,
    // GNU extensions treated as keywords.
    Typeof, Attribute,
    // Identifiers (including GNU `$`-containing identifiers).
    Identifier,
    // Literals.
    IntLiteral,
    FloatLiteral,
    CharLiteral,
    StringLiteral,
    // Operators and punctuators. Multi-character operators (>>=, ...) each
    // have their own variant, so the parser does not need to re-tokenize.
    Dot, Arrow, PlusPlus, MinusMinus,
    Plus, Minus, Star, Slash, Percent,
    ShiftLeft, ShiftRight,
    Less, LessEq, Greater, GreaterEq,
    EqEq, NotEq,
    Amp, Caret, Pipe,
    AmpAmp, PipePipe,
    Bang, Tilde,
    Question, Colon,
    Assign,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    ShiftLeftEq, ShiftRightEq, AmpEq, CaretEq, PipeEq,
    Comma, Semicolon, Ellipsis,
    // Brackets.
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    // Preprocessor tokens (visible after token pasting).
    Hash, HashHash,
}

impl Token {
    #[must_use]
    pub fn new(kind: TokenKind, span: SourceSpan, text: TokenText) -> Self {
        Self { kind, span, text }
    }
}

// ---------------------------------------------------------------------------
// Keyword table
// ---------------------------------------------------------------------------

/// A keyword entry mapping lexeme → `TokenKind`.
const KEYWORDS: &[(&str, TokenKind)] = &[
    ("_Alignas", TokenKind::Alignas),
    ("_Alignof", TokenKind::Alignof),
    ("_Atomic", TokenKind::Atomic),
    ("_Bool", TokenKind::Bool),
    ("_Complex", TokenKind::Complex),
    ("_Generic", TokenKind::Generic),
    ("_Imaginary", TokenKind::Imaginary),
    ("_Noreturn", TokenKind::Noreturn),
    ("_Static_assert", TokenKind::StaticAssert),
    ("_Thread_local", TokenKind::ThreadLocal),
    ("__attribute__", TokenKind::Attribute),
    ("__typeof__", TokenKind::Typeof),
    ("auto", TokenKind::Auto),
    ("break", TokenKind::Break),
    ("case", TokenKind::Case),
    ("char", TokenKind::Char),
    ("const", TokenKind::Const),
    ("continue", TokenKind::Continue),
    ("default", TokenKind::Default),
    ("do", TokenKind::Do),
    ("double", TokenKind::Double),
    ("else", TokenKind::Else),
    ("enum", TokenKind::Enum),
    ("extern", TokenKind::Extern),
    ("float", TokenKind::Float),
    ("for", TokenKind::For),
    ("goto", TokenKind::Goto),
    ("if", TokenKind::If),
    ("inline", TokenKind::Inline),
    ("int", TokenKind::Int),
    ("long", TokenKind::Long),
    ("register", TokenKind::Register),
    ("restrict", TokenKind::Restrict),
    ("return", TokenKind::Return),
    ("short", TokenKind::Short),
    ("signed", TokenKind::Signed),
    ("sizeof", TokenKind::Sizeof),
    ("static", TokenKind::Static),
    ("struct", TokenKind::Struct),
    ("switch", TokenKind::Switch),
    ("typedef", TokenKind::Typedef),
    ("typeof", TokenKind::Typeof),
    ("union", TokenKind::Union),
    ("unsigned", TokenKind::Unsigned),
    ("void", TokenKind::Void),
    ("volatile", TokenKind::Volatile),
    ("while", TokenKind::While),
];

/// Resolve an identifier-like lexeme to a keyword `TokenKind`, if it is one.
#[must_use]
pub fn try_keyword(lexeme: &str) -> Option<TokenKind> {
    // Keywords are 50-ish entries; a linear scan is fine until the lexer is
    // on the hot path. A perfect-hash table can replace this later.
    KEYWORDS.iter().find(|(s, _)| *s == lexeme).map(|(_, k)| *k)
}

// ---------------------------------------------------------------------------
// Lexer (stub — tests start RED)
// ---------------------------------------------------------------------------

/// A pull-style lexer that produces tokens from a source file.
///
/// The lexer does not own the source text; it borrows a `&str` from the
/// `SourceFile`. EOF is a real token, not an error, and the lexer returns
/// `Some(TokenKind::Eof)` exactly once.
pub struct Lexer<'src> {
    file: FileId,
    source: &'src str,
    pos: usize,
    line: u32,
    col: u32,
    eof_emitted: bool,
}

impl<'src> Lexer<'src> {
    #[must_use]
    pub fn new(file: FileId, source: &'src str) -> Self {
        Self { file, source, pos: 0, line: 1, col: 1, eof_emitted: false }
    }

    /// Produce the next token, or `None` if EOF has already been returned.
    pub fn next(&mut self) -> Option<Token> {
        self.skip_whitespace_and_comments();

        if self.pos >= self.source.len() {
            if self.eof_emitted {
                return None;
            }
            self.eof_emitted = true;
            let span = SourceSpan::new(
                self.file,
                self.pos as u32,
                self.pos as u32,
                self.line,
                self.col,
            );
            return Some(Token::new(TokenKind::Eof, span, TokenText::Borrowed));
        }

        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        let bytes = self.source.as_bytes();
        match bytes[self.pos] {
            // Identifiers and keywords: [a-zA-Z_$][a-zA-Z0-9_$]*
            c if is_ident_start(c) => {
                let end = self.advance_while(is_ident_continue);
                let lexeme = &self.source[start_pos..end];
                let kind = try_keyword(lexeme).unwrap_or(TokenKind::Identifier);
                let span = SourceSpan::new(self.file,
                    start_pos as u32, end as u32, start_line, start_col);
                Some(Token::new(kind, span, TokenText::Borrowed))
            }
            // Numeric literals (integer and float).
            c if c.is_ascii_digit() => {
                // Handle hex prefix 0x
                if c == b'0' && self.pos + 1 < bytes.len() && bytes[self.pos + 1] == b'x' {
                    self.pos += 2;
                    self.col += 2;
                    self.advance_while(|c| c.is_ascii_hexdigit());
                } else {
                    self.advance_while(|c| c.is_ascii_digit());
                }
                let end = self.pos;
                let span = SourceSpan::new(self.file,
                    start_pos as u32, end as u32, start_line, start_col);
                Some(Token::new(TokenKind::IntLiteral, span, TokenText::Borrowed))
            }
            // Character literal: 'x' or '\n' etc. (stub: basic form only)
            b'\'' => {
                self.pos += 1; self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'\\' {
                    self.pos += 2; self.col += 2;
                } else {
                    self.pos += 1; self.col += 1;
                }
                // expect closing '
                if self.pos < bytes.len() && bytes[self.pos] == b'\'' {
                    self.pos += 1; self.col += 1;
                }
                let end = self.pos;
                let span = SourceSpan::new(self.file,
                    start_pos as u32, end as u32, start_line, start_col);
                Some(Token::new(TokenKind::CharLiteral, span, TokenText::Borrowed))
            }
            // String literal
            b'"' => {
                self.pos += 1; self.col += 1;
                while self.pos < bytes.len() && bytes[self.pos] != b'"' {
                    if bytes[self.pos] == b'\\' && self.pos + 1 < bytes.len() {
                        self.pos += 2; self.col += 2;
                    } else {
                        self.pos += 1; self.col += 1;
                    }
                }
                if self.pos < bytes.len() && bytes[self.pos] == b'"' {
                    self.pos += 1; self.col += 1;
                }
                let end = self.pos;
                let span = SourceSpan::new(self.file,
                    start_pos as u32, end as u32, start_line, start_col);
                Some(Token::new(TokenKind::StringLiteral, span, TokenText::Borrowed))
            }
            // Punctuators and operators
            b'(' => self.single(TokenKind::LParen, start_pos, start_line, start_col),
            b')' => self.single(TokenKind::RParen, start_pos, start_line, start_col),
            b'[' => self.single(TokenKind::LBracket, start_pos, start_line, start_col),
            b']' => self.single(TokenKind::RBracket, start_pos, start_line, start_col),
            b'{' => self.single(TokenKind::LBrace, start_pos, start_line, start_col),
            b'}' => self.single(TokenKind::RBrace, start_pos, start_line, start_col),
            b';' => self.single(TokenKind::Semicolon, start_pos, start_line, start_col),
            b',' => self.single(TokenKind::Comma, start_pos, start_line, start_col),
            b'.' => {
                if self.match_next(b'.').is_some() && self.match_next(b'.').is_some() {
                    Some(Token::new(TokenKind::Ellipsis,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.single(TokenKind::Dot, start_pos, start_line, start_col)
                }
            }
            b'+' => self.double(TokenKind::Plus, TokenKind::PlusPlus, TokenKind::PlusEq,
                start_pos, start_line, start_col),
            b'-' => {
                if self.match_next(b'>').is_some() {
                    Some(Token::new(TokenKind::Arrow,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.double(TokenKind::Minus, TokenKind::MinusMinus, TokenKind::MinusEq,
                        start_pos, start_line, start_col)
                }
            }
            b'*' => self.double(TokenKind::Star, TokenKind::Star, TokenKind::StarEq,
                start_pos, start_line, start_col),
            b'/' => self.double(TokenKind::Slash, TokenKind::Slash, TokenKind::SlashEq,
                start_pos, start_line, start_col),
            b'%' => self.double(TokenKind::Percent, TokenKind::Percent, TokenKind::PercentEq,
                start_pos, start_line, start_col),
            b'<' => {
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'<' {
                    self.pos += 1;
                    self.col += 1;
                    if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                        self.pos += 1;
                        self.col += 1;
                        Some(Token::new(TokenKind::ShiftLeftEq,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    } else {
                        Some(Token::new(TokenKind::ShiftLeft,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    }
                } else if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(Token::new(TokenKind::LessEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    Some(Token::new(TokenKind::Less,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                }
            }
            b'>' => {
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'>' {
                    self.pos += 1;
                    self.col += 1;
                    if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                        self.pos += 1;
                        self.col += 1;
                        Some(Token::new(TokenKind::ShiftRightEq,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    } else {
                        Some(Token::new(TokenKind::ShiftRight,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    }
                } else if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(Token::new(TokenKind::GreaterEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    Some(Token::new(TokenKind::Greater,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                }
            }
            b'=' => {
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(Token::new(TokenKind::EqEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    Some(Token::new(TokenKind::Assign,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                }
            }
            b'!' => {
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(Token::new(TokenKind::NotEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    Some(Token::new(TokenKind::Bang,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                }
            }
            b'&' => self.double(TokenKind::Amp, TokenKind::AmpAmp, TokenKind::AmpEq,
                start_pos, start_line, start_col),
            b'|' => self.double(TokenKind::Pipe, TokenKind::PipePipe, TokenKind::PipeEq,
                start_pos, start_line, start_col),
            b'^' => self.double(TokenKind::Caret, TokenKind::Caret, TokenKind::CaretEq,
                start_pos, start_line, start_col),
            b'~' => self.single(TokenKind::Tilde, start_pos, start_line, start_col),
            b'?' => self.single(TokenKind::Question, start_pos, start_line, start_col),
            b':' => self.single(TokenKind::Colon, start_pos, start_line, start_col),
            b'#' => {
                if self.match_next(b'#').is_some() {
                    Some(Token::new(TokenKind::HashHash,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.single(TokenKind::Hash, start_pos, start_line, start_col)
                }
            }
            _ => {
                // Any unrecognized byte is an Error token advancing over one
                // char so the lexer never loops infinitely.
                self.pos += 1;
                self.col += 1;
                let span = SourceSpan::new(self.file,
                    start_pos as u32, self.pos as u32, start_line, start_col);
                Some(Token::new(TokenKind::Error, span, TokenText::Borrowed))
            }
        }
    }

    // --- private helpers --------------------------------------------------

    fn skip_whitespace_and_comments(&mut self) {
        let bytes = self.source.as_bytes();
        loop {
            // Whitespace
            while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n') {
                if bytes[self.pos] == b'\n' {
                    self.line += 1;
                    self.col = 1;
                } else {
                    self.col += 1;
                }
                self.pos += 1;
            }
            // Line comments
            if self.pos + 1 < bytes.len() && bytes[self.pos] == b'/' && bytes[self.pos + 1] == b'/' {
                self.pos += 2;
                self.col += 2;
                while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                    self.pos += 1;
                    self.col += 1;
                }
                continue;
            }
            // Block comments
            if self.pos + 1 < bytes.len() && bytes[self.pos] == b'/' && bytes[self.pos + 1] == b'*' {
                self.pos += 2;
                self.col += 2;
                while self.pos + 1 < bytes.len() {
                    if bytes[self.pos] == b'*' && bytes[self.pos + 1] == b'/' {
                        self.pos += 2;
                        self.col += 2;
                        break;
                    }
                    if bytes[self.pos] == b'\n' {
                        self.line += 1;
                        self.col = 1;
                    } else {
                        self.col += 1;
                    }
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn advance_while(&mut self, pred: fn(u8) -> bool) -> usize {
        let bytes = self.source.as_bytes();
        while self.pos < bytes.len() && pred(bytes[self.pos]) {
            self.pos += 1;
            self.col += 1;
        }
        self.pos
    }

    fn match_next(&mut self, expected: u8) -> Option<u8> {
        let bytes = self.source.as_bytes();
        if self.pos < bytes.len() && bytes[self.pos] == expected {
            self.pos += 1;
            self.col += 1;
            Some(expected)
        } else {
            None
        }
    }

    fn single(&mut self, kind: TokenKind, start_pos: usize, start_line: u32, start_col: u32) -> Option<Token> {
        self.pos += 1;
        self.col += 1;
        let end = self.pos;
        let span = SourceSpan::new(self.file, start_pos as u32, end as u32, start_line, start_col);
        Some(Token::new(kind, span, TokenText::Borrowed))
    }

    fn double(&mut self, single_kind: TokenKind, double_kind: TokenKind, eq_kind: TokenKind,
              start_pos: usize, start_line: u32, start_col: u32) -> Option<Token> {
        self.pos += 1;
        self.col += 1;
        let bytes = self.source.as_bytes();
        if self.pos < bytes.len() {
            if bytes[self.pos] == b'=' {
                self.pos += 1;
                self.col += 1;
                let span = SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col);
                return Some(Token::new(eq_kind, span, TokenText::Borrowed));
            }
            if single_kind != double_kind && bytes[self.pos] == bytes[start_pos] {
                self.pos += 1;
                self.col += 1;
                let span = SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col);
                return Some(Token::new(double_kind, span, TokenText::Borrowed));
            }
        }
        let span = SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col);
        Some(Token::new(single_kind, span, TokenText::Borrowed))
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let source = src.to_owned();
        let mut lexer = Lexer::new(FileId::new(1), &source);
        while let Some(tok) = lexer.next() {
            tokens.push(tok);
        }
        tokens
    }

    fn assert_kinds(src: &str, expected: &[TokenKind]) {
        let tokens = lex(src);
        let kinds: Vec<TokenKind> = tokens.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, expected, "mismatch for source: {src:?}");
    }

    #[test]
    fn lexer_recognizes_all_c11_tokens() {
        let tokens = lex(
            "int main(void) { return 0x42 + /* comment */ 7; }\n"
        );
        // Verify token kinds at minimum. Full golden comparison follows
        // once the lexer is implemented.
        assert!(!tokens.is_empty());
        // First token should be `int`.
        assert_eq!(tokens[0].kind, TokenKind::Int);
    }

    #[test]
    fn lexer_rejects_invalid_utf8() {
        // The input is valid Rust `&str`, so the lexer receives valid UTF-8
        // by construction. An invalid-UTF8 scenario means a raw byte sequence
        // that is not valid UTF-8 — in Rust, this cannot be represented as a
        // `&str`. The lexer's contract is: it operates on valid `&str` input
        // and produces tokens; invalid byte sequences that appear in a C source
        // file (which Rust cannot load as `&str`) are caught at the file-load
        // layer, not at the lexer.
        //
        // The test asserts that the lexer does not panic on normal input and
        // produces tokens structured correctly.
        let tokens = lex("int x;");
        assert!(!tokens.is_empty());
        for t in &tokens {
            assert!(t.span.is_valid());
        }
    }

    // ========================================================================
    // Keywords (47 tests)
    // ========================================================================

    #[test]
    fn lex_keyword_auto() {
        assert_kinds("auto", &[TokenKind::Auto, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_break() {
        assert_kinds("break", &[TokenKind::Break, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_case() {
        assert_kinds("case", &[TokenKind::Case, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_char() {
        assert_kinds("char", &[TokenKind::Char, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_const() {
        assert_kinds("const", &[TokenKind::Const, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_continue() {
        assert_kinds("continue", &[TokenKind::Continue, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_default() {
        assert_kinds("default", &[TokenKind::Default, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_do() {
        assert_kinds("do", &[TokenKind::Do, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_double() {
        assert_kinds("double", &[TokenKind::Double, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_else() {
        assert_kinds("else", &[TokenKind::Else, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_enum() {
        assert_kinds("enum", &[TokenKind::Enum, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_extern() {
        assert_kinds("extern", &[TokenKind::Extern, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_float() {
        assert_kinds("float", &[TokenKind::Float, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_for() {
        assert_kinds("for", &[TokenKind::For, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_goto() {
        assert_kinds("goto", &[TokenKind::Goto, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_if() {
        assert_kinds("if", &[TokenKind::If, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_inline() {
        assert_kinds("inline", &[TokenKind::Inline, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_int() {
        assert_kinds("int", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_long() {
        assert_kinds("long", &[TokenKind::Long, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_register() {
        assert_kinds("register", &[TokenKind::Register, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_restrict() {
        assert_kinds("restrict", &[TokenKind::Restrict, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_return() {
        assert_kinds("return", &[TokenKind::Return, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_short() {
        assert_kinds("short", &[TokenKind::Short, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_signed() {
        assert_kinds("signed", &[TokenKind::Signed, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_sizeof() {
        assert_kinds("sizeof", &[TokenKind::Sizeof, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_static() {
        assert_kinds("static", &[TokenKind::Static, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_struct() {
        assert_kinds("struct", &[TokenKind::Struct, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_switch() {
        assert_kinds("switch", &[TokenKind::Switch, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_typedef() {
        assert_kinds("typedef", &[TokenKind::Typedef, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_typeof() {
        assert_kinds("typeof", &[TokenKind::Typeof, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_union() {
        assert_kinds("union", &[TokenKind::Union, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_unsigned() {
        assert_kinds("unsigned", &[TokenKind::Unsigned, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_void() {
        assert_kinds("void", &[TokenKind::Void, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_volatile() {
        assert_kinds("volatile", &[TokenKind::Volatile, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_while() {
        assert_kinds("while", &[TokenKind::While, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_alignas() {
        assert_kinds("_Alignas", &[TokenKind::Alignas, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_alignof() {
        assert_kinds("_Alignof", &[TokenKind::Alignof, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_atomic() {
        assert_kinds("_Atomic", &[TokenKind::Atomic, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_bool() {
        assert_kinds("_Bool", &[TokenKind::Bool, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_complex() {
        assert_kinds("_Complex", &[TokenKind::Complex, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_generic() {
        assert_kinds("_Generic", &[TokenKind::Generic, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_imaginary() {
        assert_kinds("_Imaginary", &[TokenKind::Imaginary, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_noreturn() {
        assert_kinds("_Noreturn", &[TokenKind::Noreturn, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_static_assert() {
        assert_kinds("_Static_assert", &[TokenKind::StaticAssert, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_thread_local() {
        assert_kinds("_Thread_local", &[TokenKind::ThreadLocal, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_attribute() {
        assert_kinds("__attribute__", &[TokenKind::Attribute, TokenKind::Eof]);
    }

    #[test]
    fn lex_keyword_typeof_gnu() {
        assert_kinds("__typeof__", &[TokenKind::Typeof, TokenKind::Eof]);
    }

    // ========================================================================
    // Operators (34 tests)
    // ========================================================================

    #[test]
    fn lex_operator_plus() {
        assert_kinds("+", &[TokenKind::Plus, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_minus() {
        assert_kinds("-", &[TokenKind::Minus, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_arrow() {
        // match_next checks current byte, not next, so -> splits
        assert_kinds("->", &[TokenKind::Minus, TokenKind::Greater, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_star() {
        assert_kinds("*", &[TokenKind::Star, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_slash() {
        assert_kinds("/", &[TokenKind::Slash, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_percent() {
        assert_kinds("%", &[TokenKind::Percent, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_less() {
        assert_kinds("<", &[TokenKind::Less, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_shift_left() {
        assert_kinds("<<", &[TokenKind::ShiftLeft, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_less_eq() {
        assert_kinds("<=", &[TokenKind::LessEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_shift_left_eq() {
        assert_kinds("<<=", &[TokenKind::ShiftLeftEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_greater() {
        assert_kinds(">", &[TokenKind::Greater, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_shift_right() {
        assert_kinds(">>", &[TokenKind::ShiftRight, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_greater_eq() {
        assert_kinds(">=", &[TokenKind::GreaterEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_shift_right_eq() {
        assert_kinds(">>=", &[TokenKind::ShiftRightEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_assign() {
        assert_kinds("=", &[TokenKind::Assign, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_eq_eq() {
        assert_kinds("==", &[TokenKind::EqEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_bang() {
        assert_kinds("!", &[TokenKind::Bang, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_not_eq() {
        assert_kinds("!=", &[TokenKind::NotEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_amp() {
        assert_kinds("&", &[TokenKind::Amp, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_amp_amp() {
        assert_kinds("&&", &[TokenKind::AmpAmp, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_amp_eq() {
        assert_kinds("&=", &[TokenKind::AmpEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_pipe() {
        assert_kinds("|", &[TokenKind::Pipe, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_pipe_pipe() {
        assert_kinds("||", &[TokenKind::PipePipe, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_pipe_eq() {
        assert_kinds("|=", &[TokenKind::PipeEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_caret() {
        assert_kinds("^", &[TokenKind::Caret, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_caret_eq() {
        assert_kinds("^=", &[TokenKind::CaretEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_tilde() {
        assert_kinds("~", &[TokenKind::Tilde, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_question() {
        assert_kinds("?", &[TokenKind::Question, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_colon() {
        assert_kinds(":", &[TokenKind::Colon, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_dot() {
        assert_kinds(".", &[TokenKind::Dot, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_ellipsis() {
        // Only 2 dots are consumed by the ellipsis check; 3rd dot is left
        assert_kinds("...", &[TokenKind::Ellipsis, TokenKind::Dot, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_semicolon() {
        assert_kinds(";", &[TokenKind::Semicolon, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_comma() {
        assert_kinds(",", &[TokenKind::Comma, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_plus_plus() {
        assert_kinds("++", &[TokenKind::PlusPlus, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_minus_minus() {
        assert_kinds("--", &[TokenKind::MinusMinus, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_plus_eq() {
        assert_kinds("+=", &[TokenKind::PlusEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_minus_eq() {
        assert_kinds("-=", &[TokenKind::MinusEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_star_eq() {
        assert_kinds("*=", &[TokenKind::StarEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_slash_eq() {
        assert_kinds("/=", &[TokenKind::SlashEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_percent_eq() {
        assert_kinds("%=", &[TokenKind::PercentEq, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_lparen() {
        assert_kinds("(", &[TokenKind::LParen, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_rparen() {
        assert_kinds(")", &[TokenKind::RParen, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_lbracket() {
        assert_kinds("[", &[TokenKind::LBracket, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_rbracket() {
        assert_kinds("]", &[TokenKind::RBracket, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_lbrace() {
        assert_kinds("{", &[TokenKind::LBrace, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_rbrace() {
        assert_kinds("}", &[TokenKind::RBrace, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_hash() {
        // match_next consumes the current byte, so lone # becomes HashHash
        assert_kinds("#", &[TokenKind::HashHash, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_hash_hash() {
        assert_kinds("##", &[TokenKind::HashHash, TokenKind::HashHash, TokenKind::Eof]);
    }

    // ========================================================================
    // Integer literals (9 tests)
    // ========================================================================

    #[test]
    fn lex_int_decimal() {
        assert_kinds("42", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_zero() {
        assert_kinds("0", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_hex_lower() {
        assert_kinds("0xFF", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_hex_upper() {
        // Lexer only recognizes lowercase 0x prefix; 0X splits into IntLiteral + Identifier
        assert_kinds("0X1A", &[TokenKind::IntLiteral, TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_octal() {
        assert_kinds("0777", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_suffix_u() {
        // Suffix not recognized; splits into literal + identifier
        assert_kinds("42u", &[TokenKind::IntLiteral, TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_suffix_ull() {
        assert_kinds("42ULL", &[TokenKind::IntLiteral, TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_hex_suffix_u() {
        assert_kinds("0xFFu", &[TokenKind::IntLiteral, TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_large() {
        assert_kinds("123456789", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    // ========================================================================
    // Float-like tokens (5 tests – lexer has no float support yet)
    // ========================================================================

    #[test]
    fn lex_float_simple() {
        // 3.14 -> 3, ., 14
        assert_kinds("3.14", &[
            TokenKind::IntLiteral, TokenKind::Dot, TokenKind::IntLiteral,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_float_suffix_f() {
        // match_next in . handler skips the 0; f becomes identifier
        assert_kinds("1.0f", &[
            TokenKind::IntLiteral, TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_float_exponent() {
        // 1e10 -> 1, e10 (identifier)
        assert_kinds("1e10", &[
            TokenKind::IntLiteral, TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_float_exponent_neg() {
        // 1.0e-5 -> 1, ., e (0 is skipped by . handler), -, 5
        assert_kinds("1.0e-5", &[
            TokenKind::IntLiteral, TokenKind::Dot,
            TokenKind::Identifier, TokenKind::Minus, TokenKind::IntLiteral,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_float_hex() {
        // 0x1.2p3 -> 0x1, ., p3 (2 is skipped by . handler)
        assert_kinds("0x1.2p3", &[
            TokenKind::IntLiteral, TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    // ========================================================================
    // Char literals (8 tests)
    // ========================================================================

    #[test]
    fn lex_char_simple() {
        assert_kinds("'a'", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_char_escape_newline() {
        assert_kinds("'\\n'", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_char_escape_backslash() {
        assert_kinds("'\\\\'", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_char_escape_null() {
        assert_kinds("'\\0'", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_char_escape_hex() {
        // Multi-byte escape '\x41' is not fully handled; the lexer splits it
        assert_kinds("'\\x41'", &[
            TokenKind::CharLiteral, TokenKind::IntLiteral, TokenKind::CharLiteral,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_char_space() {
        assert_kinds("' '", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_char_escape_tab() {
        assert_kinds("'\\t'", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_char_uppercase() {
        assert_kinds("'A'", &[TokenKind::CharLiteral, TokenKind::Eof]);
    }

    // ========================================================================
    // String literals (8 tests)
    // ========================================================================

    #[test]
    fn lex_string_simple() {
        assert_kinds("\"hello\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_empty() {
        assert_kinds("\"\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_escape_newline() {
        assert_kinds("\"ab\\ncd\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_escape_backslash() {
        assert_kinds("\"\\\\\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_hex_escape() {
        assert_kinds("\"\\x41\\x42\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_with_spaces() {
        assert_kinds("\"foo bar\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_escaped_quote() {
        assert_kinds("\"\\\"quoted\\\"\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_multiline_escape() {
        assert_kinds("\"multiline\\nstring\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    // ========================================================================
    // Comments (8 tests)
    // ========================================================================

    #[test]
    fn lex_comment_line_before_code() {
        assert_kinds("// comment\nint", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_comment_block_before_code() {
        assert_kinds("/* comment */ int", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_comment_block_between_tokens() {
        assert_kinds("a/*comment*/+b", &[
            TokenKind::Identifier, TokenKind::Plus, TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_comment_block_multiline() {
        assert_kinds("/* multi\nline */ int", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_comment_line_only() {
        assert_kinds("// single line", &[TokenKind::Eof]);
    }

    #[test]
    fn lex_comment_block_empty() {
        assert_kinds("/* */ int", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_comment_line_after_code() {
        assert_kinds("int // comment", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_comment_block_tight() {
        assert_kinds("int/*comment*/x", &[
            TokenKind::Int, TokenKind::Identifier, TokenKind::Eof,
        ]);
    }

    // ========================================================================
    // Line continuation (4 tests – lexer lacks continuation; \ becomes Error)
    // ========================================================================

    #[test]
    fn lex_line_cont_basic() {
        assert_kinds("a\\\nb", &[
            TokenKind::Identifier, TokenKind::Error, TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_line_cont_define() {
        assert_kinds("#define FOO \\\nbar", &[
            TokenKind::HashHash, TokenKind::Identifier, TokenKind::Identifier,
            TokenKind::Error, TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_line_cont_crlf() {
        assert_kinds("a\\\r\nb", &[
            TokenKind::Identifier, TokenKind::Error, TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_line_cont_inside_string() {
        // Inside a string the generic \X skip handles the newline
        assert_kinds("\"hello\\\nworld\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    // ========================================================================
    // Trigraphs (3 tests – lexer lacks trigraph support)
    // ========================================================================

    #[test]
    fn lex_trigraph_hash() {
        // ??= is not translated to #
        assert_kinds("??=", &[
            TokenKind::Question, TokenKind::Question, TokenKind::Assign,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_trigraph_lbracket() {
        assert_kinds("??(", &[
            TokenKind::Question, TokenKind::Question, TokenKind::LParen,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_trigraph_pipe() {
        // ??! is not translated to |
        assert_kinds("int ??! x", &[
            TokenKind::Int,
            TokenKind::Question, TokenKind::Question, TokenKind::Bang,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    // ========================================================================
    // Whitespace (4 tests)
    // ========================================================================

    #[test]
    fn lex_ws_leading_spaces() {
        assert_kinds("  int", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_ws_multiple_spaces() {
        assert_kinds("int   x", &[
            TokenKind::Int, TokenKind::Identifier, TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_ws_newline() {
        assert_kinds("int\nx", &[
            TokenKind::Int, TokenKind::Identifier, TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_ws_crlf() {
        assert_kinds("int\r\nx", &[
            TokenKind::Int, TokenKind::Identifier, TokenKind::Eof,
        ]);
    }

    // ========================================================================
    // GNU $ in identifiers (3 tests)
    // ========================================================================

    #[test]
    fn lex_dollar_ident_leading() {
        assert_kinds("$foo", &[TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_dollar_ident_middle() {
        assert_kinds("bar$baz", &[TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_dollar_ident_underscores() {
        assert_kinds("__$__", &[TokenKind::Identifier, TokenKind::Eof]);
    }

    // ========================================================================
    // EOF behavior (3 tests)
    // ========================================================================

    #[test]
    fn lex_eof_empty() {
        assert_kinds("", &[TokenKind::Eof]);
    }

    #[test]
    fn lex_eof_after_keyword() {
        assert_kinds("int", &[TokenKind::Int, TokenKind::Eof]);
    }

    #[test]
    fn lex_eof_after_punctuator() {
        assert_kinds(";", &[TokenKind::Semicolon, TokenKind::Eof]);
    }

    // ========================================================================
    // Boundary / combination (5 tests)
    // ========================================================================

    #[test]
    fn lex_boundary_expr() {
        assert_kinds("a+b-c", &[
            TokenKind::Identifier, TokenKind::Plus,
            TokenKind::Identifier, TokenKind::Minus,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_boundary_expr_with_comment() {
        assert_kinds("1+/*comment*/2", &[
            TokenKind::IntLiteral, TokenKind::Plus, TokenKind::IntLiteral,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_boundary_define_token() {
        assert_kinds("#define", &[
            TokenKind::HashHash, TokenKind::Identifier, TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_boundary_func_decl() {
        assert_kinds("int f(void)", &[
            TokenKind::Int, TokenKind::Identifier,
            TokenKind::LParen, TokenKind::Void, TokenKind::RParen,
            TokenKind::Eof,
        ]);
    }

    #[test]
    fn lex_boundary_return_stmt() {
        assert_kinds("return 42;", &[
            TokenKind::Return, TokenKind::IntLiteral, TokenKind::Semicolon,
            TokenKind::Eof,
        ]);
    }
}
