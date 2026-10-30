//! Parser statements.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    // ── statements ─────────────────────────────────────────────────────

    pub(super) fn enclosing_return_type(&self) -> Option<TypeId> {
        let mut scope = Some(self.current_scope);
        while let Some(id) = scope {
            let entry = self.symbols.scope(id).ok()?;
            if let Some(function) = entry.function_owner() {
                let ty = self.types.get(self.symbols.symbol(function).ok()?.ty())?;
                return match ty.kind {
                    TypeKind::Function { return_type, .. } => Some(return_type),
                    _ => None,
                };
            }
            scope = entry.parent();
        }
        None
    }

    pub(super) fn parse_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        match self.peek_kind()? {
            _ if self.attribute_starts_at(self.pos) => {
                let start = self.span();
                let attributes = self.parse_attribute_specifiers(AttributePlacement::NullStatement);
                if self.peek_kind() != Some(TokenKind::Semicolon) {
                    self.report("E2924", "attribute-on-non-null-statement", start);
                    return None;
                }
                let semicolon = self.advance().expect("peeked semicolon");
                let span = self.merge_span(start, semicolon.span);
                Some(self.alloc_node(CHirStmt::AttributedNull(attributes, span)))
            }
            TokenKind::Asm => {
                let mut assembly = self.parse_asm_construct()?;
                let semicolon = self.expect(TokenKind::Semicolon)?;
                assembly.span = self.merge_span(assembly.span, semicolon.span);
                Some(self.alloc_node(CHirStmt::Asm(assembly)))
            }
            TokenKind::If => self.parse_if_statement(),
            TokenKind::Switch => self.parse_switch_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::Do => self.parse_do_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Return => {
                let ret_tok = self.advance().unwrap();
                let start_span = ret_tok.span;
                let return_type = self.enclosing_return_type();
                if self.peek_kind() == Some(TokenKind::Semicolon) {
                    let semi_span = self.advance().expect("peeked semicolon").span;
                    let span = self.merge_span(start_span, semi_span);
                    if return_type.is_some_and(|ty| {
                        !matches!(self.types.get(ty).map(|ty| &ty.kind), Some(TypeKind::Void))
                    }) {
                        self.report("E1207", "invalid-return-value", start_span);
                    }
                    return Some(self.alloc_node(CHirStmt::Return(None, span)));
                }
                let mut expr = self.parse_expression()?;
                if let Some(target) = return_type {
                    let target_void = matches!(
                        self.types.get(target).map(|ty| &ty.kind),
                        Some(TypeKind::Void)
                    );
                    let source_void = self.converted_expression_type(expr).is_some_and(|ty| {
                        matches!(self.operand_type_class(ty.ty), OperandTypeClass::Void)
                    });
                    if target_void || source_void {
                        if target_void != source_void {
                            self.report("E1207", "invalid-return-value", expr.span());
                        }
                    } else {
                        if self.incompatible_pointer_conversion(target, expr)
                            && self.record_incompatible_pointers(expr.span())
                        {
                            expr = self.incompatible_pointer_cast(target, expr);
                        }
                        if self.assignment_constraint(AssignmentOp::Simple, target, expr)
                            == UpdateConstraint::Invalid
                        {
                            // Returning initializes the result value. Reuse the
                            // same compatibility rule as initialization/assignment;
                            // an explicit source cast still supplies its own type.
                            self.report("E2202", "invalid-initializer-type", expr.span());
                        } else {
                            expr = self.explicit_floating_integer_conversion(target, expr);
                        }
                    }
                }
                let semi = self.expect(TokenKind::Semicolon);
                let end_span = semi.as_ref().map_or(expr.span(), |token| token.span);
                let span = self.merge_span(start_span, end_span);
                Some(self.alloc_node(CHirStmt::Return(Some(expr), span)))
            }
            TokenKind::Break => {
                let keyword = self.advance().expect("peeked break");
                if self.loop_depth == 0 && self.switch_depth == 0 {
                    self.report("E1201", "break-outside-loop-or-switch", keyword.span);
                }
                let semicolon = self.expect(TokenKind::Semicolon);
                let end = semicolon.as_ref().map_or(keyword.span, |token| token.span);
                let span = self.merge_span(keyword.span, end);
                Some(self.alloc_node(CHirStmt::Break(span)))
            }
            TokenKind::Continue => {
                let keyword = self.advance().expect("peeked continue");
                if self.loop_depth == 0 {
                    self.report("E1202", "continue-outside-loop", keyword.span);
                }
                let semicolon = self.expect(TokenKind::Semicolon);
                let end = semicolon.as_ref().map_or(keyword.span, |token| token.span);
                let span = self.merge_span(keyword.span, end);
                Some(self.alloc_node(CHirStmt::Continue(span)))
            }
            TokenKind::Goto => self.parse_goto_statement(),
            TokenKind::Case => self.parse_case_statement(),
            TokenKind::Default => self.parse_default_statement(),
            TokenKind::LBrace => Some(self.parse_compound_statement(true)),
            TokenKind::Semicolon => {
                let semi = self.advance().unwrap();
                Some(self.alloc_node(CHirStmt::Null(semi.span)))
            }
            TokenKind::Identifier
                if self.tokens.get(self.pos + 1).map(|token| token.kind)
                    == Some(TokenKind::Colon) =>
            {
                self.parse_label_statement()
            }
            _ => {
                let start = self.span();
                let expr = self.parse_expression()?;
                let semi = self.expect(TokenKind::Semicolon);
                let end = semi.as_ref().map_or(expr.span(), |token| token.span);
                let span = self.merge_span(start, end);
                Some(self.alloc_node(CHirStmt::Expr(expr, span)))
            }
        }
    }

    pub(super) fn parse_if_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked if");
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.require_scalar_controlling_expression(
            condition,
            "E2103",
            "controlling-expression-not-scalar",
        )?;
        let then_branch = self.parse_statement()?;
        let else_branch = if self.match_kind(TokenKind::Else) {
            Some(self.parse_statement()?)
        } else {
            None
        };
        let end = else_branch.map_or_else(|| then_branch.span(), CHirStmt::span);
        let span = self.merge_span(keyword.span, end);
        Some(self.alloc_node(CHirStmt::If(condition, then_branch, else_branch, span)))
    }

    pub(super) fn parse_switch_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked switch");
        self.expect(TokenKind::LParen)?;
        let expression = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        let selector_type = self
            .converted_expression_type(expression)
            .and_then(|inferred| {
                let OperandTypeClass::Integer(ty) = self.operand_type_class(inferred.ty) else {
                    return None;
                };
                Some(CInteger::new(0, ty).promoted().ty())
            });
        self.switch_cases.push(SwitchCaseContext {
            selector_type,
            intervals: Vec::new(),
        });
        self.switch_depth += 1;
        let body = self.parse_statement();
        self.switch_depth -= 1;
        self.switch_cases
            .pop()
            .expect("every active switch owns one case context");
        let body = body?;
        let span = self.merge_span(keyword.span, body.span());
        Some(self.alloc_node(CHirStmt::Switch(expression, body, span)))
    }

    pub(super) fn parse_while_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked while");
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.require_scalar_controlling_expression(
            condition,
            "E2103",
            "controlling-expression-not-scalar",
        )?;
        self.loop_depth += 1;
        let body = self.parse_statement();
        self.loop_depth -= 1;
        let body = body?;
        let span = self.merge_span(keyword.span, body.span());
        Some(self.alloc_node(CHirStmt::While(condition, body, span)))
    }

    pub(super) fn parse_do_while_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked do");
        self.loop_depth += 1;
        let body = self.parse_statement();
        self.loop_depth -= 1;
        let body = body?;
        self.expect(TokenKind::While)?;
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.require_scalar_controlling_expression(
            condition,
            "E2103",
            "controlling-expression-not-scalar",
        )?;
        let semicolon = self.expect(TokenKind::Semicolon)?;
        let span = self.merge_span(keyword.span, semicolon.span);
        Some(self.alloc_node(CHirStmt::DoWhile(body, condition, span)))
    }

    pub(super) fn parse_for_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked for");
        let outer_scope = self.current_scope;
        self.current_scope = self.create_scope(ScopeKind::For, Some(keyword.span));
        let result = self.parse_for_statement_in_scope(keyword);
        self.current_scope = outer_scope;
        result
    }

    pub(super) fn parse_for_statement_in_scope(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirStmt<'arena>> {
        self.expect(TokenKind::LParen)?;

        // Both oracles admit a prefixed expression here, but disagree on a
        // prefixed declaration: GCC accepts it and Clang rejects it. Keep
        // that declaration form outside the consensus profile.
        let initializer = if self.match_kind(TokenKind::Semicolon) {
            CHirForInit::Empty
        } else if self.peek_kind() != Some(TokenKind::GnuExtension) && self.is_declaration_start() {
            if self.legacy_declarations {
                self.report(
                    "E1001",
                    "expected-expression-in-legacy-for-initializer",
                    self.span(),
                );
                return None;
            }
            let declarations = self.parse_declaration(DeclarationContext::ForInitializer)?;
            CHirForInit::Declaration(declarations)
        } else {
            let expression = self.parse_expression()?;
            self.expect(TokenKind::Semicolon)?;
            CHirForInit::Expression(expression)
        };

        let condition = if self.match_kind(TokenKind::Semicolon) {
            None
        } else {
            let expression = self.parse_expression()?;
            self.expect(TokenKind::Semicolon)?;
            self.require_scalar_controlling_expression(
                expression,
                "E2103",
                "controlling-expression-not-scalar",
            )?;
            Some(expression)
        };
        let step = if self.peek_kind() == Some(TokenKind::RParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(TokenKind::RParen)?;

        self.loop_depth += 1;
        let body = self.parse_statement();
        self.loop_depth -= 1;
        let body = body?;
        let span = self.merge_span(keyword.span, body.span());
        Some(self.alloc_node(CHirStmt::For(initializer, condition, step, body, span)))
    }

    pub(super) fn parse_goto_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked goto");
        if self.match_kind(TokenKind::Star) {
            let target = self.parse_expression()?;
            let semicolon = self.expect(TokenKind::Semicolon)?;
            let span = self.merge_span(keyword.span, semicolon.span);
            return Some(self.alloc_node(CHirStmt::ComputedGoto(target, span)));
        }
        let token = self.expect(TokenKind::Identifier)?;
        let spelling = self.token_text(&token).to_owned();
        let label = SpannedName {
            spelling: self.resolve_local_label(&spelling).unwrap_or(spelling),
            span: token.span,
        };
        let semicolon = self.expect(TokenKind::Semicolon)?;
        let span = self.merge_span(keyword.span, semicolon.span);
        Some(self.alloc_node(CHirStmt::Goto(label, span)))
    }

    pub(super) fn parse_label_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let token = self.advance().expect("peeked label");
        let spelling = self.token_text(&token).to_owned();
        let label = SpannedName {
            spelling: self.resolve_local_label(&spelling).unwrap_or(spelling),
            span: token.span,
        };
        self.expect(TokenKind::Colon)?;
        // `label: __attribute__((unused));` -- the attribute has no node to
        // live on, so it is preserved on the unit with its placement.
        let label_attributes = self.parse_attribute_specifiers(AttributePlacement::Label);
        self.detached_attributes.extend(label_attributes);
        let nested = self.parse_statement()?;
        let span = self.merge_span(token.span, nested.span());
        Some(self.alloc_node(CHirStmt::Label(label, nested, span)))
    }

    pub(super) fn parse_case_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked case");
        if self.switch_depth == 0 {
            self.report("E1203", "case-outside-switch", keyword.span);
        }
        let first = self.parse_conditional()?;
        let last = if self.match_kind(TokenKind::Ellipsis) {
            Some(self.parse_conditional()?)
        } else {
            None
        };
        let endpoint = last.unwrap_or(first);
        let interval_span = self.merge_span(first.span(), endpoint.span());
        self.register_switch_interval(first, endpoint, interval_span, last.is_some())?;
        self.expect(TokenKind::Colon)?;
        let nested = self.parse_statement()?;
        let span = self.merge_span(keyword.span, nested.span());
        Some(match last {
            Some(last) => self.alloc_node(CHirStmt::CaseRange(first, last, nested, span)),
            None => self.alloc_node(CHirStmt::Case(first, nested, span)),
        })
    }

    pub(super) fn register_switch_interval(
        &mut self,
        first: &CHirExpr<'_>,
        last: &CHirExpr<'_>,
        span: SourceSpan,
        require_constant: bool,
    ) -> Option<()> {
        let selector_type = self
            .switch_cases
            .last()
            .and_then(|context| context.selector_type);
        let converted = selector_type.and_then(|ty| {
            let first = self.eval_integer_constant_expression(first).ok()?.cast(ty);
            let last = self.eval_integer_constant_expression(last).ok()?.cast(ty);
            Some((
                first.try_mathematical_value()?,
                last.try_mathematical_value()?,
            ))
        });
        let Some((first, last)) = converted else {
            if require_constant {
                self.report("E2916", "case-range-invalid", span);
                return None;
            }
            return Some(());
        };
        if first > last {
            self.report("E2916", "case-range-invalid", span);
            return None;
        }
        let overlaps = self.switch_cases.last().is_some_and(|context| {
            context
                .intervals
                .iter()
                .any(|(other_first, other_last)| first <= *other_last && *other_first <= last)
        });
        if overlaps {
            self.report("E2916", "case-range-invalid", span);
            return None;
        }
        self.switch_cases
            .last_mut()
            .expect("case labels inside a switch have one active context")
            .intervals
            .push((first, last));
        Some(())
    }

    pub(super) fn parse_default_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked default");
        if self.switch_depth == 0 {
            self.report("E1204", "default-outside-switch", keyword.span);
        }
        self.expect(TokenKind::Colon)?;
        let nested = self.parse_statement()?;
        let span = self.merge_span(keyword.span, nested.span());
        Some(self.alloc_node(CHirStmt::Default(nested, span)))
    }

    pub(super) fn parse_compound_statement(
        &mut self,
        create_block_scope: bool,
    ) -> &'arena CHirStmt<'arena> {
        let outer_fenv = self.fenv_access;
        let lbrace = self.expect(TokenKind::LBrace).unwrap();
        let start_span = lbrace.span;
        let outer_scope = self.current_scope;
        if create_block_scope {
            self.current_scope = self.create_scope(ScopeKind::Block, Some(lbrace.span));
        }
        self.local_label_scopes.push(HashMap::new());
        let mut items = Vec::new();
        let mut at_block_start = true;
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            if self.consume_fenv_marker(at_block_start) {
                continue;
            }
            if self.consume_visibility_marker() {
                continue;
            }
            let before = self.pos;
            let side_start = self.side_declarations.len();
            let mut item_start = items.len();
            if at_block_start && self.peek_kind() == Some(TokenKind::LocalLabel) {
                self.parse_local_label_declaration();
            } else if self.peek_kind() == Some(TokenKind::StaticAssert) {
                at_block_start = false;
                self.parse_static_assert_declaration();
            } else if self.attribute_starts_at(self.pos)
                && !self.attribute_sequence_starts_declaration()
            {
                at_block_start = false;
                if let Some(statement) = self.parse_statement() {
                    items.push(CHirBlockItem::Statement(statement));
                }
            } else if self.is_declaration_start() || self.peek_kind() == Some(TokenKind::Inline) {
                at_block_start = false;
                if let Some(declarations) = self.parse_declaration(DeclarationContext::Block) {
                    items.extend(declarations.into_iter().map(CHirBlockItem::Declaration));
                }
            } else {
                at_block_start = false;
                if let Some(statement) = self.parse_statement() {
                    items.push(CHirBlockItem::Statement(statement));
                }
            }
            if self.pos == before {
                self.report("E1003", "expected-expression", self.span());
                self.advance();
            }
            // Keep statement-local implicit declarations in their block.
            // Extent objects are declared here without an initializer: the
            // corresponding cast performs the bound assignment when reached.
            for declaration in self.side_declarations.split_off(side_start) {
                if matches!(declaration, CHirDecl::Function(function)
                    if !self.symbols.is_builtin_function(function.symbol))
                    || matches!(declaration, CHirDecl::Object(object)
                        if object.name.spelling.starts_with("__evocc.extent."))
                {
                    items.insert(item_start, CHirBlockItem::Declaration(declaration));
                    item_start += 1;
                } else {
                    self.side_declarations.push(declaration);
                }
            }
        }
        let rbrace = if self.peek_kind() == Some(TokenKind::RBrace) {
            self.advance()
        } else {
            self.expect(TokenKind::RBrace)
        };
        let end = rbrace.as_ref().map_or(start_span, |token| token.span);
        let span = self.merge_span(start_span, end);
        self.local_label_scopes
            .pop()
            .expect("every compound statement owns one local-label scope");
        if create_block_scope {
            self.current_scope = outer_scope;
        }
        let fenv = std::mem::replace(&mut self.fenv_access, outer_fenv);
        self.alloc_node(CHirStmt::Compound(items, span, Some(fenv)))
    }

    pub(super) fn consume_fenv_marker(&mut self, allowed: bool) -> bool {
        let Some(setting) = self.peek().and_then(evocc_cpp::pragma_fenv_access) else {
            return false;
        };
        let span = self.advance().unwrap().span;
        if allowed {
            self.fenv_access = setting;
        } else {
            self.report("E3065", "misplaced-floating-environment-pragma", span);
        }
        true
    }

    pub(super) fn parse_local_label_declaration(&mut self) -> Option<()> {
        self.advance().expect("peeked __label__");
        loop {
            let token = self.expect(TokenKind::Identifier)?;
            let spelling = self.token_text(&token).to_owned();
            let internal = format!("{spelling}.__local_label.{}", self.next_local_label);
            let Some(next_local_label) = self.next_local_label.checked_add(1) else {
                self.report("E1001", "expected-declaration", token.span);
                return None;
            };
            self.next_local_label = next_local_label;
            self.local_label_scopes
                .last_mut()
                .expect("local-label declarations occur inside a compound statement")
                .insert(spelling, internal);
            if self.peek_kind() != Some(TokenKind::Comma) {
                break;
            }
            self.advance();
        }
        self.expect(TokenKind::Semicolon)?;
        Some(())
    }

    pub(super) fn resolve_local_label(&self, spelling: &str) -> Option<String> {
        self.local_label_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(spelling).cloned())
    }

    pub(super) fn is_declaration_start(&self) -> bool {
        let mut position = self.pos;
        while self.tokens.get(position).map(|token| token.kind) == Some(TokenKind::GnuExtension) {
            position += 1;
        }
        let Some(token) = self.tokens.get(position) else {
            return false;
        };
        // Labels occupy their own namespace. Even when the same spelling is
        // currently a typedef name, `identifier:` is unambiguously a labeled
        // statement rather than the start of a declaration.
        if token.kind == TokenKind::Identifier
            && self.tokens.get(position + 1).map(|token| token.kind) == Some(TokenKind::Colon)
        {
            return false;
        }
        self.attribute_starts_at(position)
            || matches!(
                token.kind,
                TokenKind::Attribute
                    | TokenKind::Alignas
                    | TokenKind::Noreturn
                    | TokenKind::Inline
                    | TokenKind::Auto
                    | TokenKind::Extern
                    | TokenKind::AutoType
                    | TokenKind::Register
                    | TokenKind::Static
                    | TokenKind::ThreadLocal
                    | TokenKind::Typedef
                    | TokenKind::Const
                    | TokenKind::Volatile
                    | TokenKind::Restrict
                    | TokenKind::Atomic
            )
            || self.token_starts_type_name(token)
    }

    pub(super) fn attribute_sequence_starts_declaration(&self) -> bool {
        let mut position = self.pos;
        while self.attribute_starts_at(position) {
            let Some(end) = self.attribute_specifier_end(position) else {
                return false;
            };
            position = end;
        }
        let Some(token) = self.tokens.get(position) else {
            return false;
        };
        matches!(token.kind, TokenKind::Inline | TokenKind::Noreturn)
            || matches!(
                token.kind,
                TokenKind::Auto
                    | TokenKind::Extern
                    | TokenKind::GnuExtension
                    | TokenKind::AutoType
                    | TokenKind::Register
                    | TokenKind::Static
                    | TokenKind::ThreadLocal
                    | TokenKind::Typedef
                    | TokenKind::Const
                    | TokenKind::Volatile
                    | TokenKind::Restrict
                    | TokenKind::Atomic
            )
            || self.token_starts_type_name(token)
    }

    pub(super) fn peek_typedef_name(&self) -> Option<(SymbolId, TypeId)> {
        self.typedef_name(self.peek()?)
    }

    pub(super) fn typedef_name(&self, token: &Token) -> Option<(SymbolId, TypeId)> {
        if token.kind != TokenKind::Identifier {
            return None;
        }
        let spelling = self.token_text(token);
        if self.temporary_parameter_shadow_is_visible(spelling) {
            return None;
        }
        let symbol = self
            .symbols
            .resolve(self.current_scope, spelling)
            .expect("the parser current scope is always live")?;
        let declaration = self
            .symbols
            .symbol(symbol)
            .expect("resolved symbol belongs to the same table");
        (declaration.kind() == SymbolKind::Typedef).then_some((symbol, declaration.ty()))
    }

    pub(super) fn temporary_parameter_shadow_is_visible(&self, spelling: &str) -> bool {
        let mut scope = Some(self.current_scope);
        while let Some(current) = scope {
            if self
                .temporary_parameter_shadows
                .iter()
                .rev()
                .any(|shadow| shadow.scope == current && shadow.spelling == spelling)
            {
                return true;
            }
            scope = self
                .symbols
                .scope(current)
                .expect("the parser current scope ancestry is always live")
                .parent();
        }
        false
    }

    pub(super) fn token_starts_type_name(&self, token: &Token) -> bool {
        matches!(
            token.kind,
            TokenKind::Const
                | TokenKind::Volatile
                | TokenKind::Restrict
                | TokenKind::Atomic
                | TokenKind::Attribute
                | TokenKind::Alignas
        ) || self.is_type_specifier_kind(token.kind)
            || self.typedef_name(token).is_some()
    }

    pub(super) fn is_type_specifier_kind(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Int
                | TokenKind::Void
                | TokenKind::Long
                | TokenKind::Char
                | TokenKind::Short
                | TokenKind::Signed
                | TokenKind::Unsigned
                | TokenKind::Float
                | TokenKind::Double
                | TokenKind::Complex
                | TokenKind::Bool
                | TokenKind::Struct
                | TokenKind::Union
                | TokenKind::Enum
                | TokenKind::BuiltinVaList
                | TokenKind::TargetBuiltinType
                | TokenKind::GnuInt128
                | TokenKind::Typeof
                | TokenKind::TypeofUnqual
        )
    }
}
