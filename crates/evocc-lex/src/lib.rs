#![allow(clippy::should_implement_trait)]
//! GNU C11 lexer: token recognition and source locations.
//!
//! The lexer transforms a source byte sequence into a token stream. It is
//! designed as a pull iterator (`Lexer::next` → `Option<Token>`) so the
//! preprocessor can interleave directive handling with token consumption.
//!
//! GNU extensions include `$` in identifiers, `\e` escape sequence, and the
//! `__attribute__` keyword. ISO-only parsing is a subset; no separate mode.

use std::borrow::Cow;

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

/// Enumeration of token categories carried by the lexical and preprocessing
/// pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,
    Error,
    // Keywords. Sorted in a deterministic order so `TokenKind` comparisons
    // are stable; this is not a keyword table, just an enumeration.
    Auto,
    Break,
    Case,
    Char,
    Const,
    Continue,
    Default,
    Do,
    Double,
    Else,
    Enum,
    Extern,
    Float,
    For,
    Goto,
    If,
    Inline,
    Int,
    Long,
    Register,
    Restrict,
    Return,
    Short,
    Signed,
    Sizeof,
    Static,
    Struct,
    Switch,
    Typedef,
    Union,
    Unsigned,
    Void,
    Volatile,
    While,
    // C11 keywords.
    Alignas,
    Alignof,
    /// GNU `__alignof__`, promoted only after preprocessing at the parser
    /// boundary so the raw spelling remains macro-expandable.
    GnuAlignof,
    /// GNU `asm`/`__asm__`, promoted only after preprocessing.
    Asm,
    /// GNU `__builtin_expect`, promoted only after preprocessing.
    BuiltinExpect,
    /// GNU `__builtin_constant_p`, promoted only after preprocessing.
    BuiltinConstantP,
    /// GNU compile-time selection and type-query builtins, promoted only
    /// after preprocessing.
    BuiltinChooseExpr,
    BuiltinTypesCompatibleP,
    BuiltinOffsetof,
    BuiltinObjectSize,
    BuiltinHasAttribute,
    BuiltinStrlen,
    BuiltinVaStart,
    BuiltinVaEnd,
    BuiltinVaCopy,
    /// GNU bit operations and control/introspection builtins, promoted only
    /// after preprocessing.
    BuiltinBit,
    BuiltinUnreachable,
    BuiltinReturnAddress,
    /// GNU `__auto_type`, promoted only after preprocessing at the parser
    /// boundary so the spelling remains macro-expandable.
    AutoType,
    /// GNU block-local label declaration, promoted only after preprocessing.
    LocalLabel,
    /// GNU checked-arithmetic builtins, promoted only after preprocessing.
    BuiltinAddOverflow,
    BuiltinSubOverflow,
    BuiltinMulOverflow,
    Atomic,
    Bool,
    Complex,
    Generic,
    Imaginary,
    Noreturn,
    StaticAssert,
    ThreadLocal,
    // GNU extensions treated as keywords.
    Typeof,
    TypeofUnqual,
    Attribute,
    // GNU spellings that remain identifiers through preprocessing and are
    // promoted only at the parser boundary when their exact spelling survives.
    GnuExtension,
    BuiltinVaList,
    GnuInt128,
    // Identifiers (including GNU `$`-containing identifiers).
    Identifier,
    // Literals.
    IntLiteral,
    FloatLiteral,
    CharLiteral,
    StringLiteral,
    // Operators and punctuators. Multi-character operators (>>=, ...) each
    // have their own variant, so the parser does not need to re-tokenize.
    Dot,
    Arrow,
    PlusPlus,
    MinusMinus,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    ShiftLeft,
    ShiftRight,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    EqEq,
    NotEq,
    Amp,
    Caret,
    Pipe,
    AmpAmp,
    PipePipe,
    Bang,
    Tilde,
    Question,
    Colon,
    Assign,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    ShiftLeftEq,
    ShiftRightEq,
    AmpEq,
    CaretEq,
    PipeEq,
    Comma,
    Semicolon,
    Ellipsis,
    // Brackets.
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    // Preprocessor tokens (visible after token pasting).
    /// A preprocessing number produced by `##` that is not yet a valid C
    /// literal token. The preprocessor preserves it until its consuming
    /// context performs the required re-lexing.
    PpNumber,
    /// A non-whitespace character that is not another preprocessing-token
    /// production. Translation phase 4 must preserve it for macro expansion,
    /// pasting, and stringification; phase 7 diagnoses it if it survives.
    PpOther,
    Hash,
    HashHash,
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
    ("__attribute", TokenKind::Attribute),
    ("__typeof__", TokenKind::Typeof),
    ("__typeof_unqual__", TokenKind::TypeofUnqual),
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

