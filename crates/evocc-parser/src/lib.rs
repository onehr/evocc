//! C declarator, expression, and statement parser producing C-HIR

use evocc_chir::{
    BinaryOp, CHirDecl, CHirExpr, CHirStmt, CHirUnit, CharLit, CharPrefix, FloatLit, FloatSuffix,
    IntBase, IntLit, IntSuffix, StringLit, StringPrefix, UnaryOp,
};
use evocc_common::{FileId, SourceSpan};
use evocc_lex::{Token, TokenKind};
use evocc_types::{ArrayBound, CType, FunctionProto, IntRank, Qualifiers};

#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    pub span: SourceSpan,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
enum DerivedDeclarator {
    Pointer,
    Array(ArrayBound),
    Function(Vec<(String, CType)>, bool),
}

#[derive(Debug, Clone)]
struct ParsedDeclarator {
    name: String,
    name_span: SourceSpan,
    derived: Vec<DerivedDeclarator>,
}

pub struct Parser<'src> {
    tokens: Vec<Token>,
    pos: usize,
    file: FileId,
    source: &'src str,
    had_error: bool,
    diagnostic: Option<ParseDiagnostic>,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<Token>, file: FileId, source: &'src str) -> Self {
        Self {
            tokens,
            pos: 0,
            file,
            source,
            had_error: false,
            diagnostic: None,
        }
    }

    pub fn parse_translation_unit(&mut self) -> CHirUnit {
        let mut decls = Vec::new();
        while !self.is_done() {
            if let Some(decl) = self.parse_external_declaration() {
                decls.push(decl);
            } else {
                self.had_error = true;
                self.advance();
            }
        }
        decls
    }

    pub fn had_error(&self) -> bool {
        self.had_error
    }

    pub fn diagnostic(&self) -> Option<&ParseDiagnostic> {
        self.diagnostic.as_ref()
    }

    // ── helpers ────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn is_done(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek_kind(), Some(TokenKind::Eof))
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek_kind() == Some(kind) {
            self.advance()
        } else {
            self.report(
                "E1002",
                format!("expected-token: {}", token_spelling(kind)),
                self.span(),
            );
            None
        }
    }

    fn report(&mut self, code: &'static str, message: impl Into<String>, span: SourceSpan) {
        self.had_error = true;
        if self.diagnostic.is_none() {
            self.diagnostic = Some(ParseDiagnostic {
                span,
                code,
                message: message.into(),
            });
        }
    }

    fn token_text<'a>(&'a self, tok: &'a Token) -> &'a str {
        if let evocc_lex::TokenText::Decoded(text) = &tok.text {
            return text;
        }
        self.source
            .get(tok.span.byte_start as usize..tok.span.byte_end as usize)
            .unwrap_or("")
    }

    fn span(&self) -> SourceSpan {
        if let Some(tok) = self.peek() {
            tok.span
        } else {
            SourceSpan::new(self.file, 0, 0, 0, 0)
        }
    }

    fn merge_span(&self, start: SourceSpan, end: SourceSpan) -> SourceSpan {
        SourceSpan::new(
            self.file,
            start.byte_start,
            end.byte_end,
            start.line_start,
            start.column_start,
        )
    }

    // ── external declarations ──────────────────────────────────────────

    fn parse_external_declaration(&mut self) -> Option<CHirDecl> {
        let start_span = self.span();
        let ty = self.parse_type_specifier()?;
        let Some(declarator) = self.parse_declarator() else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };
        let name = declarator.name.clone();
        let name_span = declarator.name_span;
        let full_type = apply_declarator(ty, &declarator.derived);

        if let CType::Function(proto) = &full_type {
            let params = declarator
                .derived
                .iter()
                .find_map(|derived| match derived {
                    DerivedDeclarator::Function(params, _) => Some(params.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let return_type = (*proto.ret).clone();
            if self.peek_kind() == Some(TokenKind::LBrace) {
                let body = self.parse_compound_statement();
                let span = self.merge_span(start_span, body_span_end(&body));
                return Some(CHirDecl::FuncDecl(
                    name,
                    params,
                    return_type,
                    Some(Box::new(body)),
                    span,
                ));
            } else if self.peek_kind() == Some(TokenKind::Semicolon) {
                let semi_span = self.advance().unwrap().span;
                let span = self.merge_span(start_span, semi_span);
                return Some(CHirDecl::FuncDecl(name, params, return_type, None, span));
            }
            self.expect(TokenKind::Semicolon);
            return None;
        }

        let init = if self.match_kind(TokenKind::Assign) {
            Some(self.parse_expression())
        } else {
            None
        };

        let semi = if self.peek_kind() == Some(TokenKind::Semicolon) {
            self.advance()
        } else {
            let span = self.span();
            if self.peek_kind() == Some(TokenKind::Identifier) {
                self.report("E1004", "trailing-token", span);
            } else {
                self.report("E1002", "expected-token: ;", span);
            }
            None
        };
        let end_span = semi.as_ref().map(|t| t.span).unwrap_or(name_span);
        let span = self.merge_span(start_span, end_span);

        Some(CHirDecl::VarDecl(name, full_type, init, span))
    }

    // ── type specifier ─────────────────────────────────────────────────

    fn parse_type_specifier(&mut self) -> Option<CType> {
        match self.peek_kind()? {
            TokenKind::Int => {
                self.advance();
                Some(CType::Int(IntRank::Int))
            }
            TokenKind::Void => {
                self.advance();
                Some(CType::Void)
            }
            TokenKind::Long => {
                self.advance();
                Some(CType::Int(IntRank::Long))
            }
            TokenKind::Char => {
                self.advance();
                Some(CType::Int(IntRank::Char))
            }
            TokenKind::Short => {
                self.advance();
                Some(CType::Int(IntRank::Short))
            }
            _ => None,
        }
    }

    // ── declarators ────────────────────────────────────────────────────

    fn parse_declarator(&mut self) -> Option<ParsedDeclarator> {
        let mut pointers = Vec::new();
        while self.peek_kind() == Some(TokenKind::Star) {
            self.advance();
            pointers.push(DerivedDeclarator::Pointer);
        }

        let mut declarator = if self.peek_kind() == Some(TokenKind::Identifier) {
            let token = self.advance().unwrap();
            ParsedDeclarator {
                name: self.token_text(&token).to_owned(),
                name_span: token.span,
                derived: Vec::new(),
            }
        } else if self.match_kind(TokenKind::LParen) {
            let inner = self.parse_declarator()?;
            self.expect(TokenKind::RParen)?;
            inner
        } else {
            return None;
        };

        loop {
            match self.peek_kind() {
                Some(TokenKind::LBracket) => {
                    let open = self.advance().unwrap();
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Function(_, _))
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let bound = if self.peek_kind() == Some(TokenKind::RBracket) {
                        ArrayBound::Unbounded
                    } else if self.peek_kind() == Some(TokenKind::IntLiteral) {
                        let token = self.advance().unwrap();
                        ArrayBound::Fixed(parse_int_literal(self.token_text(&token)))
                    } else {
                        ArrayBound::Variable
                    };
                    self.expect(TokenKind::RBracket)?;
                    declarator.derived.push(DerivedDeclarator::Array(bound));
                }
                Some(TokenKind::LParen) => {
                    let open = self.advance().unwrap();
                    if matches!(declarator.derived.last(), Some(DerivedDeclarator::Array(_))) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let (params, variadic) = self.parse_parameter_list();
                    self.expect(TokenKind::RParen)?;
                    declarator
                        .derived
                        .push(DerivedDeclarator::Function(params, variadic));
                }
                _ => break,
            }
        }
        declarator.derived.extend(pointers);
        Some(declarator)
    }

    fn parse_parameter_list(&mut self) -> (Vec<(String, CType)>, bool) {
        let mut params = Vec::new();
        let mut variadic = false;
        if self.peek_kind() == Some(TokenKind::RParen) {
            return (params, variadic);
        }
        if self.peek_kind() == Some(TokenKind::Void)
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(TokenKind::RParen)
        {
            self.advance();
            return (params, variadic);
        }
        loop {
            if self.match_kind(TokenKind::Ellipsis) {
                variadic = true;
                break;
            }
            let Some(base) = self.parse_type_specifier() else {
                self.report("E1001", "expected-declaration", self.span());
                break;
            };
            if let Some(declarator) = self.parse_declarator() {
                let ty = apply_declarator(base, &declarator.derived);
                params.push((declarator.name, ty));
            } else {
                params.push((String::new(), base));
            }
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        (params, variadic)
    }

    // ── statements ─────────────────────────────────────────────────────

    fn parse_statement(&mut self) -> Option<CHirStmt> {
        match self.peek_kind()? {
            TokenKind::Return => {
                let ret_tok = self.advance().unwrap();
                let start_span = ret_tok.span;
                if self.peek_kind() == Some(TokenKind::Semicolon) {
                    let semi_span = self.peek().unwrap().span;
                    self.advance();
                    let span = self.merge_span(start_span, semi_span);
                    return Some(CHirStmt::Return(None, span));
                }
                let expr = self.parse_expression();
                let end_span = expr_span_end(&expr);
                let span = self.merge_span(start_span, end_span);
                self.expect(TokenKind::Semicolon);
                Some(CHirStmt::Return(Some(expr), span))
            }
            TokenKind::LBrace => Some(self.parse_compound_statement()),
            TokenKind::Semicolon => {
                let semi = self.advance().unwrap();
                Some(CHirStmt::Null(semi.span))
            }
            kind if self.is_type_specifier_kind(kind) => {
                let decl = self.parse_external_declaration()?;
                let span = decl_span(&decl);
                Some(CHirStmt::Decl(Box::new(decl), span))
            }
            _ => {
                let expr = self.parse_expression();
                let end_span = expr_span_end(&expr);
                let span = self.merge_span(self.span(), end_span);
                self.expect(TokenKind::Semicolon);
                Some(CHirStmt::Expr(expr, span))
            }
        }
    }

    fn parse_compound_statement(&mut self) -> CHirStmt {
        let lbrace = self.expect(TokenKind::LBrace).unwrap();
        let start_span = lbrace.span;
        let mut stmts = Vec::new();
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            if let Some(stmt) = self.parse_statement() {
                stmts.push(stmt);
            } else {
                self.advance();
            }
        }
        let rbrace_span = self.peek().map(|t| t.span).unwrap_or(start_span);
        self.expect(TokenKind::RBrace);
        let span = self.merge_span(start_span, rbrace_span);
        CHirStmt::Compound(stmts, span)
    }

    fn is_type_specifier_kind(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Int | TokenKind::Void | TokenKind::Long | TokenKind::Char | TokenKind::Short
        )
    }

    // ── expressions ────────────────────────────────────────────────────

    fn parse_expression(&mut self) -> CHirExpr {
        self.parse_comma()
    }

    fn parse_comma(&mut self) -> CHirExpr {
        let mut expression = self.parse_assignment();
        while self.match_kind(TokenKind::Comma) {
            let right = self.parse_assignment();
            let span = self.merge_span(expr_span_start(&expression), expr_span_end(&right));
            expression = CHirExpr::Comma(Box::new(expression), Box::new(right), span);
        }
        expression
    }

    fn parse_argument_expression(&mut self) -> CHirExpr {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> CHirExpr {
        let lhs = self.parse_conditional();
        if self.match_kind(TokenKind::Assign) {
            let rhs = self.parse_assignment();
            let span = self.merge_span(expr_span_start(&lhs), expr_span_end(&rhs));
            return CHirExpr::Assign(Box::new(lhs), Box::new(rhs), span);
        }
        lhs
    }

    fn parse_conditional(&mut self) -> CHirExpr {
        let condition = self.parse_binary(1);
        if !self.match_kind(TokenKind::Question) {
            return condition;
        }
        let when_true = self.parse_expression();
        self.expect(TokenKind::Colon);
        let when_false = self.parse_conditional();
        let span = self.merge_span(expr_span_start(&condition), expr_span_end(&when_false));
        CHirExpr::Ternary(
            Box::new(condition),
            Box::new(when_true),
            Box::new(when_false),
            span,
        )
    }

    fn parse_binary(&mut self, min_prec: u8) -> CHirExpr {
        let mut lhs = self.parse_unary();
        loop {
            let op_kind = match self.peek_kind() {
                Some(k) if prec(k) >= min_prec => k,
                _ => break,
            };
            let op = match binary_op(op_kind) {
                Some(op) => op,
                None => break,
            };
            let op_prec = prec(op_kind);
            self.advance();
            let rhs = self.parse_binary(op_prec + 1);
            let span = self.merge_span(expr_span_start(&lhs), expr_span_end(&rhs));
            lhs = CHirExpr::Binary(op, Box::new(lhs), Box::new(rhs), span);
        }
        lhs
    }

    fn parse_unary(&mut self) -> CHirExpr {
        let unary = match self.peek_kind() {
            Some(TokenKind::Minus) => Some(UnaryOp::Neg),
            Some(TokenKind::Plus) => Some(UnaryOp::Plus),
            Some(TokenKind::Bang) => Some(UnaryOp::Not),
            Some(TokenKind::Tilde) => Some(UnaryOp::BitNot),
            Some(TokenKind::Amp) => Some(UnaryOp::AddrOf),
            Some(TokenKind::Star) => Some(UnaryOp::Deref),
            Some(TokenKind::PlusPlus) => Some(UnaryOp::PreInc),
            Some(TokenKind::MinusMinus) => Some(UnaryOp::PreDec),
            _ => None,
        };
        if let Some(op) = unary {
            let token = self.advance().unwrap();
            let operand = self.parse_unary();
            let span = self.merge_span(token.span, expr_span_end(&operand));
            return CHirExpr::Unary(op, Box::new(operand), span);
        }

        if self.peek_kind() == Some(TokenKind::Sizeof) {
            let token = self.advance().unwrap();
            if self.looks_like_parenthesized_type() {
                self.advance();
                let ty = self.parse_type_specifier().unwrap();
                let end = self.peek().map(|token| token.span).unwrap_or(token.span);
                self.expect(TokenKind::RParen);
                return CHirExpr::SizeOf(Box::new(ty), self.merge_span(token.span, end));
            }
            let operand = self.parse_unary();
            let span = self.merge_span(token.span, expr_span_end(&operand));
            return CHirExpr::SizeOfExpr(Box::new(operand), span);
        }

        if self.peek_kind() == Some(TokenKind::Alignof) {
            let token = self.advance().unwrap();
            self.expect(TokenKind::LParen);
            let ty = if self
                .peek_kind()
                .is_some_and(|kind| self.is_type_specifier_kind(kind))
            {
                self.parse_type_specifier().unwrap()
            } else {
                self.report("E1003", "expected-expression", self.span());
                CType::Int(IntRank::Int)
            };
            let end = self.peek().map(|token| token.span).unwrap_or(token.span);
            self.expect(TokenKind::RParen);
            return CHirExpr::AlignOf(Box::new(ty), self.merge_span(token.span, end));
        }

        if self.looks_like_parenthesized_type() {
            let open = self.advance().unwrap();
            let ty = self.parse_type_specifier().unwrap();
            let close = self.expect(TokenKind::RParen);
            if !self.peek_kind().is_some_and(can_start_expression) {
                let span = close.map_or_else(|| self.span(), |token| token.span);
                self.report("E1003", "expected-expression", span);
                return CHirExpr::Ident("__error__".into(), span);
            }
            let operand = self.parse_unary();
            let span = self.merge_span(open.span, expr_span_end(&operand));
            return CHirExpr::Cast(Box::new(ty), Box::new(operand), span);
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> CHirExpr {
        let mut expression = self.parse_primary();
        loop {
            match self.peek_kind() {
                Some(TokenKind::LParen) => {
                    self.advance();
                    let mut arguments = Vec::new();
                    if self.peek_kind() != Some(TokenKind::RParen) {
                        loop {
                            arguments.push(self.parse_argument_expression());
                            if !self.match_kind(TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    let end = self
                        .peek()
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.span());
                    self.expect(TokenKind::RParen);
                    let span = self.merge_span(expr_span_start(&expression), end);
                    expression = CHirExpr::Call(Box::new(expression), arguments, span);
                }
                Some(TokenKind::LBracket) => {
                    self.advance();
                    let index = self.parse_expression();
                    let end = self
                        .peek()
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.span());
                    self.expect(TokenKind::RBracket);
                    let span = self.merge_span(expr_span_start(&expression), end);
                    expression = CHirExpr::Index(Box::new(expression), Box::new(index), span);
                }
                Some(TokenKind::Dot) | Some(TokenKind::Arrow) => {
                    let pointer = self.peek_kind() == Some(TokenKind::Arrow);
                    self.advance();
                    let Some(member) = self.advance() else {
                        self.report("E1003", "expected-expression", self.span());
                        break;
                    };
                    if member.kind != TokenKind::Identifier {
                        self.report("E1003", "expected-expression", member.span);
                        break;
                    }
                    let name = self.token_text(&member).to_owned();
                    let span = self.merge_span(expr_span_start(&expression), member.span);
                    expression = if pointer {
                        CHirExpr::PtrMember(Box::new(expression), name, span)
                    } else {
                        CHirExpr::Member(Box::new(expression), name, span)
                    };
                }
                Some(TokenKind::PlusPlus) | Some(TokenKind::MinusMinus) => {
                    let token = self.advance().unwrap();
                    let op = if token.kind == TokenKind::PlusPlus {
                        UnaryOp::PostInc
                    } else {
                        UnaryOp::PostDec
                    };
                    let span = self.merge_span(expr_span_start(&expression), token.span);
                    expression = CHirExpr::Unary(op, Box::new(expression), span);
                }
                _ => break,
            }
        }
        expression
    }

    fn parse_primary(&mut self) -> CHirExpr {
        let Some(tok) = self.advance() else {
            self.report("E1003", "expected-expression", self.span());
            return CHirExpr::Ident("__error__".into(), self.span());
        };
        match tok.kind {
            TokenKind::IntLiteral => {
                let text = self.token_text(&tok);
                let value = parse_int_literal(text);
                CHirExpr::Lit(
                    IntLit {
                        value,
                        suffix: IntSuffix::None,
                        base: IntBase::Dec,
                    },
                    tok.span,
                )
            }
            TokenKind::Identifier => {
                let name = self.token_text(&tok).to_owned();
                CHirExpr::Ident(name, tok.span)
            }
            TokenKind::FloatLiteral => CHirExpr::FloatLit(
                FloatLit {
                    value: self.token_text(&tok).parse().unwrap_or(0.0),
                    suffix: FloatSuffix::None,
                },
                tok.span,
            ),
            TokenKind::CharLiteral => CHirExpr::CharLit(
                CharLit {
                    value: 0,
                    prefix: CharPrefix::None,
                },
                tok.span,
            ),
            TokenKind::StringLiteral => CHirExpr::StringLit(
                StringLit {
                    value: self.token_text(&tok).to_owned(),
                    prefix: StringPrefix::None,
                },
                tok.span,
            ),
            TokenKind::LParen => {
                let expr = self.parse_expression();
                let rparen_span = self.peek().map(|t| t.span).unwrap_or(tok.span);
                self.expect(TokenKind::RParen);
                let span = self.merge_span(tok.span, rparen_span);
                pack_expr(expr, span)
            }
            _ => {
                self.report("E1003", "expected-expression", tok.span);
                CHirExpr::Ident("__error__".into(), tok.span)
            }
        }
    }

    fn looks_like_parenthesized_type(&self) -> bool {
        self.peek_kind() == Some(TokenKind::LParen)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| self.is_type_specifier_kind(token.kind))
    }
}

// ── precedence table ───────────────────────────────────────────────────────

fn prec(kind: TokenKind) -> u8 {
    match kind {
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 11,
        TokenKind::Plus | TokenKind::Minus => 10,
        TokenKind::ShiftLeft | TokenKind::ShiftRight => 9,
        TokenKind::Less | TokenKind::LessEq | TokenKind::Greater | TokenKind::GreaterEq => 8,
        TokenKind::EqEq | TokenKind::NotEq => 7,
        TokenKind::Amp => 6,
        TokenKind::Caret => 5,
        TokenKind::Pipe => 4,
        TokenKind::AmpAmp => 3,
        TokenKind::PipePipe => 2,
        _ => 0,
    }
}

fn binary_op(kind: TokenKind) -> Option<BinaryOp> {
    match kind {
        TokenKind::Plus => Some(BinaryOp::Add),
        TokenKind::Minus => Some(BinaryOp::Sub),
        TokenKind::Star => Some(BinaryOp::Mul),
        TokenKind::Slash => Some(BinaryOp::Div),
        TokenKind::Percent => Some(BinaryOp::Mod),
        TokenKind::Less => Some(BinaryOp::Lt),
        TokenKind::LessEq => Some(BinaryOp::Le),
        TokenKind::Greater => Some(BinaryOp::Gt),
        TokenKind::GreaterEq => Some(BinaryOp::Ge),
        TokenKind::EqEq => Some(BinaryOp::Eq),
        TokenKind::NotEq => Some(BinaryOp::Ne),
        TokenKind::ShiftLeft => Some(BinaryOp::Shl),
        TokenKind::ShiftRight => Some(BinaryOp::Shr),
        TokenKind::Amp => Some(BinaryOp::BitAnd),
        TokenKind::Caret => Some(BinaryOp::BitXor),
        TokenKind::Pipe => Some(BinaryOp::BitOr),
        TokenKind::AmpAmp => Some(BinaryOp::And),
        TokenKind::PipePipe => Some(BinaryOp::Or),
        _ => None,
    }
}

fn apply_declarator(mut ty: CType, derived: &[DerivedDeclarator]) -> CType {
    for layer in derived.iter().rev() {
        ty = match layer {
            DerivedDeclarator::Pointer => CType::Pointer(Box::new(ty), Qualifiers::default()),
            DerivedDeclarator::Array(bound) => CType::Array(Box::new(ty), bound.clone()),
            DerivedDeclarator::Function(params, is_variadic) => CType::Function(FunctionProto {
                params: params.iter().map(|(_, ty)| ty.clone()).collect(),
                is_variadic: *is_variadic,
                ret: Box::new(ty),
            }),
        };
    }
    ty
}

fn token_spelling(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Semicolon => ";",
        TokenKind::RParen => ")",
        TokenKind::RBracket => "]",
        TokenKind::RBrace => "}",
        TokenKind::LParen => "(",
        TokenKind::Colon => ":",
        _ => "token",
    }
}

fn can_start_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::IntLiteral
            | TokenKind::FloatLiteral
            | TokenKind::CharLiteral
            | TokenKind::StringLiteral
            | TokenKind::LParen
            | TokenKind::Minus
            | TokenKind::Plus
            | TokenKind::Bang
            | TokenKind::Tilde
            | TokenKind::Amp
            | TokenKind::Star
            | TokenKind::PlusPlus
            | TokenKind::MinusMinus
            | TokenKind::Sizeof
            | TokenKind::Alignof
    )
}

