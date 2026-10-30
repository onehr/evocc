//! Parser builtin expressions.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_builtin_expect(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let value = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let expected = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");

        for operand in [value, expected] {
            let Some(inferred) = self.converted_expression_type(operand) else {
                self.report("E1003", "expected-expression", operand.span());
                return None;
            };
            if !is_arithmetic_class(self.operand_type_class(inferred.ty)) {
                self.report("E2913", "builtin-operand-not-convertible", operand.span());
                return None;
            }
        }

        let long_ty = self.intern(TypeKind::Int(IntRank::Long), Qualifiers::default());
        let converted_value = self.alloc_node(CHirExpr::Cast(long_ty, value, value.span()));
        let converted_expected =
            self.alloc_node(CHirExpr::Cast(long_ty, expected, expected.span()));
        let span = self.merge_span(keyword.span, close.span);
        self.front_end_builtins.push(FrontEndBuiltinRecord::Expect {
            span,
            value: converted_value,
            expected: converted_expected,
        });
        if self.eval_integer_constant_expression(value).is_ok() {
            Some(self.alloc_node(CHirExpr::Cast(long_ty, value, span)))
        } else {
            Some(self.alloc_node(CHirExpr::BuiltinExpect(
                converted_value,
                converted_expected,
                span,
            )))
        }
    }

    pub(super) fn parse_builtin_constant_p(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let operand = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        if self.expression_type(operand).is_none() {
            self.report("E1003", "expected-expression", operand.span());
            return None;
        }

        let target = self.target_layout;
        let result = builtin_constant_p(operand, &self.types, &self.symbols, &target);
        let span = self.merge_span(keyword.span, close.span);
        self.front_end_builtins
            .push(FrontEndBuiltinRecord::ConstantP {
                span,
                operand,
                result,
            });
        let ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        Some(self.alloc_node(CHirExpr::BuiltinConstantP {
            operand,
            facts: ExprFacts::value(ty),
            span,
        }))
    }

    pub(super) fn parse_builtin_choose_expr(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let selector = self.parse_required_constant_assignment()?;
        let selector_span = selector.span();
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let when_true = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let when_false = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        self.advance();

        let selected_true = match self.eval_integer_constant_expression(selector) {
            Ok(value) => value.truth(),
            Err(()) => {
                self.report("E2912", "static-assertion-not-constant", selector_span);
                return None;
            }
        };
        // Both arms are semantically checked, but only the selected C-HIR node
        // is retained. This preserves its exact type and value category while
        // making evaluation of the other arm impossible downstream.
        for arm in [when_true, when_false] {
            if self.expression_type(arm).is_none() {
                self.report("E1003", "expected-expression", arm.span());
                return None;
            }
        }
        Some(if selected_true { when_true } else { when_false })
    }

    pub(super) fn parse_builtin_types_compatible_p(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let (left, left_tag) = self.parse_unevaluated_type_name()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let (right, right_tag) = self.parse_unevaluated_type_name()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let span = self.merge_span(keyword.span, close.span);
        for event in [left_tag, right_tag].into_iter().flatten() {
            let declaration = self.finish_tag_event(event, span, Vec::new());
            self.side_declarations.push(declaration);
        }
        let value = self.types.compatible_ignoring_top_qualifiers(left, right);
        Some(self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: u64::from(value),
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        )))
    }

    pub(super) fn parse_builtin_classify_type(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if !self.match_kind(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let operand = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let ty = self.generic_controlling_type(operand)?;
        let value = match &self.types.get(ty)?.kind {
            TypeKind::Int(IntRank::Bool) | TypeKind::TargetBuiltin(_) | TypeKind::Vector { .. } => {
                // GCC and Clang disagree on _Bool; opaque target header types
                // have no agreed classification in this execution profile.
                self.report("E2941", "unsupported-type-classification", operand.span());
                return None;
            }
            TypeKind::Void => {
                self.report("E2913", "builtin-operand-not-convertible", operand.span());
                return None;
            }
            TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_) => 1,
            TypeKind::Pointer(_) | TypeKind::Array { .. } | TypeKind::Function { .. } => 5,
            TypeKind::Float(_) => 8,
            TypeKind::Complex(_) => 9,
            TypeKind::Struct(_) | TypeKind::BuiltinVaList => 12,
            TypeKind::Union(_) => 13,
        };
        // Like types_compatible_p, this is resolved by the front end. Keeping
        // only the int constant makes downstream operand execution impossible.
        let span = self.merge_span(keyword.span, close.span);
        Some(self.alloc_node(CHirExpr::Lit(
            IntLit {
                value,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        )))
    }

    pub(super) fn parse_builtin_offsetof(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let (mut current_ty, tag_event) = self.parse_unevaluated_type_name()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let first = self.advance()?;
        if first.kind != TokenKind::Identifier {
            self.report("E2906", "designator-outside-object", first.span);
            return None;
        }
        let mut fixed_offset = 0_u64;
        let mut runtime_offset = None;
        let size_ty = self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
        let mut field_name = Some((self.token_text(&first).to_owned(), first.span));
        loop {
            if let Some((name, span)) = field_name.take() {
                let Some((field_offset, field_ty)) = self.offsetof_field(current_ty, &name) else {
                    self.report("E2906", "designator-outside-object", span);
                    return None;
                };
                fixed_offset = fixed_offset.wrapping_add(field_offset);
                current_ty = field_ty;
            }
            match self.peek_kind() {
                Some(TokenKind::Dot) => {
                    self.advance();
                    let member = self.advance()?;
                    if member.kind != TokenKind::Identifier {
                        self.report("E2906", "designator-outside-object", member.span);
                        return None;
                    }
                    field_name = Some((self.token_text(&member).to_owned(), member.span));
                }
                Some(TokenKind::LBracket) => {
                    let open = self.advance().expect("peeked subscript");
                    let index_expression = self.parse_conditional()?;
                    let end = self.expect(TokenKind::RBracket)?;
                    let span = self.merge_span(open.span, end.span);
                    let Some(TypeKind::Array { element, .. }) =
                        self.types.get(current_ty).map(|ty| ty.kind.clone())
                    else {
                        self.report("E2906", "designator-outside-object", span);
                        return None;
                    };
                    let target = self.target_layout;
                    let Ok(layout) = target.layout_of(&self.types, element) else {
                        self.report("E2906", "designator-outside-object", span);
                        return None;
                    };
                    if let Ok(index) = self.eval_integer_constant_expression(index_expression) {
                        let scaled = eval_binary_integer(
                            BinaryOp::Mul,
                            index.cast(CIntegerType::ULONG),
                            CInteger::new(layout.size_bytes, CIntegerType::ULONG),
                        )
                        .ok()?;
                        fixed_offset = u64::try_from(
                            eval_binary_integer(
                                BinaryOp::Add,
                                CInteger::new(fixed_offset, CIntegerType::ULONG),
                                scaled,
                            )
                            .ok()?
                            .raw_bits(),
                        )
                        .ok()?;
                    } else {
                        let Some(inferred) = self.converted_expression_type(index_expression)
                        else {
                            self.report("E2906", "designator-outside-object", span);
                            return None;
                        };
                        if !matches!(
                            self.operand_type_class(inferred.ty),
                            OperandTypeClass::Integer(_)
                        ) {
                            self.report("E2906", "designator-outside-object", span);
                            return None;
                        }
                        let base = match runtime_offset {
                            Some(expression) if fixed_offset == 0 => expression,
                            Some(expression) => {
                                let fixed = self.size_literal(fixed_offset, span);
                                self.alloc_node(CHirExpr::Binary(
                                    BinaryOp::Add,
                                    expression,
                                    fixed,
                                    span,
                                ))
                            }
                            None => self.size_literal(fixed_offset, span),
                        };
                        fixed_offset = 0;
                        let evaluated = self.alloc_node(CHirExpr::EvaluateOnce(
                            index_expression,
                            index_expression.span(),
                        ));
                        let converted = self.alloc_node(CHirExpr::Cast(
                            size_ty,
                            evaluated,
                            index_expression.span(),
                        ));
                        let stride = self.size_literal(layout.size_bytes, span);
                        let scaled = self.alloc_node(CHirExpr::Binary(
                            BinaryOp::Mul,
                            converted,
                            stride,
                            span,
                        ));
                        runtime_offset = Some(
                            if matches!(base, CHirExpr::Lit(IntLit { value: 0, .. }, _)) {
                                scaled
                            } else {
                                self.alloc_node(CHirExpr::Binary(BinaryOp::Add, base, scaled, span))
                            },
                        );
                    }
                    current_ty = element;
                }
                _ => break,
            }
        }
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let span = self.merge_span(keyword.span, close.span);
        if let Some(event) = tag_event {
            let declaration = self.finish_tag_event(event, span, Vec::new());
            self.side_declarations.push(declaration);
        }
        Some(match runtime_offset {
            Some(expression) if fixed_offset == 0 => expression,
            Some(expression) => {
                let fixed = self.size_literal(fixed_offset, span);
                self.alloc_node(CHirExpr::Binary(BinaryOp::Add, expression, fixed, span))
            }
            None => self.size_literal(fixed_offset, span),
        })
    }

    pub(super) fn size_literal(&self, value: u64, span: SourceSpan) -> &'arena CHirExpr<'arena> {
        self.alloc_node(CHirExpr::Lit(
            IntLit {
                value,
                suffix: IntSuffix::UL,
                base: IntBase::Dec,
            },
            span,
        ))
    }

    pub(super) fn promote_call_arguments(
        &mut self,
        arguments: &mut [&'arena CHirExpr<'arena>],
    ) -> Option<Vec<TypeId>> {
        let mut parameters = Vec::with_capacity(arguments.len());
        for argument in arguments {
            self.require_supported_integer_promotion(argument)?;
            let inferred = self.arithmetic_operand_type(argument)?.ty;
            let promoted = match self.operand_type_class(inferred) {
                OperandTypeClass::Integer(integer) => {
                    InferredType::Integer(CInteger::new(0, integer).promoted().ty())
                }
                OperandTypeClass::Real(FloatKind::F32) => InferredType::Real(FloatKind::F64),
                _ => inferred,
            };
            let ty = self.materialize_inferred_type(promoted, Some(argument))?;
            parameters.push(ty);
            *argument = self.alloc_node(CHirExpr::Cast(ty, argument, argument.span()));
        }
        Some(parameters)
    }

    pub(super) fn parse_builtin_strlen(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_builtin_library_call(keyword)
    }
}