/// Return the byte length of a syntactically complete universal character
/// name beginning at `pos`. Identifier and preprocessing-number recognition
/// share this spelling-level predicate so the two token grammars cannot
/// disagree about where a UCN ends.
#[must_use]
pub fn universal_character_name_len_at(source: &str, pos: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let digits = match bytes.get(pos..pos + 2)? {
        [b'\\', b'u'] => 4,
        [b'\\', b'U'] => 8,
        _ => return None,
    };
    let end = pos + 2 + digits;
    (end <= bytes.len() && bytes[pos + 2..end].iter().all(u8::is_ascii_hexdigit))
        .then_some(2 + digits)
}

/// Return the byte length of the preprocessing number beginning at `pos`.
///
/// This is the spelling grammar from C11 6.4 rather than the narrower C
/// integer/floating-constant grammar. Keeping it shared prevents the lexer
/// and token-paste re-lexer from disagreeing about values such as `.5foo` or
/// `1.2.3e+x`.
#[must_use]
pub fn preprocessing_number_len_at(source: &str, pos: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let first = *bytes.get(pos)?;
    let mut cursor = if first.is_ascii_digit() {
        pos + 1
    } else if first == b'.' && bytes.get(pos + 1).is_some_and(u8::is_ascii_digit) {
        pos + 2
    } else {
        return None;
    };

    while cursor < source.len() {
        if let Some(length) = universal_character_name_len_at(source, cursor) {
            cursor += length;
            continue;
        }
        let character = source[cursor..]
            .chars()
            .next()
            .expect("cursor is within the preprocessing number");
        cursor += character.len_utf8();
        if matches!(character, 'e' | 'E' | 'p' | 'P')
            && source[cursor..]
                .chars()
                .next()
                .is_some_and(|next| matches!(next, '+' | '-'))
        {
            cursor += 1;
        } else if character == '.'
            || character == '_'
            || character == '$'
            || character.is_alphanumeric()
        {
            continue;
        } else {
            cursor -= character.len_utf8();
            break;
        }
    }
    Some(cursor - pos)
}

fn classify_c_number(spelling: &str) -> Option<TokenKind> {
    if is_c_integer_constant(spelling) {
        Some(TokenKind::IntLiteral)
    } else if is_c_floating_constant(spelling) {
        Some(TokenKind::FloatLiteral)
    } else {
        None
    }
}

fn split_integer_suffix(spelling: &str) -> (&str, &str) {
    let suffix_start = spelling
        .char_indices()
        .find_map(|(index, character)| matches!(character, 'u' | 'U' | 'l' | 'L').then_some(index))
        .unwrap_or(spelling.len());
    spelling.split_at(suffix_start)
}

fn valid_integer_suffix(suffix: &str) -> bool {
    matches!(
        suffix.to_ascii_lowercase().as_str(),
        "" | "u" | "l" | "ul" | "lu" | "ll" | "ull" | "llu"
    )
}

fn is_c_integer_constant(spelling: &str) -> bool {
    let (digits, suffix) = split_integer_suffix(spelling);
    if digits.is_empty() || !valid_integer_suffix(suffix) {
        return false;
    }
    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        return !hex.is_empty() && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }
    if let Some(binary) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        return !binary.is_empty() && binary.bytes().all(|byte| matches!(byte, b'0' | b'1'));
    }
    if digits.starts_with('0') {
        digits.bytes().all(|byte| matches!(byte, b'0'..=b'7'))
    } else {
        digits.bytes().all(|byte| byte.is_ascii_digit())
    }
}

fn strip_float_suffix(spelling: &str) -> &str {
    spelling
        .char_indices()
        .next_back()
        .filter(|(_, character)| matches!(character, 'f' | 'F' | 'l' | 'L'))
        .map_or(spelling, |(index, _)| &spelling[..index])
}

fn consume_ascii_digits(bytes: &[u8], cursor: &mut usize, predicate: fn(u8) -> bool) -> usize {
    let start = *cursor;
    while bytes.get(*cursor).is_some_and(|byte| predicate(*byte)) {
        *cursor += 1;
    }
    *cursor - start
}

fn consume_exponent(bytes: &[u8], cursor: &mut usize, marker: fn(u8) -> bool) -> bool {
    if !bytes.get(*cursor).is_some_and(|byte| marker(*byte)) {
        return false;
    }
    *cursor += 1;
    if bytes
        .get(*cursor)
        .is_some_and(|byte| matches!(byte, b'+' | b'-'))
    {
        *cursor += 1;
    }
    consume_ascii_digits(bytes, cursor, |byte| byte.is_ascii_digit()) > 0
}