// ── integer literal parsing ────────────────────────────────────────────────

fn parse_int_literal(text: &str) -> u64 {
    if text.starts_with("0x") || text.starts_with("0X") {
        u64::from_str_radix(&text[2..], 16).unwrap_or(0)
    } else if text.starts_with('0')
        && text.len() > 1
        && !text.starts_with("0x")
        && !text.starts_with("0X")
    {
        u64::from_str_radix(&text[1..], 8).unwrap_or(0)
    } else {
        text.parse().unwrap_or(0)
    }
}

// ── span helpers ───────────────────────────────────────────────────────────

fn expr_span_start(e: &CHirExpr) -> SourceSpan {
    span_of_expr(e)
}

fn expr_span_end(e: &CHirExpr) -> SourceSpan {
    span_of_expr(e)
}

fn span_of_expr(e: &CHirExpr) -> SourceSpan {
    match e {
        CHirExpr::Lit(_, s) => *s,
        CHirExpr::FloatLit(_, s) => *s,
        CHirExpr::CharLit(_, s) => *s,
        CHirExpr::StringLit(_, s) => *s,
        CHirExpr::Ident(_, s) => *s,
        CHirExpr::Unary(_, _, s) => *s,
        CHirExpr::Binary(_, _, _, s) => *s,
        CHirExpr::Ternary(_, _, _, s) => *s,
        CHirExpr::Call(_, _, s) => *s,
        CHirExpr::Cast(_, _, s) => *s,
        CHirExpr::SizeOf(_, s) => *s,
        CHirExpr::SizeOfExpr(_, s) => *s,
        CHirExpr::AlignOf(_, s) => *s,
        CHirExpr::Member(_, _, s) => *s,
        CHirExpr::PtrMember(_, _, s) => *s,
        CHirExpr::Index(_, _, s) => *s,
        CHirExpr::Assign(_, _, s) => *s,
        CHirExpr::CompoundLit(_, _, s) => *s,
        CHirExpr::Comma(_, _, s) => *s,
    }
}

