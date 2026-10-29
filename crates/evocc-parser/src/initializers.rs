//! Parser initializers.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_written_initializer(&mut self) -> Option<WrittenInitializer<'arena>> {
        if self.peek_kind() != Some(TokenKind::LBrace) {
            return self.parse_assignment().map(WrittenInitializer::Expression);
        }

        let open = self.advance().expect("peeked initializer brace");
        let mut elements = Vec::new();
        let mut designations = Vec::new();
        if self.peek_kind() != Some(TokenKind::RBrace) {
            loop {
                let field_colon = self.peek_kind() == Some(TokenKind::Identifier)
                    && self.tokens.get(self.pos + 1).map(|token| token.kind)
                        == Some(TokenKind::Colon);
                let designation = if field_colon
                    || matches!(self.peek_kind(), Some(TokenKind::Dot | TokenKind::LBracket))
                {
                    Some(self.parse_initializer_designation()?)
                } else {
                    None
                };
                elements.push(self.parse_written_initializer()?);
                designations.push(designation);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
                if self.peek_kind() == Some(TokenKind::RBrace) {
                    break;
                }
            }
        }
        let close = self.expect(TokenKind::RBrace)?;
        Some(WrittenInitializer::List {
            elements,
            designations,
            span: self.merge_span(open.span, close.span),
        })
    }

    pub(super) fn parse_initializer_designation(
        &mut self,
    ) -> Option<InitializerDesignation<'arena>> {
        let start = self.span();
        if self.peek_kind() == Some(TokenKind::Identifier) {
            let token = self.advance().expect("peeked field designator");
            let colon = self.expect(TokenKind::Colon)?;
            return Some(InitializerDesignation {
                designators: vec![WrittenDesignator::Field(SpannedName {
                    spelling: self.token_text(&token).to_owned(),
                    span: token.span,
                })],
                span: self.merge_span(start, colon.span),
            });
        }
        let mut designators = Vec::new();
        loop {
            match self.peek_kind() {
                Some(TokenKind::Dot) => {
                    self.advance();
                    let token = self.expect(TokenKind::Identifier)?;
                    designators.push(WrittenDesignator::Field(SpannedName {
                        spelling: self.token_text(&token).to_owned(),
                        span: token.span,
                    }));
                }
                Some(TokenKind::LBracket) => {
                    let open = self.advance().expect("peeked designator bracket");
                    let first = self.parse_conditional()?;
                    if self.match_kind(TokenKind::Ellipsis) {
                        let last = self.parse_conditional()?;
                        let close = self.expect(TokenKind::RBracket)?;
                        designators.push(WrittenDesignator::Range {
                            first,
                            last,
                            span: self.merge_span(open.span, close.span),
                        });
                    } else {
                        let close = self.expect(TokenKind::RBracket)?;
                        designators.push(WrittenDesignator::Index {
                            expression: first,
                            span: self.merge_span(open.span, close.span),
                        });
                    }
                }
                _ => break,
            }
        }
        let end = if self.peek_kind() != Some(TokenKind::Assign) {
            match designators.as_slice() {
                [WrittenDesignator::Index { span, .. } | WrittenDesignator::Range { span, .. }] => {
                    *span
                }
                _ => self.expect(TokenKind::Assign)?.span,
            }
        } else {
            self.advance().expect("peeked designation assignment").span
        };
        Some(InitializerDesignation {
            designators,
            span: self.merge_span(start, end),
        })
    }

    pub(super) fn infer_initializer_target_type(
        &mut self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
    ) -> Option<TypeId> {
        if self.initialized_array_contains_vla(target) {
            self.report("E2203", "variable-length-object", written.span());
            return None;
        }
        let canonical = self.types.get(target)?.clone();
        let TypeKind::Array { element, bound } = canonical.kind else {
            return Some(target);
        };
        match bound {
            ArrayBound::Fixed(_) => Some(target),
            ArrayBound::Variable | ArrayBound::Captured(_) => {
                unreachable!("VLA targets are rejected above")
            }
            ArrayBound::Unbounded => {
                let count = match written {
                    WrittenInitializer::Expression(CHirExpr::StringLit(literal, span)) => {
                        if !matches!(
                            literal.prefix,
                            StringPrefix::None
                                | StringPrefix::U8
                                | StringPrefix::L
                                | StringPrefix::L16
                                | StringPrefix::LInt
                        ) {
                            self.report(
                                "E2205",
                                "not-covered-day5: wide-string-initializer",
                                *span,
                            );
                            return None;
                        }
                        if !self.string_literal_element_matches(element, literal.prefix) {
                            self.report("E2202", "invalid-initializer-type", *span);
                            return None;
                        }
                        literal.explicit_unit_count()?.checked_add(1)?
                    }
                    WrittenInitializer::Expression(expression) => {
                        self.report("E2202", "invalid-initializer-type", expression.span());
                        return None;
                    }
                    WrittenInitializer::List {
                        elements,
                        designations,
                        span,
                    } => {
                        if let Some(WrittenInitializer::Expression(CHirExpr::StringLit(
                            literal,
                            literal_span,
                        ))) = elements.first()
                        {
                            if !matches!(
                                literal.prefix,
                                StringPrefix::None
                                    | StringPrefix::U8
                                    | StringPrefix::L
                                    | StringPrefix::L16
                                    | StringPrefix::LInt
                            ) {
                                self.report(
                                    "E2205",
                                    "not-covered-day5: wide-string-initializer",
                                    *literal_span,
                                );
                                return None;
                            }
                            if self.string_literal_element_matches(element, literal.prefix) {
                                if elements.len() > 1 {
                                    self.report("E2201", "excess-initializer", elements[1].span());
                                    return None;
                                }
                                literal.explicit_unit_count()?.checked_add(1)?
                            } else {
                                self.inferred_array_element_count(
                                    target,
                                    elements,
                                    designations,
                                    *span,
                                )?
                            }
                        } else {
                            self.inferred_array_element_count(
                                target,
                                elements,
                                designations,
                                *span,
                            )?
                        }
                    }
                };
                Some(self.intern(
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Fixed(count),
                    },
                    canonical.qualifiers,
                ))
            }
        }
    }

    /// Resolve the base type denoted by `__auto_type` from its required
    /// initializer. The converted initializer determines the declared object
    /// type; written pointer layers are peeled to recover the declaration
    /// specifier's base and then reapplied by the ordinary declarator path.
    pub(super) fn infer_auto_type_base(
        &mut self,
        written: &WrittenInitializer<'arena>,
        derived: &[DerivedDeclarator<'arena>],
        declarator_span: SourceSpan,
    ) -> Option<TypeId> {
        let WrittenInitializer::Expression(expression) = written else {
            self.report("E2915", "auto-type-misuse", declarator_span);
            return None;
        };
        let mut inferred = self.generic_controlling_type(expression).or_else(|| {
            self.report("E2202", "invalid-initializer-type", expression.span());
            None
        })?;
        for layer in derived {
            let DerivedDeclarator::Pointer { .. } = layer else {
                // Arrays and functions are rejected at the declarator before
                // the initializer is parsed; keep this helper total if a
                // future caller forgets that boundary.
                self.report("E2915", "auto-type-misuse", declarator_span);
                return None;
            };
            let Some(TypeKind::Pointer(pointee)) = self
                .types
                .get(inferred)
                .map(|canonical| canonical.kind.clone())
            else {
                self.report("E2202", "invalid-initializer-type", expression.span());
                return None;
            };
            inferred = pointee;
        }
        Some(inferred)
    }

    pub(super) fn initialized_array_contains_vla(&self, target: TypeId) -> bool {
        match self.types.get(target).map(|canonical| &canonical.kind) {
            Some(TypeKind::Array {
                bound: ArrayBound::Variable | ArrayBound::Captured(_),
                ..
            }) => true,
            Some(TypeKind::Array { element, .. }) => self.initialized_array_contains_vla(*element),
            _ => false,
        }
    }

    pub(super) fn initializer_has_static_storage(
        &self,
        context: DeclarationContext,
        storage: StorageClass,
    ) -> bool {
        context == DeclarationContext::File
            || storage.base() == StorageClass::Static
            || storage.is_thread_local()
    }

    pub(super) fn validate_static_initializer_plan(
        &mut self,
        plan: &InitPlan<'arena>,
    ) -> Option<()> {
        let mut pending = vec![plan];
        while let Some(plan) = pending.pop() {
            for action in plan.actions() {
                match action {
                    InitAction::StoreScalar { value, .. }
                        if !self.static_initializer_expression_is_constant(value) =>
                    {
                        if evocc_chir::constant::unmodeled_complex_arithmetic(
                            value,
                            &self.types,
                            &self.target_layout,
                        ) {
                            // Darwin folds no complex product or quotient:
                            // its compiler-rt sequence has no model yet.
                            self.report("E2960", "unmodeled-complex-constant", value.span());
                            return None;
                        }
                        self.report("E2208", "non-constant-static-initializer", value.span());
                        return None;
                    }
                    InitAction::CopyAggregate {
                        value: CHirExpr::CompoundLit(_, initializer, _),
                        ..
                    } => pending.push(initializer.plan()),
                    _ => {}
                }
            }
        }
        Some(())
    }

    pub(super) fn static_integer_value(&self, expression: &CHirExpr<'_>) -> Option<CInteger> {
        evocc_chir::constant::eval_static_integer_constant(
            expression,
            &self.types,
            &self.symbols,
            &self.target_layout,
            &self.initialized_const_integers,
        )
    }

    pub(super) fn remember_initialized_const_integer(
        &mut self,
        symbol: SymbolId,
        ty: TypeId,
        plan: &InitPlan<'arena>,
    ) {
        let Some(canonical) = self.types.get(ty) else {
            return;
        };
        if !canonical.qualifiers.is_const
            || canonical.qualifiers.is_volatile
            || canonical.qualifiers.is_atomic
        {
            return;
        }
        let Ok(integer) = self.canonical_integer_type(ty) else {
            return;
        };
        let [InitAction::StoreScalar { path, value, .. }] = plan.actions() else {
            return;
        };
        if !path.is_root() {
            return;
        }
        let Some(value) = self.static_integer_value(value) else {
            return;
        };
        let previous = self
            .initialized_const_integers
            .insert(symbol, value.cast(integer));
        if self.declaration_transaction_depth > 0 {
            self.initialized_const_undo.push((symbol, previous));
        }
    }

    /// Resolve only the initialization plan. The written expression and all
    /// other occurrences of the object retain their original source meaning.
    pub(super) fn resolve_static_integer_initializers(
        &mut self,
        plan: InitPlan<'arena>,
    ) -> InitPlan<'arena> {
        if self.initialized_const_integers.is_empty() {
            return plan;
        }
        let mut actions = plan.actions().to_vec();
        for action in &mut actions {
            match action {
                InitAction::StoreScalar { value, .. } => {
                    if self.eval_integer_constant_expression(value).is_err() {
                        if let Some(constant) = self.static_integer_value(value) {
                            *value = self.resolved_integer_expression(constant, value.span());
                        }
                    }
                }
                InitAction::CopyAggregate { value, .. } => {
                    if let CHirExpr::CompoundLit(ty, initializer, span) = value {
                        let inner =
                            self.resolve_static_integer_initializers(initializer.plan().clone());
                        *value = self.alloc_node(CHirExpr::CompoundLit(
                            *ty,
                            ObjectInitializer::new(initializer.written().clone(), inner),
                            *span,
                        ));
                    }
                }
                _ => {}
            }
        }
        let mut resolved = InitPlan::new(plan.target_type(), actions)
            .with_flexible_array_elements(plan.flexible_array_elements());
        if plan.has_shared_range_values() {
            resolved = resolved.with_shared_range_values();
        }
        if plan.zero_fills_unwritten() {
            resolved = resolved.with_zero_fill_unwritten();
        }
        resolved
    }

    pub(super) fn resolved_integer_expression(
        &mut self,
        value: CInteger,
        span: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        let ty = self.intern_c_integer_type(value.ty());
        let literal = |bits| {
            self.alloc_node(CHirExpr::Lit(
                IntLit {
                    value: bits,
                    suffix: IntSuffix::ULL,
                    base: IntBase::Hex,
                },
                span,
            ))
        };
        let low = self.alloc_node(CHirExpr::Cast(ty, literal(value.raw_bits() as u64), span));
        if value.raw_bits() >> 64 == 0 {
            return low;
        }
        let high = self.alloc_node(CHirExpr::Cast(
            ty,
            literal((value.raw_bits() >> 64) as u64),
            span,
        ));
        let high = self.alloc_node(CHirExpr::Binary(BinaryOp::Shl, high, literal(64), span));
        self.alloc_node(CHirExpr::Binary(BinaryOp::BitOr, high, low, span))
    }

    pub(super) fn static_initializer_expression_is_constant(
        &self,
        expression: &CHirExpr<'_>,
    ) -> bool {
        self.is_arithmetic_constant_expression(expression)
            || evocc_chir::constant::label_address_difference(expression, &self.types).is_some()
            || self.is_static_address_constant(expression)
            || evocc_chir::constant::eval_absolute_address_constant(
                expression,
                &self.types,
                &self.symbols,
                &self.target_layout,
            )
            .is_some()
    }

    /// Reading storage is never an arithmetic constant expression: C11 6.6
    /// admits only operands whose value is fixed at translation time.
    pub(super) fn is_arithmetic_constant_expression(&self, expression: &CHirExpr<'_>) -> bool {
        if evocc_chir::constant::fold_floating_integer_arithmetic(
            expression,
            &self.types,
            &self.symbols,
            &self.target_layout,
        )
        .is_some()
        {
            return true;
        }
        if self.integer_expression_type(expression).is_ok() {
            return self.eval_integer_constant_expression(expression).is_ok();
        }
        match expression {
            CHirExpr::Lit(_, _) | CHirExpr::FloatLit(_, _) | CHirExpr::CharLit(_, _) => true,
            CHirExpr::BitBuiltin { .. }
            | CHirExpr::BuiltinConstantP { .. }
            | CHirExpr::BuiltinObjectSize { .. } => {
                self.eval_integer_constant_expression(expression).is_ok()
            }
            CHirExpr::MemoryRead { .. } => false,
            CHirExpr::Floating(operation) => {
                evocc_chir::constant::floating_constant_representation(
                    expression,
                    operation.facts.ty(),
                    &self.types,
                    &self.symbols,
                    &self.target_layout,
                )
                .is_some()
            }
            CHirExpr::BuiltinExpect(..) | CHirExpr::Atomic(_) | CHirExpr::Vector(_) => false,
            CHirExpr::Complex(operation) => {
                evocc_chir::complex::constant_components(
                    expression,
                    &self.types,
                    &self.symbols,
                    &self.target_layout,
                )
                .is_some()
                    || evocc_chir::constant::floating_complex_constant(
                        expression,
                        &self.types,
                        &self.symbols,
                        &self.target_layout,
                    )
                    .is_ok()
                    || evocc_chir::constant::floating_constant_representation(
                        expression,
                        operation.facts.ty(),
                        &self.types,
                        &self.symbols,
                        &self.target_layout,
                    )
                    .is_some()
            }
            CHirExpr::Ident { symbol, .. } => self
                .symbols
                .symbol(*symbol)
                .is_ok_and(|symbol| symbol.kind() == SymbolKind::Enumerator),
            CHirExpr::Unary(
                UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Not | UnaryOp::BitNot,
                operand,
                _,
            ) => self.is_arithmetic_constant_expression(operand),
            CHirExpr::EvaluateOnce(operand, _) => self.is_arithmetic_constant_expression(operand),
            CHirExpr::Binary(op, left, right, _) => {
                if *op == BinaryOp::And
                    && self
                        .constant_arithmetic_truth(left)
                        .is_some_and(|truth| !truth)
                {
                    return true;
                }
                if *op == BinaryOp::Or
                    && self
                        .constant_arithmetic_truth(left)
                        .is_some_and(|truth| truth)
                {
                    return true;
                }
                self.is_arithmetic_constant_expression(left)
                    && self.is_arithmetic_constant_expression(right)
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                let Some(condition) = self.constant_arithmetic_truth(condition) else {
                    return false;
                };
                self.is_arithmetic_constant_expression(if condition {
                    when_true
                } else {
                    when_false
                })
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let Some(truth) = self.constant_arithmetic_truth(operand) else {
                    return false;
                };
                truth || self.is_arithmetic_constant_expression(fallback)
            }
            CHirExpr::Cast(target, operand, _) => {
                matches!(
                    self.operand_type_class(InferredType::Canonical(*target)),
                    OperandTypeClass::Integer(_)
                        | OperandTypeClass::Real(_)
                        | OperandTypeClass::Complex(_)
                ) && self.is_arithmetic_constant_expression(operand)
            }
            CHirExpr::SizeOf(ty, _) => self.target_layout.sizeof_type(&self.types, *ty).is_ok(),
            CHirExpr::AlignOf(ty, _) => self.target_layout.alignof_type(&self.types, *ty).is_ok(),
            CHirExpr::SizeOfExpr(_, ty, _) => {
                self.target_layout.sizeof_type(&self.types, *ty).is_ok()
            }
            CHirExpr::Unary(UnaryOp::Abs | UnaryOp::AddrOf | UnaryOp::Deref, _, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. }
            | CHirExpr::BuiltinVarArgs { .. }
            | CHirExpr::BuiltinVarArgPack { .. }
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. }
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Comma(_, _, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _) => false,
        }
    }

    pub(super) fn constant_arithmetic_truth(&self, expression: &CHirExpr<'_>) -> Option<bool> {
        if let Ok(value) = self.eval_integer_constant_expression(expression) {
            return Some(value.truth());
        }
        evocc_chir::constant::floating_constant_truth(
            expression,
            &self.types,
            &self.symbols,
            &self.target_layout,
        )
    }

    pub(super) fn is_static_address_constant(&self, expression: &CHirExpr<'_>) -> bool {
        if self.is_null_pointer_constant(expression) {
            return true;
        }
        if self
            .expression_type(expression)
            .is_some_and(|inferred| match inferred.ty {
                InferredType::Canonical(ty) => matches!(
                    self.types.get(ty).map(|canonical| &canonical.kind),
                    Some(TypeKind::Array { .. })
                ),
                InferredType::StringArray(_) => true,
                InferredType::Integer(_)
                | InferredType::Real(_)
                | InferredType::Complex(_)
                | InferredType::Pointer(_)
                | InferredType::StringPointer(_) => false,
            })
            && self.lvalue_has_static_address(expression)
        {
            return true;
        }
        match expression {
            CHirExpr::StringLit(_, _) | CHirExpr::LabelAddress { .. } => true,
            CHirExpr::CompoundLit(_, _, _) => self.current_scope == self.symbols.file_scope(),
            CHirExpr::Ident { symbol, .. } => self.symbol_decays_to_static_address(*symbol),
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => self.lvalue_has_static_address(operand),
            CHirExpr::Binary(BinaryOp::Add, left, right, _) => {
                (self.is_static_address_constant(left)
                    && self.eval_integer_constant_expression(right).is_ok())
                    || (self.is_static_address_constant(right)
                        && self.eval_integer_constant_expression(left).is_ok())
            }
            CHirExpr::Binary(BinaryOp::Sub, left, right, _) => {
                self.is_static_address_constant(left)
                    && self.eval_integer_constant_expression(right).is_ok()
            }
            CHirExpr::Cast(target, operand, _)
                if matches!(
                    self.types.get(*target).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_))
                ) =>
            {
                self.is_static_address_constant(operand)
                    || self.address_constant_through_integer_cast(operand)
                    || self.eval_integer_constant_expression(operand).is_ok()
            }
            CHirExpr::Cast(_, _, _) if self.address_constant_through_integer_cast(expression) => {
                true
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => self
                .static_initializer_condition_truth(condition)
                .is_some_and(|condition| {
                    self.is_static_address_constant(if condition { when_true } else { when_false })
                }),
            _ => false,
        }
    }

    pub(super) fn static_initializer_condition_truth(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<bool> {
        if let Some(truth) = self.constant_arithmetic_truth(expression) {
            return Some(truth);
        }
        if self.is_null_pointer_constant(expression) {
            return Some(false);
        }
        if self.is_static_address_constant(expression) {
            return Some(true);
        }
        let CHirExpr::Binary(BinaryOp::Eq | BinaryOp::Ne, left, right, _) = expression else {
            return None;
        };
        let left_null = self.is_null_pointer_constant(left);
        let right_null = self.is_null_pointer_constant(right);
        let one_null_and_one_nonnull_address =
            (left_null && self.is_static_address_constant(right) && !right_null)
                || (right_null && self.is_static_address_constant(left) && !left_null);
        one_null_and_one_nonnull_address
            .then_some(matches!(expression, CHirExpr::Binary(BinaryOp::Ne, ..)))
    }

    pub(super) fn address_constant_through_integer_cast(&self, expression: &CHirExpr<'_>) -> bool {
        let CHirExpr::Cast(target, operand, _) = expression else {
            return false;
        };
        if !self.integer_type_can_hold_pointer(*target) {
            return false;
        }
        self.is_static_address_constant(operand)
    }

    pub(super) fn integer_type_can_hold_pointer(&self, ty: TypeId) -> bool {
        if !matches!(
            self.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_))
        ) {
            return false;
        }
        let layout = self.target_layout;
        layout
            .layout_of(&self.types, ty)
            .is_ok_and(|integer| integer.size_bytes >= layout.pointer_layout().size_bytes)
    }

    pub(super) fn lvalue_has_static_address(&self, expression: &CHirExpr<'_>) -> bool {
        match expression {
            CHirExpr::StringLit(_, _) => true,
            CHirExpr::Ident { symbol, .. } => self.symbol_has_static_address(*symbol),
            CHirExpr::MemoryRead { place, .. } => self.memory_place_has_static_address(place),
            CHirExpr::Member(base, _, _) => self.lvalue_has_static_address(base),
            CHirExpr::Index(base, index, _) => {
                self.is_static_address_constant(base)
                    && self.eval_integer_constant_expression(index).is_ok()
            }
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => self.is_static_address_constant(pointer),
            CHirExpr::CompoundLit(_, _, _) => self.current_scope == self.symbols.file_scope(),
            _ => false,
        }
    }

    pub(super) fn memory_place_has_static_address(&self, place: &MemoryPlace<'_>) -> bool {
        let root_is_static = match place.root() {
            PlaceRoot::Object { symbol } => self.symbol_has_static_address(*symbol),
            PlaceRoot::PointerExpression { pointer, .. } => {
                self.is_static_address_constant(pointer)
            }
            PlaceRoot::CompoundLiteral { .. } => self.current_scope == self.symbols.file_scope(),
        };
        root_is_static
            && place
                .projections()
                .iter()
                .all(|projection| match projection {
                    PlaceProjection::Index(index) => {
                        self.eval_integer_constant_expression(index.index()).is_ok()
                    }
                    PlaceProjection::Field(_) => true,
                })
    }

    pub(super) fn symbol_has_static_address(&self, symbol: SymbolId) -> bool {
        let Ok(symbol) = self.symbols.symbol(symbol) else {
            return false;
        };
        if symbol
            .storage_class()
            .is_some_and(StorageClass::is_thread_local)
        {
            return false;
        }
        symbol.kind() == SymbolKind::Function
            || (symbol.kind() == SymbolKind::Object
                && (symbol.scope() == self.symbols.file_scope()
                    || matches!(
                        symbol.storage_class(),
                        Some(StorageClass::Static | StorageClass::Extern)
                    )))
    }

    pub(super) fn symbol_decays_to_static_address(&self, symbol: SymbolId) -> bool {
        let Ok(symbol_data) = self.symbols.symbol(symbol) else {
            return false;
        };
        self.symbol_has_static_address(symbol)
            && (symbol_data.kind() == SymbolKind::Function
                || matches!(
                    self.types.get(symbol_data.ty()).map(|ty| &ty.kind),
                    Some(TypeKind::Array { .. })
                ))
    }

    pub(super) fn inferred_array_element_count(
        &mut self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        designations: &[Option<InitializerDesignation<'arena>>],
        span: SourceSpan,
    ) -> Option<u64> {
        if elements.len() != designations.len() {
            unreachable!("initializer elements and designations are positional peers");
        }
        let TypeKind::Array { element, .. } = self.types.get(target)?.kind else {
            unreachable!("only array initializers infer an element count");
        };
        if designations.iter().any(Option::is_some) {
            if self
                .validate_initializer_object_graph(element, &mut HashSet::new())
                .is_err()
            {
                self.report("E2202", "invalid-initializer-type", span);
                return None;
            }
            let path = SubobjectPath::root();
            let mut cursor = self.next_initializer_child(target, &path, None);
            let mut count = 0;
            for (written, designation) in elements.iter().zip(designations) {
                for destination in self.initializer_element_destinations(
                    target,
                    &path,
                    written,
                    designation.as_ref(),
                    &mut cursor,
                )? {
                    let SubobjectStep::Index { index } = destination.path.steps()[0] else {
                        unreachable!("an array subobject begins with an index");
                    };
                    count = count.max(index.checked_add(1)?);
                }
            }
            return Some(count);
        }
        let mut source_cursor = 0_usize;
        let mut array_cursor = 0_u64;
        let mut count = 0_u64;
        while source_cursor < elements.len() {
            let segment = &elements[source_cursor..];
            let next = match self.initializer_stream_action_count(element, segment, 0) {
                Ok((next, _)) => next,
                Err(
                    InitializerPreflightError::TooLarge
                    | InitializerPreflightError::RecursiveObject
                    | InitializerPreflightError::InvalidObject,
                ) => {
                    self.report("E2202", "invalid-initializer-type", span);
                    return None;
                }
            };
            source_cursor = source_cursor.checked_add(next.max(1))?;
            array_cursor = array_cursor.checked_add(1)?;
            count = count.max(array_cursor);
        }
        Some(count)
    }

    pub(super) fn initializer_stream_consumption(
        &self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        cursor: usize,
    ) -> Option<usize> {
        let initializer = elements.get(cursor)?;
        if matches!(initializer, WrittenInitializer::List { .. })
            || self.direct_initializer_expression(target, initializer)
        {
            return Some(cursor + 1);
        }

        let children = self.aggregate_initializer_children(target)?;
        if children.is_empty() {
            return Some(cursor + 1);
        }
        let mut next = cursor;
        for (_, child) in children {
            if next >= elements.len() {
                break;
            }
            next = self.initializer_stream_consumption(child, elements, next)?;
        }
        Some(next)
    }

    pub(super) fn normalize_initializer_plan(
        &mut self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
        allow_flexible_member: bool,
    ) -> Option<InitPlan<'arena>> {
        let previous = self.flexible_initializer_allowed;
        let previous_extent = self.flexible_initializer_extent;
        self.flexible_initializer_allowed = allow_flexible_member;
        self.flexible_initializer_extent = self
            .final_flexible_array_member(target)
            .filter(|_| allow_flexible_member)
            .map(|(index, _, _)| (index, 0));
        let result = self.normalize_initializer_plan_inner(target, written);
        self.flexible_initializer_allowed = previous;
        self.flexible_initializer_extent = previous_extent;
        result
    }

    pub(super) fn normalize_initializer_plan_inner(
        &mut self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
    ) -> Option<InitPlan<'arena>> {
        match self.validate_initializer_object_graph(target, &mut HashSet::new()) {
            Ok(()) => {}
            Err(
                InitializerPreflightError::TooLarge
                | InitializerPreflightError::RecursiveObject
                | InitializerPreflightError::InvalidObject,
            ) => {
                self.report("E2202", "invalid-initializer-type", written.span());
                return None;
            }
        }
        let mut actions = Vec::new();
        self.normalize_initializer_value(target, SubobjectPath::root(), written, &mut actions)?;
        let flexible_array_elements = self
            .flexible_initializer_element_count(target, &actions)
            .max(
                self.flexible_initializer_extent
                    .map_or(0, |(_, count)| count),
            );
        let mut plan = InitPlan::with_last_write_wins(target, actions, &self.types)
            .with_flexible_array_elements(flexible_array_elements);
        if self.initializer_has_dynamic_range(written, false) {
            plan = plan.with_shared_range_values();
        }
        if matches!(
            self.types.get(target).map(|canonical| &canonical.kind),
            Some(
                TypeKind::Array { .. }
                    | TypeKind::Struct(_)
                    | TypeKind::Union(_)
                    | TypeKind::Vector { .. }
            )
        ) {
            plan = plan.with_zero_fill_unwritten();
        }
        if plan.validate(&self.types).is_err() {
            self.report("E2202", "invalid-initializer-type", written.span());
            return None;
        }
        Some(plan)
    }

    pub(super) fn initializer_has_dynamic_range(
        &self,
        written: &WrittenInitializer<'arena>,
        in_range: bool,
    ) -> bool {
        match written {
            WrittenInitializer::Expression(value) => {
                in_range && !self.static_initializer_expression_is_constant(value)
            }
            WrittenInitializer::List {
                elements,
                designations,
                ..
            } => elements
                .iter()
                .zip(designations)
                .any(|(element, designation)| {
                    let range = in_range
                        || designation.as_ref().is_some_and(|designation| {
                            designation
                                .designators
                                .iter()
                                .any(|part| matches!(part, WrittenDesignator::Range { .. }))
                        });
                    self.initializer_has_dynamic_range(element, range)
                }),
        }
    }

    pub(super) fn validate_initializer_object_graph(
        &self,
        target: TypeId,
        visiting: &mut HashSet<TypeId>,
    ) -> Result<(), InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        match kind {
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_) => Ok(()),
            TypeKind::TargetBuiltin(kind) => kind
                .layout()
                .map(|_| ())
                .ok_or(InitializerPreflightError::InvalidObject),
            TypeKind::Array { element, .. } | TypeKind::Vector { element, .. } => {
                self.validate_initializer_object_graph(element, visiting)
            }
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                if !visiting.insert(target) {
                    return Err(InitializerPreflightError::RecursiveObject);
                }
                let tag = self
                    .types
                    .tag(tag)
                    .ok_or(InitializerPreflightError::InvalidObject)?;
                let fields = match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields))
                    | TagState::Complete(TagDefinition::Union(fields)) => fields,
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        visiting.remove(&target);
                        return Err(InitializerPreflightError::InvalidObject);
                    }
                };
                for field in fields {
                    self.validate_initializer_object_graph(field.ty, visiting)?;
                }
                visiting.remove(&target);
                Ok(())
            }
            TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. } => {
                Err(InitializerPreflightError::InvalidObject)
            }
        }
    }
}
