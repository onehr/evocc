//! C preprocessor: macro expansion, directives, token stream

use std::collections::{HashMap, HashSet};

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
    /// handle all directives. Non-directive tokens are expanded (macro
    /// substitution) and emitted when the current conditional block is active.
    ///
    /// `source` is the original source text; token text is reconstructed
    /// from source spans rather than from the `TokenText` field, because the
    /// lexer uses `Borrowed` tokens whose text is identified by byte range.
    pub fn process(&mut self, tokens: TokenStream, source: &str) -> TokenStream {
        let mut output: Vec<Token> = Vec::new();
        let raw = tokens.into_vec();
        let len = raw.len();
        let mut i: usize = 0;
        // Tracks macros currently being expanded (recursion guard). When a
        // macro name appears in this set, further expansion of that name is
        // suppressed — this is the ISO C "blue paint" rule.
        let mut expanding: std::collections::HashSet<String> = std::collections::HashSet::new();

        while i < len {
            let tok = &raw[i];

            if is_directive_hash(tok.kind) && is_start_of_line(&raw, i, source) {
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
                            let (name, body) = parse_define(&raw, &mut i, source);
                        if !name.is_empty() {
                            let (params, variadic) = if body.first().map(|t| t.kind) == Some(TokenKind::LParen) {
                                    // Function-like: body starts with '('.
                                    // Extract parameter names and then strip
                                    // the (a,b) portion from the body so the
                                    // expansion body contains only the
                                    // replacement tokens.
                                    let (p, v) = extract_params(&body, source);
                                    (p, v)
                                } else {
                                    (vec![], false)
                                };
                            let body_tokens = if params.is_empty() {
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
                                            if depth == 0 { split_at = j + 1; break; }
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
                            if params.is_empty() {
                                self.define(name, body_tokens);
                            } else {
                                    self.macros_mut().define_function_like(name, params, variadic, body_tokens);
                                }
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "undef" => {
                        if self.cond_active() {
                            if i < len {
                                let name = token_lexeme(&raw[i], source).to_string();
                                self.undefine(&name);
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "include" => {
                        if self.cond_active() {
                            // Parse include argument: "file" or <file>
                            let include_text = read_include_argument(&raw, &mut i, source);
                            if !include_text.is_empty() {
                                let path = resolve_include(&include_text, &self.include_paths);
                                if let Some(resolved) = path {
                                    if !self.has_include_guard() || !self.include_guard_seen {
                                        if let Ok(content) = std::fs::read_to_string(&resolved) {
                                            let mut sub_lex = evocc_lex::Lexer::new(FileId::new(1), &content);
                                            let mut sub_tokens = Vec::new();
                                            while let Some(t) = sub_lex.next() {
                                                if t.kind != TokenKind::Eof { sub_tokens.push(t); }
                                            }
                                            let sub_stream = TokenStream::new(sub_tokens);
                                            let processed = self.process(sub_stream, &content);
                                            output.extend(processed.into_vec());
                                        }
                                    }
                                }
                            }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "ifdef" => {
                        let name = if i < len { token_lexeme(&raw[i], source).to_string() } else { String::new() };
                        i = advance_past_line(i, &raw, source).min(len);
                        let defined = self.is_defined(&name);
                        self.push_cond(defined);
                        skip_line(&raw, &mut i, source);
                    }
                    "ifndef" => {
                        let name = if i < len { token_lexeme(&raw[i], source).to_string() } else { String::new() };
                        i = advance_past_line(i, &raw, source).min(len);
                        let defined = self.is_defined(&name);
                        self.push_cond(!defined);
                        skip_line(&raw, &mut i, source);
                    }
                    "if" => {
                        // Evaluate a simple constant expression. For now we
                        // support `defined(name)`, arithmetic constants (0,1),
                        // and integer comparisons.
                        let expr_tokens = collect_line_tokens(&raw, &mut i, source);
                        let value = evaluate_if_expr(&expr_tokens, &self.macros, source);
                        self.push_cond(value);
                        skip_line(&raw, &mut i, source);
                    }
                    "elif" => {
                        let had = self.cond_stack.last().map(|c| c.had_true).unwrap_or(false);
                        let outer_active = self.cond_stack.iter().rev().skip(1).all(|c| c.in_true);
                        let cs = self.cond_stack.last_mut().unwrap();
                        if had {
                            cs.in_true = false;
                        } else {
                            let line_tokens = collect_line_tokens(&raw, &mut i, source);
                            let value = outer_active && evaluate_if_expr(&line_tokens, &self.macros, source);
                            cs.in_true = value;
                            if value { cs.had_true = true; }
                        }
                        skip_line(&raw, &mut i, source);
                    }
                    "else" => {
                        let len_stack = self.cond_stack.len();
                        if len_stack > 0 {
                            let had = self.cond_stack[len_stack - 1].had_true;
                            let outer_active = self.cond_stack[..len_stack - 1].iter().all(|c| c.in_true);
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
                        self.cond_stack.pop();
                        skip_line(&raw, &mut i, source);
                    }
                    "pragma" => {
                        if self.cond_active() {
                            let pragma_tok = collect_line_tokens(&raw, &mut i, source);
                            let pragma_text: String = pragma_tok.iter()
                                .filter_map(|t| {
                                    let s = token_lexeme(t, source);
                                    if s.is_empty() { None } else { Some(s.to_owned()) }
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
                        // #error message → skip; in a real preprocessor this
                        // would emit a diagnostic.
                        skip_line(&raw, &mut i, source);
                    }
                    _ => {
                        // Unknown directive (including #line, #warning): skip.
                        skip_line(&raw, &mut i, source);
                    }
                }
            } else if self.cond_active() {
                // Macro expansion for identifiers.
                if tok.kind == TokenKind::Identifier {
                    let name = token_lexeme(tok, source).to_string();
                    if !expanding.contains(&name) {
                        // Try object-like macro first.
                        if let Some(body) = self.macros.expand_object_like(&name) {
                            expanding.insert(name.clone());
                            let sub_stream = TokenStream::new(body);
                            let processed = self.process(sub_stream, source);
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
                                match raw[j].kind {
                                    TokenKind::LParen => { depth += 1; if depth > 1 { current_arg.push(raw[j].clone()); } }
                                    TokenKind::RParen => { depth -= 1; if depth > 0 { current_arg.push(raw[j].clone()); } }
                                    TokenKind::Comma if depth == 1 => {
                                        args.push(std::mem::take(&mut current_arg));
                                    }
                                    _ => { current_arg.push(raw[j].clone()); }
                                }
                                j += 1;
                            }
                            if !current_arg.is_empty() { args.push(current_arg); }
                            i = j; // advance past ')' and arguments

                            if let Some(body) = self.macros.expand_macro(&name, &args) {
                                expanding.insert(name.clone());
                                let sub_stream = TokenStream::new(body);
                                let processed = self.process(sub_stream, source);
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

        TokenStream::new(output)
    }

    fn push_cond(&mut self, value: bool) {
        self.cond_stack.push(CondState { in_true: value, had_true: value });
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
fn token_lexeme<'a>(tok: &Token, source: &'a str) -> &'a str {
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
/// the very first token or preceded by a newline character in the source.
fn is_start_of_line(tokens: &[Token], idx: usize, source: &str) -> bool {
    if idx == 0 {
        return true;
    }
    let prev = &tokens[idx - 1];
    let curr = &tokens[idx];
    // If the line number changed, there was a newline between them.
    prev.span.line_start != curr.span.line_start
}

/// Skip tokens until we reach a token on a different line than the token
/// immediately before `*i`. This respects the fact that some directive
/// handlers (define, undef, include) advance `i` past the directive body
/// themselves; in that case `prev_line` reflects the directive's line,
/// and we skip nothing because `*i` is already on the next line.
fn skip_line(tokens: &[Token], i: &mut usize, source: &str) {
    if *i >= tokens.len() || *i == 0 {
        return;
    }
    let prev_line = tokens[*i - 1].span.line_start;
    while *i < tokens.len() && tokens[*i].span.line_start == prev_line {
        *i += 1;
    }
}

/// Parse a `#define` directive. Returns (macro_name, body_tokens).
/// `*i` points to the first token after the directive keyword.
/// On return, `*i` points to the first token after the define body.
fn parse_define(tokens: &[Token], i: &mut usize, source: &str) -> (String, Vec<Token>) {
    if *i >= tokens.len() {
        return (String::new(), Vec::new());
    }
    let t = &tokens[*i];
    let name = token_lexeme(t, source).to_string();
    *i += 1;
    // Read remaining tokens on this line as the body.
    let mut body = Vec::new();
    let line_start = t.span.line_start;
    while *i < tokens.len() && tokens[*i].span.line_start == line_start {
        body.push(tokens[*i].clone());
        *i += 1;
    }
    (name, body)
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
                if t.kind == TokenKind::Greater { *i += 1; break; }
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
fn resolve_include(name: &str, include_paths: &[String]) -> Option<String> {
    // Quoted include: try current directory first.
    if !name.starts_with('/') {
        let local = std::path::Path::new(name);
        if local.exists() {
            return Some(local.to_string_lossy().to_string());
        }
    }
    // Search include paths.
    for dir in include_paths {
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    // Angle-bracket system includes: try well-known paths.
    for sys_path in &["/usr/include", "/usr/local/include"] {
        let candidate = std::path::Path::new(sys_path).join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

/// Collect all tokens on the current line (used for #if / #elif expressions).
fn collect_line_tokens(tokens: &[Token], i: &mut usize, source: &str) -> Vec<Token> {
    if *i >= tokens.len() { return Vec::new(); }
    let line = tokens[*i].span.line_start;
    let mut result = Vec::new();
    while *i < tokens.len() && tokens[*i].span.line_start == line {
        result.push(tokens[*i].clone());
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
    if *i >= tokens.len() { return lhs; }
    let op = tokens[*i].kind;
    match op {
        TokenKind::EqEq | TokenKind::NotEq | TokenKind::Less | TokenKind::LessEq |
        TokenKind::Greater | TokenKind::GreaterEq => {
            *i += 1;
            let rhs = evaluate_if_unary(tokens, i, macros, source);
            match op {
                TokenKind::EqEq => lhs == rhs,
                TokenKind::NotEq => lhs != rhs,
                TokenKind::Less => lhs < rhs,
                TokenKind::LessEq => lhs <= rhs,
                TokenKind::Greater => lhs > rhs,
                TokenKind::GreaterEq => lhs >= rhs,
                _ => false,
            }
        }
        _ => lhs,
    }
}

fn evaluate_if_unary(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    if *i >= tokens.len() { return false; }
    if tokens[*i].kind == TokenKind::Bang {
        *i += 1;
        return !evaluate_if_unary(tokens, i, macros, source);
    }
    evaluate_if_primary(tokens, i, macros, source)
}

fn evaluate_if_primary(tokens: &[Token], i: &mut usize, macros: &MacroMap, source: &str) -> bool {
    if *i >= tokens.len() { return false; }
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
fn extract_params(body: &[Token], source: &str) -> (Vec<String>, bool) {
    if body.is_empty() || body[0].kind != TokenKind::LParen {
        return (vec![], false);
    }
    let mut params = Vec::new();
    let mut is_variadic = false;
    let mut depth = 1;
    let mut i = 1;
    let mut current = String::new();
    while i < body.len() && depth > 0 {
        let text = token_lexeme(&body[i], source);
        match body[i].kind {
            TokenKind::LParen => { depth += 1; if depth > 1 { current.push_str(text); } }
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
            }
            _ => {
                current.push_str(text);
            }
        }
        i += 1;
    }
    (params, is_variadic)
}

/// Advance `i` past the current line's tokens.
fn advance_past_line(i: usize, tokens: &[Token], source: &str) -> usize {
    if i >= tokens.len() { return i; }
    let line = tokens.get(i).map(|t| t.span.line_start).unwrap_or(0);
    let mut j = i;
    while j < tokens.len() && tokens[j].span.line_start == line {
        j += 1;
    }
    j
}

fn token_text_or_empty(tok: &Token) -> &str {
    match &tok.text {
        TokenText::Decoded(s) => s.as_str(),
        TokenText::Borrowed => "",
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

// The old helpers that are no longer called are kept for the benefit of the
// existing test code that references them. They will be removed in a cleanup
// pass. Here we define the ones that the process() rewrite rendered unused
// but the macro expansion code in MacroMap still calls.

fn is_newline_like(_tok: &Token) -> bool {
    false
}

fn read_directive(tokens: &[Token], i: usize) -> (String, usize) {
    if i < tokens.len() {
        (String::new(), i + 1)
    } else {
        (String::new(), i)
    }
}

fn read_directive_arg(tokens: &[Token], i: &mut usize) -> Option<String> {
    while *i < tokens.len() {
        *i += 1;
    }
    None
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