fn body_span_end(stmt: &CHirStmt) -> SourceSpan {
    match stmt {
        CHirStmt::Expr(_, s) => *s,
        CHirStmt::If(_, _, _, s) => *s,
        CHirStmt::Switch(_, _, s) => *s,
        CHirStmt::While(_, _, s) => *s,
        CHirStmt::DoWhile(_, _, s) => *s,
        CHirStmt::For(_, _, _, _, s) => *s,
        CHirStmt::Return(_, s) => *s,
        CHirStmt::Compound(_, s) => *s,
        CHirStmt::Break(s) => *s,
        CHirStmt::Continue(s) => *s,
        CHirStmt::Goto(_, s) => *s,
        CHirStmt::Label(_, s) => *s,
        CHirStmt::Case(_, s) => *s,
        CHirStmt::Default(s) => *s,
        CHirStmt::Decl(_, s) => *s,
        CHirStmt::Null(s) => *s,
    }
}

fn decl_span(d: &CHirDecl) -> SourceSpan {
    match d {
        CHirDecl::VarDecl(_, _, _, s) => *s,
        CHirDecl::FuncDecl(_, _, _, _, s) => *s,
        CHirDecl::TypedefDecl(_, _, s) => *s,
        CHirDecl::StructDecl(_, _, s) => *s,
        CHirDecl::UnionDecl(_, _, s) => *s,
        CHirDecl::EnumDecl(_, _, s) => *s,
    }
}

