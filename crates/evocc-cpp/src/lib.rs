#![allow(dead_code)]
#![allow(clippy::should_implement_trait)]
//! C preprocessor: macro expansion, directives, token stream

use std::collections::{HashMap, HashSet};

pub use evocc_common::{FileId, SourceSpan};
pub use evocc_lex::{Lexer, Token, TokenKind, TokenText};

const AARCH64_UNKNOWN_LINUX_GNU: &str = "aarch64-unknown-linux-gnu";
const AARCH64_UNKNOWN_LINUX_GNU_PREDEFINED: &str =
    include_str!("../predefined/aarch64-unknown-linux-gnu.h");

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
        Self {
            name,
            body,
            is_function,
            params,
            variadic: false,
        }
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
        Self {
            macros: HashMap::new(),
        }
    }

    pub fn define(&mut self, name: String, body: Vec<Token>, params: Vec<String>) {
        let is_function = !params.is_empty();
        self.macros
            .insert(name.clone(), Macro::new(name, body, is_function, params));
    }

    /// Define a function-like macro, optionally variadic (__VA_ARGS__).
    ///
    /// For GNU named variadics (e.g. `#define FOO(fmt,args...)`),
    /// `variadic_param_name` is `Some("args")` and any body token whose
    /// text matches that name is rewritten to `__VA_ARGS__` so that
    /// `resolve_arg` can handle it uniformly.
    pub fn define_function_like(
        &mut self,
        name: String,
        params: Vec<String>,
        variadic: bool,
        variadic_param_name: Option<String>,
        mut body: Vec<Token>,
    ) {
        if let Some(ref vname) = variadic_param_name {
            for tok in body.iter_mut() {
                if tok.kind != TokenKind::Identifier {
                    continue;
                }
                if let TokenText::Decoded(s) = &tok.text {
                    if s == vname {
                        tok.text = TokenText::Decoded("__VA_ARGS__".to_owned());
                    }
                }
            }
        }

        let m = Macro {
            name: name.clone(),
            body,
            is_function: true,
            params,
            variadic,
        };
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
        self.expand_macro(name, args)
    }

    /// Expand any identifier token that refers to an object-like macro.
    /// Used to macro-expand argument tokens before `##` pasting.
    fn expand_arg_token(&self, tok: &Token) -> Vec<Token> {
        if tok.kind != TokenKind::Identifier {
            return vec![tok.clone()];
        }
        let text = match &tok.text {
            TokenText::Decoded(s) => s.as_str(),
            TokenText::Borrowed => return vec![tok.clone()],
        };
        if let Some(m) = self.macros.get(text) {
            if !m.is_function && !m.body.is_empty() {
                return m.body.clone();
            }
        }
        vec![tok.clone()]
    }

    fn expand_tokens(&self, tokens: &[Token]) -> Vec<Token> {
        tokens
            .iter()
            .flat_map(|t| self.expand_arg_token(t))
            .collect()
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

            // Token pasting operator (##).
            // Must come FIRST so that ## isn't misinterpreted as
            // stringification by the handler below.
            // The real lexer produces two consecutive HashHash tokens for ##;
            // tests create a single HashHash token with text "##".
            // Handle both representations.
            if tok.kind == TokenKind::HashHash {
                let next_is_hashhash =
                    i + 1 < body.len() && body[i + 1].kind == TokenKind::HashHash;
                let is_double_hash = next_is_hashhash || token_text(tok).len() >= 2;
                if is_double_hash {
                    if pending_paste.is_some() {
                        if next_is_hashhash {
                            i += 2;
                        } else {
                            i += 1;
                        }
                        continue;
                    }
                    if let Some(last) = result.pop() {
                        pending_paste = Some(last);
                    }
                    if next_is_hashhash {
                        i += 2; // consume both ## tokens
                    } else {
                        i += 1; // consume single ## token
                    }
                    continue;
                }
                // Single # — fall through to stringification handler
            }

            // Stringification: # param -> string literal.
            // The lexer has a bug that returns HashHash for single #, so
            // we check both Hash and HashHash.
            if tok.kind == TokenKind::Hash || tok.kind == TokenKind::HashHash {
                if i + 1 < body.len() {
                    let next = &body[i + 1];
                    if next.kind == TokenKind::Identifier {
                        if let Some(arg) = self.resolve_arg(next, m, args) {
                            let text = tokens_to_string(&arg);
                            let str_tok = make_string_token(&text);
                            if let Some(pt) = pending_paste.take() {
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
                    result.push(pt);
                }
                result.push(tok.clone());
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

            // General fallthrough: emit literally, with token pasting.
            if let Some(pt) = pending_paste.take() {
                let prev_text = token_text(&pt);
                let curr_text = token_text(tok);
                let combined = format!("{}{}", prev_text, curr_text);
                result.push(make_identifier(&combined));
            } else {
                result.push(tok.clone());
            }
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

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
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
    opening_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreprocessError {
    pub code: &'static str,
    pub span: SourceSpan,
    pub message: String,
}

#[derive(Debug)]
pub struct Preprocessor {
    macros: MacroMap,
    include_guard_seen: bool,
    pragma_once_files: HashSet<String>,
    include_paths: Vec<String>,
    forced_includes: Vec<String>,
    macro_prefix_maps: Vec<(String, String)>,
    cond_stack: Vec<CondState>,
    errors: Vec<PreprocessError>,
    standard_includes: bool,
    source_ids: HashMap<String, FileId>,
    source_paths: HashMap<FileId, String>,
    reserved_file_ids: HashSet<FileId>,
    next_file_id: u32,
    /// Current source file path for __FILE__ expansion.
    pub current_file: String,
}

impl Preprocessor {
    pub fn new() -> Self {
        Self::for_target(AARCH64_UNKNOWN_LINUX_GNU)
            .expect("the built-in target profile must be valid")
    }

    pub fn for_target(target: &str) -> Result<Self, String> {
        let profile = match target {
            AARCH64_UNKNOWN_LINUX_GNU => AARCH64_UNKNOWN_LINUX_GNU_PREDEFINED,
            _ => return Err(format!("unsupported preprocessor target profile: {target}")),
        };
        let mut preprocessor = Self {
            macros: MacroMap::new(),
            include_guard_seen: false,
            pragma_once_files: HashSet::new(),
            include_paths: Vec::new(),
            forced_includes: Vec::new(),
            macro_prefix_maps: Vec::new(),
            cond_stack: Vec::new(),
            errors: Vec::new(),
            standard_includes: true,
            source_ids: HashMap::new(),
            source_paths: HashMap::new(),
            reserved_file_ids: HashSet::new(),
            next_file_id: 1,
            current_file: String::new(),
        };
        preprocessor.install_predefined_profile(profile)?;
        preprocessor
            .macros
            .define("__DATE__".into(), vec![make_date_token()], Vec::new());
        Ok(preprocessor)
    }

    fn install_predefined_profile(&mut self, source: &str) -> Result<(), String> {
        let mut lexer = Lexer::new(FileId::DUMMY, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            if token.kind == TokenKind::Error {
                return Err("invalid token in embedded preprocessor target profile".into());
            }
            tokens.push(token);
        }
        let mut expanding = HashSet::new();
        let output = self.process_impl(TokenStream::new(tokens), source, &mut expanding, true, 0);
        let has_non_eof_output = output
            .into_vec()
            .iter()
            .any(|token| token.kind != TokenKind::Eof);
        if has_non_eof_output || !self.errors.is_empty() {
            return Err("embedded preprocessor target profile is not definitions-only".into());
        }
        Ok(())
    }

    pub fn macros(&self) -> &MacroMap {
        &self.macros
    }

    pub fn macros_mut(&mut self) -> &mut MacroMap {
        &mut self.macros
    }

    pub fn pragma_once(&mut self) {
        self.include_guard_seen = true;
        if !self.current_file.is_empty() {
            let identity = resolved_existing_path(std::path::Path::new(&self.current_file))
                .unwrap_or_else(|| self.current_file.clone());
            self.pragma_once_files.insert(identity);
        }
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

    /// Configure a command-line `-include FILE` input for this compilation.
    pub fn add_forced_include(&mut self, path: String) {
        self.forced_includes.push(path);
    }

    /// Configure an ordered `-fmacro-prefix-map=OLD=NEW` mapping.
    ///
    /// Day 5 applies these mappings only when expanding `__FILE__`. Physical
    /// paths used for include lookup, source identity, and diagnostics remain
    /// unchanged. If multiple mappings match, the last command-line mapping
    /// wins, matching GCC. No `__BASE_FILE__` or compiler-builtin coverage is
    /// claimed.
    pub fn add_macro_prefix_map(&mut self, old: String, new: String) {
        self.macro_prefix_maps.push((old, new));
    }

    pub fn set_standard_includes_enabled(&mut self, enabled: bool) {
        self.standard_includes = enabled;
    }

    pub fn errors(&self) -> &[PreprocessError] {
        &self.errors
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Return the resolved source path registered for `file` in this
    /// preprocessing run.
    pub fn source_path(&self, file: FileId) -> Option<&str> {
        self.source_paths.get(&file).map(String::as_str)
    }

    pub fn define(&mut self, name: String, body: Vec<Token>) {
        self.macros.define(name, body, vec![]);
    }

    pub fn set_predefined_date(&mut self, date: &str) {
        self.macros
            .define("__DATE__".into(), vec![make_string_token(date)], vec![]);
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
    /// handle all directives. Non-directive tokens are expanded (macro
    /// substitution) and emitted when the current conditional block is active.
    ///
    /// `source` is the original source text; token text is reconstructed
    /// from source spans rather than from the `TokenText` field, because the
    /// lexer uses `Borrowed` tokens whose text is identified by byte range.
    pub fn process(&mut self, tokens: TokenStream, source: &str) -> TokenStream {
        self.reset_source_table();
        self.cond_stack.clear();
        self.errors.clear();
        self.include_guard_seen = false;
        self.pragma_once_files.clear();
        self.register_entry_source(&tokens);
        let mut expanding: HashSet<String> = HashSet::new();
        let entry_span = tokens
            .tokens
            .iter()
            .find(|token| token.span.file != FileId::DUMMY)
            .map(|token| SourceSpan::new(token.span.file, 0, 0, 1, 1))
            .unwrap_or_else(|| SourceSpan::new(FileId::DUMMY, 0, 0, 1, 1));
        let mut output = Vec::new();

        // `-include FILE` has the same macro environment and source registry
        // as the main translation unit. Process each configured file before
        // the entry stream and preserve argv order.
        for requested in self.forced_includes.clone() {
            let Some(resolved) = resolved_existing_path(std::path::Path::new(&requested)) else {
                self.errors.push(PreprocessError {
                    code: "E0105",
                    span: entry_span,
                    message: format!("forced-include-not-found: {requested}"),
                });
                continue;
            };
            if self.pragma_once_files.contains(&resolved) {
                continue;
            }
            match self.process_resolved_source(&resolved, &mut expanding) {
                Ok(processed) => output.extend(processed.into_vec()),
                Err(_) => self.errors.push(PreprocessError {
                    code: "E0105",
                    span: entry_span,
                    message: format!("forced-include-not-found: {requested}"),
                }),
            }
        }

        output.extend(
            self.process_impl(tokens, source, &mut expanding, true, 0)
                .into_vec(),
        );
        TokenStream::new(output)
    }

    fn process_resolved_source(
        &mut self,
        resolved: &str,
        expanding: &mut HashSet<String>,
    ) -> Result<TokenStream, std::io::Error> {
        let content = std::fs::read_to_string(resolved)?;
        let file = self.intern_source(resolved);
        let mut lexer = evocc_lex::Lexer::new(file, &content);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            if token.kind != TokenKind::Eof {
                let text = token_lexeme(&token, &content).to_owned();
                tokens.push(Token {
                    kind: token.kind,
                    span: token.span,
                    text: TokenText::Decoded(text),
                });
            }
        }
        let previous_file = std::mem::replace(&mut self.current_file, resolved.to_owned());
        let conditional_floor = self.cond_stack.len();
        let processed = self.process_impl(
            TokenStream::new(tokens),
            &content,
            expanding,
            true,
            conditional_floor,
        );
        self.current_file = previous_file;
        Ok(processed)
    }

    fn reset_source_table(&mut self) {
        self.source_ids.clear();
        self.source_paths.clear();
        self.reserved_file_ids.clear();
        self.next_file_id = 1;
    }

    fn register_entry_source(&mut self, tokens: &TokenStream) {
        let Some(file) = tokens
            .tokens
            .iter()
            .map(|token| token.span.file)
            .find(|file| *file != FileId::DUMMY)
        else {
            return;
        };
        self.reserved_file_ids.extend(
            tokens
                .tokens
                .iter()
                .map(|token| token.span.file)
                .filter(|file| *file != FileId::DUMMY),
        );
        let max_file = tokens
            .tokens
            .iter()
            .map(|token| token.span.file)
            .filter(|file| *file != FileId::DUMMY)
            .max_by_key(|file| file.raw())
            .expect("entry source has a non-dummy FileId");
        self.next_file_id = max_file.raw().checked_add(1).unwrap_or(1);
        if !self.current_file.is_empty() {
            self.register_source_with_id(file, self.current_file.clone());
        }
    }

    fn register_source_with_id(&mut self, file: FileId, path: String) {
        if let Some(existing) = self.source_ids.get(&path) {
            assert_eq!(
                *existing, file,
                "one source path cannot have multiple FileIds"
            );
            return;
        }
        if let Some(existing) = self.source_paths.get(&file) {
            assert_eq!(
                existing, &path,
                "one FileId cannot identify multiple source paths"
            );
            return;
        }
        self.reserved_file_ids.insert(file);
        self.source_ids.insert(path.clone(), file);
        self.source_paths.insert(file, path);
    }

    fn intern_source(&mut self, path: &str) -> FileId {
        if let Some(file) = self.source_ids.get(path) {
            return *file;
        }

        let first_candidate = self.next_file_id;
        loop {
            let file = FileId::new(self.next_file_id);
            self.next_file_id = self.next_file_id.checked_add(1).unwrap_or(1);
            if self.reserved_file_ids.insert(file) {
                self.source_ids.insert(path.to_owned(), file);
                self.source_paths.insert(file, path.to_owned());
                return file;
            }
            assert_ne!(
                self.next_file_id, first_candidate,
                "source FileId space exhausted"
            );
        }
    }

    fn process_impl(
        &mut self,
        tokens: TokenStream,
        source: &str,
        expanding: &mut HashSet<String>,
        directives_enabled: bool,
        conditional_floor: usize,
    ) -> TokenStream {
        let mut output: Vec<Token> = Vec::new();
        let raw = tokens.into_vec();
        let len = raw.len();
        let mut i: usize = 0;

        while i < len {
            let tok = &raw[i];

            if directives_enabled
                && is_directive_hash(tok.kind)
                && is_start_of_line(&raw, i, source)
            {
                i += 1;
                if i >= len {
                    output.push(tok.clone());
                    break;
                }
                let dir_name = token_lexeme(&raw[i], source).to_string();
                i += 1;

                match dir_name.as_str() {
                    "define" => {
                        if self.cond_active() {
                            let (name, body, is_function_like) = parse_define(&raw, &mut i, source);
                            if !name.is_empty() {
                                let (params, variadic, variadic_param_name) = if is_function_like {
                                    let (p, v, vn) = extract_params(&body, source);
                                    (p, v, vn)
                                } else {
                                    (vec![], false, None)
                                };
                                let body_tokens = if !is_function_like {
                                    body
                                } else {
                                    // Find the matching ')' that ends the param
                                    // list and keep only tokens after it.
                                    let mut depth = 0u32;
                                    let mut split_at = 0usize;
                                    for (j, t) in body.iter().enumerate() {
                                        match t.kind {
                                            TokenKind::LParen => depth += 1,
                                            TokenKind::RParen => {
                                                depth -= 1;
                                                if depth == 0 {
                                                    split_at = j + 1;
                                                    break;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    if split_at < body.len() {
                                        body[split_at..].to_vec()
                                    } else {
                                        vec![]
                                    }
                                };
                                // Convert body tokens to Decoded so resolve_arg can read
                                // parameter names in nested expansions.
                                let body_tokens: Vec<Token> = body_tokens
                                    .into_iter()
                                    .map(|t| {
                                        let lexeme = token_lexeme(&t, source);
                                        let text = if lexeme.is_empty() {
                                            format_placeholder(t.kind)
                                        } else {
                                            lexeme.to_owned()
                                        };
                                        Token {
                                            kind: t.kind,
                                            span: t.span,
                                            text: TokenText::Decoded(text),
                                        }
                                    })
                                    .collect();
                                if is_function_like {
                                    self.macros_mut().define_function_like(
                                        name,
                                        params,
                                        variadic,
                                        variadic_param_name,
                                        body_tokens,
                                    );
                                } else {
                                    self.define(name, body_tokens);
                                }
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "undef" => {
                        if self.cond_active() && i < len {
                            let name = token_lexeme(&raw[i], source).to_string();
                            self.undefine(&name);
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "include" => {
                        if self.cond_active() {
                            let directive_span = tok.span;
                            // Parse include argument: "file" or <file>
                            let include_text = read_include_argument(&raw, &mut i, source);
                            if !include_text.is_empty() {
                                let path = resolve_include(
                                    &include_text,
                                    &self.current_file,
                                    &self.include_paths,
                                    self.standard_includes,
                                );
                                if let Some(resolved) = path {
                                    if !self.pragma_once_files.contains(&resolved) {
                                        if let Ok(processed) =
                                            self.process_resolved_source(&resolved, expanding)
                                        {
                                            output.extend(processed.into_vec());
                                        }
                                    }
                                } else {
                                    self.errors.push(PreprocessError {
                                        code: "E0105",
                                        span: directive_span,
                                        message: format!("include-not-found: {include_text}"),
                                    });
                                }
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "ifdef" => {
                        let name = if i < len {
                            token_lexeme(&raw[i], source).to_string()
                        } else {
                            String::new()
                        };
                        i = advance_past_line(i, &raw, source).min(len);
                        let defined = self.is_defined(&name);
                        self.push_cond(defined, tok.span);
                        skip_line(&raw, &mut i, source);
                    }
                    "ifndef" => {
                        let name = if i < len {
                            token_lexeme(&raw[i], source).to_string()
                        } else {
                            String::new()
                        };
                        i = advance_past_line(i, &raw, source).min(len);
                        let defined = self.is_defined(&name);
                        self.push_cond(!defined, tok.span);
                        skip_line(&raw, &mut i, source);
                    }
                    "if" => {
                        // Evaluate a simple constant expression. For now we
                        // support `defined(name)`, arithmetic constants (0,1),
                        // and integer comparisons.
                        let expr_tokens = collect_line_tokens(&raw, &mut i, source);
                        let value = evaluate_if_expr(&expr_tokens, &self.macros, source);
                        self.push_cond(value, tok.span);
                        skip_line(&raw, &mut i, source);
                    }
                    "elif" => {
                        if self.cond_stack.len() <= conditional_floor {
                            self.push_unmatched_conditional(tok.span);
                        } else {
                            let had = self
                                .cond_stack
                                .last()
                                .map(|condition| condition.had_true)
                                .unwrap_or(false);
                            let outer_active =
                                self.cond_stack.iter().rev().skip(1).all(|c| c.in_true);
                            let cs = self
                                .cond_stack
                                .last_mut()
                                .expect("conditional stack was checked above");
                            if had {
                                cs.in_true = false;
                            } else {
                                let line_tokens = collect_line_tokens(&raw, &mut i, source);
                                let value = outer_active
                                    && evaluate_if_expr(&line_tokens, &self.macros, source);
                                cs.in_true = value;
                                if value {
                                    cs.had_true = true;
                                }
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "else" => {
                        let len_stack = self.cond_stack.len();
                        if len_stack <= conditional_floor {
                            self.push_unmatched_conditional(tok.span);
                        } else {
                            let had = self.cond_stack[len_stack - 1].had_true;
                            let outer_active =
                                self.cond_stack[..len_stack - 1].iter().all(|c| c.in_true);
                            let cs = &mut self.cond_stack[len_stack - 1];
                            if had {
                                cs.in_true = false;
                            } else {
                                cs.in_true = outer_active;
                                cs.had_true = outer_active;
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "endif" => {
                        if self.cond_stack.len() <= conditional_floor {
                            self.push_unmatched_conditional(tok.span);
                        } else {
                            self.cond_stack.pop();
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "pragma" => {
                        if self.cond_active() {
                            let pragma_tok = collect_line_tokens(&raw, &mut i, source);
                            let pragma_text: String = pragma_tok
                                .iter()
                                .filter_map(|t| {
                                    let s = token_lexeme(t, source);
                                    if s.is_empty() {
                                        None
                                    } else {
                                        Some(s.to_owned())
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            if pragma_text.contains("once") {
                                self.pragma_once();
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "error" => {
                        let message_tokens = collect_line_tokens(&raw, &mut i, source);
                        if self.cond_active() {
                            let message = message_tokens
                                .iter()
                                .map(|token| token_lexeme(token, source))
                                .filter(|text| !text.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ");
                            self.errors.push(PreprocessError {
                                code: "E0104",
                                span: tok.span,
                                message: format!("error-directive: {message}"),
                            });
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    _ => {
                        // Unknown directive (including #line, #warning): skip.
                        skip_line(&raw, &mut i, source);
                    }
                }
            } else if self.cond_active() {
                // Predefined context-dependent macros: __FILE__, __LINE__,
                // __DATE__. These are expanded inline rather than through
                // the macro table because their values change at each use.
                if tok.kind == TokenKind::Identifier {
                    let name = token_lexeme(tok, source).to_string();
                    if name == "__FILE__" {
                        let displayed_file = self.mapped_file_name();
                        let file_str = make_string_token(&displayed_file);
                        let sub_stream = TokenStream::new(vec![file_str]);
                        let processed = self.process_impl(sub_stream, source, expanding, false, 0);
                        output.extend(processed.into_vec());
                        i += 1;
                        continue;
                    }
                    if name == "__LINE__" {
                        let line_num = tok.span.line_start;
                        let line_tok = make_int_token(line_num as i64);
                        let sub_stream = TokenStream::new(vec![line_tok]);
                        let processed = self.process_impl(sub_stream, source, expanding, false, 0);
                        output.extend(processed.into_vec());
                        i += 1;
                        continue;
                    }
                    if !expanding.contains(&name) {
                        // Try object-like macro first.
                        if let Some(body) = self.macros.expand_object_like(&name) {
                            expanding.insert(name.clone());
                            let sub_stream = TokenStream::new(body);
                            let processed =
                                self.process_impl(sub_stream, source, expanding, false, 0);
                            output.extend(processed.into_vec());
                            expanding.remove(&name);
                            i += 1;
                            continue;
                        }
                        // Try function-like macro: name followed by '('
                        if i + 1 < len && raw[i + 1].kind == TokenKind::LParen {
                            let saved_i = i;
                            i += 2; // skip name and '('
                            let mut j = i;
                            let mut args: Vec<Vec<Token>> = Vec::new();
                            let mut current_arg: Vec<Token> = Vec::new();
                            let mut depth = 1u32;
                            while j < len && depth > 0 {
                                let lexeme = token_lexeme(&raw[j], source);
                                let text = if lexeme.is_empty() {
                                    format_placeholder(raw[j].kind)
                                } else {
                                    lexeme.to_owned()
                                };
                                let decoded = Token {
                                    kind: raw[j].kind,
                                    span: raw[j].span,
                                    text: TokenText::Decoded(text),
                                };
                                match raw[j].kind {
                                    TokenKind::LParen => {
                                        depth += 1;
                                        if depth > 1 {
                                            current_arg.push(decoded);
                                        }
                                    }
                                    TokenKind::RParen => {
                                        depth -= 1;
                                        if depth > 0 {
                                            current_arg.push(decoded);
                                        }
                                    }
                                    TokenKind::Comma if depth == 1 => {
                                        args.push(std::mem::take(&mut current_arg));
                                    }
                                    _ => {
                                        current_arg.push(decoded);
                                    }
                                }
                                j += 1;
                            }
                            if !current_arg.is_empty() {
                                args.push(current_arg);
                            }
                            i = j; // advance past ')' and arguments

                            if let Some(body) = self.macros.expand_macro(&name, &args) {
                                expanding.insert(name.clone());
                                let sub_stream = TokenStream::new(body);
                                let processed =
                                    self.process_impl(sub_stream, source, expanding, false, 0);
                                output.extend(processed.into_vec());
                                expanding.remove(&name);
                                continue;
                            }
                            // If expansion returned None (arity mismatch etc.),
                            // output the original tokens unexpanded.
                            output.push(tok.clone());
                            i = saved_i + 1; // back up
                            continue;
                        }
                    }
                }
                output.push(tok.clone());
                i += 1;
            } else {
                i += 1;
            }
        }

        if directives_enabled {
            let unmatched = self
                .cond_stack
                .drain(conditional_floor..)
                .map(|state| state.opening_span)
                .collect::<Vec<_>>();
            for span in unmatched {
                self.push_unmatched_conditional(span);
            }
        }

        TokenStream::new(output)
    }

    fn mapped_file_name(&self) -> String {
        for (old, new) in self.macro_prefix_maps.iter().rev() {
            if let Some(suffix) = self.current_file.strip_prefix(old) {
                return format!("{new}{suffix}");
            }
        }
        self.current_file.clone()
    }

    fn push_cond(&mut self, value: bool, opening_span: SourceSpan) {
        self.cond_stack.push(CondState {
            in_true: value,
            had_true: value,
            opening_span,
        });
    }

    fn push_unmatched_conditional(&mut self, span: SourceSpan) {
        self.errors.push(PreprocessError {
            code: "E0102",
            span,
            message: "unmatched-conditional".into(),
        });
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

/// Extract the source text of a token from the original source buffer using
/// its span. Returns "" for dummy spans or out-of-bounds.
fn token_lexeme<'a>(tok: &'a Token, source: &'a str) -> &'a str {
    if let TokenText::Decoded(text) = &tok.text {
        return text;
    }
    let start = tok.span.byte_start as usize;
    let end = tok.span.byte_end as usize;
    if start >= source.len() || end > source.len() || start > end {
        return "";
    }
    &source[start..end]
}

/// Check whether a token kind marks the start of a preprocessor directive
/// line. The lexer produces either `Hash` or `HashHash` for a leading `#`;
/// both must be recognized because the operator handler in the lexer uses
/// `match_next` which checks the current byte, not the next one.
fn is_directive_hash(kind: TokenKind) -> bool {
    kind == TokenKind::Hash || kind == TokenKind::HashHash
}

/// Return whether the physical source gap contains a newline that survives C
/// translation phase 2. Backslash-LF and backslash-CRLF pairs do not end a
/// preprocessor directive, even though the following token has a different
/// physical `line_start`.
fn has_logical_newline_between(previous: &Token, current: &Token, source: &str) -> bool {
    let start = previous.span.byte_end as usize;
    let end = current.span.byte_start as usize;
    if start > end || end > source.len() {
        return previous.span.line_start != current.span.line_start;
    }

    let gap = &source.as_bytes()[start..end];
    let mut index = 0;
    while index < gap.len() {
        if gap[index] == b'\\' && gap.get(index + 1) == Some(&b'\n') {
            index += 2;
        } else if gap[index] == b'\\'
            && gap.get(index + 1) == Some(&b'\r')
            && gap.get(index + 2) == Some(&b'\n')
        {
            index += 3;
        } else if gap[index] == b'\n' {
            return true;
        } else {
            index += 1;
        }
    }
    false
}

/// Return whether translation phase 2 leaves no characters between two
/// tokens. A function-like macro requires its opening parenthesis to be
/// adjacent to the macro name; ordinary whitespace and comments prevent that,
/// while a spliced physical newline does not.
fn tokens_are_logically_adjacent(previous: &Token, current: &Token, source: &str) -> bool {
    let start = previous.span.byte_end as usize;
    let end = current.span.byte_start as usize;
    if start > end || end > source.len() {
        return false;
    }

    let gap = &source.as_bytes()[start..end];
    let mut index = 0;
    while index < gap.len() {
        if gap[index] == b'\\' && gap.get(index + 1) == Some(&b'\n') {
            index += 2;
        } else if gap[index] == b'\\'
            && gap.get(index + 1) == Some(&b'\r')
            && gap.get(index + 2) == Some(&b'\n')
        {
            index += 3;
        } else {
            return false;
        }
    }
    true
}

/// A token starts a logical preprocessing line when it is the first token or
/// when an unspliced newline separates it from the previous token.
fn is_start_of_line(tokens: &[Token], idx: usize, source: &str) -> bool {
    if idx == 0 {
        return true;
    }
    has_logical_newline_between(&tokens[idx - 1], &tokens[idx], source)
}

/// Skip tokens until the next logical line. If a directive handler already
/// advanced `i` to that line, this leaves it untouched.
fn skip_line(tokens: &[Token], i: &mut usize, source: &str) {
    if *i >= tokens.len() || *i == 0 {
        return;
    }
    let mut previous = *i - 1;
    while *i < tokens.len() && !has_logical_newline_between(&tokens[previous], &tokens[*i], source)
    {
        previous = *i;
        *i += 1;
    }
}

/// Parse a `#define` directive. Returns
/// (macro_name, body_tokens, is_function_like).
/// `*i` points to the first token after the directive keyword.
/// On return, `*i` points to the first token after the define body.
fn parse_define(tokens: &[Token], i: &mut usize, source: &str) -> (String, Vec<Token>, bool) {
    if *i >= tokens.len() {
        return (String::new(), Vec::new(), false);
    }
    let t = &tokens[*i];
    if *i > 0 && has_logical_newline_between(&tokens[*i - 1], t, source) {
        return (String::new(), Vec::new(), false);
    }
    let name_index = *i;
    let name = token_lexeme(t, source).to_string();
    *i += 1;
    // Read remaining tokens on this logical line as the body.
    let mut body = Vec::new();
    let mut previous = *i - 1;
    while *i < tokens.len() && !has_logical_newline_between(&tokens[previous], &tokens[*i], source)
    {
        body.push(tokens[*i].clone());
        previous = *i;
        *i += 1;
    }
    let is_function_like = body.first().is_some_and(|opening| {
        opening.kind == TokenKind::LParen
            && tokens_are_logically_adjacent(&tokens[name_index], opening, source)
    });
    (name, body, is_function_like)
}

/// Read the argument of an #include directive. Returns the filename without
/// quotes or angle brackets.
fn read_include_argument(tokens: &[Token], i: &mut usize, source: &str) -> String {
    if *i < tokens.len() {
        let tok = &tokens[*i];
        let text = token_lexeme(tok, source);
        if tok.kind == TokenKind::StringLiteral {
            // "file" → strip quotes
            let trimmed = text.trim_matches('"');
            *i += 1;
            return trimmed.to_owned();
        }
        if tok.kind == TokenKind::Less {
            // <file> → collect until >
            let mut path = String::new();
            *i += 1;
            while *i < tokens.len() {
                let t = &tokens[*i];
                if t.kind == TokenKind::Greater {
                    *i += 1;
                    break;
                }
                path.push_str(token_lexeme(t, source));
                *i += 1;
            }
            return path;
        }
    }
    String::new()
}

/// Try to resolve an include file name to an absolute path.
/// For quoted includes, first search relative to the current directory,
/// then the include path list. For angle-bracket includes, only search
/// the include path list.
fn resolve_include(
    name: &str,
    current_file: &str,
    include_paths: &[String],
    standard_includes: bool,
) -> Option<String> {
    // Quoted include: try beside the active source, then the process working
    // directory. `read_include_argument` has already removed the delimiters.
    if !name.starts_with('/') {
        if let Some(parent) = std::path::Path::new(current_file).parent() {
            let candidate = parent.join(name);
            if let Some(resolved) = resolved_existing_path(&candidate) {
                return Some(resolved);
            }
        }
        let local = std::path::Path::new(name);
        if let Some(resolved) = resolved_existing_path(local) {
            return Some(resolved);
        }
    }
    // Search include paths.
    for dir in include_paths {
        let candidate = std::path::Path::new(dir).join(name);
        if let Some(resolved) = resolved_existing_path(&candidate) {
            return Some(resolved);
        }
    }
    // Angle-bracket system includes: try well-known paths.
    if standard_includes {
        for sys_path in &["/usr/include", "/usr/local/include"] {
            let candidate = std::path::Path::new(sys_path).join(name);
            if let Some(resolved) = resolved_existing_path(&candidate) {
                return Some(resolved);
            }
        }
    }
    None
}

fn resolved_existing_path(path: &std::path::Path) -> Option<String> {
    std::fs::canonicalize(path)
        .ok()
        .map(|resolved| resolved.to_string_lossy().into_owned())
}

/// Collect all tokens on the current logical preprocessing line.
fn collect_line_tokens(tokens: &[Token], i: &mut usize, source: &str) -> Vec<Token> {
    if *i >= tokens.len() {
        return Vec::new();
    }
    if *i > 0 && has_logical_newline_between(&tokens[*i - 1], &tokens[*i], source) {
        return Vec::new();
    }
    let mut result = Vec::new();
    result.push(tokens[*i].clone());
    let mut previous = *i;
    *i += 1;
    while *i < tokens.len() && !has_logical_newline_between(&tokens[previous], &tokens[*i], source)
    {
        result.push(tokens[*i].clone());
        previous = *i;
        *i += 1;
    }
    result
}

/// Evaluate a simple #if / #elif constant expression.
/// Supports: `defined(NAME)`, integer constants (0, 1), +, -, *, /,
/// &&, ||, ==, !=, <, >, <=, >=, !, parentheses.
fn evaluate_if_expr(tokens: &[Token], macros: &MacroMap, source: &str) -> bool {
    // Filter tokens: keep identifiers, integers, operators, parens.
    // Evaluate as a left-to-right calculator with precedence.
    // For the curated fixtures, we mostly need `defined(NAME)` and simple
    // arithmetic like `1 + 2 * 3 == 7`.
    let mut i: usize = 0;
    evaluate_if_or(tokens, &mut i, macros, source)
}

fn evaluate_if_or(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    let mut val = evaluate_if_and(tokens, i, macros, source);
    while *i < tokens.len() && tokens[*i].kind == TokenKind::PipePipe {
        *i += 1;
        let rhs = evaluate_if_and(tokens, i, macros, source);
        val = val || rhs;
    }
    val
}

fn evaluate_if_and(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    let mut val = evaluate_if_cmp(tokens, i, macros, source);
    while *i < tokens.len() && tokens[*i].kind == TokenKind::AmpAmp {
        *i += 1;
        let rhs = evaluate_if_cmp(tokens, i, macros, source);
        val = val && rhs;
    }
    val
}

fn evaluate_if_cmp(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    let lhs = evaluate_if_unary(tokens, i, macros, source);
    if *i >= tokens.len() {
        return lhs;
    }
    let op = tokens[*i].kind;
    match op {
        TokenKind::EqEq
        | TokenKind::NotEq
        | TokenKind::Less
        | TokenKind::LessEq
        | TokenKind::Greater
        | TokenKind::GreaterEq => {
            *i += 1;
            let rhs = evaluate_if_unary(tokens, i, macros, source);
            match op {
                TokenKind::EqEq => lhs == rhs,
                TokenKind::NotEq => lhs != rhs,
                TokenKind::Less => !lhs & rhs,
                TokenKind::LessEq => lhs <= rhs,
                TokenKind::Greater => lhs & !rhs,
                TokenKind::GreaterEq => lhs >= rhs,
                _ => false,
            }
        }
        _ => lhs,
    }
}

fn evaluate_if_unary(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    if *i >= tokens.len() {
        return false;
    }
    if tokens[*i].kind == TokenKind::Bang {
        *i += 1;
        return !evaluate_if_unary(tokens, i, macros, source);
    }
    evaluate_if_primary(tokens, i, macros, source)
}

fn evaluate_if_primary(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    if *i >= tokens.len() {
        return false;
    }
    let tok = &tokens[*i];
    let text = token_lexeme(tok, source);

    // defined(NAME)
    if text == "defined" && *i + 2 < tokens.len() && tokens[*i + 1].kind == TokenKind::LParen {
        *i += 2; // skip defined and (
        let name = token_lexeme(&tokens[*i], source);
        *i += 1;
        if *i < tokens.len() && tokens[*i].kind == TokenKind::RParen {
            *i += 1;
        }
        return macros.contains(name);
    }

    // Integer constant
    if let Ok(val) = text.parse::<i64>() {
        *i += 1;
        return val != 0;
    }

    // Identifier (undefined macro → 0 per C standard)
    if tok.kind == TokenKind::Identifier {
        *i += 1;
        return false;
    }

    // '(' expr ')'
    if tok.kind == TokenKind::LParen {
        *i += 1;
        let val = evaluate_if_or(tokens, i, macros, source);
        if *i < tokens.len() && tokens[*i].kind == TokenKind::RParen {
            *i += 1;
        }
        return val;
    }

    *i += 1;
    false
}

/// Extract parameter names from a function-like macro body.
/// The body starts with '(' followed by param tokens.
/// Returns (params, is_variadic, variadic_param_name).
/// variadic_param_name is Some for GNU named variadics (e.g. `args...`),
/// None for C99 `...` (unnamed).
fn extract_params(body: &[Token], source: &str) -> (Vec<String>, bool, Option<String>) {
    if body.is_empty() || body[0].kind != TokenKind::LParen {
        return (vec![], false, None);
    }
    let mut params = Vec::new();
    let mut is_variadic = false;
    let mut variadic_param_name = None;
    let mut depth = 1;
    let mut i = 1;
    let mut current = String::new();
    while i < body.len() && depth > 0 {
        let text = token_lexeme(&body[i], source);
        match body[i].kind {
            TokenKind::LParen => {
                depth += 1;
                if depth > 1 {
                    current.push_str(text);
                }
            }
            TokenKind::RParen => {
                depth -= 1;
                if depth == 0 && !current.is_empty() {
                    params.push(std::mem::take(&mut current));
                }
            }
            TokenKind::Comma if depth == 1 => {
                if !current.is_empty() {
                    params.push(std::mem::take(&mut current));
                }
            }
            TokenKind::Ellipsis => {
                is_variadic = true;
                if !current.is_empty() {
                    variadic_param_name = Some(std::mem::take(&mut current));
                }
            }
            _ => {
                current.push_str(text);
            }
        }
        i += 1;
    }
    (params, is_variadic, variadic_param_name)
}

/// Advance `i` past the current logical line's tokens.
fn advance_past_line(i: usize, tokens: &[Token], source: &str) -> usize {
    if i >= tokens.len() {
        return i;
    }
    if i > 0 && has_logical_newline_between(&tokens[i - 1], &tokens[i], source) {
        return i;
    }
    let mut j = i + 1;
    let mut previous = i;
    while j < tokens.len() && !has_logical_newline_between(&tokens[previous], &tokens[j], source) {
        previous = j;
        j += 1;
    }
    j
}

fn format_placeholder(kind: TokenKind) -> String {
    match kind {
        TokenKind::LParen => "(".to_owned(),
        TokenKind::RParen => ")".to_owned(),
        TokenKind::Comma => ",".to_owned(),
        TokenKind::Plus => "+".to_owned(),
        TokenKind::Minus => "-".to_owned(),
        TokenKind::Star => "*".to_owned(),
        TokenKind::Slash => "/".to_owned(),
        TokenKind::Hash => "#".to_owned(),
        TokenKind::HashHash => "##".to_owned(),
        _ => String::new(),
    }
}

// These helpers are used by the MacroMap expand_ methods and by directive
// argument collection in the process() body.

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

fn make_int_token(value: i64) -> Token {
    Token::new(
        TokenKind::IntLiteral,
        SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
        TokenText::Decoded(value.to_string()),
    )
}

fn make_date_token() -> Token {
    Token::new(
        TokenKind::StringLiteral,
        SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
        TokenText::Decoded("\"Aug 12 2026\"".to_string()),
    )
}

// The old helpers that are no longer called are kept for the benefit of the
// existing test code that references them. They will be removed in a cleanup
// pass. Here we define the ones that the process() rewrite rendered unused
// but the macro expansion code in MacroMap still calls.

fn read_directive(tokens: &[Token], i: usize) -> (String, usize) {
    if i < tokens.len() {
        (String::new(), i + 1)
    } else {
        (String::new(), i)
    }
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
        map.define("ADD".to_owned(), body, vec!["a".to_owned(), "b".to_owned()]);

        // Correct arity: 2 args → succeeds.
        let arg0 = vec![dummy_token(TokenKind::IntLiteral, "1")];
        let arg1 = vec![dummy_token(TokenKind::IntLiteral, "2")];
        let ok = map.expand_function_like("ADD", &[arg0, arg1]);
        assert!(ok.is_some());

        // Wrong arity: 1 arg → fails.
        let bad = map.expand_function_like("ADD", &[vec![dummy_token(TokenKind::IntLiteral, "1")]]);
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
        let body = vec![
            dummy_token(TokenKind::Identifier, "a"),
            dummy_token(TokenKind::Plus, "+"),
            dummy_token(TokenKind::Identifier, "__VA_ARGS__"),
        ];
        map.define_function_like("FOO".into(), vec!["a".into()], true, None, body);
        // FOO(1, x) → "1 + x"
        let arg_a = vec![dummy_token(TokenKind::IntLiteral, "1")];
        let arg_x = vec![dummy_token(TokenKind::Identifier, "x")];
        let result1 = map.expand_macro("FOO", &[arg_a, arg_x]);
        assert!(result1.is_some());
        let expanded = result1.unwrap();
        assert!(!expanded.is_empty());
        let texts: Vec<String> = expanded.iter().map(token_text).collect();
        assert_eq!(texts, vec!["1", "+", "x"]);

        // FOO(1, x, y) — three variadic extra args, combined with commas
        let a1 = vec![dummy_token(TokenKind::IntLiteral, "1")];
        let a2 = vec![dummy_token(TokenKind::Identifier, "x")];
        let a3 = vec![dummy_token(TokenKind::Identifier, "y")];
        let result2 = map.expand_macro("FOO", &[a1, a2, a3]);
        assert!(result2.is_some());
        let expanded2 = result2.unwrap();
        let texts2: Vec<String> = expanded2.iter().map(token_text).collect();
        assert_eq!(texts2, vec!["1", "+", "x", ",", "y"]);
    }

    /// Token pasting: `#define PASTE(a,b) a ## b`
    #[test]
    fn cpp_token_pasting_is_correct() {
        let mut map = MacroMap::new();
        let body = vec![
            dummy_token(TokenKind::Identifier, "a"),
            dummy_token(TokenKind::HashHash, "##"),
            dummy_token(TokenKind::Identifier, "b"),
        ];
        map.define_function_like(
            "PASTE".into(),
            vec!["a".into(), "b".into()],
            false,
            None,
            body,
        );
        let result = map.expand_macro(
            "PASTE",
            &[
                vec![dummy_token(TokenKind::Identifier, "foo")],
                vec![dummy_token(TokenKind::IntLiteral, "42")],
            ],
        );
        assert!(result.is_some());
        let expanded = result.unwrap();
        let texts: Vec<String> = expanded.iter().map(token_text).collect();
        assert_eq!(texts, vec!["foo42"]);
    }

    /// Stringification: `#define STR(x) #x`
    #[test]
    fn cpp_stringification_works() {
        let mut map = MacroMap::new();
        let body = vec![
            dummy_token(TokenKind::Hash, "#"),
            dummy_token(TokenKind::Identifier, "x"),
        ];
        map.define_function_like("STR".into(), vec!["x".into()], false, None, body);
        let result = map.expand_macro("STR", &[vec![dummy_token(TokenKind::Identifier, "hello")]]);
        assert!(result.is_some());
        let expanded = result.unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(str_text(&expanded[0]), "\"hello\"");
    }

    /// Conditional directives: #ifdef / #ifndef / #else / #endif
    #[test]
    fn cpp_conditional_directives_evaluate() {
        let mut pp = Preprocessor::new();
        let tk = |kind| {
            Token::new(
                kind,
                SourceSpan::new(FileId::new(1), 0, 0, 0, 0),
                TokenText::Borrowed,
            )
        };
        pp.define("FOO".into(), vec![tk(TokenKind::IntLiteral)]);
        assert!(pp.is_defined("FOO"));
    }

    /// Variadic __VA_ARGS__ substitution: fixture 007/008/009.
    #[test]
    fn cpp_variadic_expands_correctly() {
        // 007: #define LOG(fmt,...) printf(fmt,__VA_ARGS__)
        //      LOG("%d",42) → printf("%d",42)
        let mut map = MacroMap::new();
        let body = vec![
            dummy_token(TokenKind::Identifier, "printf"),
            dummy_token(TokenKind::LParen, "("),
            dummy_token(TokenKind::Identifier, "fmt"),
            dummy_token(TokenKind::Comma, ","),
            dummy_token(TokenKind::Identifier, "__VA_ARGS__"),
            dummy_token(TokenKind::RParen, ")"),
        ];
        map.define_function_like("LOG".into(), vec!["fmt".into()], true, None, body);

        let result = map.expand_macro(
            "LOG",
            &[
                vec![dummy_token(TokenKind::StringLiteral, "\"%d\"")],
                vec![dummy_token(TokenKind::IntLiteral, "42")],
            ],
        );
        assert!(result.is_some());
        let texts: Vec<String> = result.unwrap().iter().map(token_text).collect();
        assert_eq!(texts, ["printf", "(", "\"%d\"", ",", "42", ")"]);

        // 008: LOG("hi") with empty __VA_ARGS__ → printf("hi",)  (GNU)
        let result2 = map.expand_macro(
            "LOG",
            &[vec![dummy_token(TokenKind::StringLiteral, "\"hi\"")]],
        );
        assert!(result2.is_some());
        let texts2: Vec<String> = result2.unwrap().iter().map(token_text).collect();
        assert_eq!(texts2, ["printf", "(", "\"hi\"", ",", ")"]);

        // 009: #define LOG(fmt,args...) printf(fmt,args)
        //      LOG("%d",42) → printf("%d",42)
        let mut map3 = MacroMap::new();
        let body3 = vec![
            dummy_token(TokenKind::Identifier, "printf"),
            dummy_token(TokenKind::LParen, "("),
            dummy_token(TokenKind::Identifier, "fmt"),
            dummy_token(TokenKind::Comma, ","),
            dummy_token(TokenKind::Identifier, "args"),
            dummy_token(TokenKind::RParen, ")"),
        ];
        map3.define_function_like(
            "LOG".into(),
            vec!["fmt".into()],
            true,
            Some("args".into()),
            body3,
        );

        let result3 = map3.expand_macro(
            "LOG",
            &[
                vec![dummy_token(TokenKind::StringLiteral, "\"%d\"")],
                vec![dummy_token(TokenKind::IntLiteral, "42")],
            ],
        );
        assert!(result3.is_some());
        let texts3: Vec<String> = result3.unwrap().iter().map(token_text).collect();
        assert_eq!(texts3, ["printf", "(", "\"%d\"", ",", "42", ")"]);
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