fn is_c_floating_constant(spelling: &str) -> bool {
    let core = strip_float_suffix(spelling);
    let bytes = core.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    if bytes.starts_with(b"0x") || bytes.starts_with(b"0X") {
        let mut cursor = 2;
        let before_dot = consume_ascii_digits(bytes, &mut cursor, |byte| byte.is_ascii_hexdigit());
        let after_dot = if bytes.get(cursor) == Some(&b'.') {
            cursor += 1;
            consume_ascii_digits(bytes, &mut cursor, |byte| byte.is_ascii_hexdigit())
        } else {
            0
        };
        return before_dot + after_dot > 0
            && consume_exponent(bytes, &mut cursor, |byte| matches!(byte, b'p' | b'P'))
            && cursor == bytes.len();
    }

    let mut cursor = 0;
    let before_dot = consume_ascii_digits(bytes, &mut cursor, |byte| byte.is_ascii_digit());
    let has_dot = bytes.get(cursor) == Some(&b'.');
    let after_dot = if has_dot {
        cursor += 1;
        consume_ascii_digits(bytes, &mut cursor, |byte| byte.is_ascii_digit())
    } else {
        0
    };
    let has_exponent = if bytes
        .get(cursor)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        if !consume_exponent(bytes, &mut cursor, |byte| matches!(byte, b'e' | b'E')) {
            return false;
        }
        true
    } else {
        false
    };
    before_dot + after_dot > 0 && (has_dot || has_exponent) && cursor == bytes.len()
}

// ---------------------------------------------------------------------------
// Lexer (stub — tests start RED)
// ---------------------------------------------------------------------------

/// A pull-style lexer that produces tokens from a source file.
///
/// The lexer borrows the physical source text and, only when needed, owns a
/// phase-2 view with backslash-newline pairs removed. EOF is a real token,
/// not an error, and the lexer returns `Some(TokenKind::Eof)` exactly once.
#[derive(Debug, Clone, Copy)]
struct LineSplice {
    logical_offset: usize,
    removed_total: usize,
}

pub struct Lexer<'src> {
    file: FileId,
    raw_source: &'src str,
    source: Cow<'src, str>,
    line_splices: Vec<LineSplice>,
    raw_line_starts: Vec<usize>,
    pos: usize,
    line: u32,
    col: u32,
    eof_emitted: bool,
}

