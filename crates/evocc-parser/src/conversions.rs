//! Parser conversions.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn is_arm_fp16(&self, ty: InferredType) -> bool {
        matches!(ty, InferredType::Canonical(ty) if matches!(
            self.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::TargetBuiltin(TargetBuiltinType::ArmFp16))
        ))
    }

    /// `_Float16` and `__bf16` operands of one operator have no common type.
    /// Report the named refusal at the operator (EvoIR 3.13.6).
    pub(super) fn require_one_half_format(
        &mut self,
        left: &CHirExpr<'_>,
        right: &CHirExpr<'_>,
        span: SourceSpan,
    ) -> Option<()> {
        let class = |this: &Self, expression: &CHirExpr<'_>| {
            this.converted_expression_type(expression)
                .map(|inferred| this.operand_type_class(inferred.ty))
        };
        if let (Some(left), Some(right)) = (class(self, left), class(self, right)) {
            if mixes_half_formats(left, right) {
                self.report("E2961", "mixed-half-formats", span);
                return None;
            }
        }
        Some(())
    }

    pub(super) fn usual_arithmetic_expression_type(
        &self,
        left: InferredType,
        right: InferredType,
    ) -> Option<InferredType> {
        let left = self.operand_type_class(left);
        let right = self.operand_type_class(right);
        match (left, right) {
            (
                OperandTypeClass::Complex(ComplexBase::Integer(left)),
                OperandTypeClass::Complex(ComplexBase::Integer(right)),
            ) => {
                let rank = rank_for_integer_type(usual_arithmetic_type(
                    integer_type_for_rank(left),
                    integer_type_for_rank(right),
                ))?;
                Some(InferredType::Complex(ComplexBase::Integer(rank)))
            }
            (
                OperandTypeClass::Complex(ComplexBase::Integer(complex)),
                OperandTypeClass::Integer(integer),
            )
            | (
                OperandTypeClass::Integer(integer),
                OperandTypeClass::Complex(ComplexBase::Integer(complex)),
            ) => {
                let rank = rank_for_integer_type(usual_arithmetic_type(
                    integer_type_for_rank(complex),
                    CInteger::new(0, integer).promoted().ty(),
                ))?;
                Some(InferredType::Complex(ComplexBase::Integer(rank)))
            }
            (OperandTypeClass::Integer(left), OperandTypeClass::Integer(right)) => {
                Some(InferredType::Integer(usual_arithmetic_type(
                    CInteger::new(0, left).promoted().ty(),
                    CInteger::new(0, right).promoted().ty(),
                )))
            }
            (
                OperandTypeClass::WideInteger {
                    ty: left_ty,
                    signedness: left_signedness,
                },
                OperandTypeClass::WideInteger {
                    ty: right_ty,
                    signedness: right_signedness,
                },
            ) => Some(InferredType::Canonical(
                if left_signedness == GnuInt128Signedness::Unsigned {
                    left_ty
                } else if right_signedness == GnuInt128Signedness::Unsigned {
                    right_ty
                } else {
                    left_ty
                },
            )),
            (OperandTypeClass::WideInteger { ty, .. }, OperandTypeClass::Integer(_))
            | (OperandTypeClass::Integer(_), OperandTypeClass::WideInteger { ty, .. }) => {
                Some(InferredType::Canonical(ty))
            }
            (left, right) if is_arithmetic_class(left) && is_arithmetic_class(right) => {
                let has_complex = matches!(left, OperandTypeClass::Complex(_))
                    || matches!(right, OperandTypeClass::Complex(_));
                let floating = widest_floating_kind(left, right)?;
                if has_complex {
                    Some(InferredType::Complex(complex_base_for_float(floating)))
                } else {
                    Some(InferredType::Real(floating))
                }
            }
            _ => None,
        }
    }

    pub(super) fn operand_type_class(&self, ty: InferredType) -> OperandTypeClass {
        match ty {
            InferredType::Integer(integer) => OperandTypeClass::Integer(integer),
            InferredType::Real(real) => OperandTypeClass::Real(real),
            InferredType::Complex(complex) => complex_class(complex),
            InferredType::Pointer(pointee) => OperandTypeClass::Pointer(pointee),
            InferredType::StringArray(_) => OperandTypeClass::Other,
            InferredType::StringPointer(prefix) => OperandTypeClass::StringPointer(prefix),
            InferredType::Canonical(ty) => {
                let Some(canonical) = self.types.get(ty) else {
                    return OperandTypeClass::Other;
                };
                match canonical.kind {
                    TypeKind::TargetBuiltin(kind) => {
                        evocc_chir::integer::integer_type_for_target(kind)
                            .map(OperandTypeClass::Integer)
                            .or_else(|| half_operand_class(kind))
                            .unwrap_or(OperandTypeClass::Other)
                    }
                    TypeKind::Int(rank) => OperandTypeClass::Integer(integer_type_for_rank(rank)),
                    TypeKind::Enum(_) => self
                        .canonical_integer_type(ty)
                        .map(OperandTypeClass::Integer)
                        .unwrap_or(OperandTypeClass::Other),
                    TypeKind::Float(real) => OperandTypeClass::Real(real),
                    TypeKind::Complex(complex) => complex_class(complex),
                    TypeKind::Pointer(pointee) => self
                        .inferred_pointer_type(pointee)
                        .map(OperandTypeClass::Pointer)
                        .unwrap_or(OperandTypeClass::Other),
                    TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::Vector { .. } => {
                        OperandTypeClass::Aggregate(ty)
                    }
                    TypeKind::Void => OperandTypeClass::Void,
                    TypeKind::GnuInt128(signedness) => {
                        OperandTypeClass::WideInteger { ty, signedness }
                    }
                    TypeKind::Array { .. }
                    | TypeKind::Function { .. }
                    | TypeKind::BuiltinVaList => OperandTypeClass::Other,
                }
            }
        }
    }

    pub(super) fn pointer_arithmetic_is_valid(&self, pointee: TypeId) -> bool {
        if matches!(
            self.types.get(pointee).map(|canonical| &canonical.kind),
            Some(TypeKind::Void | TypeKind::Function { .. })
        ) {
            return true;
        }
        self.target_layout
            .array_size_factors(&self.types, pointee)
            .is_ok()
    }

    pub(super) fn inferred_pointer_type(&self, pointee: TypeId) -> Option<InferredPointerType> {
        Some(InferredPointerType {
            pointee,
            qualifiers: self.types.get(pointee)?.qualifiers,
        })
    }

    pub(super) fn pointer_accepts_string(
        &self,
        pointer: InferredPointerType,
        prefix: StringPrefix,
    ) -> bool {
        self.types.get(pointer.pointee).is_some_and(|pointee| {
            matches!(pointee.kind, TypeKind::Void)
                || matches!(
                    (prefix, &pointee.kind),
                    (
                        StringPrefix::None | StringPrefix::U8,
                        TypeKind::Int(
                            IntRank::Char
                                | IntRank::SignedPlainChar
                                | IntRank::SChar
                                | IntRank::UChar
                        )
                    )
                )
                || pointee.kind == TypeKind::Int(prefix.element_rank())
        })
    }

    pub(super) fn note_pointer_arithmetic_operands(
        &self,
        left: &CHirExpr<'_>,
        right: &CHirExpr<'_>,
        span: SourceSpan,
    ) {
        for expression in [left, right] {
            if let Some(InferredExpressionType {
                ty: InferredType::Canonical(ty),
                ..
            }) = self.converted_expression_type(expression)
            {
                if let OperandTypeClass::Pointer(pointer) =
                    self.operand_type_class(InferredType::Canonical(ty))
                {
                    self.note_pointer_arithmetic_pointee(pointer.pointee, span);
                }
            }
        }
    }

    pub(super) fn note_pointer_arithmetic_pointee(&self, pointee: TypeId, span: SourceSpan) {
        if matches!(
            self.types.get(pointee).map(|ty| &ty.kind),
            Some(TypeKind::Void | TypeKind::Function { .. })
        ) {
            self.pointer_lint_findings
                .borrow_mut()
                .push(evocc_chir::lints::Finding { lint: 6, span });
        }
    }

    pub(super) fn note_pointer_arithmetic_type(&self, ty: TypeId, span: SourceSpan) {
        if matches!(
            self.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Void | TypeKind::Function { .. })
        ) {
            self.pointer_lint_findings
                .borrow_mut()
                .push(evocc_chir::lints::Finding { lint: 6, span });
        }
    }

    /// Record the GCC pointer conversion warnings that depend on the source
    /// and target pointer types. The driver applies the selected lint level.
    pub(super) fn note_pointer_conversion(&self, target_ty: TypeId, value: &CHirExpr<'_>) {
        // GCC 14 reports only the incompatibility of such a pair (W0012).
        if self.incompatible_pointer_conversion(target_ty, value) {
            return;
        }
        let Some(source_inferred) = self.converted_expression_type(value) else {
            return;
        };
        let target_class = self.operand_type_class(InferredType::Canonical(target_ty));
        let source_class = self.operand_type_class(source_inferred.ty);
        let (OperandTypeClass::Pointer(target), OperandTypeClass::Pointer(source)) =
            (target_class, source_class)
        else {
            return;
        };
        let span = value.span();
        let source_array = self
            .types
            .get(source.pointee)
            .and_then(|ty| match &ty.kind {
                TypeKind::Array { element, .. } => Some(*element),
                _ => None,
            });
        let target_array = self
            .types
            .get(target.pointee)
            .and_then(|ty| match &ty.kind {
                TypeKind::Array { element, .. } => Some(*element),
                _ => None,
            });
        if let (Some(source_element), Some(target_element)) = (source_array, target_array) {
            let source_qualifiers = self
                .types
                .get(source_element)
                .map_or(Qualifiers::default(), |ty| ty.qualifiers);
            let target_qualifiers = self
                .types
                .get(target_element)
                .map_or(Qualifiers::default(), |ty| ty.qualifiers);
            let source_array_qualifiers = self
                .types
                .get(source.pointee)
                .map_or(Qualifiers::default(), |ty| ty.qualifiers);
            let target_array_qualifiers = self
                .types
                .get(target.pointee)
                .map_or(Qualifiers::default(), |ty| ty.qualifiers);
            if drops_qualifiers(source_qualifiers, target_qualifiers)
                || drops_qualifiers(source_array_qualifiers, target_array_qualifiers)
            {
                self.pointer_lint_findings
                    .borrow_mut()
                    .push(evocc_chir::lints::Finding { lint: 8, span });
            }
        } else if drops_qualifiers(source.qualifiers, target.qualifiers) {
            self.pointer_lint_findings
                .borrow_mut()
                .push(evocc_chir::lints::Finding { lint: 7, span });
        }
        if let (Ok(source_integer), Ok(target_integer)) = (
            self.canonical_integer_type(source.pointee),
            self.canonical_integer_type(target.pointee),
        ) {
            let source_kind = self.types.get(source.pointee).map(|ty| &ty.kind);
            let target_kind = self.types.get(target.pointee).map(|ty| &ty.kind);
            if source_integer.bits() == target_integer.bits()
                && source_kind != target_kind
                && matches!(source_kind, Some(TypeKind::Int(_)))
                && matches!(target_kind, Some(TypeKind::Int(_)))
            {
                self.pointer_lint_findings
                    .borrow_mut()
                    .push(evocc_chir::lints::Finding { lint: 5, span });
            }
        }
    }

    pub(super) fn pointer_assignment_compatible(
        &self,
        target: InferredPointerType,
        source: InferredPointerType,
    ) -> bool {
        // GNU compatible-pointer conversions may discard immediate const or
        // volatile qualification. This does not change a const place, an
        // atomic pointee, or qualifiers nested inside another pointer type.
        // As in GCC 14, the targets must agree on `_Atomic` unless one of
        // them is `void`: `_Atomic int *` from `int *` is W0012.
        let void_target = |pointer: InferredPointerType| {
            matches!(
                self.types.get(pointer.pointee).map(|ty| &ty.kind),
                Some(TypeKind::Void)
            )
        };
        let atomic_agrees = match (target.qualifiers.is_atomic, source.qualifiers.is_atomic) {
            (false, false) => true,
            (true, false) if !void_target(target) && !void_target(source) => false,
            _ => qualifiers_include(
                target.qualifiers.compatibility(),
                source.qualifiers.compatibility(),
            ),
        };
        self.function_and_void_pointer_pair(target, source)
            || (self.pointer_conversion_types_compatible(target, source) && atomic_agrees)
    }

    /// C11 6.3.2.3p3: an integer constant expression of value 0, or one cast
    /// to `void *`. EvoCC folds a cast of zero to any pointer type as a null
    /// pointer, but GCC 14 converts `(T *)0` with another `T` as a pointer of
    /// type `T *` (W0012).
    pub(super) fn c11_null_pointer_constant(&self, expression: &CHirExpr<'_>) -> bool {
        if !self.is_null_pointer_constant(expression) {
            return false;
        }
        let CHirExpr::Cast(target, _, _) = expression else {
            return true;
        };
        self.types.get(*target).is_some_and(|ty| match &ty.kind {
            TypeKind::Pointer(pointee) => self.types.get(*pointee).is_some_and(|pointee| {
                pointee.kind == TypeKind::Void && pointee.qualifiers == Qualifiers::default()
            }),
            _ => false,
        })
    }

    pub(super) fn points_to_non_atomic_void(&self, pointer: InferredPointerType) -> bool {
        !pointer.qualifiers.is_atomic
            && matches!(
                self.types.get(pointer.pointee).map(|ty| &ty.kind),
                Some(TypeKind::Void)
            )
    }

    /// GCC 14's `-Wincompatible-pointer-types` for a conversion to `target`
    /// in assignment, initialization, a prototyped argument or `return`
    /// (EvoIR 10.1.18): the source is a pointer or a string literal, not a
    /// C11 null pointer constant, and neither pointer target is non-atomic
    /// `void`. Other refusals, such as `void *` from a pointer to an atomic
    /// type, keep their codes.
    pub(super) fn incompatible_pointer_conversion(
        &self,
        target: TypeId,
        value: &CHirExpr<'_>,
    ) -> bool {
        let OperandTypeClass::Pointer(left) =
            self.operand_type_class(InferredType::Canonical(target))
        else {
            return false;
        };
        if self.points_to_non_atomic_void(left) {
            return false;
        }
        let Some(source) = self.converted_expression_type(value) else {
            return false;
        };
        match self.operand_type_class(source.ty) {
            OperandTypeClass::Pointer(right) => {
                !self.c11_null_pointer_constant(value)
                    && !self.points_to_non_atomic_void(right)
                    && !self.pointer_assignment_compatible(left, right)
            }
            OperandTypeClass::StringPointer(prefix) => !self.pointer_accepts_string(left, prefix),
            _ => false,
        }
    }

    /// GCC 14's "pointer type mismatch in conditional expression"
    /// (`-Wincompatible-pointer-types`, EvoIR 10.1.18): two pointer operands,
    /// neither a C11 null pointer constant nor a pointer to non-atomic `void`,
    /// whose targets are not compatible apart from `const`, `volatile` and
    /// `restrict`. Character types of different signedness are not
    /// compatible here, unlike in a conversion.
    pub(super) fn incompatible_conditional_pointers(
        &self,
        first: &CHirExpr<'_>,
        second: &CHirExpr<'_>,
    ) -> bool {
        #[derive(Clone, Copy)]
        enum Operand {
            Pointer(InferredPointerType),
            String(StringPrefix),
        }
        let operand = |expression: &CHirExpr<'_>| {
            if self.c11_null_pointer_constant(expression) {
                return None;
            }
            match self.operand_type_class(self.converted_expression_type(expression)?.ty) {
                OperandTypeClass::Pointer(pointer) if !self.points_to_non_atomic_void(pointer) => {
                    Some(Operand::Pointer(pointer))
                }
                OperandTypeClass::StringPointer(prefix) => Some(Operand::String(prefix)),
                _ => None,
            }
        };
        let rank = |prefix: StringPrefix| match prefix {
            StringPrefix::None | StringPrefix::U8 => None,
            prefix => Some(prefix.element_rank()),
        };
        let string_target = |pointer: InferredPointerType, prefix: StringPrefix| {
            !pointer.qualifiers.is_atomic
                && self
                    .types
                    .get(pointer.pointee)
                    .is_some_and(|pointee| match rank(prefix) {
                        None => matches!(
                            pointee.kind,
                            TypeKind::Int(IntRank::Char | IntRank::SignedPlainChar)
                        ),
                        Some(rank) => pointee.kind == TypeKind::Int(rank),
                    })
        };
        let (Some(first), Some(second)) = (operand(first), operand(second)) else {
            return false;
        };
        !match (first, second) {
            (Operand::Pointer(left), Operand::Pointer(right)) => {
                left.qualifiers.is_atomic == right.qualifiers.is_atomic
                    && (self
                        .types
                        .compatible_ignoring_top_qualifiers(left.pointee, right.pointee)
                        || self
                            .types
                            .compatible_array_pointer_targets(left.pointee, right.pointee))
            }
            (Operand::Pointer(pointer), Operand::String(prefix))
            | (Operand::String(prefix), Operand::Pointer(pointer)) => {
                string_target(pointer, prefix)
            }
            (Operand::String(left), Operand::String(right)) => rank(left) == rank(right),
        }
    }

    /// Record `W0012` for the driver's lint policy, which then decides its
    /// level. Without that policy (the parser library without
    /// `enable_lints`) the caller keeps the refusal of earlier revisions, as
    /// an undeclared function does (EvoIR 10.1.18).
    pub(super) fn record_incompatible_pointers(&self, span: SourceSpan) -> bool {
        if self.lint_declarations {
            // Lint index 11 is `incompatible-pointer-types` (W0012).
            self.pointer_lint_findings
                .borrow_mut()
                .push(evocc_chir::lints::Finding { lint: 11, span });
        }
        self.lint_declarations
    }

    /// The conversion of an explicit cast to `target`, which a warned or
    /// allowed W0012 conversion is. A cast yields a value, so the target's
    /// own qualifiers do not apply to it.
    pub(super) fn incompatible_pointer_cast(
        &mut self,
        target: TypeId,
        value: &'arena CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        let ty = match self.types.get(target) {
            Some(canonical) if canonical.qualifiers != Qualifiers::default() => {
                let kind = canonical.kind.clone();
                self.intern(kind, Qualifiers::default())
            }
            _ => target,
        };
        self.alloc_node(CHirExpr::Cast(ty, value, value.span()))
    }

    /// A conditional with incompatible pointer operands has type `void *`
    /// without qualifiers, as in GCC 14 and Apple clang. Each operand
    /// converts as an explicit cast to `void *`, and `W0012` is at the `:`.
    pub(super) fn convert_incompatible_conditional_pointers(
        &mut self,
        first: &'arena CHirExpr<'arena>,
        second: &'arena CHirExpr<'arena>,
        colon: SourceSpan,
    ) -> (&'arena CHirExpr<'arena>, &'arena CHirExpr<'arena>) {
        if !self.incompatible_conditional_pointers(first, second)
            || !self.record_incompatible_pointers(colon)
        {
            return (first, second);
        }
        let void = self.intern(TypeKind::Void, Qualifiers::default());
        let pointer = self.intern(TypeKind::Pointer(void), Qualifiers::default());
        (
            self.alloc_node(CHirExpr::Cast(pointer, first, first.span())),
            self.alloc_node(CHirExpr::Cast(pointer, second, second.span())),
        )
    }

    pub(super) fn function_and_void_pointer_pair(
        &self,
        left: InferredPointerType,
        right: InferredPointerType,
    ) -> bool {
        let (Some(left), Some(right)) =
            (self.types.get(left.pointee), self.types.get(right.pointee))
        else {
            return false;
        };
        matches!(
            (&left.kind, &right.kind),
            (TypeKind::Function { .. }, TypeKind::Void)
                | (TypeKind::Void, TypeKind::Function { .. })
        )
    }

    pub(super) fn pointer_conversion_types_compatible(
        &self,
        left: InferredPointerType,
        right: InferredPointerType,
    ) -> bool {
        if self.pointer_types_compatible(left, right)
            || self
                .types
                .compatible_array_pointer_targets(left.pointee, right.pointee)
        {
            return true;
        }
        let (Some(left), Some(right)) =
            (self.types.get(left.pointee), self.types.get(right.pointee))
        else {
            return false;
        };
        matches!(
            (&left.kind, &right.kind),
            (
                TypeKind::Int(IntRank::Short),
                TypeKind::Int(IntRank::UShort)
            ) | (
                TypeKind::Int(IntRank::UShort),
                TypeKind::Int(IntRank::Short)
            ) | (TypeKind::Int(IntRank::Int), TypeKind::Int(IntRank::UInt))
                | (TypeKind::Int(IntRank::UInt), TypeKind::Int(IntRank::Int))
                | (TypeKind::Int(IntRank::Long), TypeKind::Int(IntRank::ULong))
                | (TypeKind::Int(IntRank::ULong), TypeKind::Int(IntRank::Long))
                | (
                    TypeKind::Int(IntRank::LongLong),
                    TypeKind::Int(IntRank::ULongLong)
                )
                | (
                    TypeKind::Int(IntRank::ULongLong),
                    TypeKind::Int(IntRank::LongLong)
                )
        )
    }

    pub(super) fn pointer_types_compatible(
        &self,
        left: InferredPointerType,
        right: InferredPointerType,
    ) -> bool {
        let left_pointee = left.pointee;
        let right_pointee = right.pointee;
        let (Some(left), Some(right)) =
            (self.types.get(left_pointee), self.types.get(right_pointee))
        else {
            return false;
        };
        if self
            .types
            .compatible_ignoring_top_qualifiers(left_pointee, right_pointee)
        {
            return true;
        }
        if matches!(
            (&left.kind, &right.kind),
            (
                TypeKind::Int(
                    IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
                ),
                TypeKind::Int(
                    IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
                )
            )
        ) {
            return true;
        }
        matches!(
            (&left.kind, &right.kind),
            (TypeKind::Void, right) if is_object_or_function_type_kind(right)
        ) || matches!(
            (&left.kind, &right.kind),
            (left, TypeKind::Void) if is_object_or_function_type_kind(left)
        )
    }

    pub(super) fn composite_pointer_type(
        &self,
        left: InferredPointerType,
        right: InferredPointerType,
    ) -> Option<InferredPointerType> {
        if !self.pointer_types_compatible(left, right) {
            return None;
        }
        let left_kind = &self.types.get(left.pointee)?.kind;
        let right_kind = &self.types.get(right.pointee)?.kind;
        let pointee = match (left_kind, right_kind) {
            (TypeKind::Void, _) => left.pointee,
            (_, TypeKind::Void) => right.pointee,
            _ => left.pointee,
        };
        Some(InferredPointerType {
            pointee,
            qualifiers: merge_qualifiers(left.qualifiers, right.qualifiers),
        })
    }

    pub(super) fn same_unqualified_type(&self, left: TypeId, right: TypeId) -> bool {
        match (self.types.get(left), self.types.get(right)) {
            (Some(left), Some(right)) => left.kind == right.kind,
            _ => false,
        }
    }

    pub(super) fn is_null_pointer_constant(&self, expression: &CHirExpr<'_>) -> bool {
        let integer_zero = || {
            self.integer_expression_type(expression).is_ok()
                && self
                    .eval_integer_constant_expression(expression)
                    .is_ok_and(|value| value.raw_bits() == 0)
        };
        if integer_zero() {
            return true;
        }
        let CHirExpr::Cast(target, operand, _) = expression else {
            return false;
        };
        let Some(TypeKind::Pointer(_)) = self.types.get(*target).map(|ty| &ty.kind) else {
            return false;
        };
        (self.integer_expression_type(operand).is_ok()
            && self
                .eval_integer_constant_expression(operand)
                .is_ok_and(|value| value.raw_bits() == 0))
            || self.is_null_pointer_constant(operand)
    }
}
