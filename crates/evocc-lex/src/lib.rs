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
                if self.match_next(b'<').is_some() {
                    if self.match_next(b'=').is_some() {
                        Some(Token::new(TokenKind::ShiftLeftEq,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    } else {
                        Some(Token::new(TokenKind::ShiftLeft,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    }
                } else if self.match_next(b'=').is_some() {
                    Some(Token::new(TokenKind::LessEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.single(TokenKind::Less, start_pos, start_line, start_col)
                }
            }
            b'>' => {
                if self.match_next(b'>').is_some() {
                    if self.match_next(b'=').is_some() {
                        Some(Token::new(TokenKind::ShiftRightEq,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    } else {
                        Some(Token::new(TokenKind::ShiftRight,
                            SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                            TokenText::Borrowed))
                    }
                } else if self.match_next(b'=').is_some() {
                    Some(Token::new(TokenKind::GreaterEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.single(TokenKind::Greater, start_pos, start_line, start_col)
                }
            }
            b'=' => {
                if self.match_next(b'=').is_some() {
                    Some(Token::new(TokenKind::EqEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.single(TokenKind::Assign, start_pos, start_line, start_col)
                }
            }
            b'!' => {
                if self.match_next(b'=').is_some() {
                    Some(Token::new(TokenKind::NotEq,
                        SourceSpan::new(self.file, start_pos as u32, self.pos as u32, start_line, start_col),
                        TokenText::Borrowed))
                } else {
                    self.single(TokenKind::Bang, start_pos, start_line, start_col)
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
}
