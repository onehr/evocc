#![allow(dead_code)]
#![allow(clippy::should_implement_trait)]
//! C preprocessor: macro expansion, directives, token stream

use std::collections::{HashMap, HashSet};

pub use evocc_common::{FileId, SourceSpan};
use evocc_lex::preprocessing_number_len_at;
pub use evocc_lex::{Lexer, Token, TokenKind, TokenText};

const AARCH64_UNKNOWN_LINUX_GNU: &str = "aarch64-unknown-linux-gnu";
const AARCH64_UNKNOWN_LINUX_GNU_PREDEFINED: &str =
    include_str!("../predefined/aarch64-unknown-linux-gnu.h");
const HAS_ATTRIBUTE_ANSWERS: &str = include_str!("../predefined/has-attribute.tsv");
const HAS_BUILTIN_NAMES: &str = include_str!("../predefined/has-builtin.txt");
const FEATURE_QUERY_NAMES: [&str; 4] = [
    "__has_attribute",
    "__has_builtin",
    "__has_feature",
    "__has_include",
];
const INVALID_PASTE_MARKER_PREFIX: &str = "\u{1e}evocc-invalid-paste:";

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

#[derive(Debug)]
struct PendingPaste {
    left: Token,
    operator_span: SourceSpan,
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

    /// Report which arguments must be macro-expanded before substitution.
    /// Parameters used only by `#` or `##` remain unexpanded, as required by
    /// the preprocessing translation phases.
    fn argument_prescan_mask(&self, name: &str, arg_count: usize) -> Option<Vec<bool>> {
        let macro_def = self.get(name)?;
        if !macro_def.is_function
            || arg_count < macro_def.params.len()
            || (!macro_def.variadic && arg_count > macro_def.params.len())
        {
            return None;
        }

        let mut mask = vec![false; arg_count];
        for (index, token) in macro_def.body.iter().enumerate() {
            if token.kind != TokenKind::Identifier {
                continue;
            }
            let spelling = token_text(token);
            let gnu_comma_variadic = macro_def.variadic
                && spelling == "__VA_ARGS__"
                && parameter_uses_gnu_comma_elision(&macro_def.body, index);
            if parameter_is_quoted_or_pasted(&macro_def.body, index) && !gnu_comma_variadic {
                continue;
            }
            if let Some(parameter_index) = macro_def
                .params
                .iter()
                .position(|parameter| parameter == &spelling)
            {
                if let Some(entry) = mask.get_mut(parameter_index) {
                    *entry = true;
                }
            } else if spelling == "__VA_ARGS__" && macro_def.variadic {
                for entry in mask.iter_mut().skip(macro_def.params.len()) {
                    *entry = true;
                }
            }
        }
        Some(mask)
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
        self.expand_macro_with_prescanned_args(name, args, args)
    }

