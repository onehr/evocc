//! Parser assignments.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    // ── expressions ────────────────────────────────────────────────────

    pub(super) fn parse_expression(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_comma()
    }

    pub(super) fn parse_comma(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let mut expression = self.parse_assignment()?;
        while self.match_kind(TokenKind::Comma) {
            let right = self.parse_assignment()?;
            let span = self.merge_span(expression.span(), right.span());
            expression = self.alloc_node(CHirExpr::Comma(expression, right, span));
        }
        Some(expression)
    }

    pub(super) fn parse_argument_expression(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_assignment()
    }

    pub(super) fn explicit_floating_integer_conversion(
        &self,
        target: TypeId,
        expression: &'arena CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        if evocc_chir::complex::rank(&self.types, target).is_some()
            || (self.complex_expression_type(expression).is_some()
                && self.canonical_integer_type(target).is_ok())
        {
            return self.complex_conversion(target, expression, expression.span());
        }
        // A floating complex target converts its value part by part, and a
        // floating complex value converts to a real floating target through
        // its real part (C11 6.3.1.6, 6.3.1.7).
        if evocc_chir::complex::floating(&self.types, target).is_some()
            || (self.complex_expression_type(expression).is_some()
                && matches!(
                    self.types.get(target).map(|ty| &ty.kind),
                    Some(TypeKind::Float(_))
                ))
        {
            return self.complex_conversion(target, expression, expression.span());
        }
        if self.canonical_integer_type(target).is_ok()
            && self.expression_type(expression).is_some_and(|inferred| {
                matches!(
                    self.operand_type_class(inferred.ty),
                    OperandTypeClass::Real(_)
                )
            })
        {
            self.alloc_node(CHirExpr::Cast(target, expression, expression.span()))
        } else {
            expression
        }
    }

    pub(super) fn parse_assignment(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let target = self.parse_conditional()?;
        let Some(op) = self.peek_kind().and_then(assignment_op) else {
            return Some(target);
        };
        // GCC 14 reports an incompatible pointer assignment at the operator.
        let operator_span = self.span();
        self.advance();
        // Diagnose the target first, but still consume the complete
        // right-associative RHS so recovery resumes at the statement boundary.
        let typed_target = self.typed_update_target(target);
        let mut value = self.parse_assignment()?;
        let complex_target = self.complex_expression_type(target).is_some();
        let complex_update = op != AssignmentOp::Simple
            && (complex_target || self.complex_expression_type(value).is_some());
        if complex_update {
            let span = self.merge_span(target.span(), value.span());
            value = self.complex_update_value(op, target, value, span)?;
        }
        self.require_supported_assignment_promotion(op, target, value)?;
        if op != AssignmentOp::Simple {
            self.require_one_half_format(
                target,
                value,
                self.merge_span(target.span(), value.span()),
            )?;
        }
        let mut typed_target = typed_target?;
        self.require_admitted_atomic_update(
            &typed_target,
            op != AssignmentOp::Simple,
            target.span(),
        )?;
        if op != AssignmentOp::Simple
            && matches!(&typed_target, TypedUpdateTarget::Memory(place) if place.access_qualifiers().is_atomic)
            && self
                .converted_expression_type(value)
                .is_some_and(|inferred| {
                    matches!(
                        self.operand_type_class(inferred.ty),
                        OperandTypeClass::Real(_) | OperandTypeClass::Complex(_)
                    )
                })
        {
            // A floating computation in an atomic update needs the floating
            // flag rule of C11 footnote 113, which this revision lacks.
            self.report("E2702", "atomic-update-unsupported", target.span());
            return None;
        }
        let span = self.merge_span(target.span(), value.span());
        if matches!(op, AssignmentOp::Add | AssignmentOp::Subtract) {
            self.note_pointer_arithmetic_operands(target, value, span);
        }
        if complex_update && !complex_target {
            // A real target is read and written through its object, so the
            // complex computation sees one evaluated place.
            if let TypedUpdateTarget::Direct(_) = typed_target {
                let Some(place) = self.resolve_memory_place(target) else {
                    self.report("E2102", "invalid-update-operands", span);
                    return None;
                };
                typed_target = TypedUpdateTarget::Memory(place);
            }
        }
        let vector_update =
            op != AssignmentOp::Simple && self.vector_expression_type(target).is_some();
        let constraint = if vector_update {
            let binary = evocc_chir::vector::assignment_binary(op).expect("compound operation");
            let operation = self.make_vector_binary(binary, target, value, span)?;
            let CHirExpr::Vector(operation) = operation else {
                unreachable!("typed vector operation")
            };
            value = operation.operands[1];
            UpdateConstraint::Scalar
        } else {
            if op == AssignmentOp::Simple
                && self.incompatible_pointer_conversion(typed_target.ty(), value)
                && self.record_incompatible_pointers(operator_span)
            {
                value = self.incompatible_pointer_cast(typed_target.ty(), value);
            }
            self.assignment_constraint(op, typed_target.ty(), value)
        };
        match constraint {
            UpdateConstraint::Scalar | UpdateConstraint::PointerBoundary => {}
            UpdateConstraint::AggregateBoundary => {}
            UpdateConstraint::Invalid => {
                self.report("E2102", "invalid-update-operands", span);
                return None;
            }
        }
        if constraint == UpdateConstraint::AggregateBoundary
            && self.volatile_aggregate_copy_is_unsupported(typed_target.ty(), value)
        {
            self.report("E2922", "volatile-aggregate-copy-unsupported", span);
            return None;
        }
        let value = if op == AssignmentOp::Simple {
            self.explicit_floating_integer_conversion(typed_target.ty(), value)
        } else {
            value
        };
        Some(match typed_target {
            TypedUpdateTarget::Direct(target_identity) => {
                let place = self.finish_scalar_update_place(target_identity, target.span())?;
                let facts = ExprFacts::value(place.ty());
                self.alloc_node(CHirExpr::Assign {
                    op,
                    place,
                    target,
                    value,
                    facts,
                    span,
                })
            }
            TypedUpdateTarget::Memory(place) => {
                let facts = ExprFacts::value(place.value_ty());
                self.alloc_node(CHirExpr::MemoryAssign {
                    op,
                    place,
                    target_span: target.span(),
                    value,
                    facts,
                    span,
                })
            }
        })
    }

    pub(super) fn typed_update_target(
        &mut self,
        target: &'arena CHirExpr<'arena>,
    ) -> Option<TypedUpdateTarget<'arena>> {
        if let CHirExpr::Unary(UnaryOp::Deref, pointer, _) = target {
            return self
                .dereference_place(pointer, target.span())
                .map(TypedUpdateTarget::Memory);
        }
        // A projected scalar lvalue already carries its checked place, so the
        // target is recovered from it rather than derived a second time.
        if let CHirExpr::MemoryRead { place, .. } = target {
            if !place.is_modifiable() {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                return None;
            }
            return Some(TypedUpdateTarget::Memory(place.clone()));
        }
        let aggregate_target = self.complex_expression_type(target).is_some()
            || self
                .expression_type(target)
                .is_some_and(|inferred| match inferred.ty {
                    InferredType::Canonical(ty) => matches!(
                        self.types.get(ty).map(|canonical| &canonical.kind),
                        Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::Vector { .. })
                    ),
                    _ => false,
                });
        if aggregate_target {
            let place = self.resolve_memory_place(target).or_else(|| {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                None
            })?;
            if !place.is_modifiable() {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                return None;
            }
            return Some(TypedUpdateTarget::Memory(place));
        }
        if matches!(
            target,
            CHirExpr::Member(_, _, _) | CHirExpr::PtrMember(_, _, _) | CHirExpr::Index(_, _, _)
        ) {
            self.report("E2101", "expected-modifiable-lvalue", target.span());
            return None;
        }

        let CHirExpr::Ident { symbol, facts, .. } = target else {
            self.report("E2101", "expected-modifiable-lvalue", target.span());
            return None;
        };
        if !facts.is_modifiable_lvalue() {
            self.report("E2101", "expected-modifiable-lvalue", target.span());
            return None;
        }
        // An `_Atomic` object is read and written through its address, so
        // every consumer sees one memory place (EvoIR 10.1.21).
        if self
            .types
            .get(facts.ty())
            .is_some_and(|canonical| canonical.qualifiers.is_atomic)
        {
            let Some(place) = self.resolve_memory_place(target) else {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                return None;
            };
            return Some(TypedUpdateTarget::Memory(place));
        }
        Some(TypedUpdateTarget::Direct(DirectUpdateTarget {
            symbol: *symbol,
            ty: facts.ty(),
        }))
    }

    /// Resolve a member name to its complete projection path. Anonymous record
    /// members are traversed recursively; two matching anonymous paths are
    /// ambiguous and return `None`. Member access, initializer designators and
    /// `__builtin_offsetof` all use this one lookup rule.
    pub(super) fn record_member_path(
        &self,
        record_ty: TypeId,
        name: &str,
    ) -> Option<Vec<FieldProjection>> {
        record_member_path(&self.types, record_ty, name)
    }

    /// Split a subscript into its pointer-like and integer operands.
    ///
    /// C11 6.5.2.1p1 requires one operand to have pointer-to-object type and
    /// the other integer type, without fixing which is which, so `2[a]` is as
    /// well formed as `a[2]`.
    pub(super) fn subscript_operands<'e>(
        &self,
        left: &'e CHirExpr<'arena>,
        right: &'e CHirExpr<'arena>,
    ) -> Option<(&'e CHirExpr<'arena>, &'e CHirExpr<'arena>)> {
        if self.vector_expression_type(left).is_some() {
            return Some((left, right));
        }
        let left_class = self.operand_type_class(self.converted_expression_type(left)?.ty);
        if matches!(
            left_class,
            OperandTypeClass::Pointer(_) | OperandTypeClass::StringPointer(_)
        ) {
            return Some((left, right));
        }
        let right_class = self.operand_type_class(self.converted_expression_type(right)?.ty);
        if matches!(
            right_class,
            OperandTypeClass::Pointer(_) | OperandTypeClass::StringPointer(_)
        ) {
            return Some((right, left));
        }
        None
    }

    /// The element type a subscript of this base designates.
    pub(super) fn subscript_element_type(&mut self, base: &CHirExpr<'arena>) -> Option<TypeId> {
        if let Some((_, element, _)) = self.vector_expression_type(base) {
            return Some(element);
        }
        match self.operand_type_class(self.converted_expression_type(base)?.ty) {
            OperandTypeClass::Pointer(pointer) => Some(pointer.pointee),
            OperandTypeClass::StringPointer(prefix) => Some(self.string_element_type(prefix)),
            _ => None,
        }
    }

    /// Build the checked place designated by an lvalue expression.
    ///
    /// The walk is outside-in over the written syntax and bottoms out at an
    /// object or at a pointer expression. Returning `None` means the syntax
    /// does not designate a place this slice can represent; the caller reports
    /// the diagnostic, because only the caller knows which construct the
    /// failure belongs to.
    pub(super) fn resolve_memory_place(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Option<MemoryPlace<'arena>> {
        let (root, projections, value_ty) = self.resolve_place_chain(expression)?;
        MemoryPlace::derive_with_target(
            &self.symbols,
            &self.types,
            root,
            projections,
            value_ty,
            &self.target_layout,
        )
        .ok()
    }

    /// Whether the designated object is rooted in `register` storage.
    ///
    /// Projections preserve the root object's storage class. Dereferencing a
    /// pointer does not: a register-qualified pointer object may still point
    /// at an addressable object, so pointer-rooted places deliberately return
    /// false here.
    pub(super) fn place_roots_register_object(&self, expression: &CHirExpr<'arena>) -> bool {
        let symbol = match expression {
            CHirExpr::Vector(_) | CHirExpr::Complex(_) | CHirExpr::Floating(_) => None,
            CHirExpr::Atomic(_) => None,
            CHirExpr::BuiltinConstantP { .. } | CHirExpr::BuiltinObjectSize { .. } => None,
            CHirExpr::Ident { symbol, .. } => Some(*symbol),
            CHirExpr::MemoryRead { place, .. } => match place.root() {
                PlaceRoot::Object { symbol } => Some(*symbol),
                PlaceRoot::PointerExpression { .. } | PlaceRoot::CompoundLiteral { .. } => None,
            },
            CHirExpr::Member(base, _, _) | CHirExpr::Index(base, _, _) => {
                return self.place_roots_register_object(base);
            }
            CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Unary(UnaryOp::Deref, _, _)
            | CHirExpr::Lit(_, _)
            | CHirExpr::FloatLit(_, _)
            | CHirExpr::CharLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Unary(_, _, _)
            | CHirExpr::Binary(_, _, _, _)
            | CHirExpr::Ternary(_, _, _, _)
            | CHirExpr::Elvis(_, _, _)
            | CHirExpr::BuiltinExpect(_, _, _)
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::BitBuiltin { .. }
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. }
            | CHirExpr::BuiltinVarArgs { .. }
            | CHirExpr::BuiltinVarArgPack { .. }
            | CHirExpr::Cast(_, _, _)
            | CHirExpr::EvaluateOnce(_, _)
            | CHirExpr::SizeOf(_, _)
            | CHirExpr::SizeOfExpr(_, _, _)
            | CHirExpr::AlignOf(_, _)
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. }
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Comma(_, _, _) => None,
        };
        symbol.is_some_and(|symbol| {
            self.symbols
                .symbol(symbol)
                .is_ok_and(|symbol| symbol.storage_class() == Some(StorageClass::Register))
        })
    }

    pub(super) fn resolve_place_chain(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Option<(PlaceRoot<'arena>, Vec<PlaceProjection<'arena>>, TypeId)> {
        match expression {
            // Already-resolved subobjects carry their own authority, so a
            // further projection extends that chain instead of re-deriving it
            // from syntax that is deliberately no longer present.
            CHirExpr::MemoryRead { place, .. } => Some((
                place.root().clone(),
                place.projections().to_vec(),
                place.value_ty(),
            )),
            CHirExpr::Ident { symbol, facts, .. } => {
                let declaration = self.symbols.symbol(*symbol).ok()?;
                if !matches!(
                    declaration.kind(),
                    SymbolKind::Object | SymbolKind::Parameter
                ) {
                    return None;
                }
                Some((
                    PlaceRoot::Object { symbol: *symbol },
                    Vec::new(),
                    facts.ty(),
                ))
            }
            CHirExpr::CompoundLit(ty, _, _) => Some((
                PlaceRoot::CompoundLiteral {
                    expression,
                    ty: *ty,
                },
                Vec::new(),
                *ty,
            )),
            CHirExpr::StringLit(literal, _) => {
                // The place is the entire literal array, not its first
                // character. Ordinary value conversion still decays it.
                let array_ty = literal.array_type(&self.types)?;
                let pointer_type = PointerTypeDesc::to_object(&self.types, array_ty)?;
                let pointer = self.alloc_node(CHirExpr::Unary(
                    UnaryOp::AddrOf,
                    expression,
                    expression.span(),
                ));
                Some((
                    PlaceRoot::PointerExpression {
                        pointer,
                        pointer_type,
                    },
                    Vec::new(),
                    array_ty,
                ))
            }
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                // Taking an address and immediately dereferencing it preserves
                // the original place identity. Keep that checked root and its
                // projections instead of turning `&place` into an opaque
                // pointer-expression root that a later consumer would have to
                // rediscover from syntax.
                if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = pointer {
                    return self.resolve_place_chain(addressed);
                }
                let pointer_type = self.pointer_root_descriptor(pointer)?;
                Some((
                    PlaceRoot::PointerExpression {
                        pointer,
                        pointer_type,
                    },
                    Vec::new(),
                    pointer_type.pointee(),
                ))
            }
            CHirExpr::Member(base, name, _) => {
                let (root, mut projections, base_ty) = self.resolve_place_chain(base)?;
                let fields = self.record_member_path(base_ty, name)?;
                let field_ty = fields.last()?.field_ty();
                projections.extend(fields.into_iter().map(PlaceProjection::Field));
                Some((root, projections, field_ty))
            }
            CHirExpr::PtrMember(pointer, name, _) => {
                if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = pointer {
                    let (root, mut projections, addressed_ty) =
                        self.resolve_place_chain(addressed)?;
                    let fields = self.record_member_path(addressed_ty, name)?;
                    let field_ty = fields.last()?.field_ty();
                    projections.extend(fields.into_iter().map(PlaceProjection::Field));
                    return Some((root, projections, field_ty));
                }
                if self.designates_array_object(pointer) {
                    if let Some((root, mut projections, array_ty)) =
                        self.resolve_place_chain(pointer)
                    {
                        let element_ty = match self.types.get(array_ty).map(|ty| &ty.kind) {
                            Some(TypeKind::Array { element, .. }) => *element,
                            _ => return None,
                        };
                        let zero = self.alloc_node(CHirExpr::Lit(
                            IntLit {
                                value: 0,
                                suffix: IntSuffix::None,
                                base: IntBase::Dec,
                            },
                            pointer.span(),
                        ));
                        projections.push(PlaceProjection::Index(IndexProjection::new(
                            zero,
                            element_ty,
                            CIntegerType::INT,
                        )));
                        let fields = self.record_member_path(element_ty, name)?;
                        let field_ty = fields.last()?.field_ty();
                        projections.extend(fields.into_iter().map(PlaceProjection::Field));
                        return Some((root, projections, field_ty));
                    }
                }
                // As with [], an array reached through a record value decays
                // to a pointer even when there is no source object root.
                let pointer_type = self.pointer_root_descriptor(pointer)?;
                let fields = self.record_member_path(pointer_type.pointee(), name)?;
                let field_ty = fields.last()?.field_ty();
                Some((
                    PlaceRoot::PointerExpression {
                        pointer,
                        pointer_type,
                    },
                    fields.into_iter().map(PlaceProjection::Field).collect(),
                    field_ty,
                ))
            }
            CHirExpr::Index(left, right, _) => {
                let (base, index) = self.subscript_operands(left, right)?;
                let element_ty = self.subscript_element_type(base)?;
                let promoted = self.promoted_subscript_type(index)?;
                let projection =
                    PlaceProjection::Index(IndexProjection::new(index, element_ty, promoted));
                // An array lvalue is subscripted in place; anything else has
                // already decayed to a pointer value and becomes the root.
                if self.designates_array_object(base) || self.vector_expression_type(base).is_some()
                {
                    if let Some((root, mut projections, _)) = self.resolve_place_chain(base) {
                        projections.push(projection);
                        return Some((root, projections, element_ty));
                    }
                }
                // A returned record's array member has no source lvalue root,
                // but its array-to-pointer conversion still yields a pointer.
                // Keep the value expression attached so it executes once.
                let pointer_type = self.pointer_root_descriptor(base)?;
                Some((
                    PlaceRoot::PointerExpression {
                        pointer: base,
                        pointer_type,
                    },
                    vec![projection],
                    element_ty,
                ))
            }
            _ => None,
        }
    }

    /// Does this expression designate an array object, rather than hold a
    /// pointer value that already decayed from one?
    pub(super) fn designates_array_object(&self, expression: &CHirExpr<'arena>) -> bool {
        let Some(inferred) = self.expression_type(expression) else {
            return false;
        };
        match inferred.ty {
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
        }
    }

    /// The canonical pointer descriptor of an expression used as a place root.
    pub(super) fn pointer_root_descriptor(
        &mut self,
        expression: &CHirExpr<'arena>,
    ) -> Option<PointerTypeDesc> {
        if let Some(InferredExpressionType {
            ty: InferredType::StringPointer(prefix),
            ..
        }) = self.converted_expression_type(expression)
        {
            let element = self.string_element_type(prefix);
            return PointerTypeDesc::to_object(&self.types, element);
        }
        PointerTypeDesc::from_expression_with_target(
            &self.symbols,
            &self.types,
            expression,
            &self.target_layout,
        )
        .ok()
    }

    /// The subscript's type after integer promotion, which is what a backend
    /// needs to choose a machine width for the scaled offset.
    pub(super) fn promoted_subscript_type(&self, index: &CHirExpr<'arena>) -> Option<CIntegerType> {
        match self.operand_type_class(self.converted_expression_type(index)?.ty) {
            OperandTypeClass::Integer(integer) => Some(CInteger::new(0, integer).promoted().ty()),
            OperandTypeClass::WideInteger { ty, .. } => self.canonical_integer_type(ty).ok(),
            _ => None,
        }
    }

    /// Convert a projected lvalue to a value where C requires it.
    ///
    /// Scalar loads and array decay both retain the checked place. Further
    /// projections extend that place rather than recovering discarded syntax.
    /// Record temporaries retain their value-producing expression below;
    /// record lvalues retain their checked place just like scalar loads.
    pub(super) fn convert_projected_lvalue(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        let span = expression.span();
        // A member selected from an aggregate value (for example
        // `(destination = source).field`) is not rooted in a separately
        // derivable lvalue.  Keep the value-producing base attached so the
        // interpreter and lowering evaluate the copy exactly once before
        // selecting the member.
        if let CHirExpr::Member(base, _, _) = expression {
            if !matches!(base, CHirExpr::MemoryRead { .. })
                && self
                    .expression_type(base)
                    .is_some_and(|inferred| inferred.category == ExprCategory::Value)
            {
                return expression;
            }
        }
        // A projection that cannot even be typed is ill formed, not merely
        // outside this slice, so it is reported here rather than left to
        // surface as a confusing failure in a later stage.
        let Some(inferred) = self.expression_type(expression) else {
            self.report("E2102", "invalid-projected-lvalue", span);
            return expression;
        };
        let value_ty = match inferred.ty {
            InferredType::Canonical(value_ty) => value_ty,
            inferred => {
                let Some(value_ty) = self.materialize_inferred_type(inferred, Some(expression))
                else {
                    return expression;
                };
                value_ty
            }
        };
        if !self.is_loadable_scalar(value_ty)
            && !evocc_chir::complex::admitted(&self.types, value_ty)
            && !matches!(
                self.types.get(value_ty).map(|ty| &ty.kind),
                Some(
                    TypeKind::Array { .. }
                        | TypeKind::Vector { .. }
                        | TypeKind::Struct(_)
                        | TypeKind::Union(_)
                        | TypeKind::BuiltinVaList
                )
            )
        {
            return expression;
        }
        let Some(place) = self.resolve_memory_place(expression) else {
            self.report("E2102", "invalid-projected-lvalue", span);
            return expression;
        };
        let facts = ExprFacts::value(place.value_ty());
        self.alloc_node(CHirExpr::MemoryRead {
            place,
            target_span: span,
            facts,
            span,
        })
    }

    pub(super) fn is_loadable_scalar(&self, ty: TypeId) -> bool {
        matches!(
            self.types.get(ty).map(|canonical| &canonical.kind),
            Some(
                TypeKind::Int(_)
                    | TypeKind::Enum(_)
                    | TypeKind::GnuInt128(_)
                    | TypeKind::Pointer(_)
                    | TypeKind::Float(_)
                    | TypeKind::TargetBuiltin(_)
            )
        )
    }

    pub(super) fn dereference_place(
        &mut self,
        pointer: &'arena CHirExpr<'arena>,
        target_span: SourceSpan,
    ) -> Option<MemoryPlace<'arena>> {
        if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = pointer {
            let place = self.resolve_memory_place(addressed).or_else(|| {
                self.report("E2102", "invalid-update-operands", target_span);
                None
            })?;
            if !place.is_modifiable() {
                self.report("E2101", "expected-modifiable-lvalue", target_span);
                return None;
            }
            return Some(place);
        }
        let pointer_type = self.pointer_root_descriptor(pointer).or_else(|| {
            self.report("E2102", "invalid-update-operands", target_span);
            None
        })?;
        let place = match MemoryPlace::derive_with_target(
            &self.symbols,
            &self.types,
            PlaceRoot::PointerExpression {
                pointer,
                pointer_type,
            },
            Vec::new(),
            pointer_type.pointee(),
            &self.target_layout,
        ) {
            Ok(place) => place,
            Err(_) => {
                self.report("E2102", "invalid-update-operands", target_span);
                return None;
            }
        };
        if !place.is_modifiable() {
            self.report("E2101", "expected-modifiable-lvalue", target_span);
            return None;
        }
        Some(place)
    }

    /// The admitted `_Atomic` updates (EvoIR 10.1.21). A simple assignment
    /// stores a 1-, 2-, 4- or 8-byte scalar; a compound assignment, `++` or
    /// `--` also excludes floating types. Every other update of an atomic
    /// place is `E2702`.
    pub(super) fn require_admitted_atomic_update(
        &mut self,
        target: &TypedUpdateTarget<'arena>,
        compound: bool,
        span: SourceSpan,
    ) -> Option<()> {
        let TypedUpdateTarget::Memory(place) = target else {
            return Some(());
        };
        if !place.access_qualifiers().is_atomic || place.accesses_atomic_member() {
            // A member of an atomic record is `E2956` at the access.
            return Some(());
        }
        let ty = place.value_ty();
        let floating = matches!(
            self.types.get(ty).map(|canonical| &canonical.kind),
            Some(TypeKind::Float(_))
        );
        if !evocc_chir::atomic_access_admitted(&self.types, &self.target_layout, ty)
            || (compound && floating)
        {
            self.report("E2702", "atomic-update-unsupported", span);
            return None;
        }
        Some(())
    }

    pub(super) fn finish_scalar_update_place(
        &mut self,
        target: DirectUpdateTarget,
        target_span: SourceSpan,
    ) -> Option<ScalarPlace> {
        let canonical = self
            .types
            .get(target.ty)
            .expect("identifier facts only reference interned types");
        if canonical.qualifiers.is_atomic {
            self.report("E2702", "atomic-update-unsupported", target_span);
            return None;
        }
        match ScalarPlace::bind(&self.symbols, &self.types, target.symbol) {
            Ok(place) => Some(place),
            Err(_) => {
                self.report("E2101", "expected-modifiable-lvalue", target_span);
                None
            }
        }
    }

    pub(super) fn assignment_constraint(
        &self,
        op: AssignmentOp,
        target_ty: TypeId,
        value: &CHirExpr<'_>,
    ) -> UpdateConstraint {
        if op == AssignmentOp::Simple {
            self.note_pointer_conversion(target_ty, value);
        }
        let left = self.operand_type_class(InferredType::Canonical(target_ty));
        let Some(right) = self.converted_expression_type(value) else {
            return UpdateConstraint::Invalid;
        };
        let right_type = right.ty;
        let right_class = self.operand_type_class(right_type);

        match op {
            AssignmentOp::Simple => match (left, right_class) {
                (OperandTypeClass::Other, OperandTypeClass::Other)
                    if matches!(right_type, InferredType::Canonical(source)
                        if self.types.compatible_ignoring_top_qualifiers(target_ty, source)
                        && matches!(self.types.get(target_ty).map(|ty| &ty.kind), Some(TypeKind::BuiltinVaList))) =>
                {
                    UpdateConstraint::AggregateBoundary
                }
                (OperandTypeClass::Other, OperandTypeClass::Other)
                    if matches!(right_type, InferredType::Canonical(source)
                        if self.types.compatible_ignoring_top_qualifiers(target_ty, source)
                        && matches!(self.types.get(target_ty).map(|ty| &ty.kind), Some(TypeKind::TargetBuiltin(_)))) =>
                {
                    UpdateConstraint::Scalar
                }
                (left, right) if is_arithmetic_class(left) && is_arithmetic_class(right) => {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Integer(ty), OperandTypeClass::Pointer(_))
                    if ty == CIntegerType::BOOL =>
                {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Integer(ty), OperandTypeClass::StringPointer(_))
                    if ty == CIntegerType::BOOL =>
                {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right_pointer)) => {
                    if right.null_pointer_constant
                        || self.pointer_assignment_compatible(left, right_pointer)
                    {
                        UpdateConstraint::Scalar
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                (OperandTypeClass::Pointer(left), OperandTypeClass::StringPointer(prefix)) => {
                    if self.pointer_accepts_string(left, prefix) {
                        UpdateConstraint::Scalar
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                (OperandTypeClass::Pointer(_), integer)
                    if is_integer_class(integer) && right.null_pointer_constant =>
                {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Aggregate(left), OperandTypeClass::Aggregate(right)) => {
                    if self.same_unqualified_type(left, right)
                        || evocc_chir::vector::assignment_compatible(&self.types, left, right)
                    {
                        UpdateConstraint::AggregateBoundary
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                _ => UpdateConstraint::Invalid,
            },
            AssignmentOp::Add | AssignmentOp::Subtract => match (left, right_class) {
                (left, right) if is_arithmetic_class(left) && is_arithmetic_class(right) => {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Pointer(pointee), integer) if is_integer_class(integer) => {
                    if self.pointer_arithmetic_is_valid(pointee.pointee) {
                        UpdateConstraint::PointerBoundary
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                _ => UpdateConstraint::Invalid,
            },
            AssignmentOp::Multiply | AssignmentOp::Divide => {
                if is_arithmetic_class(left) && is_arithmetic_class(right_class) {
                    UpdateConstraint::Scalar
                } else {
                    UpdateConstraint::Invalid
                }
            }
            AssignmentOp::Remainder
            | AssignmentOp::ShiftLeft
            | AssignmentOp::ShiftRight
            | AssignmentOp::BitAnd
            | AssignmentOp::BitOr
            | AssignmentOp::BitXor => {
                if is_integer_class(left) && is_integer_class(right_class) {
                    UpdateConstraint::Scalar
                } else {
                    UpdateConstraint::Invalid
                }
            }
        }
    }

    pub(super) fn volatile_aggregate_copy_is_unsupported(
        &self,
        target_ty: TypeId,
        value: &CHirExpr<'_>,
    ) -> bool {
        let Some(target) = self.types.get(target_ty) else {
            return false;
        };
        let source_is_volatile = self
            .expression_type(value)
            .and_then(|inferred| match inferred.ty {
                InferredType::Canonical(ty) => self.types.get(ty),
                InferredType::Integer(_)
                | InferredType::Real(_)
                | InferredType::Complex(_)
                | InferredType::Pointer(_)
                | InferredType::StringArray(_)
                | InferredType::StringPointer(_) => None,
            })
            .is_some_and(|source| source.qualifiers.is_volatile);
        if !target.qualifiers.is_volatile && !source_is_volatile {
            return false;
        }
        match &target.kind {
            TypeKind::Union(_) => !self
                .target_layout
                .layout_of(&self.types, target_ty)
                .is_ok_and(|layout| {
                    matches!(layout.size_bytes, 1 | 2 | 4 | 8)
                        && u64::from(layout.align_bytes) >= layout.size_bytes
                }),
            TypeKind::Struct(tag) => self.types.tag(*tag).is_some_and(|tag| {
                matches!(
                    &tag.state,
                    TagState::Complete(TagDefinition::Struct(fields))
                        if fields.iter().any(|field| field.bit_field.is_some())
                )
            }),
            TypeKind::Void
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Array { .. }
            | TypeKind::Function { .. }
            | TypeKind::Enum(_)
            | TypeKind::Vector { .. }
            | TypeKind::BuiltinVaList
            | TypeKind::TargetBuiltin(_)
            | TypeKind::GnuInt128(_) => false,
        }
    }

    pub(super) fn update_constraint(&self, target_ty: TypeId) -> UpdateConstraint {
        // L05: `++` and `--` add or subtract one in each lane of a floating
        // vector, as GCC 14 does.
        if evocc_chir::vector::floating(&self.types, target_ty).is_some() {
            return UpdateConstraint::Scalar;
        }
        match self.operand_type_class(InferredType::Canonical(target_ty)) {
            OperandTypeClass::Integer(_)
            | OperandTypeClass::WideInteger { .. }
            | OperandTypeClass::Real(_) => UpdateConstraint::Scalar,
            // GNU C: `++` and `--` add or subtract one on the real part of a
            // complex object, as GCC does for floating and integer complex.
            OperandTypeClass::Complex(base)
                if base.executable_part().is_some() || matches!(base, ComplexBase::Integer(_)) =>
            {
                UpdateConstraint::Scalar
            }
            OperandTypeClass::Pointer(pointee) => {
                if self.pointer_arithmetic_is_valid(pointee.pointee) {
                    UpdateConstraint::PointerBoundary
                } else {
                    UpdateConstraint::Invalid
                }
            }
            OperandTypeClass::Complex(_)
            | OperandTypeClass::StringPointer(_)
            | OperandTypeClass::Aggregate(_)
            | OperandTypeClass::Void
            | OperandTypeClass::Other => UpdateConstraint::Invalid,
        }
    }

    /// The C type of a real format: `_Float16` for binary16 and `__bf16` for
    /// bfloat16, which are target builtins, not standard floating types.
    pub(super) fn real_type(&mut self, kind: FloatKind) -> TypeId {
        let kind = match kind {
            FloatKind::F16 => TypeKind::TargetBuiltin(TargetBuiltinType::GnuFloat16),
            FloatKind::BF16 => TypeKind::TargetBuiltin(TargetBuiltinType::ArmBf16),
            kind => TypeKind::Float(kind),
        };
        self.intern(kind, Qualifiers::default())
    }
}
