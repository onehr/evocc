//! C preprocessor: macro expansion, directives, token stream

use std::collections::HashMap;

#[allow(unused_imports)]
use evocc_lex::{Lexer, Token, TokenKind, TokenText};

// ---------------------------------------------------------------------------
// Macro definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Macro {
    pub name: String,
    pub body: Vec<Token>,
    pub is_function: bool,
    pub params: Vec<String>,
}

impl Macro {
    pub fn new(name: String, body: Vec<Token>, is_function: bool, params: Vec<String>) -> Self {
        Self { name, body, is_function, params }
    }

    pub fn arity(&self) -> usize {
        self.params.len()
    }
}

// ---------------------------------------------------------------------------
// MacroMap: symbol table for macro definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct MacroMap {
    macros: HashMap<String, Macro>,
}

impl MacroMap {
    pub fn new() -> Self {
        Self { macros: HashMap::new() }
    }

    pub fn define(&mut self, name: String, body: Vec<Token>, params: Vec<String>) {
        let is_function = !params.is_empty();
        self.macros.insert(
            name.clone(),
            Macro::new(name, body, is_function, params),
        );
    }

    pub fn undefine(&mut self, name: &str) {
        self.macros.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&Macro> {
        self.macros.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    /// Expand an object-like macro. Returns `None` if the macro is not
    /// defined or is function-like.
    pub fn expand_object_like(&self, name: &str) -> Option<Vec<Token>> {
        let m = self.get(name)?;
        if m.is_function {
            return None;
        }
        Some(m.body.clone())
    }

    /// Expand a function-like macro with the given arguments. Returns `None`
    /// if the macro is not defined, is object-like, or the arity does not match.
    pub fn expand_function_like(&self, name: &str, args: &[Vec<Token>]) -> Option<Vec<Token>> {
        let m = self.get(name)?;
        if !m.is_function {
            return None;
        }
        if args.len() != m.arity() {
            return None;
        }
        // Stub: simple substitution without parameter replacement.
        // Full replacement will be implemented when the preprocessor is built.
        let _ = args;
        Some(m.body.clone())
    }
}

// ---------------------------------------------------------------------------
// TokenStream: position-cursor over a token sequence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct TokenStream {
    tokens: Vec<Token>,
    pos: usize,
}

impl TokenStream {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    pub fn next(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos)?;
        self.pos += 1;
        Some(tok.clone())
    }

    pub fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        let tok = self.peek()?;
        if tok.kind == kind {
            self.next()
        } else {
            None
        }
    }

    pub fn is_done(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn into_vec(self) -> Vec<Token> {
        self.tokens
    }
}

// ---------------------------------------------------------------------------
// Preprocessor (stub)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct Preprocessor {
    macros: MacroMap,
    include_guard_seen: bool,
}

impl Preprocessor {
    pub fn new() -> Self {
        Self { macros: MacroMap::new(), include_guard_seen: false }
    }

    pub fn macros(&self) -> &MacroMap {
        &self.macros
    }

    pub fn macros_mut(&mut self) -> &mut MacroMap {
        &mut self.macros
    }

    pub fn pragma_once(&mut self) {
        self.include_guard_seen = true;
    }

    pub fn has_include_guard(&self) -> bool {
        self.include_guard_seen
    }

    /// Run the full preprocessor pipeline. **NOT_IMPLEMENTED** — existing
    /// tests that depend on the preprocessor stage start RED.
    pub fn process(&mut self, _tokens: TokenStream) -> TokenStream {
        todo!("preprocessor pipeline is NOT_IMPLEMENTED — tests start RED")
    }
}

impl Default for Preprocessor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_common::{FileId, SourceSpan};

    /// Build a token with a sentinel span for use in tests.
    fn dummy_token(kind: TokenKind, text: &str) -> Token {
        Token::new(
            kind,
            SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
            TokenText::Decoded(text.to_owned()),
        )
    }

    /// Object-like macro definition (e.g. `#define FOO bar`) expands to its
    /// replacement list.
    #[test]
    fn cpp_object_macro_is_textual() {
        let mut map = MacroMap::new();
        let body = vec![dummy_token(TokenKind::Identifier, "bar")];
        map.define("FOO".to_owned(), body.clone(), vec![]);

        let result = map.expand_object_like("FOO");
        assert!(result.is_some());
        let expanded = result.unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0], body[0]);
    }

    /// Function-like macros require the correct number of arguments. A call
    /// with the wrong count is rejected.
    #[test]
    fn cpp_function_macro_arity_enforced() {
        let mut map = MacroMap::new();
        let body = vec![
            dummy_token(TokenKind::Identifier, "a"),
            dummy_token(TokenKind::Plus, "+"),
            dummy_token(TokenKind::Identifier, "b"),
        ];
        map.define(
            "ADD".to_owned(),
            body,
            vec!["a".to_owned(), "b".to_owned()],
        );

        // Correct arity: 2 args → succeeds.
        let arg0 = vec![dummy_token(TokenKind::IntLiteral, "1")];
        let arg1 = vec![dummy_token(TokenKind::IntLiteral, "2")];
        let ok = map.expand_function_like("ADD", &[arg0, arg1]);
        assert!(ok.is_some());

        // Wrong arity: 1 arg → fails.
        let bad = map.expand_function_like(
            "ADD",
            &[vec![dummy_token(TokenKind::IntLiteral, "1")]],
        );
        assert!(bad.is_none());

        // Wrong arity: 3 args → fails.
        let bad3 = map.expand_function_like(
            "ADD",
            &[
                vec![dummy_token(TokenKind::IntLiteral, "1")],
                vec![dummy_token(TokenKind::IntLiteral, "2")],
                vec![dummy_token(TokenKind::IntLiteral, "3")],
            ],
        );
        assert!(bad3.is_none());
    }

    /// `#pragma once` is tracked as a flag so include-guard logic can skip
    /// re-processing the file.
    #[test]
    fn cpp_include_guard_is_effective() {
        let mut pp = Preprocessor::new();

        // Fresh preprocessor has no guard flag set.
        assert!(!pp.has_include_guard());

        // Simulate processing `#pragma once`.
        pp.pragma_once();
        assert!(pp.has_include_guard());

        // Idempotent: setting it again stays true.
        pp.pragma_once();
        assert!(pp.has_include_guard());
    }
}