/// When a parenthesized expression appears, preserve its original span.
fn pack_expr(inner: CHirExpr, paren_span: SourceSpan) -> CHirExpr {
    match inner {
        CHirExpr::Binary(op, lhs, rhs, _) => CHirExpr::Binary(op, lhs, rhs, paren_span),
        CHirExpr::Unary(op, e, _) => CHirExpr::Unary(op, e, paren_span),
        CHirExpr::Lit(l, _) => CHirExpr::Lit(l, paren_span),
        CHirExpr::Ident(i, _) => CHirExpr::Ident(i, paren_span),
        CHirExpr::Assign(l, r, _) => CHirExpr::Assign(l, r, paren_span),
        other => other,
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_lex::{Lexer, Token};

    fn lex(source: &str) -> (Vec<Token>, String) {
        let owned = source.to_owned();
        let mut lexer = Lexer::new(FileId::new(99), &owned);
        let mut tokens = Vec::new();
        while let Some(tok) = lexer.next() {
            tokens.push(tok);
        }
        (tokens, owned)
    }

    fn parse_expr(source: &str) -> CHirExpr {
        let (tokens, owned) = lex(source);
        let mut parser = Parser::new(tokens, FileId::new(1), &owned);
        parser.parse_expression()
    }

    #[test]
    fn parser_recognizes_declarators() {
        let source = "int x;".to_owned();
        let (tokens, _before) = {
            let mut lexer = Lexer::new(FileId::new(99), &source);
            let mut tokens = Vec::new();
            while let Some(tok) = lexer.next() {
                tokens.push(tok);
            }
            (tokens, source)
        };
        let mut parser = Parser::new(tokens, FileId::new(1), &_before);
        let unit = parser.parse_translation_unit();
        assert!(!unit.is_empty());
        match &unit[0] {
            CHirDecl::VarDecl(name, ty, init, _) => {
                assert_eq!(name, "x");
                assert!(matches!(ty, CType::Int(IntRank::Int)));
                assert!(init.is_none());
            }
            _ => panic!("expected VarDecl, got {:?}", unit[0]),
        }
    }

    #[test]
    fn parser_recognizes_declaration_with_init() {
        let source = "int y = 42;".to_owned();
        let (tokens, owned) = {
            let mut lexer = Lexer::new(FileId::new(99), &source);
            let mut tokens = Vec::new();
            while let Some(tok) = lexer.next() {
                tokens.push(tok);
            }
            (tokens, source)
        };
        let mut parser = Parser::new(tokens, FileId::new(1), &owned);
        let unit = parser.parse_translation_unit();
        assert!(!unit.is_empty());
        match &unit[0] {
            CHirDecl::VarDecl(name, ty, init, _) => {
                assert_eq!(name, "y");
                assert!(matches!(ty, CType::Int(IntRank::Int)));
                assert!(init.is_some());
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn parser_builds_expression_tree() {
        let source = "int r = 1 + 2 * 3;".to_owned();
        let (tokens, owned) = {
            let mut lexer = Lexer::new(FileId::new(99), &source);
            let mut tokens = Vec::new();
            while let Some(tok) = lexer.next() {
                tokens.push(tok);
            }
            (tokens, source)
        };
        let mut parser = Parser::new(tokens, FileId::new(1), &owned);
        let unit = parser.parse_translation_unit();
        assert!(!unit.is_empty());
        match &unit[0] {
            CHirDecl::VarDecl(_, _, Some(init), _) => match init {
                CHirExpr::Binary(BinaryOp::Add, lhs, rhs, _) => {
                    assert!(matches!(**lhs, CHirExpr::Lit(_, _)));
                    assert!(matches!(**rhs, CHirExpr::Binary(BinaryOp::Mul, _, _, _)));
                }
                _ => panic!("expected Add at root with Mul as right child"),
            },
            _ => panic!("expected declaration with initializer"),
        }
    }

    #[test]
    fn parser_builds_function_definition() {
        let source = "int f(int x) { return x + 1; }".to_owned();
        let (tokens, owned) = {
            let mut lexer = Lexer::new(FileId::new(99), &source);
            let mut tokens = Vec::new();
            while let Some(tok) = lexer.next() {
                tokens.push(tok);
            }
            (tokens, source)
        };
        let mut parser = Parser::new(tokens, FileId::new(1), &owned);
        let unit = parser.parse_translation_unit();
        assert_eq!(unit.len(), 1);
        match &unit[0] {
            CHirDecl::FuncDecl(name, params, ret_ty, Some(body), _) => {
                assert_eq!(name, "f");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, "x");
                assert!(matches!(ret_ty, CType::Int(IntRank::Int)));
                match body.as_ref() {
                    CHirStmt::Compound(stmts, _) => {
                        assert!(!stmts.is_empty());
                        assert!(matches!(stmts[0], CHirStmt::Return(Some(_), _)));
                    }
                    _ => panic!("expected compound statement body"),
                }
            }
            _ => panic!("expected function declaration"),
        }
    }

    #[test]
    fn parser_handles_parenthesized_expressions() {
        let expr = parse_expr("(1 + 2) * 3;");
        match expr {
            CHirExpr::Binary(BinaryOp::Mul, lhs, rhs, _) => {
                assert!(matches!(*lhs, CHirExpr::Binary(BinaryOp::Add, _, _, _)));
                assert!(matches!(*rhs, CHirExpr::Lit(_, _)));
            }
            _ => panic!("expected Mul of (Add) * Lit"),
        }
    }

    #[test]
    fn parser_handles_comparison_operators() {
        let expr = parse_expr("1 < 2;");
        assert!(matches!(expr, CHirExpr::Binary(BinaryOp::Lt, _, _, _)));
    }

    #[test]
    fn parser_handles_equality_operators() {
        let expr = parse_expr("1 == 2;");
        assert!(matches!(expr, CHirExpr::Binary(BinaryOp::Eq, _, _, _)));
    }

    #[test]
    fn parser_handles_unary_negation() {
        let expr = parse_expr("-42;");
        match expr {
            CHirExpr::Unary(UnaryOp::Neg, inner, _) => {
                assert!(matches!(*inner, CHirExpr::Lit(_, _)));
            }
            _ => panic!("expected unary negation"),
        }
    }

    #[test]
    fn parser_handles_assignment() {
        let expr = parse_expr("x = 5;");
        match expr {
            CHirExpr::Assign(lhs, rhs, _) => {
                assert!(matches!(*lhs, CHirExpr::Ident(_, _)));
                assert!(matches!(*rhs, CHirExpr::Lit(_, _)));
            }
            _ => panic!("expected assignment"),
        }
    }

    #[test]
    fn parser_empty_statement() {
        let source = "void f(void) { ; ; }".to_owned();
        let (tokens, owned) = {
            let mut lexer = Lexer::new(FileId::new(99), &source);
            let mut tokens = Vec::new();
            while let Some(tok) = lexer.next() {
                tokens.push(tok);
            }
            (tokens, source)
        };
        let mut parser = Parser::new(tokens, FileId::new(1), &owned);
        let unit = parser.parse_translation_unit();
        assert_eq!(unit.len(), 1);
        match &unit[0] {
            CHirDecl::FuncDecl(_, _, _, Some(body), _) => match body.as_ref() {
                CHirStmt::Compound(stmts, _) => {
                    assert_eq!(stmts.len(), 2);
                    assert!(matches!(stmts[0], CHirStmt::Null(_)));
                }
                _ => panic!("expected compound"),
            },
            _ => panic!("expected function"),
        }
    }
}
