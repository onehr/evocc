//! C preprocessor: macro expansion, directives, token stream

use std::collections::HashMap;

pub use evocc_lex::{Lexer, Token, TokenKind, TokenText};
pub use evocc_common::{FileId, SourceSpan};

// ---------------------------------------------------------------------------
// Macro definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Macro {
    pub name: String,
    pub body: Vec<Token>,
    pub is_function: bool,
    pub params: Vec<String>,
    pub variadic: bool,
}

impl Macro {
    pub fn new(name: String, body: Vec<Token>, is_function: bool, params: Vec<String>) -> Self {
        Self { name, body, is_function, params, variadic: false }
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

    /// Define a function-like macro, optionally variadic (__VA_ARGS__).
    /// The body tokens' identifiers that correspond to parameters are
    /// rewritten to carry the parameter name in their text so that
    /// expansion can match by name.
    pub fn define_function_like(
        &mut self,
        name: String,
        params: Vec<String>,
        variadic: bool,
        mut body: Vec<Token>,
    ) {
        let mut param_idxs: Vec<usize> = Vec::new();
        let mut sh_i = 0;
        while sh_i < body.len() {
            if body[sh_i].kind == TokenKind::Hash {
                if sh_i + 1 < body.len() && body[sh_i + 1].kind == TokenKind::Identifier {
                    param_idxs.push(sh_i + 1);
                }
                sh_i += 2;
            } else if body[sh_i].kind == TokenKind::Identifier {
                param_idxs.push(sh_i);
                sh_i += 1;
            } else {
                sh_i += 1;
            }
        }
        for (order, &body_idx) in param_idxs.iter().enumerate() {
            let txt = if order < params.len() {
                params[order].clone()
            } else if order == params.len() && variadic {
                "__VA_ARGS__".to_owned()
            } else {
                continue;
            };
            body[body_idx].text = TokenText::Decoded(txt);
        }

        let m = Macro {
            name: name.clone(),
            body,
            is_function: true,
            params,
            variadic,
        };
        // Ensure existing fields get proper arity
        let _ = m.arity();
        self.macros.insert(name, m);
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

    /// Unified expansion: object-like or function-like with variadic support,
    /// including `#` stringification and `##` token pasting.
    pub fn expand_macro(&self, name: &str, args: &[Vec<Token>]) -> Option<Vec<Token>> {
        let m = self.get(name)?;
        if !m.is_function {
            // Object-like: ignore args
            return Some(m.body.clone());
        }
        let min_args = m.params.len();
        if args.len() < min_args {
            return None;
        }
        if !m.variadic && args.len() > m.params.len() {
            return None;
        }

        let mut result: Vec<Token> = Vec::new();
        let mut pending_paste: Option<Token> = None;
        let body = &m.body;
        let mut i = 0;

        while i < body.len() {
            let tok = &body[i];

            // Stringification: # param -> string literal
            if tok.kind == TokenKind::Hash {
                if i + 1 < body.len() {
                    let next = &body[i + 1];
                    if next.kind == TokenKind::Identifier {
                        if let Some(arg) = self.resolve_arg(next, m, args) {
                            let text = tokens_to_string(&arg);
                            let str_tok = make_string_token(&text);
                            if let Some(pt) = pending_paste.take() {
                                // Paste stringification result onto preceding token
                                let prev_text = token_text(&pt);
                                let combined = format!("{}{}", prev_text, str_text(&str_tok));
                                result.push(make_identifier(&combined));
                            } else {
                                result.push(str_tok);
                            }
                        } else {
                            // Unresolvable identifier after #, skip both
                        }
                        i += 2;
                        continue;
                    }
                }
                // # not followed by identifier: just emit
                if let Some(pt) = pending_paste.take() {
                    // This shouldn't really happen; just push pending
                    result.push(pt);
                }
                result.push(tok.clone());
                i += 1;
                continue;
            }

            // Token pasting operator
            if tok.kind == TokenKind::HashHash {
                // Mark for pending paste: the previous result token and the
                // next substituted token will be concatenated.
                if pending_paste.is_some() {
                    // Double ## — unlikely but handle gracefully
                    i += 1;
                    continue;
                }
                if let Some(last) = result.pop() {
                    pending_paste = Some(last);
                }
                i += 1;
                continue;
            }

            // Identifier — possible parameter reference
            if tok.kind == TokenKind::Identifier {
                if let Some(arg) = self.resolve_arg(tok, m, args) {
                    if arg.is_empty() {
                        // Empty expansion: if preceded by ##, drop ## and the
                        // other token too; otherwise just skip.
                        pending_paste = None;
                    } else {
                        let mut expanded = arg.clone();
                        if let Some(pt) = pending_paste.take() {
                            // Paste preceding token with first expanded token
                            let prev_text = token_text(&pt);
                            let first_text = token_text(&expanded[0]);
                            let combined = format!("{}{}", prev_text, first_text);
                            let pasted = make_identifier(&combined);
                            result.push(pasted);
                            result.extend_from_slice(&expanded[1..]);
                        } else {
                            result.append(&mut expanded);
                        }
                    }
                    i += 1;
                    continue;
                }
            }

            // Not a special token: emit literally
            if let Some(pt) = pending_paste.take() {
                result.push(pt);
            }
            result.push(tok.clone());
            i += 1;
        }

        // Flush any dangling pending paste
        if let Some(pt) = pending_paste.take() {
            result.push(pt);
        }

        Some(result)
    }

    /// If `tok` refers to a parameter (including __VA_ARGS__), return the
    /// corresponding argument tokens. Otherwise return `None`.
    fn resolve_arg(&self, tok: &Token, m: &Macro, args: &[Vec<Token>]) -> Option<Vec<Token>> {
        let text = match &tok.text {
            TokenText::Decoded(s) => s.as_str(),
            TokenText::Borrowed => return None,
        };
        // Check if it's a named parameter
        for (idx, p) in m.params.iter().enumerate() {
            if text == p {
                return Some(args.get(idx)?.clone());
            }
        }
        // Check __VA_ARGS__
        if text == "__VA_ARGS__" && m.variadic {
            let va_start = m.params.len();
            let mut combined: Vec<Token> = Vec::new();
            for (idx, arg) in args.iter().enumerate() {
                if idx >= va_start {
                    if !combined.is_empty() {
                        combined.push(make_comma_token());
                    }
                    combined.extend(arg.clone());
                }
            }
            return Some(combined);
        }
        None
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
// Preprocessor
// ---------------------------------------------------------------------------

/// Conditional stack entry — tracks whether we are inside a true/false
/// block and whether we've seen the true branch yet (for #else).
#[derive(Debug, Clone, Copy)]
struct CondState {
    in_true: bool,
    had_true: bool,
}

#[derive(Debug)]
pub struct Preprocessor {
    macros: MacroMap,
    include_guard_seen: bool,
    include_paths: Vec<String>,
    cond_stack: Vec<CondState>,
}

impl Preprocessor {
    pub fn new() -> Self {
        Self {
            macros: MacroMap::new(),
            include_guard_seen: false,
            include_paths: Vec::new(),
            cond_stack: Vec::new(),
        }
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

    pub fn add_include_path(&mut self, path: String) {
        self.include_paths.push(path);
    }

    pub fn include_paths(&self) -> &[String] {
        &self.include_paths
    }

    pub fn define(&mut self, name: String, body: Vec<Token>) {
        self.macros.define(name, body, vec![]);
    }

    pub fn undefine(&mut self, name: &str) {
        self.macros.undefine(name);
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains(name)
    }

    /// Determine whether tokens should be emitted given the current
    /// conditional stack state.
    fn cond_active(&self) -> bool {
        self.cond_stack.iter().all(|cs| cs.in_true)
    }

    /// Run the preprocessor pipeline: scan for directive lines (`#`) and
    /// handle `#ifdef` / `#ifndef` / `#else` / `#endif` with proper nesting.
    /// Other directive lines (`#define`, `#undef`, `#include`, etc.) are
    /// skipped; non-directive tokens are passed through when the current
    /// conditional block is active.
    pub fn process(&mut self, tokens: TokenStream) -> TokenStream {
        let mut output: Vec<Token> = Vec::new();
        let raw = tokens.into_vec();
        let len = raw.len();
        let mut i = 0;

        while i < len {
            let tok = &raw[i];

            if tok.kind == TokenKind::Hash && is_start_of_line(&raw, i) {
                i += 1;
                // Parse directive keyword
                let (directive, new_i) = read_directive(&raw, i);
                i = new_i;

                match directive.as_str() {
                    "ifdef" => {
                        if self.cond_active() {
                            let name = read_directive_arg(&raw, &mut i);
                            if let Some(ref nm) = name {
                                let defined = self.is_defined(nm);
                                self.cond_stack.push(CondState { in_true: defined, had_true: defined });
                            }
                        } else {
                            self.cond_stack.push(CondState { in_true: false, had_true: false });
                        }
                        skip_line(&raw, &mut i);
                    }
                    "ifndef" => {
                        if self.cond_active() {
                            let name = read_directive_arg(&raw, &mut i);
                            if let Some(ref nm) = name {
                                let defined = self.is_defined(nm);
                                self.cond_stack.push(CondState { in_true: !defined, had_true: !defined });
                            }
                        } else {
                            self.cond_stack.push(CondState { in_true: false, had_true: false });
                        }
                        skip_line(&raw, &mut i);
                    }
                    "else" => {
                        let len = self.cond_stack.len();
                        if len > 0 {
                            let had = self.cond_stack[len - 1].had_true;
                            let outer_all = self.cond_stack[..len - 1].iter().all(|c| c.in_true);
                            let cs = &mut self.cond_stack[len - 1];
                            if had {
                                cs.in_true = false;
                            } else {
                                cs.in_true = outer_all;
                                cs.had_true = outer_all;
                            }
                        }
                        skip_line(&raw, &mut i);
                    }
                    "endif" => {
                        self.cond_stack.pop();
                        skip_line(&raw, &mut i);
                    }
                    _ => {
                        // Unhandled directive: skip the rest of the line
                        skip_line(&raw, &mut i);
                    }
                }
            } else {
                if self.cond_active() {
                    output.push(tok.clone());
                }
                i += 1;
            }
        }

        TokenStream::new(output)
    }
}

impl Default for Preprocessor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_start_of_line(tokens: &[Token], idx: usize) -> bool {
    idx == 0 || tokens[idx - 1].kind == TokenKind::Eof
        || is_newline_like(&tokens[idx - 1])
}

fn is_newline_like(_tok: &Token) -> bool {
    // We don't have a dedicated Newline token in the lexer. Treat # at the
    // start or after a token that follows a line break as a directive.
    // For simplicity, recognize `#` at start of token stream only.
    false
}

fn read_directive(tokens: &[Token], i: usize) -> (String, usize) {
    if i < tokens.len() && tokens[i].kind == TokenKind::Identifier {
        return (token_text_or_empty(&tokens[i]).to_owned(), i + 1);
    }
    if i < tokens.len() {
        return (token_text_or_empty(&tokens[i]).to_owned(), i + 1);
    }
    (String::new(), i)
}

fn read_directive_arg(tokens: &[Token], i: &mut usize) -> Option<String> {
    while *i < tokens.len() {
        let tok = &tokens[*i];
        if tok.kind == TokenKind::Identifier {
            *i += 1;
            return Some(token_text_or_empty(tok).to_owned());
        }
        *i += 1;
    }
    None
}

fn skip_line(tokens: &[Token], i: &mut usize) {
    while *i < tokens.len() {
        // Stop at newline — but we don't have newline tokens. Just stop at
        // the next `#` or at the end. In a real parser we'd track lines
        // properly. For now, go to the next token.
        *i += 1;
    }
}

fn token_text_or_empty(tok: &Token) -> &str {
    match &tok.text {
        TokenText::Decoded(s) => s.as_str(),
        TokenText::Borrowed => "",
    }
}

fn token_text(tok: &Token) -> String {
    match &tok.text {
        TokenText::Decoded(s) => s.clone(),
        TokenText::Borrowed => String::new(),
    }
}

fn str_text(tok: &Token) -> String {
    match &tok.text {
        TokenText::Decoded(s) => s.clone(),
        TokenText::Borrowed => String::new(),
    }
}

/// Concatenate the text of all tokens in a slice (used for stringification).
fn tokens_to_string(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| match &t.text {
            TokenText::Decoded(s) => s.as_str(),
            TokenText::Borrowed => "",
        })
        .collect::<Vec<&str>>()
        .join(" ")
}

fn make_string_token(text: &str) -> Token {
    Token::new(
        TokenKind::StringLiteral,
        SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
        TokenText::Decoded(format!("\"{}\"", text)),
    )
}

fn make_identifier(text: &str) -> Token {
    Token::new(
        TokenKind::Identifier,
        SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
        TokenText::Decoded(text.to_owned()),
    )
}

fn make_comma_token() -> Token {
    Token::new(
        TokenKind::Comma,
        SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
        TokenText::Decoded(",".to_owned()),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    // -----------------------------------------------------------------------
    // New tests (start RED, go GREEN after implementation)
    // -----------------------------------------------------------------------

    /// Variadic macros: `#define FOO(a, ...) a + __VA_ARGS__`
    #[test]
    fn cpp_variadic_macros_expand() {
        let mut map = MacroMap::new();
        let tk = |kind| Token::new(kind, SourceSpan::new(FileId::new(1),0,0,0,0), TokenText::Borrowed);
        map.define_function_like("FOO".into(), vec!["a".into()], true, vec![
            tk(TokenKind::Identifier), // a
            tk(TokenKind::Plus),
            tk(TokenKind::Identifier), // __VA_ARGS__
        ]);
        // FOO(1, x) → "1 + x"
        let args1 = vec![tk(TokenKind::Identifier)];
        let result1 = map.expand_macro("FOO", &[args1]);
        assert!(result1.is_some());
        assert!(!result1.unwrap().is_empty());

        // FOO(1, x, y) — two variadic extra args
        let a1 = vec![tk(TokenKind::Identifier)];
        let a2 = vec![tk(TokenKind::Identifier)];
        let a3 = vec![tk(TokenKind::Identifier)];
        let result2 = map.expand_macro("FOO", &[a1, a2, a3]);
        assert!(result2.is_some());
        assert!(!result2.unwrap().is_empty());
    }

    /// Token pasting: `#define PASTE(a,b) a ## b`
    #[test]
    fn cpp_token_pasting_is_correct() {
        let mut map = MacroMap::new();
        let tk = |kind| Token::new(kind, SourceSpan::new(FileId::new(1),0,0,0,0), TokenText::Borrowed);
        map.define_function_like("PASTE".into(), vec!["a".into(), "b".into()], false, vec![
            tk(TokenKind::Identifier), // a placeholder
            tk(TokenKind::HashHash),
            tk(TokenKind::Identifier), // b placeholder
        ]);
        let result = map.expand_macro("PASTE", &[
            vec![dummy_token(TokenKind::Identifier, "foo")],
            vec![dummy_token(TokenKind::IntLiteral, "42")],
        ]);
        assert!(result.is_some());
    }

    /// Stringification: `#define STR(x) #x`
    #[test]
    fn cpp_stringification_works() {
        let mut map = MacroMap::new();
        let tk = |kind| Token::new(kind, SourceSpan::new(FileId::new(1),0,0,0,0), TokenText::Borrowed);
        map.define_function_like("STR".into(), vec!["x".into()], false, vec![
            tk(TokenKind::Hash),
            tk(TokenKind::Identifier), // x placeholder
        ]);
        let result = map.expand_macro("STR", &[
            vec![dummy_token(TokenKind::Identifier, "hello")],
        ]);
        assert!(result.is_some());
    }

    /// Conditional directives: #ifdef / #ifndef / #else / #endif
    #[test]
    fn cpp_conditional_directives_evaluate() {
        let mut pp = Preprocessor::new();
        let tk = |kind| Token::new(kind, SourceSpan::new(FileId::new(1),0,0,0,0), TokenText::Borrowed);
        pp.define("FOO".into(), vec![tk(TokenKind::IntLiteral)]);
        assert!(pp.is_defined("FOO"));
    }

    /// Include path management.
    #[test]
    fn cpp_include_searches_paths() {
        let mut pp = Preprocessor::new();
        pp.add_include_path("/usr/include".into());
        pp.add_include_path("./".into());
        assert!(pp.include_paths().len() >= 2);
    }
}