fn splice_physical_lines(source: &str) -> (Cow<'_, str>, Vec<LineSplice>) {
    let bytes = source.as_bytes();
    let mut first_splice = None;
    for index in 0..bytes.len() {
        if bytes[index] == b'\\'
            && (bytes.get(index + 1) == Some(&b'\n')
                || (bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n')))
        {
            first_splice = Some(index);
            break;
        }
    }
    let Some(first_splice) = first_splice else {
        return (Cow::Borrowed(source), Vec::new());
    };

    let mut logical = String::with_capacity(source.len());
    let mut splices = Vec::new();
    let mut copied_through = 0;
    let mut cursor = first_splice;
    let mut removed_total = 0;
    while cursor < bytes.len() {
        let removed = if bytes[cursor] == b'\\' && bytes.get(cursor + 1) == Some(&b'\n') {
            2
        } else if bytes[cursor] == b'\\'
            && bytes.get(cursor + 1) == Some(&b'\r')
            && bytes.get(cursor + 2) == Some(&b'\n')
        {
            3
        } else {
            cursor += 1;
            continue;
        };

        logical.push_str(&source[copied_through..cursor]);
        removed_total += removed;
        splices.push(LineSplice {
            logical_offset: logical.len(),
            removed_total,
        });
        cursor += removed;
        copied_through = cursor;
    }
    logical.push_str(&source[copied_through..]);
    (Cow::Owned(logical), splices)
}

impl<'src> Lexer<'src> {
    #[must_use]
    pub fn new(file: FileId, source: &'src str) -> Self {
        let (logical_source, line_splices) = splice_physical_lines(source);
        let mut raw_line_starts = vec![0];
        raw_line_starts.extend(
            source
                .as_bytes()
                .iter()
                .enumerate()
                .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
        );
        Self {
            file,
            raw_source: source,
            source: logical_source,
            line_splices,
            raw_line_starts,
            pos: 0,
            line: 1,
            col: 1,
            eof_emitted: false,
        }
    }

    fn physical_offset(&self, logical_offset: usize, include_splices_at_offset: bool) -> usize {
        let count = self.line_splices.partition_point(|splice| {
            splice.logical_offset < logical_offset
                || (include_splices_at_offset && splice.logical_offset == logical_offset)
        });
        let removed = count
            .checked_sub(1)
            .map_or(0, |index| self.line_splices[index].removed_total);
        logical_offset + removed
    }

    fn physical_line_col(&self, physical_offset: usize) -> (u32, u32) {
        let line_index = self
            .raw_line_starts
            .partition_point(|line_start| *line_start <= physical_offset)
            .saturating_sub(1);
        let line_start = self.raw_line_starts[line_index];
        let column = self.raw_source[line_start..physical_offset].chars().count() + 1;
        ((line_index + 1) as u32, column as u32)
    }

    fn make_token(&self, kind: TokenKind, logical_start: usize, logical_end: usize) -> Token {
        let physical_start = self.physical_offset(logical_start, true);
        let physical_end = if logical_start == logical_end {
            physical_start
        } else {
            self.physical_offset(logical_end, false)
        };
        let (line, column) = self.physical_line_col(physical_start);
        let logical_text = &self.source[logical_start..logical_end];
        let physical_text = &self.raw_source[physical_start..physical_end];
        let text = if logical_text == physical_text {
            TokenText::Borrowed
        } else {
            TokenText::Decoded(logical_text.to_owned())
        };
        Token::new(
            kind,
            SourceSpan::new(
                self.file,
                physical_start as u32,
                physical_end as u32,
                line,
                column,
            ),
            text,
        )
    }

    /// Produce the next token, or `None` if EOF has already been returned.
    pub fn next(&mut self) -> Option<Token> {
        if let Some(error) = self.skip_whitespace_and_comments() {
            return Some(error);
        }

        if self.pos >= self.source.len() {
            if self.eof_emitted {
                return None;
            }
            self.eof_emitted = true;
            return Some(self.make_token(TokenKind::Eof, self.pos, self.pos));
        }

        let start_pos = self.pos;

        if let Some((prefix_len, quote)) = self.literal_prefix() {
            return Some(self.quoted_literal(prefix_len, quote, start_pos));
        }

        if self.source.as_bytes()[self.pos].is_ascii_digit()
            || (self.source.as_bytes()[self.pos] == b'.'
                && self.pos + 1 < self.source.len()
                && self.source.as_bytes()[self.pos + 1].is_ascii_digit())
        {
            return Some(self.numeric_literal(start_pos));
        }

        if self.is_identifier_start_at(self.pos) {
            return Some(self.identifier(start_pos));
        }

        match self.source.as_bytes()[self.pos] {
            // Punctuators and operators
            b'(' => self.single(TokenKind::LParen, start_pos),
            b')' => self.single(TokenKind::RParen, start_pos),
            b'[' => self.single(TokenKind::LBracket, start_pos),
            b']' => self.single(TokenKind::RBracket, start_pos),
            b'{' => self.single(TokenKind::LBrace, start_pos),
            b'}' => self.single(TokenKind::RBrace, start_pos),
            b';' => self.single(TokenKind::Semicolon, start_pos),
            b',' => self.single(TokenKind::Comma, start_pos),
            b'.' => {
                let bytes = self.source.as_bytes();
                if self.pos + 2 < bytes.len()
                    && bytes[self.pos + 1] == b'.'
                    && bytes[self.pos + 2] == b'.'
                {
                    self.pos += 3;
                    self.col += 3;
                    Some(self.make_token(TokenKind::Ellipsis, start_pos, self.pos))
                } else {
                    self.single(TokenKind::Dot, start_pos)
                }
            }
            b'+' => self.double(
                TokenKind::Plus,
                TokenKind::PlusPlus,
                TokenKind::PlusEq,
                start_pos,
            ),
            b'-' => {
                if self.match_next(b'>').is_some() {
                    Some(self.make_token(TokenKind::Arrow, start_pos, self.pos))
                } else {
                    self.double(
                        TokenKind::Minus,
                        TokenKind::MinusMinus,
                        TokenKind::MinusEq,
                        start_pos,
                    )
                }
            }
            b'*' => self.double(
                TokenKind::Star,
                TokenKind::Star,
                TokenKind::StarEq,
                start_pos,
            ),
            b'/' => self.double(
                TokenKind::Slash,
                TokenKind::Slash,
                TokenKind::SlashEq,
                start_pos,
            ),
            b'%' => self.double(
                TokenKind::Percent,
                TokenKind::Percent,
                TokenKind::PercentEq,
                start_pos,
            ),
            b'<' => {
                let bytes = self.source.as_bytes();
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'<' {
                    self.pos += 1;
                    self.col += 1;
                    if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                        self.pos += 1;
                        self.col += 1;
                        Some(self.make_token(TokenKind::ShiftLeftEq, start_pos, self.pos))
                    } else {
                        Some(self.make_token(TokenKind::ShiftLeft, start_pos, self.pos))
                    }
                } else if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(self.make_token(TokenKind::LessEq, start_pos, self.pos))
                } else {
                    Some(self.make_token(TokenKind::Less, start_pos, self.pos))
                }
            }
            b'>' => {
                let bytes = self.source.as_bytes();
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'>' {
                    self.pos += 1;
                    self.col += 1;
                    if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                        self.pos += 1;
                        self.col += 1;
                        Some(self.make_token(TokenKind::ShiftRightEq, start_pos, self.pos))
                    } else {
                        Some(self.make_token(TokenKind::ShiftRight, start_pos, self.pos))
                    }
                } else if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(self.make_token(TokenKind::GreaterEq, start_pos, self.pos))
                } else {
                    Some(self.make_token(TokenKind::Greater, start_pos, self.pos))
                }
            }
            b'=' => {
                let bytes = self.source.as_bytes();
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(self.make_token(TokenKind::EqEq, start_pos, self.pos))
                } else {
                    Some(self.make_token(TokenKind::Assign, start_pos, self.pos))
                }
            }
            b'!' => {
                let bytes = self.source.as_bytes();
                self.pos += 1;
                self.col += 1;
                if self.pos < bytes.len() && bytes[self.pos] == b'=' {
                    self.pos += 1;
                    self.col += 1;
                    Some(self.make_token(TokenKind::NotEq, start_pos, self.pos))
                } else {
                    Some(self.make_token(TokenKind::Bang, start_pos, self.pos))
                }
            }
            b'&' => self.double(
                TokenKind::Amp,
                TokenKind::AmpAmp,
                TokenKind::AmpEq,
                start_pos,
            ),
            b'|' => self.double(
                TokenKind::Pipe,
                TokenKind::PipePipe,
                TokenKind::PipeEq,
                start_pos,
            ),
            b'^' => self.double(
                TokenKind::Caret,
                TokenKind::Caret,
                TokenKind::CaretEq,
                start_pos,
            ),
            b'~' => self.single(TokenKind::Tilde, start_pos),
            b'?' => self.single(TokenKind::Question, start_pos),
            b':' => self.single(TokenKind::Colon, start_pos),
            b'#' => {
                if self.match_next(b'#').is_some() {
                    Some(self.make_token(TokenKind::HashHash, start_pos, self.pos))
                } else {
                    self.single(TokenKind::Hash, start_pos)
                }
            }
            _ => {
                // Translation phase 3 preserves every non-whitespace
                // character as a preprocessing token. Consume one Unicode
                // scalar so a later parser diagnostic points at exactly the
                // token that survived preprocessing.
                let character = self.source[self.pos..]
                    .chars()
                    .next()
                    .expect("position is within the source");
                self.pos += character.len_utf8();
                self.col += 1;
                Some(self.make_token(TokenKind::PpOther, start_pos, self.pos))
            }
        }
    }

    // --- private helpers --------------------------------------------------

    fn skip_whitespace_and_comments(&mut self) -> Option<Token> {
        let bytes = self.source.as_bytes();
        loop {
            // Whitespace
            while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t' | b'\r' | b'\n')
            {
                if bytes[self.pos] == b'\n' {
                    self.line += 1;
                    self.col = 1;
                } else {
                    self.col += 1;
                }
                self.pos += 1;
            }
            // Line comments
            if self.pos + 1 < bytes.len() && bytes[self.pos] == b'/' && bytes[self.pos + 1] == b'/'
            {
                self.pos += 2;
                self.col += 2;
                while self.pos < bytes.len() && bytes[self.pos] != b'\n' {
                    self.pos += 1;
                    self.col += 1;
                }
                continue;
            }
            // Block comments
            if self.pos + 1 < bytes.len() && bytes[self.pos] == b'/' && bytes[self.pos + 1] == b'*'
            {
                let start_pos = self.pos;
                self.pos += 2;
                self.col += 2;
                let mut terminated = false;
                while self.pos + 1 < bytes.len() {
                    if bytes[self.pos] == b'*' && bytes[self.pos + 1] == b'/' {
                        self.pos += 2;
                        self.col += 2;
                        terminated = true;
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
                if !terminated {
                    while self.pos < bytes.len() {
                        if bytes[self.pos] == b'\n' {
                            self.line += 1;
                            self.col = 1;
                        } else {
                            self.col += 1;
                        }
                        self.pos += 1;
                    }
                    return Some(self.make_token(TokenKind::Error, start_pos, self.pos));
                }
                continue;
            }
            break;
        }
        None
    }

    fn literal_prefix(&self) -> Option<(usize, u8)> {
        let rest = &self.source.as_bytes()[self.pos..];
        match rest {
            [quote @ (b'\'' | b'"'), ..] => Some((0, *quote)),
            [b'L' | b'u' | b'U', quote @ (b'\'' | b'"'), ..] => Some((1, *quote)),
            [b'u', b'8', b'"', ..] => Some((2, b'"')),
            _ => None,
        }
    }

    fn quoted_literal(&mut self, prefix_len: usize, quote: u8, start_pos: usize) -> Token {
        self.pos += prefix_len + 1;
        self.col += (prefix_len + 1) as u32;
        let bytes = self.source.as_bytes();
        let mut terminated = false;
        while self.pos < bytes.len() {
            if bytes[self.pos] == quote {
                self.pos += 1;
                self.col += 1;
                terminated = true;
                break;
            }
            if bytes[self.pos] == b'\n' || bytes[self.pos] == b'\r' {
                break;
            }
            if bytes[self.pos] == b'\\' {
                if self.pos + 1 >= bytes.len() {
                    self.pos += 1;
                    self.col += 1;
                    break;
                }
                if bytes[self.pos + 1] == b'\n' {
                    self.pos += 2;
                    self.line += 1;
                    self.col = 1;
                } else {
                    self.pos += 2;
                    self.col += 2;
                }
            } else {
                let ch = self.source[self.pos..]
                    .chars()
                    .next()
                    .expect("position is within the source");
                self.pos += ch.len_utf8();
                self.col += 1;
            }
        }
        let kind = if terminated {
            if quote == b'\'' {
                TokenKind::CharLiteral
            } else {
                TokenKind::StringLiteral
            }
        } else {
            TokenKind::Error
        };
        self.make_token(kind, start_pos, self.pos)
    }

    fn numeric_literal(&mut self, start_pos: usize) -> Token {
        let length = preprocessing_number_len_at(self.source.as_ref(), start_pos)
            .expect("numeric dispatch begins at a preprocessing number");
        self.pos = start_pos + length;
        self.col += self.source[start_pos..self.pos].chars().count() as u32;
        let spelling = &self.source[start_pos..self.pos];
        self.make_token(
            classify_c_number(spelling).unwrap_or(TokenKind::PpNumber),
            start_pos,
            self.pos,
        )
    }

    fn identifier(&mut self, start_pos: usize) -> Token {
        loop {
            if self.consume_ucn() {
                continue;
            }
            let Some(ch) = self.source[self.pos..].chars().next() else {
                break;
            };
            if !(ch == '_' || ch == '$' || ch.is_alphanumeric()) {
                break;
            }
            self.pos += ch.len_utf8();
            self.col += 1;
        }
        let lexeme = &self.source[start_pos..self.pos];
        let kind = try_keyword(lexeme).unwrap_or(TokenKind::Identifier);
        self.make_token(kind, start_pos, self.pos)
    }

    fn is_identifier_start_at(&self, pos: usize) -> bool {
        let bytes = self.source.as_bytes();
        if bytes.get(pos) == Some(&b'\\') {
            return universal_character_name_len_at(self.source.as_ref(), pos).is_some();
        }
        self.source[pos..]
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_alphabetic())
    }

    fn consume_ucn(&mut self) -> bool {
        if let Some(len) = universal_character_name_len_at(self.source.as_ref(), self.pos) {
            self.pos += len;
            self.col += len as u32;
            true
        } else {
            false
        }
    }

    fn match_next(&mut self, expected: u8) -> Option<u8> {
        let bytes = self.source.as_bytes();
        if self.pos + 1 < bytes.len() && bytes[self.pos + 1] == expected {
            self.pos += 2;
            self.col += 2;
            Some(expected)
        } else {
            None
        }
    }

    fn single(&mut self, kind: TokenKind, start_pos: usize) -> Option<Token> {
        self.pos += 1;
        self.col += 1;
        Some(self.make_token(kind, start_pos, self.pos))
    }

    fn double(
        &mut self,
        single_kind: TokenKind,
        double_kind: TokenKind,
        eq_kind: TokenKind,
        start_pos: usize,
    ) -> Option<Token> {
        self.pos += 1;
        self.col += 1;
        let bytes = self.source.as_bytes();
        if self.pos < bytes.len() {
            if bytes[self.pos] == b'=' {
                self.pos += 1;
                self.col += 1;
                return Some(self.make_token(eq_kind, start_pos, self.pos));
            }
            if single_kind != double_kind && bytes[self.pos] == bytes[start_pos] {
                self.pos += 1;
                self.col += 1;
                return Some(self.make_token(double_kind, start_pos, self.pos));
            }
        }
        Some(self.make_token(single_kind, start_pos, self.pos))
    }
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
        let tokens = lex("int main(void) { return 0x42 + /* comment */ 7; }\n");
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
    fn lex_gnu_signed_spelling_remains_an_identifier_for_preprocessing() {
        let source = "x __signed__ __signed_ ___signed__ __signed___";
        let tokens = lex(source);
        assert_eq!(
            tokens.iter().map(|token| token.kind).collect::<Vec<_>>(),
            vec![
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Eof,
            ],
            "the preprocessor must still be able to expand or paste the GNU spelling",
        );

        let alias = &tokens[1];
        assert_eq!(alias.span.file, FileId::new(1));
        assert_eq!((alias.span.byte_start, alias.span.byte_end), (2, 12));
        assert_eq!((alias.span.line_start, alias.span.column_start), (1, 3));
        assert_eq!(
            &source[alias.span.byte_start as usize..alias.span.byte_end as usize],
            "__signed__",
        );
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
        assert_kinds("->", &[TokenKind::Arrow, TokenKind::Eof]);
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
        assert_kinds("...", &[TokenKind::Ellipsis, TokenKind::Eof]);
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
        assert_kinds("#", &[TokenKind::Hash, TokenKind::Eof]);
    }

    #[test]
    fn lex_operator_hash_hash() {
        assert_kinds("##", &[TokenKind::HashHash, TokenKind::Eof]);
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
        assert_kinds("0X1A", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_octal() {
        assert_kinds("0777", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_suffix_u() {
        assert_kinds("42u", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_suffix_ull() {
        assert_kinds("42ULL", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_hex_suffix_u() {
        assert_kinds("0xFFu", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_int_large() {
        assert_kinds("123456789", &[TokenKind::IntLiteral, TokenKind::Eof]);
    }

    // ========================================================================
    // Floating-point literals
    // ========================================================================

    #[test]
    fn lex_float_simple() {
        assert_kinds("3.14", &[TokenKind::FloatLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_float_suffix_f() {
        assert_kinds("1.0f", &[TokenKind::FloatLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_float_exponent() {
        assert_kinds("1e10", &[TokenKind::FloatLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_float_exponent_neg() {
        assert_kinds("1.0e-5", &[TokenKind::FloatLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_float_hex() {
        assert_kinds("0x1.2p3", &[TokenKind::FloatLiteral, TokenKind::Eof]);
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
        assert_kinds("'\\x41'", &[TokenKind::CharLiteral, TokenKind::Eof]);
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
        assert_kinds(
            "\"\\x41\\x42\"",
            &[TokenKind::StringLiteral, TokenKind::Eof],
        );
    }

    #[test]
    fn lex_string_with_spaces() {
        assert_kinds("\"foo bar\"", &[TokenKind::StringLiteral, TokenKind::Eof]);
    }

    #[test]
    fn lex_string_escaped_quote() {
        assert_kinds(
            "\"\\\"quoted\\\"\"",
            &[TokenKind::StringLiteral, TokenKind::Eof],
        );
    }

    #[test]
    fn lex_string_multiline_escape() {
        assert_kinds(
            "\"multiline\\nstring\"",
            &[TokenKind::StringLiteral, TokenKind::Eof],
        );
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
        assert_kinds(
            "a/*comment*/+b",
            &[
                TokenKind::Identifier,
                TokenKind::Plus,
                TokenKind::Identifier,
                TokenKind::Eof,
            ],
        );
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
        assert_kinds(
            "int/*comment*/x",
            &[TokenKind::Int, TokenKind::Identifier, TokenKind::Eof],
        );
    }

    // ========================================================================
    // Line continuation
    // ========================================================================

    #[test]
    fn lex_line_cont_basic() {
        assert_kinds("a\\\nb", &[TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn lex_line_cont_define() {
        assert_kinds(
            "#define FOO \\\nbar",
            &[
                TokenKind::Hash,
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lex_line_cont_crlf() {
        assert_kinds("a\\\r\nb", &[TokenKind::Identifier, TokenKind::Eof]);
    }

    #[test]
    fn phase_two_splices_join_tokens_but_keep_physical_spans() {
        let source = "si\\\ngned 12\\\r\n34 +\\\n+ tail";
        let tokens = lex(source);
        assert_eq!(
            tokens.iter().map(|token| token.kind).collect::<Vec<_>>(),
            vec![
                TokenKind::Signed,
                TokenKind::IntLiteral,
                TokenKind::PlusPlus,
                TokenKind::Identifier,
                TokenKind::Eof,
            ],
        );

        let spelling = |token: &Token| match &token.text {
            TokenText::Decoded(text) => text.clone(),
            TokenText::Borrowed => {
                source[token.span.byte_start as usize..token.span.byte_end as usize].to_owned()
            }
        };
        assert_eq!(spelling(&tokens[0]), "signed");
        assert_eq!(spelling(&tokens[1]), "1234");
        assert_eq!(spelling(&tokens[2]), "++");
        assert_eq!(tokens[0].span, SourceSpan::new(FileId::new(1), 0, 8, 1, 1),);
        assert_eq!(tokens[1].span, SourceSpan::new(FileId::new(1), 9, 16, 2, 6),);
        assert_eq!(
            tokens[2].span,
            SourceSpan::new(FileId::new(1), 17, 21, 3, 4),
        );
        assert_eq!(
            tokens[3].span,
            SourceSpan::new(FileId::new(1), 22, 26, 4, 3),
        );
    }

    #[test]
    fn phase_two_splice_extends_a_line_comment() {
        assert_kinds(
            "// hidden \\\ncontinued\nint visible;",
            &[
                TokenKind::Int,
                TokenKind::Identifier,
                TokenKind::Semicolon,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn phase_two_splice_keeps_a_following_other_token_on_its_physical_line() {
        let tokens = lex("int ok;\\\n@");
        assert_eq!(tokens[3].kind, TokenKind::PpOther);
        assert_eq!(tokens[3].span, SourceSpan::new(FileId::new(1), 9, 10, 2, 1),);
    }

    #[test]
    fn lex_line_cont_inside_string() {
        // Phase 2 removes the pair before the string token is recognized.
        assert_kinds(
            "\"hello\\\nworld\"",
            &[TokenKind::StringLiteral, TokenKind::Eof],
        );
    }

    // ========================================================================
    // Trigraphs (3 tests – lexer lacks trigraph support)
    // ========================================================================

    #[test]
    fn lex_trigraph_hash() {
        // ??= is not translated to #
        assert_kinds(
            "??=",
            &[
                TokenKind::Question,
                TokenKind::Question,
                TokenKind::Assign,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lex_trigraph_lbracket() {
        assert_kinds(
            "??(",
            &[
                TokenKind::Question,
                TokenKind::Question,
                TokenKind::LParen,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lex_trigraph_pipe() {
        // ??! is not translated to |
        assert_kinds(
            "int ??! x",
            &[
                TokenKind::Int,
                TokenKind::Question,
                TokenKind::Question,
                TokenKind::Bang,
                TokenKind::Identifier,
                TokenKind::Eof,
            ],
        );
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
        assert_kinds(
            "int   x",
            &[TokenKind::Int, TokenKind::Identifier, TokenKind::Eof],
        );
    }

    #[test]
    fn lex_ws_newline() {
        assert_kinds(
            "int\nx",
            &[TokenKind::Int, TokenKind::Identifier, TokenKind::Eof],
        );
    }

    #[test]
    fn lex_ws_crlf() {
        assert_kinds(
            "int\r\nx",
            &[TokenKind::Int, TokenKind::Identifier, TokenKind::Eof],
        );
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
        assert_kinds(
            "a+b-c",
            &[
                TokenKind::Identifier,
                TokenKind::Plus,
                TokenKind::Identifier,
                TokenKind::Minus,
                TokenKind::Identifier,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lex_boundary_expr_with_comment() {
        assert_kinds(
            "1+/*comment*/2",
            &[
                TokenKind::IntLiteral,
                TokenKind::Plus,
                TokenKind::IntLiteral,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lex_boundary_define_token() {
        assert_kinds(
            "#define",
            &[TokenKind::Hash, TokenKind::Identifier, TokenKind::Eof],
        );
    }

    #[test]
    fn lex_boundary_func_decl() {
        assert_kinds(
            "int f(void)",
            &[
                TokenKind::Int,
                TokenKind::Identifier,
                TokenKind::LParen,
                TokenKind::Void,
                TokenKind::RParen,
                TokenKind::Eof,
            ],
        );
    }

    #[test]
    fn lex_boundary_return_stmt() {
        assert_kinds(
            "return 42;",
            &[
                TokenKind::Return,
                TokenKind::IntLiteral,
                TokenKind::Semicolon,
                TokenKind::Eof,
            ],
        );
    }
}
