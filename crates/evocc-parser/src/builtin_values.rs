//! Parser builtin values.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_builtin_object_size(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let dynamic = self.token_text(&keyword) == "__builtin_dynamic_object_size";
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
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let mode_expression = self.parse_required_constant_assignment()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        if self.expression_type(operand).is_none() {
            self.report("E2913", "builtin-operand-not-convertible", operand.span());
            return None;
        }
        let Some(mode) = self
            .eval_integer_constant_expression(mode_expression)
            .ok()
            .and_then(CInteger::try_mathematical_value)
            .and_then(|value| u8::try_from(value).ok())
            .filter(|mode| *mode <= 3)
        else {
            self.report("E2925", "builtin-mode-out-of-range", mode_expression.span());
            return None;
        };
        let result = evocc_chir::constant::builtin_object_size(
            operand,
            mode,
            &self.types,
            &self.symbols,
            &self.target_layout,
        );
        let span = self.merge_span(keyword.span, close.span);
        if dynamic
            || self.current_function_parameters.is_some()
                && (result == u64::MAX || mode >= 2 && result == 0)
        {
            let size_ty = self.intern_c_integer_type(CIntegerType::ULONG);
            return Some(self.alloc_node(CHirExpr::BuiltinObjectSize {
                operand,
                mode,
                dynamic,
                facts: ExprFacts::value(size_ty),
                span,
            }));
        }
        self.front_end_builtins
            .push(FrontEndBuiltinRecord::ObjectSize {
                span,
                operand,
                mode,
                result,
            });
        if result != u64::MAX {
            return Some(self.size_literal(result, span));
        }
        let one = self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: 1,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        ));
        let negative_one = self.alloc_node(CHirExpr::Unary(UnaryOp::Neg, one, span));
        let size_ty = self.intern_c_integer_type(CIntegerType::ULONG);
        Some(self.alloc_node(CHirExpr::Cast(size_ty, negative_one, span)))
    }

    pub(super) fn parse_builtin_has_attribute(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let is_type_name = self
            .peek()
            .is_some_and(|token| self.token_starts_type_name(token));
        let (operand, tag_event) = if is_type_name {
            let (_, tag_event) = self.parse_unevaluated_type_name()?;
            (None, tag_event)
        } else {
            (Some(self.parse_argument_expression()?), None)
        };
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let attribute = self.advance()?;
        if attribute.kind != TokenKind::Identifier {
            self.report("E2909", "builtin-arity", attribute.span);
            return None;
        }
        let attribute_name = Attribute::canonical_name(self.token_text(&attribute));
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
        let result = operand.is_some_and(|expression| {
            self.expression_designates_written_attribute(expression, &attribute_name)
        });
        self.front_end_builtins
            .push(FrontEndBuiltinRecord::HasAttribute {
                span,
                operand,
                attribute: attribute_name,
                result,
            });

        let literal = self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: u64::from(result),
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        ));
        let bool_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        Some(self.alloc_node(CHirExpr::Cast(bool_ty, literal, span)))
    }

    pub(super) fn expression_designates_written_attribute(
        &self,
        expression: &CHirExpr<'arena>,
        attribute: &str,
    ) -> bool {
        match attribute_query_target(expression, &self.types) {
            Some(AttributeQueryTarget::Entity(symbol)) => self
                .entity_attributes
                .get(&symbol)
                .is_some_and(|attributes| attributes.contains(attribute)),
            Some(AttributeQueryTarget::Member { tag, field_index }) => self
                .member_attributes
                .get(&(tag, field_index))
                .is_some_and(|attributes| attributes.contains(attribute)),
            None => false,
        }
    }

    pub(super) fn parse_builtin_varargs(
        &mut self,
        keyword: Token,
        op: VarArgsBuiltinOp,
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
        let first = self.parse_argument_expression()?;
        let second = match op {
            VarArgsBuiltinOp::Start | VarArgsBuiltinOp::Copy => {
                if !self.match_kind(TokenKind::Comma) {
                    self.report("E2909", "builtin-arity", keyword.span);
                    return None;
                }
                Some(self.parse_argument_expression()?)
            }
            VarArgsBuiltinOp::End | VarArgsBuiltinOp::Arg => None,
        };
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");

        let mut va_list_operands = vec![first];
        if op == VarArgsBuiltinOp::Copy {
            va_list_operands.push(second.expect("copy parsed its source operand"));
        }
        if let Some(invalid) = va_list_operands
            .into_iter()
            .find(|operand| !self.expression_is_va_list_object(operand))
        {
            self.report("E2913", "builtin-operand-not-convertible", invalid.span());
            return None;
        }

        if op == VarArgsBuiltinOp::Start {
            let parameter = second.expect("va_start parsed its parameter operand");
            let valid_parameter = match (self.current_function_parameters.as_ref(), parameter) {
                (
                    Some(ParameterSyntax::Prototype {
                        parameters,
                        variadic: true,
                    }),
                    CHirExpr::Ident { symbol, .. },
                ) => parameters
                    .iter()
                    .rev()
                    .find_map(|parameter| parameter.symbol)
                    .is_some_and(|last| last == *symbol),
                _ => false,
            };
            if !valid_parameter {
                self.report(
                    "E2926",
                    "va-start-outside-variadic-function",
                    parameter.span(),
                );
                return None;
            }
        }

        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        Some(self.alloc_node(CHirExpr::BuiltinVarArgs {
            op,
            first,
            second,
            requested_ty: None,
            facts: ExprFacts::value(void_ty),
            span: self.merge_span(keyword.span, close.span),
        }))
    }

    pub(super) fn parse_builtin_va_arg(
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
        let list = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let type_span = self.peek()?.span;
        let (requested_ty, tag_event) = self.parse_type_name()?;
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
        let requested_is_object = self.types.get(requested_ty).is_some_and(|canonical| {
            !matches!(
                canonical.kind,
                TypeKind::Void | TypeKind::Array { .. } | TypeKind::Function { .. }
            )
        });
        if !self.expression_is_va_list_object(list) || !requested_is_object {
            let diagnostic_span = if self.expression_is_va_list_object(list) {
                span
            } else {
                list.span()
            };
            self.report("E2913", "builtin-operand-not-convertible", diagnostic_span);
            return None;
        }
        let changed_by_promotion =
            match self.operand_type_class(InferredType::Canonical(requested_ty)) {
                OperandTypeClass::Integer(integer) => {
                    CInteger::new(0, integer).promoted().ty() != integer
                }
                OperandTypeClass::Real(FloatKind::F32) => true,
                _ => false,
            };
        if changed_by_promotion {
            self.report("E2929", "unpromoted-va-arg-type", type_span);
            return None;
        }
        Some(self.alloc_node(CHirExpr::BuiltinVarArgs {
            op: VarArgsBuiltinOp::Arg,
            first: list,
            second: None,
            requested_ty: Some(requested_ty),
            facts: ExprFacts::value(requested_ty),
            span,
        }))
    }

    pub(super) fn expression_is_va_list_object(&self, expression: &CHirExpr<'arena>) -> bool {
        let ty = match expression {
            // Projected operands retain their object identity in the checked
            // place, even after ordinary lvalue conversion. A list returned
            // by a call has no such place and is still not a valid operand.
            CHirExpr::MemoryRead { place, .. } => place.value_ty(),
            _ => {
                let Some(InferredExpressionType {
                    ty: InferredType::Canonical(ty),
                    category: ExprCategory::LValue,
                    ..
                }) = self.expression_type(expression)
                else {
                    return false;
                };
                ty
            }
        };
        matches!(
            self.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::BuiltinVaList)
        )
    }

    pub(super) fn offsetof_field(&self, record_ty: TypeId, name: &str) -> Option<(u64, TypeId)> {
        let fields = self.record_member_path(record_ty, name)?;
        let target = self.target_layout;
        let mut offset = 0_u64;
        for field in &fields {
            let layout = target.record_layout(&self.types, field.tag()).ok()?;
            let placed = layout
                .fields
                .iter()
                .find(|placed| placed.field_index == field.field_index())?;
            offset = offset.checked_add(placed.byte_offset)?;
        }
        Some((offset, fields.last()?.field_ty()))
    }

    pub(super) fn parse_builtin_bit(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        let spelling = self.token_text(&keyword).to_owned();
        let (op, rank) = match spelling.as_str() {
            "__builtin_ffs" => (BitBuiltinOp::FindFirstSet, IntRank::Int),
            "__builtin_ffsl" => (BitBuiltinOp::FindFirstSet, IntRank::Long),
            "__builtin_ffsll" => (BitBuiltinOp::FindFirstSet, IntRank::LongLong),
            "__builtin_clz" => (BitBuiltinOp::CountLeadingZeros, IntRank::UInt),
            "__builtin_clzl" => (BitBuiltinOp::CountLeadingZeros, IntRank::ULong),
            "__builtin_clzll" => (BitBuiltinOp::CountLeadingZeros, IntRank::ULongLong),
            "__builtin_ctz" => (BitBuiltinOp::CountTrailingZeros, IntRank::UInt),
            "__builtin_ctzl" => (BitBuiltinOp::CountTrailingZeros, IntRank::ULong),
            "__builtin_ctzll" => (BitBuiltinOp::CountTrailingZeros, IntRank::ULongLong),
            "__builtin_popcount" => (BitBuiltinOp::PopulationCount, IntRank::UInt),
            "__builtin_popcountl" => (BitBuiltinOp::PopulationCount, IntRank::ULong),
            "__builtin_popcountll" => (BitBuiltinOp::PopulationCount, IntRank::ULongLong),
            "__builtin_clrsb" => (BitBuiltinOp::RedundantSignBits, IntRank::Int),
            "__builtin_clrsbl" => (BitBuiltinOp::RedundantSignBits, IntRank::Long),
            "__builtin_clrsbll" => (BitBuiltinOp::RedundantSignBits, IntRank::LongLong),
            "__builtin_parity" => (BitBuiltinOp::Parity, IntRank::UInt),
            "__builtin_parityl" => (BitBuiltinOp::Parity, IntRank::ULong),
            "__builtin_parityll" => (BitBuiltinOp::Parity, IntRank::ULongLong),
            "__builtin_bswap16" => (BitBuiltinOp::ByteSwap, IntRank::UShort),
            "__builtin_bswap32" => (BitBuiltinOp::ByteSwap, IntRank::UInt),
            "__builtin_bswap64" => (BitBuiltinOp::ByteSwap, IntRank::ULong),
            _ => unreachable!("BuiltinBit is assigned only to the frozen builtin spellings"),
        };
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
        let Some(inferred) = self.converted_expression_type(operand) else {
            self.report("E2913", "builtin-operand-not-convertible", operand.span());
            return None;
        };
        if !matches!(
            self.operand_type_class(inferred.ty),
            OperandTypeClass::Integer(_)
        ) {
            self.report("E2913", "builtin-operand-not-convertible", operand.span());
            return None;
        }

        let parameter_ty = self.intern(TypeKind::Int(rank), Qualifiers::default());
        let parameter_integer = integer_type_for_rank(rank);
        let span = self.merge_span(keyword.span, close.span);
        if let Ok(value) = self.eval_integer_constant_expression(operand) {
            if let Some(result) = eval_bit_builtin(op, value.cast(parameter_integer)) {
                if op == BitBuiltinOp::ByteSwap {
                    let literal = self.alloc_node(CHirExpr::Lit(
                        IntLit {
                            value: result.raw_bits() as u64,
                            suffix: IntSuffix::None,
                            base: IntBase::Dec,
                        },
                        span,
                    ));
                    return Some(self.alloc_node(CHirExpr::Cast(parameter_ty, literal, span)));
                }
                return Some(
                    self.alloc_node(CHirExpr::Lit(
                        IntLit {
                            value: u64::try_from(result.raw_bits())
                                .expect("bit builtin result has type int"),
                            suffix: IntSuffix::None,
                            base: IntBase::Dec,
                        },
                        span,
                    )),
                );
            }
        }
        let converted = self.alloc_node(CHirExpr::Cast(parameter_ty, operand, operand.span()));
        let result_ty = if op == BitBuiltinOp::ByteSwap {
            parameter_ty
        } else {
            self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default())
        };
        Some(self.alloc_node(CHirExpr::BitBuiltin {
            op,
            operand: converted,
            facts: ExprFacts::value(result_ty),
            span,
        }))
    }

    pub(super) fn parse_builtin_termination(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        let facts = ExprFacts::value(void_ty);
        let span = self.merge_span(keyword.span, close.span);
        Some(self.alloc_node(if keyword.kind == TokenKind::BuiltinTrap {
            CHirExpr::BuiltinTrap { facts, span }
        } else {
            CHirExpr::BuiltinUnreachable { facts, span }
        }))
    }

    pub(super) fn parse_builtin_return_address(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_builtin_stack_address(keyword, false)
    }

    pub(super) fn parse_builtin_frame_address(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_builtin_stack_address(keyword, true)
    }

    pub(super) fn parse_builtin_stack_address(
        &mut self,
        keyword: Token,
        frame_address: bool,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let argument = self.parse_required_constant_assignment()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        // Both builtins take unsigned int, including narrowing a wider ICE.
        // The source profile bounds both depths to 16 bits. Nonzero depths
        // remain builtin nodes, not C null pointer constants, until execution.
        let depth = self
            .eval_integer_constant_expression(argument)
            .map(|value| value.cast(CIntegerType::UINT).raw_bits());
        if !depth
            .as_ref()
            .is_ok_and(|value| *value <= u128::from(u16::MAX))
        {
            self.report("E2913", "builtin-operand-not-convertible", argument.span());
            return None;
        }
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        let pointer_ty = self.intern(TypeKind::Pointer(void_ty), Qualifiers::default());
        let facts = ExprFacts::value(pointer_ty);
        let span = self.merge_span(keyword.span, close.span);
        Some(self.alloc_node(if frame_address {
            CHirExpr::BuiltinFrameAddress {
                depth: depth.expect("validated constant depth"),
                facts,
                span,
            }
        } else {
            CHirExpr::BuiltinReturnAddress {
                depth: depth.expect("validated constant depth"),
                facts,
                span,
            }
        }))
    }

    pub(super) fn parse_checked_arithmetic(
        &mut self,
        keyword: Token,
        op: CheckedArithmeticOp,
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
        let lhs = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let rhs = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let target = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");

        for operand in [lhs, rhs] {
            let Some(inferred) = self.converted_expression_type(operand) else {
                self.report("E2913", "builtin-operand-not-convertible", operand.span());
                return None;
            };
            if !is_integer_class(self.operand_type_class(inferred.ty)) {
                self.report("E2913", "builtin-operand-not-convertible", operand.span());
                return None;
            }
        }

        let Some(target_inferred) = self.converted_expression_type(target) else {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        };
        let OperandTypeClass::Pointer(pointer) = self.operand_type_class(target_inferred.ty) else {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        };
        let Ok(target_integer) = self.canonical_integer_type(pointer.pointee) else {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        };
        if target_integer == CIntegerType::BOOL
            || self.types.is_const_qualified_object(pointer.pointee)
        {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        }

        let result_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        let span = self.merge_span(keyword.span, close.span);
        Some(self.alloc_node(CHirExpr::CheckedArithmetic {
            op,
            lhs,
            rhs,
            target,
            target_ty: pointer.pointee,
            facts: ExprFacts::value(result_ty),
            span,
        }))
    }

    pub(super) fn parse_static_assert_declaration(&mut self) -> Option<()> {
        let keyword = self.advance().expect("peeked static assertion");
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_required_constant_conditional()?;
        let condition_span = condition.span();
        if self.match_kind(TokenKind::Comma) {
            self.expect(TokenKind::StringLiteral)?;
            while self.peek_kind() == Some(TokenKind::StringLiteral) {
                self.advance();
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Semicolon)?;
        match self.eval_integer_constant_expression(condition) {
            Ok(value) if value.truth() => Some(()),
            Ok(_) => {
                self.report("E2911", "static-assertion-failed", keyword.span);
                None
            }
            Err(()) => {
                self.report("E2912", "static-assertion-not-constant", condition_span);
                None
            }
        }
    }

    pub(super) fn parse_generic_selection(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.expect(TokenKind::LParen)?;
        let controlling = self.parse_assignment()?;
        let controlling_span = controlling.span();
        let controlling_ty = self.generic_controlling_type(controlling).or_else(|| {
            self.report("E1003", "expected-expression", controlling_span);
            None
        })?;
        self.expect(TokenKind::Comma)?;

        let mut association_types = Vec::new();
        let mut default = None;
        let mut matching = None;
        let mut association_count = 0_usize;
        let mut duplicate = false;
        loop {
            let index = association_count;
            let association_start = self.span();
            let association_ty = if self.peek_kind() == Some(TokenKind::Default) {
                let token = self.advance().expect("peeked generic default");
                if default.is_some() {
                    self.report(
                        "E2907",
                        "generic-selection-duplicate-association",
                        token.span,
                    );
                    duplicate = true;
                }
                None
            } else {
                let (ty, tag_event) = self.parse_type_name()?;
                if association_types
                    .iter()
                    .any(|previous| self.types.compatible(*previous, ty))
                {
                    self.report(
                        "E2907",
                        "generic-selection-duplicate-association",
                        association_start,
                    );
                    duplicate = true;
                }
                association_types.push(ty);
                if let Some(event) = tag_event {
                    let declaration = self.finish_tag_event(event, association_start, Vec::new());
                    self.side_declarations.push(declaration);
                }
                Some(ty)
            };
            self.expect(TokenKind::Colon)?;
            let expression = self.parse_assignment()?;
            if let Some(association_ty) = association_ty {
                if self.types.compatible(controlling_ty, association_ty) {
                    matching = Some((index, expression));
                }
            } else if default.is_none() {
                default = Some((index, expression));
            }
            association_count += 1;
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        let close = self.expect(TokenKind::RParen)?;
        if duplicate {
            return None;
        }
        let Some((selected_index, selected)) = matching.or(default) else {
            self.report("E2908", "generic-selection-no-match", keyword.span);
            return None;
        };
        self.generic_selections.push(GenericSelectionRecord {
            span: self.merge_span(keyword.span, close.span),
            controlling_span,
            selected_index,
            association_count,
        });
        Some(selected)
    }

    pub(super) fn looks_like_parenthesized_type(&self) -> bool {
        self.peek_kind() == Some(TokenKind::LParen)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| self.token_starts_type_name(token))
    }
}
