#![allow(clippy::all)]
//! C declarator, expression, and statement parser producing C-HIR

use evocc_chir::{
    BinaryOp, CHirDecl, CHirExpr, CHirStmt, CHirUnit, IntBase, IntLit, IntSuffix, UnaryOp,
};
use evocc_common::{FileId, SourceSpan};
use evocc_lex::{Token, TokenKind};
use evocc_types::{CType, IntRank};

pub struct Parser<'src> {
    tokens: Vec<Token>,
    pos: usize,
    file: FileId,
    source: &'src str,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<Token>, file: FileId, source: &'src str) -> Self {
        Self {
            tokens,
            pos: 0,
            file,
            source,
        }
    }

    pub fn parse_translation_unit(&mut self) -> CHirUnit {
        let mut decls = Vec::new();
        while !self.is_done() {
            if let Some(decl) = self.parse_external_declaration() {
                decls.push(decl);
            } else {
                self.advance();
            }
        }
        decls
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
            None
        }
    }

    fn token_text(&self, tok: &Token) -> &'src str {
        &self.source[tok.span.byte_start as usize..tok.span.byte_end as usize]
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
        let ty = self.parse_type_specifier()?;
        let name_tok = self.peek()?;
        if name_tok.kind != TokenKind::Identifier {
            return None;
        }
        let name = self.token_text(name_tok).to_owned();
        let name_span = name_tok.span;
        self.advance();

        let start_span = ty_span_start(&ty).unwrap_or(name_span);

        if self.peek_kind() == Some(TokenKind::LParen) {
            self.advance();
            let params = self.parse_parameter_list();
            let rparen_span = self.peek().map(|t| t.span).unwrap_or(name_span);
            if self.expect(TokenKind::RParen).is_none() {
                return None; // malformed: missing closing ')' in parameter list
            }

            if self.peek_kind() == Some(TokenKind::LBrace) {
                let body = self.parse_compound_statement();
                let span = self.merge_span(start_span, body_span_end(&body));
                return Some(CHirDecl::FuncDecl(
                    name,
                    params,
                    ty,
                    Some(Box::new(body)),
                    span,
                ));
            } else if self.peek_kind() == Some(TokenKind::Semicolon) {
                let semi_span = self.peek().map(|t| t.span).unwrap_or(rparen_span);
                self.advance();
                let span = self.merge_span(start_span, semi_span);
                return Some(CHirDecl::FuncDecl(name, params, ty, None, span));
            }
            return None;
        }

        let init = if self.match_kind(TokenKind::Assign) {
            Some(self.parse_expression())
        } else {
            None
        };

        let semi = self.expect(TokenKind::Semicolon);
        let end_span = semi.as_ref().map(|t| t.span).unwrap_or(name_span);
        let span = self.merge_span(start_span, end_span);

        Some(CHirDecl::VarDecl(name, ty, init, span))
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

    // ── parameter list ─────────────────────────────────────────────────

    fn parse_parameter_list(&mut self) -> Vec<(String, CType)> {
        let mut params = Vec::new();
        if self.peek_kind() == Some(TokenKind::RParen) {
            return params;
        }
        loop {
            let ty = match self.parse_type_specifier() {
                Some(t) => t,
                None => break,
            };
            if let Some(tok) = self.peek() {
                if tok.kind == TokenKind::Identifier {
                    let name = self.token_text(tok).to_owned();
                    self.advance();
                    params.push((name, ty));
                } else {
                    break;
                }
            } else {
                break;
            }
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        params
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
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> CHirExpr {
        let lhs = self.parse_binary(0);
        if self.match_kind(TokenKind::Assign) {
            let rhs = self.parse_assignment();
            let span = self.merge_span(expr_span_start(&lhs), expr_span_end(&rhs));
            return CHirExpr::Assign(Box::new(lhs), Box::new(rhs), span);
        }
        lhs
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
        match self.peek_kind() {
            Some(TokenKind::Minus) => {
                let tok = self.advance().unwrap();
                let inner = self.parse_unary();
                let span = self.merge_span(tok.span, expr_span_end(&inner));
                CHirExpr::Unary(UnaryOp::Neg, Box::new(inner), span)
            }
            Some(TokenKind::Plus) => {
                let tok = self.advance().unwrap();
                let inner = self.parse_unary();
                let span = self.merge_span(tok.span, expr_span_end(&inner));
                CHirExpr::Unary(UnaryOp::Plus, Box::new(inner), span)
            }
            Some(TokenKind::Bang) => {
                let tok = self.advance().unwrap();
                let inner = self.parse_unary();
                let span = self.merge_span(tok.span, expr_span_end(&inner));
                CHirExpr::Unary(UnaryOp::Not, Box::new(inner), span)
            }
            Some(TokenKind::Tilde) => {
                let tok = self.advance().unwrap();
                let inner = self.parse_unary();
                let span = self.merge_span(tok.span, expr_span_end(&inner));
                CHirExpr::Unary(UnaryOp::BitNot, Box::new(inner), span)
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> CHirExpr {
        let tok = self.advance().expect("expected primary expression");
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
            TokenKind::LParen => {
                let expr = self.parse_expression();
                let rparen_span = self.peek().map(|t| t.span).unwrap_or(tok.span);
                self.expect(TokenKind::RParen);
                let span = self.merge_span(tok.span, rparen_span);
                pack_expr(expr, span)
            }
            _ => CHirExpr::Ident("__error__".into(), tok.span),
        }
    }
}

// ── precedence table ───────────────────────────────────────────────────────

fn prec(kind: TokenKind) -> u8 {
    match kind {
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 120,
        TokenKind::Plus | TokenKind::Minus => 110,
        TokenKind::Less | TokenKind::LessEq | TokenKind::Greater | TokenKind::GreaterEq => 80,
        TokenKind::EqEq | TokenKind::NotEq => 70,
        TokenKind::Assign => 10,
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
        _ => None,
    }
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

fn ty_span_start(_ty: &CType) -> Option<SourceSpan> {
    None
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
