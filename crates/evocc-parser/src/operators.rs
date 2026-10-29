//! Parser operators.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_conditional(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let condition = self.parse_binary(1)?;
        if !self.match_kind(TokenKind::Question) {
            return Some(condition);
        }
        let (conditional_code, conditional_message) =
            if self.vector_expression_type(condition).is_some() {
                ("E2202", "invalid-initializer-type")
            } else {
                ("E2103", "controlling-expression-not-scalar")
            };
        self.require_scalar_controlling_expression(
            condition,
            conditional_code,
            conditional_message,
        )?;
        let elvis_colon = self.span();
        if self.match_kind(TokenKind::Colon) {
            let fallback = self.parse_conditional()?;
            self.require_supported_conditional_promotion(condition, fallback)?;
            self.require_one_half_format(
                condition,
                fallback,
                self.merge_span(condition.span(), fallback.span()),
            )?;
            let span = self.merge_span(condition.span(), fallback.span());
            let (condition, fallback) =
                self.convert_incompatible_conditional_pointers(condition, fallback, elvis_colon);
            let (condition, fallback) = self.convert_conditional_arms(condition, fallback)?;
            return Some(self.alloc_node(CHirExpr::Elvis(condition, fallback, span)));
        }
        let when_true = self.parse_expression()?;
        let colon = self.expect(TokenKind::Colon)?;
        let when_false = self.parse_conditional()?;
        self.require_supported_conditional_promotion(when_true, when_false)?;
        self.require_one_half_format(
            when_true,
            when_false,
            self.merge_span(when_true.span(), when_false.span()),
        )?;
        let span = self.merge_span(condition.span(), when_false.span());
        let (when_true, when_false) =
            self.convert_incompatible_conditional_pointers(when_true, when_false, colon.span);
        let (when_true, when_false) = self.convert_conditional_arms(when_true, when_false)?;
        Some(self.alloc_node(CHirExpr::Ternary(condition, when_true, when_false, span)))
    }

    pub(super) fn convert_conditional_arms(
        &mut self,
        mut when_true: &'arena CHirExpr<'arena>,
        mut when_false: &'arena CHirExpr<'arena>,
    ) -> Option<(&'arena CHirExpr<'arena>, &'arena CHirExpr<'arena>)> {
        if self.complex_expression_type(when_true).is_some()
            || self.complex_expression_type(when_false).is_some()
        {
            let common = self.conditional_expression_type(
                self.converted_expression_type(when_true)?,
                self.converted_expression_type(when_false)?,
            )?;
            let ty = self.materialize_inferred_type(common, None)?;
            when_true = self.complex_conversion(ty, when_true, when_true.span());
            when_false = self.complex_conversion(ty, when_false, when_false.span());
        }
        Some((when_true, when_false))
    }

    pub(super) fn parse_binary(&mut self, min_prec: u8) -> Option<&'arena CHirExpr<'arena>> {
        let mut lhs = self.parse_unary()?;
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
            let rhs = self.parse_binary(op_prec + 1)?;
            let span = self.merge_span(lhs.span(), rhs.span());
            if matches!(op, BinaryOp::And | BinaryOp::Or) {
                for operand in [lhs, rhs] {
                    let (code, message) = if self.vector_expression_type(operand).is_some() {
                        ("E2102", "invalid-vector-operand")
                    } else {
                        ("E2103", "controlling-expression-not-scalar")
                    };
                    self.require_scalar_controlling_expression(operand, code, message)?;
                }
            }
            if self.complex_expression_type(lhs).is_some()
                || self.complex_expression_type(rhs).is_some()
            {
                lhs = self.complex_binary(op, lhs, rhs, span)?;
                continue;
            }
            if self.vector_expression_type(lhs).is_some()
                || self.vector_expression_type(rhs).is_some()
            {
                lhs = self.make_vector_binary(op, lhs, rhs, span)?;
                continue;
            }
            if matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne
            ) {
                self.require_one_half_format(lhs, rhs, span)?;
            }
            if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
                self.note_pointer_arithmetic_operands(lhs, rhs, span);
            }
            let expression = self.alloc_node(CHirExpr::Binary(op, lhs, rhs, span));
            self.discharge_unsigned_sign_comparison(op, lhs, rhs);
            self.require_supported_binary_promotion(op, lhs, rhs, expression)?;
            lhs = expression;
        }
        Some(lhs)
    }

    /// The operand of `&`: a projected aggregate lvalue keeps its resolved
    /// place, so address-of consumers recover it without executing a load.
    pub(super) fn address_operand(
        &mut self,
        operand: &'arena CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        if matches!(operand, CHirExpr::Ident { .. }) {
            return operand;
        }
        if let Some(place) = self
            .resolve_memory_place(operand)
            .filter(MemoryPlace::is_addressable)
            .filter(|place| !self.is_loadable_scalar(place.value_ty()))
        {
            let facts = ExprFacts::value(place.value_ty());
            self.alloc_node(CHirExpr::MemoryRead {
                place,
                facts,
                target_span: operand.span(),
                span: operand.span(),
            })
        } else {
            operand
        }
    }

    pub(super) fn parse_unary(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_extension_prefix();
        if self.refuse_block_caret() {
            return None;
        }
        if let Some(token) = self
            .tokens
            .get(self.pos)
            .filter(|token| token.kind == TokenKind::Identifier)
        {
            let spelling = self.token_text(token);
            if matches!(spelling, "__real" | "__real__" | "__imag" | "__imag__") {
                let imaginary = spelling.starts_with("__imag");
                let start = self.advance()?.span;
                let operand = self.parse_unary()?;
                return self.complex_component(
                    operand,
                    imaginary,
                    self.merge_span(start, operand.span()),
                );
            }
        }
        if self.peek_kind() == Some(TokenKind::AmpAmp) {
            let operator = self.advance().expect("peeked label-address operator");
            let token = self.expect(TokenKind::Identifier)?;
            let spelling = self.token_text(&token).to_owned();
            if !self.in_function_scope() {
                self.report("E2920", "undeclared-label", token.span);
                return None;
            }
            let label = SpannedName {
                spelling: self.resolve_local_label(&spelling).unwrap_or(spelling),
                span: token.span,
            };
            let void = self.intern(TypeKind::Void, Qualifiers::default());
            let pointer = self.intern(TypeKind::Pointer(void), Qualifiers::default());
            let span = self.merge_span(operator.span, token.span);
            return Some(self.alloc_node(CHirExpr::LabelAddress {
                label,
                facts: ExprFacts::value(pointer),
                span,
            }));
        }

        if matches!(
            self.peek_kind(),
            Some(TokenKind::PlusPlus | TokenKind::MinusMinus)
        ) {
            let token = self.advance().expect("peeked prefix update");
            let target = self.parse_unary()?;
            self.require_supported_update_promotion(target)?;
            let typed_target = self.typed_update_target(target)?;
            self.require_admitted_atomic_update(&typed_target, true, target.span())?;
            if let OperandTypeClass::Pointer(pointer) =
                self.operand_type_class(InferredType::Canonical(typed_target.ty()))
            {
                self.note_pointer_arithmetic_pointee(pointer.pointee, target.span());
            }
            let op = if token.kind == TokenKind::PlusPlus {
                UpdateOp::PreIncrement
            } else {
                UpdateOp::PreDecrement
            };
            let span = self.merge_span(token.span, target.span());
            match self.update_constraint(typed_target.ty()) {
                UpdateConstraint::Scalar | UpdateConstraint::PointerBoundary => {}
                UpdateConstraint::AggregateBoundary | UpdateConstraint::Invalid => {
                    self.report("E2102", "invalid-update-operands", span);
                    return None;
                }
            }
            return Some(match typed_target {
                TypedUpdateTarget::Direct(target_identity) => {
                    let place = self.finish_scalar_update_place(target_identity, target.span())?;
                    self.alloc_node(CHirExpr::Update {
                        op,
                        place,
                        target,
                        facts: ExprFacts::value(place.ty()),
                        span,
                    })
                }
                TypedUpdateTarget::Memory(place) => {
                    let facts = ExprFacts::value(place.value_ty());
                    self.alloc_node(CHirExpr::MemoryUpdate {
                        op,
                        place,
                        target_span: target.span(),
                        facts,
                        span,
                    })
                }
            });
        }

        let unary = match self.peek_kind() {
            Some(TokenKind::Minus) => Some(UnaryOp::Neg),
            Some(TokenKind::Plus) => Some(UnaryOp::Plus),
            Some(TokenKind::Bang) => Some(UnaryOp::Not),
            Some(TokenKind::Tilde) => Some(UnaryOp::BitNot),
            Some(TokenKind::Amp) => Some(UnaryOp::AddrOf),
            Some(TokenKind::Star) => Some(UnaryOp::Deref),
            _ => None,
        };
        if let Some(op) = unary {
            let token = self.advance().unwrap();
            let operand = self.parse_unary()?;
            if op == UnaryOp::Not {
                let (code, message) = if self.vector_expression_type(operand).is_some() {
                    ("E2102", "invalid-vector-operand")
                } else {
                    ("E2103", "controlling-expression-not-scalar")
                };
                self.require_scalar_controlling_expression(operand, code, message)?;
            }
            if let Some(ty) = self.complex_expression_type(operand) {
                if matches!(
                    op,
                    UnaryOp::Plus | UnaryOp::Neg | UnaryOp::BitNot | UnaryOp::Not
                ) {
                    let ty = if op == UnaryOp::Not {
                        self.intern_c_integer_type(CIntegerType::INT)
                    } else {
                        ty
                    };
                    return Some(self.complex_node(
                        evocc_chir::complex::ComplexOp::Unary(op),
                        vec![operand],
                        ty,
                        self.merge_span(token.span, operand.span()),
                    ));
                }
            }
            if self.vector_expression_type(operand).is_some()
                && matches!(
                    op,
                    UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Not | UnaryOp::BitNot
                )
            {
                let span = self.merge_span(token.span, operand.span());
                return self.make_vector_unary(op, operand, span);
            }
            if matches!(op, UnaryOp::Neg | UnaryOp::BitNot) {
                self.require_supported_integer_promotion(operand)?;
            }
            if op == UnaryOp::AddrOf && self.place_roots_register_object(operand) {
                self.report("E2928", "address-of-register-object", token.span);
                return None;
            }
            if op == UnaryOp::AddrOf
                && self
                    .resolve_memory_place(operand)
                    .is_some_and(|place| !place.is_addressable())
            {
                self.report("E2102", "invalid-address-operand", token.span);
                return None;
            }
            if op == UnaryOp::AddrOf && self.function_has_incomplete_enum_return(operand) {
                self.report("E2930", "use-with-incomplete-enumeration", token.span);
                return None;
            }
            // Preserve the resolved place even when the operand's aggregate
            // type does not undergo scalar lvalue conversion. Address-of
            // consumers recover this place without executing a load.
            // A direct object already carries its authoritative symbol. Do not
            // snapshot its modifiability: its record tag may be completed later.
            let operand = if op == UnaryOp::AddrOf {
                self.address_operand(operand)
            } else {
                operand
            };
            let span = self.merge_span(token.span, operand.span());
            return Some(self.alloc_node(CHirExpr::Unary(op, operand, span)));
        }

        if self.peek_kind() == Some(TokenKind::Sizeof) {
            let token = self.advance().unwrap();
            if self.looks_like_parenthesized_type() {
                self.advance();
                let side_start = self.side_declarations.len();
                let (ty, tag_event, dynamic_size) =
                    self.parse_type_name_details(None, true, false)?;
                let evaluations = self.take_extent_evaluations(side_start);
                let end = self.peek().map(|token| token.span).unwrap_or(token.span);
                self.expect(TokenKind::RParen)?;
                let span = self.merge_span(token.span, end);
                if let Some(event) = tag_event {
                    let declaration = self.finish_tag_event(event, span, Vec::new());
                    self.side_declarations.push(declaration);
                }
                let runtime_size = dynamic_size.or_else(|| {
                    self.initialized_array_contains_vla(ty)
                        .then(|| self.captured_size_expression(ty, span))
                        .flatten()
                });
                if let Some(mut size) = runtime_size {
                    for evaluation in evaluations.into_iter().rev() {
                        size = self.alloc_node(CHirExpr::Comma(evaluation, size, span));
                    }
                    return Some(size);
                }
                self.note_pointer_arithmetic_type(ty, span);
                return Some(self.alloc_node(CHirExpr::SizeOf(ty, span)));
            }
            let operand = self.parse_unary()?;
            let span = self.merge_span(token.span, operand.span());
            let inferred = self.expression_type(operand).or_else(|| {
                self.report("E1003", "expected-expression", operand.span());
                None
            })?;
            let operand_ty = self.materialize_inferred_type(inferred.ty, Some(operand))?;
            if self.initialized_array_contains_vla(operand_ty) {
                if let Some(size) = self.captured_size_expression(operand_ty, span) {
                    // Unlike a fixed-size operand, a VLA expression executes.
                    // Its array value decays without reading the elements;
                    // cast captures and pointer updates must still happen.
                    return Some(self.alloc_node(CHirExpr::Comma(operand, size, span)));
                }
            }
            self.note_pointer_arithmetic_type(operand_ty, span);
            return Some(self.alloc_node(CHirExpr::SizeOfExpr(operand, operand_ty, span)));
        }

        if matches!(
            self.peek_kind(),
            Some(TokenKind::Alignof | TokenKind::GnuAlignof)
        ) {
            let token = self.advance().unwrap();
            let is_gnu = token.kind == TokenKind::GnuAlignof;
            if is_gnu && self.peek_kind() != Some(TokenKind::LParen) {
                self.report("E2910", "builtin-not-called", token.span);
                return None;
            }
            self.expect(TokenKind::LParen)?;
            if is_gnu && self.peek_kind() == Some(TokenKind::RParen) {
                self.report("E2909", "builtin-arity", token.span);
                return None;
            }
            let (ty, tag_event) = if self
                .peek()
                .is_some_and(|token| self.token_starts_type_name(token))
            {
                self.parse_unevaluated_type_name()
            } else {
                let operand = self.parse_expression()?;
                let inferred = self.expression_type(operand).or_else(|| {
                    self.report("E1003", "expected-expression", operand.span());
                    None
                })?;
                let mut ty = self.materialize_inferred_type(inferred.ty, Some(operand))?;
                let alignment = if let CHirExpr::Ident { symbol, .. } = operand {
                    self.symbols.symbol(*symbol).ok()?.declared_alignment()
                } else {
                    self.expression_record_field(operand)
                        .and_then(|field| field.layout.aligned)
                        .unwrap_or(1)
                };
                if alignment > 1 {
                    let canonical = self.types.get(ty)?.clone();
                    ty = self.intern(
                        canonical.kind,
                        Qualifiers {
                            aligned: Some(canonical.qualifiers.aligned.unwrap_or(1).max(alignment)),
                            ..canonical.qualifiers
                        },
                    );
                }
                Some((ty, None))
            }?;
            if self.peek_kind() != Some(TokenKind::RParen) {
                self.report(
                    if is_gnu { "E2909" } else { "E1002" },
                    if is_gnu {
                        "builtin-arity"
                    } else {
                        "unexpected-token"
                    },
                    self.span(),
                );
                return None;
            }
            let end = self.peek().map(|token| token.span).unwrap_or(token.span);
            self.expect(TokenKind::RParen)?;
            let span = self.merge_span(token.span, end);
            if let Some(event) = tag_event {
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
            self.note_pointer_arithmetic_type(ty, span);
            return Some(self.alloc_node(CHirExpr::AlignOf(ty, span)));
        }

        if self.looks_like_parenthesized_type() {
            let open = self.advance().unwrap();
            let (ty, tag_event, evaluations) = self.parse_cast_type_name()?;
            let close = self.expect(TokenKind::RParen)?;
            if self.peek_kind() == Some(TokenKind::LBrace) {
                if let Some(event) = tag_event {
                    let declaration = self.finish_tag_event(
                        event,
                        self.merge_span(open.span, close.span),
                        Vec::new(),
                    );
                    self.side_declarations.push(declaration);
                }
                let written = self.parse_written_initializer()?;
                let ty = self.infer_initializer_target_type(ty, &written)?;
                let mut plan = self.normalize_initializer_plan(ty, &written, false)?;
                if self.current_scope == self.symbols.file_scope() {
                    plan = self.resolve_static_integer_initializers(plan);
                    self.validate_static_initializer_plan(&plan)?;
                }
                let span = self.merge_span(open.span, written.span());
                let expression = self.alloc_node(CHirExpr::CompoundLit(
                    ty,
                    ObjectInitializer::new(written, plan),
                    span,
                ));
                return self.parse_postfix_tail(expression);
            }
            if !self.peek_kind().is_some_and(can_start_expression) {
                self.report("E1003", "expected-expression", close.span);
                return None;
            }
            let operand = self.parse_unary()?;
            let span = self.merge_span(open.span, operand.span());
            if let Some(event) = tag_event {
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
            let operand = self.resolve_record_cast_operand(ty, operand, span)?;
            let opaque = |ty| {
                matches!(
                    self.types.get(ty).map(|ty| &ty.kind),
                    Some(TypeKind::TargetBuiltin(TargetBuiltinType::ArmMfp8))
                )
            };
            let source_is_opaque = self.converted_expression_type(operand).is_some_and(
                |source| matches!(source.ty, InferredType::Canonical(ty) if opaque(ty)),
            );
            if opaque(ty)
                || (source_is_opaque
                    && !matches!(self.types.get(ty).map(|ty| &ty.kind), Some(TypeKind::Void)))
            {
                self.report("E2102", "invalid-opaque-fp8-conversion", span);
                return None;
            }
            if matches!(
                self.types.get(ty).map(|ty| &ty.kind),
                Some(TypeKind::Vector { .. })
            ) || self.vector_expression_type(operand).is_some()
            {
                return self.make_vector_cast(ty, operand, span);
            }
            if evocc_chir::complex::admitted(&self.types, ty)
                || self.complex_expression_type(operand).is_some()
            {
                let target = self.operand_type_class(InferredType::Canonical(ty));
                if matches!(target, OperandTypeClass::Void) {
                    return Some(self.alloc_node(CHirExpr::Cast(ty, operand, span)));
                }
                if !is_arithmetic_class(target)
                    || !is_arithmetic_class(
                        self.operand_type_class(self.converted_expression_type(operand)?.ty),
                    )
                {
                    self.report("E2102", "invalid-complex-conversion", span);
                    return None;
                }
                return Some(self.complex_conversion(ty, operand, span));
            }
            self.discharge_narrowed_unsigned_mask(ty, operand);
            let mut expression = self.alloc_node(CHirExpr::Cast(ty, operand, span));
            for evaluation in evaluations.into_iter().rev() {
                expression = self.alloc_node(CHirExpr::Comma(evaluation, expression, span));
            }
            return Some(expression);
        }

        self.parse_postfix()
    }

    /// A record cast preserves type identity. GNU union member casts can also
    /// construct a union value, but cannot reinterpret an arbitrary operand.
    /// Reuse checked initialization; Cast keeps the result non-modifiable.
    pub(super) fn resolve_record_cast_operand(
        &mut self,
        target: TypeId,
        operand: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let (tag, union) = match self.types.get(target)?.kind {
            TypeKind::Struct(tag) => (tag, false),
            TypeKind::Union(tag) => (tag, true),
            _ => return Some(operand),
        };
        let inferred = self.converted_expression_type(operand)?;
        let source = self.materialize_inferred_type(inferred.ty, Some(operand))?;
        if self
            .types
            .compatible_ignoring_top_qualifiers(target, source)
        {
            return Some(operand);
        }
        if !union {
            self.report("E2953", "incompatible-record-cast", span);
            return None;
        }
        let member = match &self.types.tag(tag)?.state {
            TagState::Complete(TagDefinition::Union(fields)) => fields
                .iter()
                .enumerate()
                .find(|(_, field)| {
                    self.types
                        .compatible_ignoring_top_qualifiers(field.ty, source)
                        && field.bit_field.is_none_or(|bit| {
                            self.canonical_integer_type(field.ty)
                                .is_ok_and(|integer| bit.width_bits == u16::from(integer.bits()))
                        })
                })
                .map(|(index, field)| (index, field.ty)),
            _ => None,
        };
        let Some((index, ty)) = member else {
            self.report("E2942", "incompatible-union-cast", span);
            return None;
        };
        self.construct_union_member(target, index, ty, operand, span)
    }

    pub(super) fn resolve_union_argument(
        &mut self,
        target: TypeId,
        operand: &'arena CHirExpr<'arena>,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let Some(TypeKind::Union(tag)) = self.types.get(target).map(|ty| ty.kind.clone()) else {
            return Some(operand);
        };
        let inferred = self.converted_expression_type(operand)?;
        let source = self.materialize_inferred_type(inferred.ty, Some(operand))?;
        if self
            .types
            .compatible_ignoring_top_qualifiers(target, source)
        {
            return Some(operand);
        }
        let definition = self.types.tag(tag)?;
        let member = match &definition.state {
            TagState::Complete(TagDefinition::Union(fields)) if definition.transparent_union => {
                fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| {
                        self.types
                            .compatible_ignoring_top_qualifiers(field.ty, source)
                            || (matches!(
                                self.operand_type_class(InferredType::Canonical(field.ty)),
                                OperandTypeClass::Pointer(_)
                                    if self.is_null_pointer_constant(operand)
                            ) || matches!(
                                (
                                    self.operand_type_class(InferredType::Canonical(field.ty)),
                                    self.operand_type_class(InferredType::Canonical(source)),
                                ),
                                (
                                    OperandTypeClass::Pointer(target),
                                    OperandTypeClass::Pointer(source),
                                ) if self.pointer_assignment_compatible(target, source)
                            ))
                    })
                    .map(|(index, field)| (index, field.ty))
            }
            _ => None,
        };
        let Some((index, member_type)) = member else {
            self.report("E2202", "invalid-initializer-type", operand.span());
            return None;
        };
        let value =
            self.construct_union_member(target, index, member_type, operand, operand.span())?;
        Some(self.alloc_node(CHirExpr::Cast(target, value, operand.span())))
    }

    pub(super) fn construct_union_member(
        &mut self,
        target: TypeId,
        index: usize,
        ty: TypeId,
        operand: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let written = WrittenInitializer::Expression(operand);
        let mut actions = Vec::new();
        self.normalize_initializer_value(
            ty,
            SubobjectPath::root().with_field(index),
            &written,
            &mut actions,
        )?;
        let plan = InitPlan::new(target, actions);
        if plan.validate(&self.types).is_err() {
            self.report("E2942", "incompatible-union-cast", span);
            return None;
        }
        Some(self.alloc_node(CHirExpr::CompoundLit(
            target,
            ObjectInitializer::new(written, plan),
            span,
        )))
    }
}