    /// Expand a macro using raw arguments for `#`/`##` occurrences and
    /// prescanned arguments for ordinary parameter substitution.
    fn expand_macro_with_prescanned_args(
        &self,
        name: &str,
        args: &[Vec<Token>],
        prescanned_args: &[Vec<Token>],
    ) -> Option<Vec<Token>> {
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
        let mut pending_paste: Option<PendingPaste> = None;
        let body = &m.body;
        let mut i = 0;

        while i < body.len() {
            let tok = &body[i];

            // Token pasting operator (##). The lexer emits one HashHash token.
            if tok.kind == TokenKind::HashHash {
                if pending_paste.is_some() {
                    i += 1;
                    continue;
                }
                if let Some(last) = result.pop() {
                    pending_paste = Some(PendingPaste {
                        left: last,
                        operator_span: tok.span,
                    });
                }
                i += 1;
                continue;
            }

            // Stringification: # param -> string literal.
            if tok.kind == TokenKind::Hash {
                if i + 1 < body.len() {
                    let next = &body[i + 1];
                    if next.kind == TokenKind::Identifier {
                        if let Some(arg) = self.resolve_arg(next, m, args) {
                            let text = tokens_to_string(&arg);
                            let mut str_tok = make_string_token(&text);
                            str_tok.spelling_span = tok.spelling_span;
                            str_tok.preceded_by_whitespace = tok.preceded_by_whitespace;
                            if let Some(pt) = pending_paste.take() {
                                result.push(paste_tokens(&pt.left, &str_tok, pt.operator_span));
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
                    result.push(pt.left);
                }
                result.push(tok.clone());
                i += 1;
                continue;
            }

            // Identifier — possible parameter reference
            if tok.kind == TokenKind::Identifier {
                let gnu_comma_variadic = m.variadic
                    && token_text(tok) == "__VA_ARGS__"
                    && parameter_uses_gnu_comma_elision(body, i);
                let selected_args = if gnu_comma_variadic {
                    prescanned_args
                } else if pending_paste.is_some() || parameter_is_followed_by_paste(body, i) {
                    args
                } else {
                    prescanned_args
                };
                if let Some(mut arg) = self.resolve_arg(tok, m, selected_args) {
                    if gnu_comma_variadic {
                        let comma = pending_paste
                            .take()
                            .expect("GNU comma elision has a preceding paste operand");
                        if args.len() > m.params.len() {
                            result.push(comma.left);
                            result.append(&mut arg);
                        }
                        i += 1;
                        continue;
                    }
                    if arg.is_empty() {
                        // An empty paste operand is a placemarker: the other
                        // operand survives unchanged.
                        if let Some(pt) = pending_paste.take() {
                            result.push(pt.left);
                        }
                    } else {
                        if let Some(pt) = pending_paste.take() {
                            // Paste preceding token with first expanded token
                            result.push(paste_tokens(&pt.left, &arg[0], pt.operator_span));
                            result.extend_from_slice(&arg[1..]);
                        } else {
                            result.append(&mut arg);
                        }
                    }
                    i += 1;
                    continue;
                }
            }

            // General fallthrough: emit literally, with token pasting.
            if let Some(pt) = pending_paste.take() {
                result.push(paste_tokens(&pt.left, tok, pt.operator_span));
            } else {
                result.push(tok.clone());
            }
            i += 1;
        }

        // Flush any dangling pending paste
        if let Some(pt) = pending_paste.take() {
            result.push(pt.left);
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
            for (offset, arg) in args.iter().skip(va_start).enumerate() {
                if offset > 0 {
                    combined.push(make_comma_token());
                }
                combined.extend(arg.clone());
            }
            return Some(combined);
        }
        None
    }
}

fn hash_token_is_paste(body: &[Token], index: usize) -> bool {
    body.get(index)
        .is_some_and(|token| token.kind == TokenKind::HashHash)
}

fn parameter_is_followed_by_paste(body: &[Token], index: usize) -> bool {
    hash_token_is_paste(body, index + 1)
}

fn parameter_uses_gnu_comma_elision(body: &[Token], index: usize) -> bool {
    index
        .checked_sub(1)
        .is_some_and(|paste| hash_token_is_paste(body, paste))
        && index
            .checked_sub(2)
            .and_then(|comma| body.get(comma))
            .is_some_and(|token| token.kind == TokenKind::Comma)
}

fn parameter_is_quoted_or_pasted(body: &[Token], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|previous| body.get(previous))
        .is_some_and(|token| matches!(token.kind, TokenKind::Hash | TokenKind::HashHash))
        || parameter_is_followed_by_paste(body, index)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PragmaPackAction {
    Set(u32),
    Reset,
    Push(Option<u32>),
    Pop,
}

const PRAGMA_PACK_MARKER_PREFIX: &str = "\u{1e}evocc-pragma-pack:";

/// Decode the synthetic token that carries a supported `#pragma pack` across
/// the preprocessor/parser boundary. The control-character prefix cannot be
/// produced by the source lexer as an identifier.
pub fn pragma_pack_action(token: &Token) -> Option<PragmaPackAction> {
    if token.kind != TokenKind::Identifier {
        return None;
    }
    let TokenText::Decoded(text) = &token.text else {
        return None;
    };
    let encoded = text.strip_prefix(PRAGMA_PACK_MARKER_PREFIX)?;
    match encoded {
        "reset" => Some(PragmaPackAction::Reset),
        "push" => Some(PragmaPackAction::Push(None)),
        "pop" => Some(PragmaPackAction::Pop),
        _ => encoded
            .strip_prefix("set=")
            .and_then(|value| value.parse().ok())
            .map(PragmaPackAction::Set)
            .or_else(|| {
                encoded
                    .strip_prefix("push=")
                    .and_then(|value| value.parse().ok())
                    .map(|value| PragmaPackAction::Push(Some(value)))
            }),
    }
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
    counter: u32,
    builtin_feature_queries: HashSet<String>,
    poisoned_identifiers: HashSet<String>,
    ignored_pragma_count: u64,
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
            counter: 0,
            builtin_feature_queries: HashSet::new(),
            poisoned_identifiers: HashSet::new(),
            ignored_pragma_count: 0,
            current_file: String::new(),
        };
        preprocessor.install_predefined_profile(profile)?;
        preprocessor.install_feature_query_macros();
        preprocessor
            .macros
            .define("__DATE__".into(), vec![make_date_token()], Vec::new());
        Ok(preprocessor)
    }

    fn install_feature_query_macros(&mut self) {
        for name in FEATURE_QUERY_NAMES {
            self.macros.define_function_like(
                name.to_owned(),
                vec!["query".to_owned()],
                false,
                None,
                Vec::new(),
            );
            self.builtin_feature_queries.insert(name.to_owned());
        }
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

    pub fn ignored_pragma_count(&self) -> u64 {
        self.ignored_pragma_count
    }

    /// Return the resolved source path registered for `file` in this
    /// preprocessing run.
    pub fn source_path(&self, file: FileId) -> Option<&str> {
        self.source_paths.get(&file).map(String::as_str)
    }

    pub fn define(&mut self, name: String, body: Vec<Token>) {
        self.builtin_feature_queries.remove(&name);
        self.macros.define(name, body, vec![]);
    }

    pub fn set_predefined_date(&mut self, date: &str) {
        self.macros
            .define("__DATE__".into(), vec![make_string_token(date)], vec![]);
    }

    pub fn undefine(&mut self, name: &str) {
        self.builtin_feature_queries.remove(name);
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
        self.counter = 0;
        self.poisoned_identifiers.clear();
        self.ignored_pragma_count = 0;
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
                    text: TokenText::Decoded(text),
                    ..token
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

    fn install_define_directive(&mut self, tokens: &[Token], i: &mut usize, source: &str) {
        let (name, body, is_function_like) = parse_define(tokens, i, source);
        if name.is_empty() {
            return;
        }
        let (params, variadic, variadic_param_name) = if is_function_like {
            let (params, variadic, variadic_name) = extract_params(&body, source);
            (params, variadic, variadic_name)
        } else {
            (Vec::new(), false, None)
        };
        let body_tokens = if !is_function_like {
            body
        } else {
            let mut depth = 0u32;
            let mut split_at = 0usize;
            for (index, token) in body.iter().enumerate() {
                match token.kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            split_at = index + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            body.get(split_at..).unwrap_or_default().to_vec()
        };
        let body_tokens = body_tokens
            .into_iter()
            .map(|token| {
                let lexeme = token_lexeme(&token, source);
                let text = if lexeme.is_empty() {
                    format_placeholder(token.kind)
                } else {
                    lexeme.to_owned()
                };
                Token {
                    text: TokenText::Decoded(text),
                    ..token
                }
            })
            .collect();
        self.builtin_feature_queries.remove(&name);
        if is_function_like {
            self.macros.define_function_like(
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

    fn process_macro_argument_directive(
        &mut self,
        tokens: &[Token],
        i: &mut usize,
        source: &str,
        expanding: &mut HashSet<String>,
        conditional_floor: usize,
    ) {
        let directive_span = tokens[*i].span;
        *i += 1;
        let directive = if *i < tokens.len()
            && !has_logical_newline_between(&tokens[*i - 1], &tokens[*i], source)
        {
            let name = token_lexeme(&tokens[*i], source).to_owned();
            *i += 1;
            name
        } else {
            String::new()
        };

        match directive.as_str() {
            "define" => {
                if self.cond_active() {
                    self.install_define_directive(tokens, i, source);
                }
            }
            "undef" => {
                if self.cond_active() && *i < tokens.len() {
                    let name = token_lexeme(&tokens[*i], source).to_owned();
                    self.undefine(&name);
                }
            }
            "ifdef" | "ifndef" => {
                let name = if *i < tokens.len() {
                    token_lexeme(&tokens[*i], source).to_owned()
                } else {
                    String::new()
                };
                let mut selected = self.is_defined(&name);
                if directive == "ifndef" {
                    selected = !selected;
                }
                self.push_cond(selected, directive_span);
            }
            "if" => {
                let expression = collect_line_tokens(tokens, i, source);
                let selected = self.cond_active()
                    && self.evaluate_conditional_expression(&expression, source, expanding);
                self.push_cond(selected, directive_span);
            }
            "elif" => {
                if self.cond_stack.len() <= conditional_floor {
                    self.push_unmatched_conditional(directive_span);
                } else {
                    let mut state = self
                        .cond_stack
                        .pop()
                        .expect("conditional stack was checked above");
                    let outer_active = self.cond_active();
                    if state.had_true {
                        state.in_true = false;
                    } else {
                        let expression = collect_line_tokens(tokens, i, source);
                        let selected = outer_active
                            && self.evaluate_conditional_expression(&expression, source, expanding);
                        state.in_true = selected;
                        state.had_true = selected;
                    }
                    self.cond_stack.push(state);
                }
            }
            "else" => {
                if self.cond_stack.len() <= conditional_floor {
                    self.push_unmatched_conditional(directive_span);
                } else {
                    let last = self.cond_stack.len() - 1;
                    let outer_active = self.cond_stack[..last].iter().all(|state| state.in_true);
                    let state = &mut self.cond_stack[last];
                    state.in_true = !state.had_true && outer_active;
                    state.had_true |= state.in_true;
                }
            }
            "endif" => {
                if self.cond_stack.len() <= conditional_floor {
                    self.push_unmatched_conditional(directive_span);
                } else {
                    self.cond_stack.pop();
                }
            }
            _ => {
                if self.cond_active() {
                    self.errors.push(PreprocessError {
                        code: "E0113",
                        span: directive_span,
                        message: "directive-in-macro-argument".into(),
                    });
                }
            }
        }
        skip_line(tokens, i, source);
    }

    fn collect_macro_arguments(
        &mut self,
        tokens: &[Token],
        start: usize,
        source: &str,
        expanding: &mut HashSet<String>,
    ) -> (Vec<Vec<Token>>, Vec<Token>, usize) {
        let conditional_floor = self.cond_stack.len();
        let mut args = Vec::new();
        let mut current_arg = Vec::new();
        let mut depth = 1u32;
        let mut index = start;

        while index < tokens.len() && depth > 0 {
            if is_directive_hash(tokens[index].kind) && is_start_of_line(tokens, index, source) {
                self.process_macro_argument_directive(
                    tokens,
                    &mut index,
                    source,
                    expanding,
                    conditional_floor,
                );
                continue;
            }
            if !self.cond_active() {
                index += 1;
                continue;
            }

            let lexeme = token_lexeme(&tokens[index], source);
            let text = if lexeme.is_empty() {
                format_placeholder(tokens[index].kind)
            } else {
                lexeme.to_owned()
            };
            let decoded = Token {
                text: TokenText::Decoded(text),
                ..tokens[index].clone()
            };
            match tokens[index].kind {
                TokenKind::LParen => {
                    depth += 1;
                    current_arg.push(decoded);
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
                _ => current_arg.push(decoded),
            }
            index += 1;
        }

        let unmatched = self
            .cond_stack
            .drain(conditional_floor..)
            .map(|state| state.opening_span)
            .collect::<Vec<_>>();
        for span in unmatched {
            self.push_unmatched_conditional(span);
        }

        (args, current_arg, index)
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
        let mut raw = tokens.into_vec();
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
                            self.install_define_directive(&raw, &mut i, source);
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
                            let argument = collect_line_tokens(&raw, &mut i, source);
                            let header = if let Some(header) = parse_header_name(&argument, source)
                            {
                                Some(header)
                            } else {
                                let expanded = self
                                    .process_impl(
                                        TokenStream::new(argument),
                                        source,
                                        expanding,
                                        false,
                                        0,
                                    )
                                    .into_vec();
                                parse_header_name(&expanded, source)
                            };
                            if let Some((include_text, include_kind)) = header {
                                let path = resolve_include(
                                    &include_text,
                                    include_kind,
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
                        let expr_tokens = collect_line_tokens(&raw, &mut i, source);
                        let value = self.cond_active()
                            && self.evaluate_conditional_expression(
                                &expr_tokens,
                                source,
                                expanding,
                            );
                        self.push_cond(value, tok.span);
                        skip_line(&raw, &mut i, source);
                    }
                    "elif" => {
                        if self.cond_stack.len() <= conditional_floor {
                            self.push_unmatched_conditional(tok.span);
                        } else {
                            let mut state = self
                                .cond_stack
                                .pop()
                                .expect("conditional stack was checked above");
                            let outer_active = self.cond_active();
                            if state.had_true {
                                state.in_true = false;
                            } else {
                                let line_tokens = collect_line_tokens(&raw, &mut i, source);
                                let value = outer_active
                                    && self.evaluate_conditional_expression(
                                        &line_tokens,
                                        source,
                                        expanding,
                                    );
                                state.in_true = value;
                                if value {
                                    state.had_true = true;
                                }
                            }
                            self.cond_stack.push(state);
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
                            let pragma_tokens = collect_line_tokens(&raw, &mut i, source);
                            if let Some(marker) =
                                self.process_pragma(&pragma_tokens, source, tok.span)
                            {
                                output.push(marker);
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
                if tok.kind == TokenKind::Error {
                    let (code, message) = if invalid_paste_spelling(tok).is_some() {
                        ("E0110", "invalid-token-paste")
                    } else {
                        ("E0001", "invalid token")
                    };
                    self.errors.push(PreprocessError {
                        code,
                        span: tok.span,
                        message: message.into(),
                    });
                    i += 1;
                    continue;
                }
                // Predefined context-dependent macros are expanded inline
                // rather than through the macro table because their values
                // change at each use.
                if tok.kind == TokenKind::Identifier {
                    let name = token_lexeme(tok, source).to_string();
                    if self.poisoned_identifiers.contains(&name) {
                        self.errors.push(PreprocessError {
                            code: "E0108",
                            span: tok.span,
                            message: "poisoned-identifier-used".into(),
                        });
                    }
                    if name == "_Pragma" {
                        if let Some((pragma_text, next_index)) =
                            parse_pragma_operator(&raw, i, source)
                        {
                            let pragma_tokens = lex_decoded_tokens(tok.span.file, &pragma_text);
                            if let Some(marker) =
                                self.process_pragma(&pragma_tokens, &pragma_text, tok.span)
                            {
                                output.push(marker);
                            }
                            i = next_index;
                            continue;
                        }
                    }
                    if name == "__COUNTER__" {
                        let value = self.counter;
                        self.counter = self.counter.wrapping_add(1);
                        output.push(synthetic_expansion_token(
                            make_int_token(i64::from(value)),
                            tok,
                            tok.span,
                        ));
                        i += 1;
                        continue;
                    }
                    if let Some((answer, next_index)) = self.expand_feature_query(&raw, i, source) {
                        let expansion_span = raw
                            .get(next_index.saturating_sub(1))
                            .map_or(tok.span, |close| {
                                compose_expansion_span(tok.span, close.span)
                            });
                        output.push(synthetic_expansion_token(
                            make_int_token(answer),
                            tok,
                            expansion_span,
                        ));
                        i = next_index;
                        continue;
                    }
                    if name == "__FILE__" {
                        let displayed_file = self.mapped_file_name();
                        let file_str = synthetic_expansion_token(
                            make_string_token(&displayed_file),
                            tok,
                            tok.span,
                        );
                        let sub_stream = TokenStream::new(vec![file_str]);
                        let processed = self.process_impl(sub_stream, source, expanding, false, 0);
                        output.extend(processed.into_vec());
                        i += 1;
                        continue;
                    }
                    if name == "__LINE__" {
                        let line_num = tok.span.line_start;
                        let line_tok = synthetic_expansion_token(
                            make_int_token(line_num as i64),
                            tok,
                            tok.span,
                        );
                        let sub_stream = TokenStream::new(vec![line_tok]);
                        let processed = self.process_impl(sub_stream, source, expanding, false, 0);
                        output.extend(processed.into_vec());
                        i += 1;
                        continue;
                    }
                    if !expanding.contains(&name) {
                        // Try object-like macro first.
                        if let Some(mut body) = self.macros.expand_object_like(&name) {
                            let invocation_whitespace = tok.preceded_by_whitespace;
                            if body.is_empty() {
                                if let Some(next) = raw.get_mut(i + 1) {
                                    next.preceded_by_whitespace |= invocation_whitespace;
                                }
                                i += 1;
                                continue;
                            }
                            stamp_macro_expansion(&mut body, tok.span, invocation_whitespace);
                            expanding.insert(name.clone());
                            let sub_stream = TokenStream::new(body);
                            let processed = self
                                .process_impl(sub_stream, source, expanding, false, 0)
                                .into_vec();
                            let (processed, consumed) = self.rescan_with_following_invocations(
                                processed,
                                &raw[i + 1..],
                                source,
                                expanding,
                            );
                            output.extend(processed);
                            expanding.remove(&name);
                            i += 1 + consumed;
                            continue;
                        }
                        // Try function-like macro: name followed by '('
                        if i + 1 < len && raw[i + 1].kind == TokenKind::LParen {
                            let saved_i = i;
                            i += 2; // skip name and '('
                            let (mut args, current_arg, j) =
                                self.collect_macro_arguments(&raw, i, source, expanding);
                            let expects_named_argument = self
                                .macros
                                .get(&name)
                                .is_some_and(|macro_def| !macro_def.params.is_empty());
                            if !current_arg.is_empty() || !args.is_empty() || expects_named_argument
                            {
                                args.push(current_arg);
                            }
                            i = j; // advance past ')' and arguments

                            let Some(prescan_mask) =
                                self.macros.argument_prescan_mask(&name, args.len())
                            else {
                                output.push(tok.clone());
                                i = saved_i + 1;
                                continue;
                            };
                            let mut prescanned_args = Vec::with_capacity(args.len());
                            for (argument, needs_prescan) in args.iter().zip(prescan_mask) {
                                if needs_prescan {
                                    let processed = self.process_impl(
                                        TokenStream::new(argument.clone()),
                                        source,
                                        expanding,
                                        false,
                                        0,
                                    );
                                    prescanned_args.push(processed.into_vec());
                                } else {
                                    prescanned_args.push(argument.clone());
                                }
                            }

                            if let Some(mut body) = self.macros.expand_macro_with_prescanned_args(
                                &name,
                                &args,
                                &prescanned_args,
                            ) {
                                let invocation_whitespace = tok.preceded_by_whitespace;
                                if body.is_empty() {
                                    if let Some(next) = raw.get_mut(i) {
                                        next.preceded_by_whitespace |= invocation_whitespace;
                                    }
                                    continue;
                                }
                                let invocation_span =
                                    raw.get(j.saturating_sub(1)).map_or(tok.span, |close| {
                                        compose_expansion_span(tok.span, close.span)
                                    });
                                stamp_macro_expansion(
                                    &mut body,
                                    invocation_span,
                                    invocation_whitespace,
                                );
                                expanding.insert(name.clone());
                                let sub_stream = TokenStream::new(body);
                                let processed = self
                                    .process_impl(sub_stream, source, expanding, false, 0)
                                    .into_vec();
                                let (processed, consumed) = self.rescan_with_following_invocations(
                                    processed,
                                    &raw[i..],
                                    source,
                                    expanding,
                                );
                                output.extend(processed);
                                expanding.remove(&name);
                                i += consumed;
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

    /// Complete translation-phase-4 rescanning at a replacement-list
    /// boundary. If rescanning leaves a function-like macro name at the end,
    /// its argument list may begin in the surrounding token sequence. Consume
    /// only those invocation tokens; unrelated following tokens remain in the
    /// caller where the currently expanding macro is enabled again.
    fn rescan_with_following_invocations(
        &mut self,
        mut replacement: Vec<Token>,
        following: &[Token],
        source: &str,
        expanding: &mut HashSet<String>,
    ) -> (Vec<Token>, usize) {
        let mut consumed = 0;
        while let Some(last) = replacement.last() {
            if last.kind != TokenKind::Identifier {
                break;
            }
            let name = token_lexeme(last, source).to_owned();
            if expanding.contains(&name)
                || !self
                    .macros
                    .get(&name)
                    .is_some_and(|macro_def| macro_def.is_function)
            {
                break;
            }
            let Some(invocation_len) = complete_parenthesized_prefix(&following[consumed..]) else {
                break;
            };
            replacement.extend_from_slice(&following[consumed..consumed + invocation_len]);
            consumed += invocation_len;
            replacement = self
                .process_impl(TokenStream::new(replacement), source, expanding, false, 0)
                .into_vec();
        }
        (replacement, consumed)
    }

    fn expand_feature_query(
        &mut self,
        tokens: &[Token],
        index: usize,
        source: &str,
    ) -> Option<(i64, usize)> {
        let query = token_lexeme(tokens.get(index)?, source);
        if !self.builtin_feature_queries.contains(query)
            || tokens.get(index + 1).map(|token| token.kind) != Some(TokenKind::LParen)
        {
            return None;
        }

        let mut close = index + 2;
        let mut nested = false;
        while close < tokens.len() && tokens[close].kind != TokenKind::RParen {
            if matches!(tokens[close].kind, TokenKind::LParen | TokenKind::RParen) {
                nested = true;
            }
            close += 1;
        }
        let next_index = if close < tokens.len() {
            close + 1
        } else {
            tokens.len()
        };
        let arguments = tokens.get(index + 2..close).unwrap_or_default();
        let answer = if close >= tokens.len() || nested {
            None
        } else {
            match query {
                "__has_attribute" => one_query_token(arguments, source)
                    .map(normalize_attribute_name)
                    .map(attribute_answer),
                "__has_builtin" => one_query_token(arguments, source)
                    .map(|name| i64::from(HAS_BUILTIN_NAMES.lines().any(|known| known == name))),
                "__has_feature" => one_query_token(arguments, source).map(|_| 0),
                "__has_include" => parse_header_name(arguments, source).map(|(name, kind)| {
                    i64::from(
                        resolve_include(
                            &name,
                            kind,
                            &self.current_file,
                            &self.include_paths,
                            self.standard_includes,
                        )
                        .is_some(),
                    )
                }),
                _ => None,
            }
        };

        if let Some(answer) = answer {
            Some((answer, next_index))
        } else {
            self.errors.push(PreprocessError {
                code: "E0106",
                span: tokens[index].span,
                message: "feature-query-malformed".into(),
            });
            Some((0, next_index))
        }
    }

    fn process_pragma(
        &mut self,
        tokens: &[Token],
        source: &str,
        span: SourceSpan,
    ) -> Option<Token> {
        let first = tokens
            .first()
            .map(|token| token_lexeme(token, source))
            .unwrap_or_default();
        let second = tokens
            .get(1)
            .map(|token| token_lexeme(token, source))
            .unwrap_or_default();

        if first == "pack" {
            if let Some(action) = parse_pragma_pack_action(tokens, source) {
                return Some(make_pragma_pack_marker(action, span));
            }
            self.errors.push(PreprocessError {
                code: "E0107",
                span,
                message: "pragma-pack-unsupported".into(),
            });
            return None;
        }

        match (first, second) {
            ("once", _) => self.pragma_once(),
            ("GCC", "poison") => {
                for token in tokens.iter().skip(2) {
                    if token.kind == TokenKind::Identifier {
                        let name = token_lexeme(token, source);
                        if !name.is_empty() {
                            self.poisoned_identifiers.insert(name.to_owned());
                        }
                    }
                }
            }
            ("GCC", "diagnostic" | "visibility") => {
                self.ignored_pragma_count = self.ignored_pragma_count.saturating_add(1);
            }
            _ => {
                self.ignored_pragma_count = self.ignored_pragma_count.saturating_add(1);
            }
        }
        None
    }

    fn evaluate_conditional_expression(
        &mut self,
        tokens: &[Token],
        source: &str,
        expanding: &mut HashSet<String>,
    ) -> bool {
        let defined_replaced = replace_defined_operators(tokens, &self.macros, source);
        let expanded = self.process_impl(
            TokenStream::new(defined_replaced),
            source,
            expanding,
            false,
            0,
        );
        let expanded = expanded.into_vec();
        if let Some(token) = expanded.iter().find(|token| {
            matches!(
                token.kind,
                TokenKind::IntLiteral | TokenKind::FloatLiteral | TokenKind::PpNumber
            ) && parse_pp_integer_checked(token_lexeme(token, source)).is_none()
        }) {
            self.errors.push(PreprocessError {
                code: "E0111",
                span: token.span,
                message: "conditional-operand-not-integer".into(),
            });
            return false;
        }
        evaluate_if_expr(&expanded, source)
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

fn parse_pragma_pack_action(tokens: &[Token], source: &str) -> Option<PragmaPackAction> {
    if tokens.first().map(|token| token_lexeme(token, source)) != Some("pack")
        || tokens.get(1).map(|token| token.kind) != Some(TokenKind::LParen)
        || tokens.last().map(|token| token.kind) != Some(TokenKind::RParen)
    {
        return None;
    }
    let arguments = tokens.get(2..tokens.len().checked_sub(1)?)?;
    match arguments {
        [] => Some(PragmaPackAction::Reset),
        [argument] if token_lexeme(argument, source) == "push" => {
            Some(PragmaPackAction::Push(None))
        }
        [argument] if token_lexeme(argument, source) == "pop" => Some(PragmaPackAction::Pop),
        [alignment] => pragma_pack_alignment(alignment, source).map(PragmaPackAction::Set),
        [push, comma, alignment]
            if token_lexeme(push, source) == "push" && comma.kind == TokenKind::Comma =>
        {
            pragma_pack_alignment(alignment, source)
                .map(|alignment| PragmaPackAction::Push(Some(alignment)))
        }
        _ => None,
    }
}

fn pragma_pack_alignment(token: &Token, source: &str) -> Option<u32> {
    if token.kind != TokenKind::IntLiteral {
        return None;
    }
    token_lexeme(token, source)
        .parse::<u32>()
        .ok()
        .filter(|alignment| alignment.is_power_of_two())
}

fn make_pragma_pack_marker(action: PragmaPackAction, span: SourceSpan) -> Token {
    let action = match action {
        PragmaPackAction::Set(alignment) => format!("set={alignment}"),
        PragmaPackAction::Reset => "reset".to_owned(),
        PragmaPackAction::Push(None) => "push".to_owned(),
        PragmaPackAction::Push(Some(alignment)) => format!("push={alignment}"),
        PragmaPackAction::Pop => "pop".to_owned(),
    };
    Token::new(
        TokenKind::Identifier,
        span,
        TokenText::Decoded(format!("{PRAGMA_PACK_MARKER_PREFIX}{action}")),
    )
}

/// Extract the source text of a token from the original source buffer using
/// its span. Returns "" for dummy spans or out-of-bounds.
fn token_lexeme<'a>(tok: &'a Token, source: &'a str) -> &'a str {
    if let TokenText::Decoded(text) = &tok.text {
        return text;
    }
    let start = tok.spelling_span.byte_start as usize;
    let end = tok.spelling_span.byte_end as usize;
    if start >= source.len() || end > source.len() || start > end {
        return "";
    }
    &source[start..end]
}

/// Compose parser-visible expansion locations with the same total rule used
/// for C-HIR nodes. Cross-file or reversed endpoints keep the first span.
fn compose_expansion_span(first: SourceSpan, last: SourceSpan) -> SourceSpan {
    if first.file != last.file || last.byte_start < first.byte_start {
        return first;
    }
    SourceSpan::new(
        first.file,
        first.byte_start,
        last.byte_end,
        first.line_start,
        first.column_start,
    )
}

fn complete_parenthesized_prefix(tokens: &[Token]) -> Option<usize> {
    if tokens.first().map(|token| token.kind) != Some(TokenKind::LParen) {
        return None;
    }
    let mut depth = 0_u32;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LParen => depth += 1,
            TokenKind::RParen => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn stamp_macro_expansion(
    tokens: &mut [Token],
    expansion_span: SourceSpan,
    invocation_preceded_by_whitespace: bool,
) {
    for token in tokens.iter_mut() {
        // An invalid paste is consumed as a preprocessor diagnostic at the
        // written `##` operator; it never crosses the parser boundary as an
        // expanded token.
        if invalid_paste_spelling(token).is_none() {
            token.span = expansion_span;
        }
    }
    if let Some(first) = tokens.first_mut() {
        first.preceded_by_whitespace = invocation_preceded_by_whitespace;
    }
}

fn synthetic_expansion_token(
    mut token: Token,
    invocation: &Token,
    expansion_span: SourceSpan,
) -> Token {
    token.spelling_span = invocation.spelling_span;
    token.span = expansion_span;
    token.preceded_by_whitespace = invocation.preceded_by_whitespace;
    token
}

/// Check whether a token kind marks the start of a preprocessor directive line.
fn is_directive_hash(kind: TokenKind) -> bool {
    kind == TokenKind::Hash
}

/// Return whether the physical source gap contains a newline that survives C
/// translation phase 2. Backslash-LF and backslash-CRLF pairs do not end a
/// preprocessor directive, even though the following token has a different
/// physical `line_start`.
fn has_logical_newline_between(previous: &Token, current: &Token, source: &str) -> bool {
    let start = previous.spelling_span.byte_end as usize;
    let end = current.spelling_span.byte_start as usize;
    if start > end || end > source.len() {
        return previous.spelling_span.line_start != current.spelling_span.line_start;
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
    let start = previous.spelling_span.byte_end as usize;
    let end = current.spelling_span.byte_start as usize;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncludeKind {
    Quote,
    Angle,
}

/// Read the argument of an #include directive. Returns the filename without
/// delimiters and the search kind.
fn read_include_argument(
    tokens: &[Token],
    i: &mut usize,
    source: &str,
) -> Option<(String, IncludeKind)> {
    if *i < tokens.len() {
        let tok = &tokens[*i];
        let text = token_lexeme(tok, source);
        if tok.kind == TokenKind::StringLiteral {
            // "file" → strip quotes
            let trimmed = text.trim_matches('"');
            *i += 1;
            return Some((trimmed.to_owned(), IncludeKind::Quote));
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
            return Some((path, IncludeKind::Angle));
        }
    }
    None
}

/// Try to resolve an include file name to an absolute path.
/// For quoted includes, first search relative to the current directory,
/// then the include path list. For angle-bracket includes, only search
/// the include path list.
fn resolve_include(
    name: &str,
    kind: IncludeKind,
    current_file: &str,
    include_paths: &[String],
    standard_includes: bool,
) -> Option<String> {
    // Quoted include: try beside the active source, then the process working
    // directory. `read_include_argument` has already removed the delimiters.
    if kind == IncludeKind::Quote && !name.starts_with('/') {
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

fn one_query_token<'a>(tokens: &'a [Token], source: &'a str) -> Option<&'a str> {
    if tokens.len() == 1 {
        let spelling = token_lexeme(&tokens[0], source);
        (!spelling.is_empty()).then_some(spelling)
    } else {
        None
    }
}

fn normalize_attribute_name(name: &str) -> &str {
    name.strip_prefix("__")
        .and_then(|inner| inner.strip_suffix("__"))
        .filter(|inner| !inner.is_empty())
        .unwrap_or(name)
}

fn attribute_answer(name: &str) -> i64 {
    HAS_ATTRIBUTE_ANSWERS
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .find_map(|(known, answer)| {
            (known == name)
                .then(|| answer.parse::<i64>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn parse_header_name(tokens: &[Token], source: &str) -> Option<(String, IncludeKind)> {
    if tokens.len() == 1 && tokens[0].kind == TokenKind::StringLiteral {
        let spelling = token_lexeme(&tokens[0], source);
        return spelling
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .map(|name| (name.to_owned(), IncludeKind::Quote));
    }
    if tokens.len() >= 3
        && tokens.first().map(|token| token.kind) == Some(TokenKind::Less)
        && tokens.last().map(|token| token.kind) == Some(TokenKind::Greater)
        && tokens[1..tokens.len() - 1]
            .iter()
            .all(|token| !matches!(token.kind, TokenKind::Less | TokenKind::Greater))
    {
        let name = tokens[1..tokens.len() - 1]
            .iter()
            .map(|token| token_lexeme(token, source))
            .collect::<String>();
        return (!name.is_empty()).then_some((name, IncludeKind::Angle));
    }
    None
}

fn parse_pragma_operator(tokens: &[Token], index: usize, source: &str) -> Option<(String, usize)> {
    if tokens.get(index + 1).map(|token| token.kind) != Some(TokenKind::LParen)
        || tokens.get(index + 2).map(|token| token.kind) != Some(TokenKind::StringLiteral)
        || tokens.get(index + 3).map(|token| token.kind) != Some(TokenKind::RParen)
    {
        return None;
    }
    let spelling = token_lexeme(&tokens[index + 2], source);
    let inner = spelling
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))?;
    let mut destringized = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            destringized.push(character);
            continue;
        }
        let Some(escaped) = characters.next() else {
            destringized.push('\\');
            break;
        };
        if matches!(escaped, '\\' | '"') {
            destringized.push(escaped);
        } else {
            destringized.push('\\');
            destringized.push(escaped);
        }
    }
    Some((destringized, index + 4))
}

fn lex_decoded_tokens(file: FileId, source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Eof {
            break;
        }
        let spelling = token_lexeme(&token, source).to_owned();
        tokens.push(Token {
            text: TokenText::Decoded(spelling),
            ..token
        });
    }
    tokens
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

fn replace_defined_operators(tokens: &[Token], macros: &MacroMap, source: &str) -> Vec<Token> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        if token_lexeme(&tokens[index], source) != "defined" {
            result.push(tokens[index].clone());
            index += 1;
            continue;
        }

        let (name_index, next_index) = if tokens.get(index + 1).map(|token| token.kind)
            == Some(TokenKind::LParen)
            && tokens.get(index + 2).is_some()
            && tokens.get(index + 3).map(|token| token.kind) == Some(TokenKind::RParen)
        {
            (index + 2, index + 4)
        } else if tokens.get(index + 1).is_some() {
            (index + 1, index + 2)
        } else {
            result.push(tokens[index].clone());
            index += 1;
            continue;
        };
        let name = token_lexeme(&tokens[name_index], source);
        let mut answer = make_int_token(i64::from(macros.contains(name)));
        answer.span = tokens[index].span;
        result.push(answer);
        index = next_index;
    }
    result
}

#[derive(Debug, Clone, Copy)]
struct PpInteger {
    bits: u64,
    unsigned: bool,
}

impl PpInteger {
    const ZERO: Self = Self {
        bits: 0,
        unsigned: false,
    };

    fn from_bool(value: bool) -> Self {
        Self {
            bits: u64::from(value),
            unsigned: false,
        }
    }

    fn is_true(self) -> bool {
        self.bits != 0
    }

    fn common_bits(self, other: Self) -> (u64, u64, bool) {
        (self.bits, other.bits, self.unsigned || other.unsigned)
    }

    fn compare(self, other: Self, compare: impl FnOnce(i128, i128) -> bool) -> Self {
        let (left, right, unsigned) = self.common_bits(other);
        let (left, right) = if unsigned {
            (i128::from(left), i128::from(right))
        } else {
            (i128::from(left as i64), i128::from(right as i64))
        };
        Self::from_bool(compare(left, right))
    }
}

struct IfExpression<'a> {
    tokens: &'a [Token],
    source: &'a str,
    index: usize,
}

impl<'a> IfExpression<'a> {
    fn new(tokens: &'a [Token], source: &'a str) -> Self {
        Self {
            tokens,
            source,
            index: 0,
        }
    }

    fn take(&mut self, kind: TokenKind) -> bool {
        if self.tokens.get(self.index).map(|token| token.kind) == Some(kind) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn conditional(&mut self) -> PpInteger {
        let condition = self.logical_or();
        if !self.take(TokenKind::Question) {
            return condition;
        }
        let when_true = self.conditional();
        if !self.take(TokenKind::Colon) {
            return PpInteger::ZERO;
        }
        let when_false = self.conditional();
        if condition.is_true() {
            when_true
        } else {
            when_false
        }
    }

    fn logical_or(&mut self) -> PpInteger {
        let mut value = self.logical_and();
        while self.take(TokenKind::PipePipe) {
            let right = self.logical_and();
            value = PpInteger::from_bool(value.is_true() || right.is_true());
        }
        value
    }

    fn logical_and(&mut self) -> PpInteger {
        let mut value = self.bitwise_or();
        while self.take(TokenKind::AmpAmp) {
            let right = self.bitwise_or();
            value = PpInteger::from_bool(value.is_true() && right.is_true());
        }
        value
    }

    fn bitwise_or(&mut self) -> PpInteger {
        let mut value = self.bitwise_xor();
        while self.take(TokenKind::Pipe) {
            let right = self.bitwise_xor();
            value = PpInteger {
                bits: value.bits | right.bits,
                unsigned: value.unsigned || right.unsigned,
            };
        }
        value
    }

    fn bitwise_xor(&mut self) -> PpInteger {
        let mut value = self.bitwise_and();
        while self.take(TokenKind::Caret) {
            let right = self.bitwise_and();
            value = PpInteger {
                bits: value.bits ^ right.bits,
                unsigned: value.unsigned || right.unsigned,
            };
        }
        value
    }

    fn bitwise_and(&mut self) -> PpInteger {
        let mut value = self.equality();
        while self.take(TokenKind::Amp) {
            let right = self.equality();
            value = PpInteger {
                bits: value.bits & right.bits,
                unsigned: value.unsigned || right.unsigned,
            };
        }
        value
    }

    fn equality(&mut self) -> PpInteger {
        let mut value = self.relational();
        loop {
            if self.take(TokenKind::EqEq) {
                let right = self.relational();
                value = value.compare(right, |left, right| left == right);
            } else if self.take(TokenKind::NotEq) {
                let right = self.relational();
                value = value.compare(right, |left, right| left != right);
            } else {
                return value;
            }
        }
    }

    fn relational(&mut self) -> PpInteger {
        let mut value = self.shift();
        loop {
            if self.take(TokenKind::Less) {
                let right = self.shift();
                value = value.compare(right, |left, right| left < right);
            } else if self.take(TokenKind::LessEq) {
                let right = self.shift();
                value = value.compare(right, |left, right| left <= right);
            } else if self.take(TokenKind::Greater) {
                let right = self.shift();
                value = value.compare(right, |left, right| left > right);
            } else if self.take(TokenKind::GreaterEq) {
                let right = self.shift();
                value = value.compare(right, |left, right| left >= right);
            } else {
                return value;
            }
        }
    }

    fn shift(&mut self) -> PpInteger {
        let mut value = self.additive();
        loop {
            if self.take(TokenKind::ShiftLeft) {
                let right = self.additive();
                value.bits = if right.bits < 64 {
                    value.bits.wrapping_shl(right.bits as u32)
                } else {
                    0
                };
            } else if self.take(TokenKind::ShiftRight) {
                let right = self.additive();
                value.bits = if right.bits < 64 {
                    if value.unsigned {
                        value.bits >> right.bits
                    } else {
                        ((value.bits as i64) >> right.bits) as u64
                    }
                } else {
                    0
                };
            } else {
                return value;
            }
        }
    }

    fn additive(&mut self) -> PpInteger {
        let mut value = self.multiplicative();
        loop {
            if self.take(TokenKind::Plus) {
                let right = self.multiplicative();
                value.bits = value.bits.wrapping_add(right.bits);
                value.unsigned |= right.unsigned;
            } else if self.take(TokenKind::Minus) {
                let right = self.multiplicative();
                value.bits = value.bits.wrapping_sub(right.bits);
                value.unsigned |= right.unsigned;
            } else {
                return value;
            }
        }
    }

    fn multiplicative(&mut self) -> PpInteger {
        let mut value = self.unary();
        loop {
            if self.take(TokenKind::Star) {
                let right = self.unary();
                value.bits = value.bits.wrapping_mul(right.bits);
                value.unsigned |= right.unsigned;
            } else if self.take(TokenKind::Slash) {
                let right = self.unary();
                value = pp_divide(value, right);
            } else if self.take(TokenKind::Percent) {
                let right = self.unary();
                value = pp_remainder(value, right);
            } else {
                return value;
            }
        }
    }

    fn unary(&mut self) -> PpInteger {
        if self.take(TokenKind::Plus) {
            return self.unary();
        }
        if self.take(TokenKind::Minus) {
            let mut value = self.unary();
            value.bits = value.bits.wrapping_neg();
            return value;
        }
        if self.take(TokenKind::Bang) {
            return PpInteger::from_bool(!self.unary().is_true());
        }
        if self.take(TokenKind::Tilde) {
            let mut value = self.unary();
            value.bits = !value.bits;
            return value;
        }
        self.primary()
    }

    fn primary(&mut self) -> PpInteger {
        let Some(token) = self.tokens.get(self.index) else {
            return PpInteger::ZERO;
        };
        if token.kind == TokenKind::LParen {
            self.index += 1;
            let value = self.conditional();
            let _ = self.take(TokenKind::RParen);
            return value;
        }
        self.index += 1;
        match token.kind {
            TokenKind::IntLiteral | TokenKind::PpNumber => {
                parse_pp_integer(token_lexeme(token, self.source))
            }
            TokenKind::CharLiteral => parse_pp_character(token_lexeme(token, self.source)),
            _ => PpInteger::ZERO,
        }
    }
}

fn pp_divide(left: PpInteger, right: PpInteger) -> PpInteger {
    let unsigned = left.unsigned || right.unsigned;
    if right.bits == 0 {
        return PpInteger { bits: 0, unsigned };
    }
    let bits = if unsigned {
        left.bits / right.bits
    } else {
        let dividend = left.bits as i64;
        let divisor = right.bits as i64;
        if dividend == i64::MIN && divisor == -1 {
            i64::MIN as u64
        } else {
            (dividend / divisor) as u64
        }
    };
    PpInteger { bits, unsigned }
}

fn pp_remainder(left: PpInteger, right: PpInteger) -> PpInteger {
    let unsigned = left.unsigned || right.unsigned;
    if right.bits == 0 {
        return PpInteger { bits: 0, unsigned };
    }
    let bits = if unsigned {
        left.bits % right.bits
    } else {
        let dividend = left.bits as i64;
        let divisor = right.bits as i64;
        if dividend == i64::MIN && divisor == -1 {
            0
        } else {
            (dividend % divisor) as u64
        }
    };
    PpInteger { bits, unsigned }
}

fn parse_pp_integer(spelling: &str) -> PpInteger {
    parse_pp_integer_checked(spelling).unwrap_or(PpInteger::ZERO)
}

fn parse_pp_integer_checked(spelling: &str) -> Option<PpInteger> {
    let suffix_start = spelling
        .char_indices()
        .rev()
        .find(|(_, character)| !matches!(character, 'u' | 'U' | 'l' | 'L'))
        .map_or(0, |(index, character)| index + character.len_utf8());
    let (digits, suffix) = spelling.split_at(suffix_start);
    let explicitly_unsigned = suffix
        .chars()
        .any(|character| matches!(character, 'u' | 'U'));
    let normalized_suffix = suffix.to_ascii_lowercase();
    if !matches!(
        normalized_suffix.as_str(),
        "" | "u" | "l" | "ul" | "lu" | "ll" | "ull" | "llu"
    ) {
        return None;
    }
    let (radix, digits) = if let Some(rest) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (2, rest)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    if digits.is_empty() {
        return None;
    }
    let bits = u64::from_str_radix(digits, radix).ok()?;
    Some(PpInteger {
        bits,
        unsigned: explicitly_unsigned || (radix != 10 && bits > i64::MAX as u64),
    })
}

fn parse_pp_character(spelling: &str) -> PpInteger {
    let inner = spelling
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or(spelling);
    let value = match inner.as_bytes() {
        [b'\\', b'n'] => b'\n',
        [b'\\', b'r'] => b'\r',
        [b'\\', b't'] => b'\t',
        [b'\\', b'0'] => 0,
        [b'\\', escaped] => *escaped,
        [single] => *single,
        _ => 0,
    };
    PpInteger {
        bits: u64::from(value),
        unsigned: false,
    }
}

fn evaluate_if_expr(tokens: &[Token], source: &str) -> bool {
    IfExpression::new(tokens, source).conditional().is_true()
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

/// Spell a macro argument for `#` stringification.
///
/// Whitespace between preprocessing tokens collapses to one space, while
/// adjacent source tokens remain adjacent. Backslashes and quotes are escaped
/// only inside character/string literal spellings; an `other` backslash token
/// is preserved verbatim, matching the preprocessing-token grammar.
fn tokens_to_string(tokens: &[Token]) -> String {
    let mut result = String::new();
    for (index, token) in tokens.iter().enumerate() {
        if index > 0 && token.preceded_by_whitespace {
            result.push(' ');
        }
        let spelling = token_text(token);
        if matches!(
            token.kind,
            TokenKind::CharLiteral | TokenKind::StringLiteral
        ) {
            for character in spelling.chars() {
                if matches!(character, '\\' | '"') {
                    result.push('\\');
                }
                result.push(character);
            }
        } else {
            result.push_str(&spelling);
        }
    }
    result
}

/// Re-lex a token-paste result. The production lexer recognizes complete C
/// tokens; the extra pp-number path preserves preprocessing tokens such as
/// `1_MODULE` until a later consuming context can classify them.
fn paste_tokens(left: &Token, right: &Token, operator_span: SourceSpan) -> Token {
    let combined = format!("{}{}", token_text(left), token_text(right));
    let mut lexer = Lexer::new(FileId::DUMMY, &combined);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind != TokenKind::Eof {
            tokens.push(token);
        }
    }
    let kind = if tokens.len() == 1 && tokens[0].kind != TokenKind::Error {
        tokens.remove(0).kind
    } else if preprocessing_number_len_at(&combined, 0) == Some(combined.len()) {
        TokenKind::PpNumber
    } else {
        TokenKind::Error
    };
    let (span, text) = if kind == TokenKind::Error {
        (
            operator_span,
            TokenText::Decoded(format!("{INVALID_PASTE_MARKER_PREFIX}{combined}")),
        )
    } else {
        (left.span, TokenText::Decoded(combined))
    };
    let mut pasted = Token::new(kind, span, text);
    if kind != TokenKind::Error {
        pasted.spelling_span = left.spelling_span;
        pasted.span = left.span;
        pasted.preceded_by_whitespace = left.preceded_by_whitespace;
    }
    pasted
}

fn invalid_paste_spelling(token: &Token) -> Option<&str> {
    let TokenText::Decoded(text) = &token.text else {
        return None;
    };
    text.strip_prefix(INVALID_PASTE_MARKER_PREFIX)
}

fn make_string_token(text: &str) -> Token {
    Token::new(
        TokenKind::StringLiteral,
        SourceSpan::new(FileId::DUMMY, 0, 0, 0, 0),
        TokenText::Decoded(format!("\"{}\"", text)),
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
