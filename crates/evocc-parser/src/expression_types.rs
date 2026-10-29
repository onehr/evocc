//! Parser expression types.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn materialize_inferred_type(
        &mut self,
        inferred: InferredType,
        expression: Option<&CHirExpr<'_>>,
    ) -> Option<TypeId> {
        Some(match inferred {
            InferredType::Canonical(ty) => ty,
            InferredType::Integer(integer) => self.intern_c_integer_type(integer),
            InferredType::Real(real) => self.real_type(real),
            InferredType::Complex(complex) => {
                self.intern(TypeKind::Complex(complex), Qualifiers::default())
            }
            InferredType::Pointer(pointer) => {
                let pointee = self.qualify_typedef_type(pointer.pointee, pointer.qualifiers);
                self.intern(TypeKind::Pointer(pointee), Qualifiers::default())
            }
            InferredType::StringArray(prefix) => {
                let CHirExpr::StringLit(literal, _) = expression? else {
                    return None;
                };
                let element = self.string_element_type(prefix);
                let bound = literal.explicit_unit_count()?.checked_add(1)?;
                self.intern(
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Fixed(bound),
                    },
                    Qualifiers::default(),
                )
            }
            InferredType::StringPointer(prefix) => {
                let element = self.string_element_type(prefix);
                self.intern(TypeKind::Pointer(element), Qualifiers::default())
            }
        })
    }

    pub(super) fn generic_controlling_type(&mut self, expression: &CHirExpr<'_>) -> Option<TypeId> {
        let converted = self.converted_expression_type(expression)?;
        let ty = self.materialize_inferred_type(converted.ty, Some(expression))?;
        let canonical = self.types.get(ty)?.clone();
        Some(self.intern(canonical.kind, Qualifiers::default()))
    }

    pub(super) fn string_element_type(&mut self, prefix: StringPrefix) -> TypeId {
        let rank = prefix.element_rank();
        self.intern(TypeKind::Int(rank), Qualifiers::default())
    }

    pub(super) fn type_is_variably_modified(&self, ty: TypeId) -> bool {
        self.type_is_variably_modified_from(ty, &mut HashSet::new())
    }

    pub(super) fn type_is_variably_modified_from(
        &self,
        ty: TypeId,
        seen: &mut HashSet<TypeId>,
    ) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match self.types.get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Array {
                bound: ArrayBound::Variable | ArrayBound::Captured(_),
                ..
            }) => true,
            Some(TypeKind::Array { element, .. } | TypeKind::Pointer(element)) => {
                self.type_is_variably_modified_from(*element, seen)
            }
            // Bounds in a function prototype belong to its parameters, not
            // to the object holding a pointer to that function. They neither
            // run here nor make callback members variably modified.
            Some(TypeKind::Function { return_type, .. }) => {
                self.type_is_variably_modified_from(*return_type, seen)
            }
            _ => false,
        }
    }

    pub(super) fn converted_expression_type(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<InferredExpressionType> {
        let mut inferred = self.expression_type(expression)?;
        if let InferredType::Canonical(ty) = inferred.ty {
            match self.types.get(ty).map(|ty| &ty.kind)? {
                TypeKind::Array { element, .. } => {
                    inferred.ty = InferredType::Pointer(self.inferred_pointer_type(*element)?);
                }
                TypeKind::Function { .. } => {
                    inferred.ty = InferredType::Pointer(self.inferred_pointer_type(ty)?);
                }
                _ => {}
            }
        } else if let InferredType::StringArray(prefix) = inferred.ty {
            // Once it decays, a literal is an ordinary pointer. Keeping a
            // separate string-pointer class here made unary dereference miss
            // the rule already used by array subscripts and named pointers.
            let element = self.types.find(&CanonicalType {
                kind: TypeKind::Int(prefix.element_rank()),
                qualifiers: Qualifiers::default(),
            })?;
            inferred.ty = InferredType::Pointer(self.inferred_pointer_type(element)?);
        }
        if inferred.category != ExprCategory::Value {
            inferred.category = ExprCategory::Value;
        }
        Some(inferred)
    }

    /// Preserve a bit-field's written type for typeof and generic selection;
    /// only arithmetic contexts promote its range, not its allocation width.
    pub(super) fn arithmetic_operand_type(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<InferredExpressionType> {
        let mut inferred = self.converted_expression_type(expression)?;
        if let (OperandTypeClass::Integer(integer), Some(width)) = (
            self.operand_type_class(inferred.ty),
            self.expression_bit_field_width(expression),
        ) {
            inferred.ty = InferredType::Integer(integer.bit_field_promoted(width));
        }
        Some(inferred)
    }

    /// Only a consumer that observes disputed field precision is refused.
    pub(super) fn require_supported_integer_promotion(
        &mut self,
        expression: &CHirExpr<'_>,
    ) -> Option<()> {
        self.require_supported_promotion_at(expression, expression)
    }

    pub(super) fn require_supported_promotion_at(
        &mut self,
        expression: &CHirExpr<'_>,
        consumer: &CHirExpr<'_>,
    ) -> Option<()> {
        if self.wide_field_integer(expression).is_some() {
            let previous = self.profile_refusals.len();
            if !self.defer_profile_refusal(
                "E2939",
                "unsupported-wide-bit-field-promotion",
                expression.span(),
            ) {
                return None;
            }
            if let Some((_, _, cause)) = self.profile_refusals.get_mut(previous) {
                *cause = BinaryExpressionIdentity::of(consumer);
            }
        }
        Some(())
    }

    pub(super) fn promoted_integer_operand(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<CIntegerType> {
        match self.operand_type_class(self.converted_expression_type(expression)?.ty) {
            OperandTypeClass::Integer(integer) => Some(CInteger::new(0, integer).promoted().ty()),
            OperandTypeClass::WideInteger { ty, .. } => self.canonical_integer_type(ty).ok(),
            _ => None,
        }
    }

    pub(super) fn wide_field_integer(&self, expression: &CHirExpr<'_>) -> Option<CIntegerType> {
        self.wide_field_precision(expression)
            .map(|(_, integer)| integer)
    }

    pub(super) fn require_supported_binary_promotion(
        &mut self,
        operator: BinaryOp,
        left: &CHirExpr<'_>,
        right: &CHirExpr<'_>,
        consumer: &CHirExpr<'_>,
    ) -> Option<()> {
        if operator == BinaryOp::Shl {
            // The approved direct-read profile obtains the declared base
            // integer before shifting. Neither the storage width nor the
            // right operand's width determines its arithmetic precision.
            // This does not adjudicate a comma/conditional/update rvalue.
            if self.expression_record_field(left).is_some() {
                return Some(());
            }
            return self.require_supported_promotion_at(left, consumer);
        }
        if matches!(
            operator,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) {
            for (field, other) in [(left, right), (right, left)] {
                if self.wide_field_integer(field).is_some_and(|integer| {
                    !integer.is_signed()
                        && self.promoted_integer_operand(other).is_some_and(|other| {
                            other.is_signed() && other.bits() <= integer.bits()
                        })
                }) && !self
                    .eval_integer_constant_expression(other)
                    .is_ok_and(|value| {
                        value
                            .try_mathematical_value()
                            .is_some_and(|value| value >= 0)
                    })
                {
                    self.require_supported_promotion_at(field, consumer)?;
                }
            }
        }
        if matches!(
            operator,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
        ) {
            for (field, other) in [(left, right), (right, left)] {
                if let Some(integer) = self.wide_field_integer(field) {
                    if self.wide_field_integer(other).is_some()
                        || self.promoted_integer_operand(other).is_none_or(|other| {
                            other.bits() < integer.bits()
                                || (!integer.is_signed()
                                    && other.is_signed()
                                    && other.bits() == integer.bits())
                        })
                    {
                        self.require_supported_promotion_at(field, consumer)?;
                    }
                }
            }
        }
        Some(())
    }

    pub(super) fn require_supported_assignment_promotion(
        &mut self,
        operator: AssignmentOp,
        target: &CHirExpr<'_>,
        value: &CHirExpr<'_>,
    ) -> Option<()> {
        let binary = match operator {
            AssignmentOp::Simple => return Some(()),
            AssignmentOp::Add => BinaryOp::Add,
            AssignmentOp::Subtract => BinaryOp::Sub,
            AssignmentOp::Multiply => BinaryOp::Mul,
            AssignmentOp::Divide => BinaryOp::Div,
            AssignmentOp::Remainder => BinaryOp::Mod,
            AssignmentOp::ShiftLeft => BinaryOp::Shl,
            AssignmentOp::ShiftRight => BinaryOp::Shr,
            AssignmentOp::BitAnd => BinaryOp::BitAnd,
            AssignmentOp::BitOr => BinaryOp::BitOr,
            AssignmentOp::BitXor => BinaryOp::BitXor,
        };
        // Unsigned total arithmetic followed by truncation into the original
        // field has the same stored value at either intermediate precision.
        // Division and shifts do not have this property and keep their checks.
        if self
            .wide_field_integer(target)
            .is_some_and(|integer| !integer.is_signed())
            && matches!(
                binary,
                BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
            )
        {
            return Some(());
        }
        self.require_supported_binary_promotion(binary, target, value, target)
    }

    pub(super) fn require_supported_update_promotion(
        &mut self,
        target: &CHirExpr<'_>,
    ) -> Option<()> {
        if self
            .wide_field_integer(target)
            .is_some_and(|integer| !integer.is_signed())
        {
            return Some(());
        }
        self.require_supported_integer_promotion(target)
    }

    pub(super) fn require_supported_conditional_promotion(
        &mut self,
        left: &CHirExpr<'_>,
        right: &CHirExpr<'_>,
    ) -> Option<()> {
        for (field, other) in [(left, right), (right, left)] {
            let Some(integer) = self.wide_field_integer(field) else {
                continue;
            };
            let Some(other_integer) = self.promoted_integer_operand(other) else {
                continue;
            };
            let other_is_wide_field = self.wide_field_integer(other).is_some();
            if !integer.is_signed()
                && other_integer.is_signed()
                && other_integer.bits() == integer.bits()
            {
                self.require_supported_integer_promotion(field)?;
                continue;
            }
            if !other_is_wide_field && other_integer.bits() >= integer.bits() {
                continue;
            }
            if !integer.is_signed()
                && other_integer.is_signed()
                && !self
                    .eval_integer_constant_expression(other)
                    .is_ok_and(|value| {
                        value
                            .try_mathematical_value()
                            .is_some_and(|value| value >= 0)
                    })
            {
                self.require_supported_integer_promotion(field)?;
            }
        }
        Some(())
    }

    pub(super) fn discharge_unsigned_sign_comparison(
        &mut self,
        operator: BinaryOp,
        left: &CHirExpr<'_>,
        right: &CHirExpr<'_>,
    ) {
        let (value, zero) = match operator {
            BinaryOp::Lt | BinaryOp::Ge => (left, right),
            BinaryOp::Gt | BinaryOp::Le => (right, left),
            _ => return,
        };
        if !self
            .eval_integer_constant_expression(zero)
            .is_ok_and(|value| !value.truth())
            || !self
                .promoted_integer_operand(value)
                .is_some_and(|integer| !integer.is_signed())
            || !matches!(
                value,
                CHirExpr::Binary(
                    BinaryOp::Add
                        | BinaryOp::Sub
                        | BinaryOp::Mul
                        | BinaryOp::BitAnd
                        | BinaryOp::BitOr
                        | BinaryOp::BitXor,
                    ..
                )
            )
        {
            return;
        }
        // Both possible unsigned precisions give the same sign comparison.
        // Do not discard evaluation, or clear nested/unrelated diagnostics.
        let identity = BinaryExpressionIdentity::of(value);
        let CHirExpr::Binary(_, a, b, _) = value else {
            return;
        };
        for (field, other) in [(a, b), (b, a)] {
            if self.wide_field_integer(field).is_some_and(|integer| {
                !integer.is_signed()
                    && self
                        .promoted_integer_operand(other)
                        .is_some_and(|other| other.is_signed() && other.bits() == integer.bits())
            }) {
                // GCC can use a signed long here while Clang retains the
                // unsigned declared type, so the sign is not invariant.
                return;
            }
        }
        self.profile_refusals
            .retain(|(refusal, _, cause)| refusal.code != "E2939" || *cause != identity);
    }

    pub(super) fn discharge_narrowed_unsigned_mask(
        &mut self,
        target: TypeId,
        operand: &CHirExpr<'_>,
    ) {
        let Ok(target) = self.canonical_integer_type(target) else {
            return;
        };
        if target.is_signed() || target == CIntegerType::BOOL {
            return;
        }
        let CHirExpr::Binary(BinaryOp::BitAnd, left, right, _) = operand else {
            return;
        };
        let consensus = [(left, right), (right, left)]
            .into_iter()
            .any(|(field, mask)| {
                if self.expression_record_field(field).is_none() {
                    return false;
                }
                let Some((width, integer)) = self.wide_field_precision(field) else {
                    return false;
                };
                !integer.is_signed()
                    && u16::from(target.bits()) <= width
                    && self
                        .eval_integer_constant_expression(mask)
                        .is_ok_and(|mask| {
                            mask.try_mathematical_value()
                                .is_some_and(|value| value >= 0)
                                && mask.raw_bits() <= (u128::MAX >> (128 - width))
                        })
            });
        if consensus {
            // Both precisions preserve these low bits. Clear only this AND's
            // deferred refusal, never a neighboring or nested operation's.
            let identity = BinaryExpressionIdentity::of(operand);
            self.profile_refusals
                .retain(|(refusal, _, cause)| refusal.code != "E2939" || *cause != identity);
        }
    }

    pub(super) fn expression_bit_field_width(&self, expression: &CHirExpr<'_>) -> Option<u16> {
        if let Some(field) = self.expression_record_field(expression) {
            return Some(field.bit_field?.width_bits);
        }
        self.wide_field_precision(expression)
            .map(|(width, _)| width)
    }

    // Carry the width and integer type together. Asking expression_type again
    // for the same conditional arm recursively repeats this precision walk.
    pub(super) fn wide_field_precision(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<(u16, CIntegerType)> {
        match expression {
            CHirExpr::Unary(UnaryOp::Plus, operand, _)
            | CHirExpr::Binary(BinaryOp::Shr, operand, _, _)
            | CHirExpr::Comma(_, operand, _)
            | CHirExpr::EvaluateOnce(operand, _) => {
                return self.wide_field_precision(operand);
            }
            CHirExpr::MemoryAssign { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
                let PlaceProjection::Field(field) = place.projections().last()? else {
                    return None;
                };
                let TagState::Complete(
                    TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                ) = &self.types.tag(field.tag())?.state
                else {
                    return None;
                };
                let field = fields.get(field.field_index())?;
                let width = field.bit_field?.width_bits;
                let integer = self.canonical_integer_type(field.ty).ok()?;
                return (width > 32 && width < u16::from(integer.bits()))
                    .then_some((width, integer));
            }
            CHirExpr::Ternary(_, left, right, _) | CHirExpr::Elvis(left, right, _) => {
                let left_precision = self.wide_field_precision(left);
                let right_precision = self.wide_field_precision(right);
                return match (left_precision, right_precision) {
                    (Some((left_width, left)), Some((right_width, right))) => Some((
                        left_width.max(right_width),
                        usual_arithmetic_type(left, right),
                    )),
                    (Some((width, integer)), None) | (None, Some((width, integer))) => {
                        let other = if left_precision.is_some() {
                            right
                        } else {
                            left
                        };
                        let other = self.promoted_integer_operand(other)?;
                        (other.bits() < integer.bits())
                            .then_some((width, usual_arithmetic_type(integer, other)))
                    }
                    _ => None,
                };
            }
            _ => {}
        }
        let field = self.expression_record_field(expression)?;
        let width = field.bit_field?.width_bits;
        let integer = self.canonical_integer_type(field.ty).ok()?;
        (width > 32 && width < u16::from(integer.bits())).then_some((width, integer))
    }

    pub(super) fn expression_record_field(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<&RecordField> {
        let field = match expression {
            CHirExpr::MemoryRead { place, .. } => match place.projections().last()? {
                PlaceProjection::Field(field) => *field,
                PlaceProjection::Index(_) => return None,
            },
            CHirExpr::Member(base, name, _) => {
                let InferredType::Canonical(ty) = self.expression_type(base)?.ty else {
                    return None;
                };
                *self.record_member_path(ty, name)?.last()?
            }
            CHirExpr::PtrMember(base, name, _) => {
                let OperandTypeClass::Pointer(pointer) =
                    self.operand_type_class(self.converted_expression_type(base)?.ty)
                else {
                    return None;
                };
                *self.record_member_path(pointer.pointee, name)?.last()?
            }
            CHirExpr::EvaluateOnce(operand, _) => return self.expression_record_field(operand),
            _ => return None,
        };
        let TagState::Complete(TagDefinition::Struct(fields) | TagDefinition::Union(fields)) =
            &self.types.tag(field.tag())?.state
        else {
            return None;
        };
        fields.get(field.field_index())
    }

    pub(super) fn promoted_arithmetic_type(&self, ty: InferredType) -> Option<InferredType> {
        Some(match self.operand_type_class(ty) {
            OperandTypeClass::Integer(integer) => {
                InferredType::Integer(CInteger::new(0, integer).promoted().ty())
            }
            OperandTypeClass::WideInteger { ty, .. } => InferredType::Canonical(ty),
            OperandTypeClass::Real(real) => InferredType::Real(real),
            OperandTypeClass::Complex(complex) => InferredType::Complex(complex),
            OperandTypeClass::Pointer(_)
            | OperandTypeClass::StringPointer(_)
            | OperandTypeClass::Aggregate(_)
            | OperandTypeClass::Void
            | OperandTypeClass::Other => return None,
        })
    }

    pub(super) fn binary_expression_type(
        &self,
        op: BinaryOp,
        left: InferredExpressionType,
        right: InferredExpressionType,
    ) -> Option<InferredType> {
        let left_class = self.operand_type_class(left.ty);
        let right_class = self.operand_type_class(right.ty);
        match op {
            BinaryOp::Add => match (left_class, right_class) {
                (OperandTypeClass::Pointer(pointee), integer)
                | (integer, OperandTypeClass::Pointer(pointee))
                    if is_integer_class(integer) =>
                {
                    Some(InferredType::Pointer(pointee))
                }
                (OperandTypeClass::StringPointer(prefix), integer)
                | (integer, OperandTypeClass::StringPointer(prefix))
                    if is_integer_class(integer) =>
                {
                    Some(InferredType::StringPointer(prefix))
                }
                _ => self.usual_arithmetic_expression_type(left.ty, right.ty),
            },
            BinaryOp::Sub => match (left_class, right_class) {
                (OperandTypeClass::Pointer(pointee), integer) if is_integer_class(integer) => {
                    Some(InferredType::Pointer(pointee))
                }
                (OperandTypeClass::StringPointer(prefix), integer) if is_integer_class(integer) => {
                    Some(InferredType::StringPointer(prefix))
                }
                (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right))
                    if self.pointer_types_compatible(left, right) =>
                {
                    Some(InferredType::Integer(CIntegerType::LONG))
                }
                (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Pointer(pointee))
                | (OperandTypeClass::Pointer(pointee), OperandTypeClass::StringPointer(prefix))
                    if self.pointer_accepts_string(pointee, prefix) =>
                {
                    Some(InferredType::Integer(CIntegerType::LONG))
                }
                (OperandTypeClass::StringPointer(left), OperandTypeClass::StringPointer(right))
                    if left == right =>
                {
                    Some(InferredType::Integer(CIntegerType::LONG))
                }
                _ => self.usual_arithmetic_expression_type(left.ty, right.ty),
            },
            BinaryOp::Mul | BinaryOp::Div => {
                self.usual_arithmetic_expression_type(left.ty, right.ty)
            }
            BinaryOp::Mod | BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                if !is_integer_class(left_class) || !is_integer_class(right_class) {
                    return None;
                }
                self.usual_arithmetic_expression_type(left.ty, right.ty)
            }
            BinaryOp::Shl | BinaryOp::Shr => {
                if !is_integer_class(right_class) {
                    return None;
                }
                match left_class {
                    OperandTypeClass::Integer(left) => Some(InferredType::Integer(
                        CInteger::new(0, left).promoted().ty(),
                    )),
                    OperandTypeClass::WideInteger { ty, .. } => Some(InferredType::Canonical(ty)),
                    _ => None,
                }
            }
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => {
                let compatible_pointers = match (left_class, right_class) {
                    (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right)) => {
                        if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                            self.pointer_conversion_types_compatible(left, right)
                        } else {
                            self.pointer_types_compatible(left, right)
                        }
                    }
                    (
                        OperandTypeClass::Pointer(pointee),
                        OperandTypeClass::StringPointer(prefix),
                    )
                    | (
                        OperandTypeClass::StringPointer(prefix),
                        OperandTypeClass::Pointer(pointee),
                    ) => self.pointer_accepts_string(pointee, prefix),
                    (
                        OperandTypeClass::StringPointer(left),
                        OperandTypeClass::StringPointer(right),
                    ) => left == right,
                    _ => false,
                };
                let comparable = (is_arithmetic_class(left_class)
                    && is_arithmetic_class(right_class))
                    || compatible_pointers
                    || (matches!(left_class, OperandTypeClass::Pointer(_))
                        && right.null_pointer_constant)
                    || (matches!(right_class, OperandTypeClass::Pointer(_))
                        && left.null_pointer_constant)
                    || (matches!(left_class, OperandTypeClass::StringPointer(_))
                        && right.null_pointer_constant)
                    || (matches!(right_class, OperandTypeClass::StringPointer(_))
                        && left.null_pointer_constant);
                comparable.then_some(InferredType::Integer(CIntegerType::INT))
            }
            BinaryOp::And | BinaryOp::Or => (is_scalar_class(left_class)
                && is_scalar_class(right_class))
            .then_some(InferredType::Integer(CIntegerType::INT)),
            BinaryOp::Comma
            | BinaryOp::Assign
            | BinaryOp::AddAssign
            | BinaryOp::SubAssign
            | BinaryOp::MulAssign
            | BinaryOp::DivAssign
            | BinaryOp::ModAssign
            | BinaryOp::ShlAssign
            | BinaryOp::ShrAssign
            | BinaryOp::BitAndAssign
            | BinaryOp::BitOrAssign
            | BinaryOp::BitXorAssign => None,
        }
    }

    pub(super) fn conditional_expression_type(
        &self,
        when_true: InferredExpressionType,
        when_false: InferredExpressionType,
    ) -> Option<InferredType> {
        // An `__fp16` operand promotes to `float`, also in a conditional.
        if when_true.ty == when_false.ty && !self.is_arm_fp16(when_true.ty) {
            return Some(when_true.ty);
        }
        if let Some(arithmetic) = self.usual_arithmetic_expression_type(when_true.ty, when_false.ty)
        {
            return Some(arithmetic);
        }
        match (
            self.operand_type_class(when_true.ty),
            self.operand_type_class(when_false.ty),
        ) {
            (OperandTypeClass::Pointer(_), OperandTypeClass::Pointer(right))
                if when_true.null_pointer_constant =>
            {
                Some(InferredType::Pointer(right))
            }
            (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(_))
                if when_false.null_pointer_constant =>
            {
                Some(InferredType::Pointer(left))
            }
            (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right))
                if self.pointer_types_compatible(left, right) =>
            {
                self.composite_pointer_type(left, right)
                    .map(InferredType::Pointer)
            }
            (OperandTypeClass::Pointer(pointee), OperandTypeClass::StringPointer(prefix))
            | (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Pointer(pointee))
                if self.pointer_accepts_string(pointee, prefix) =>
            {
                Some(InferredType::Pointer(pointee))
            }
            (OperandTypeClass::Pointer(pointee), integer)
                if is_integer_class(integer) && when_false.null_pointer_constant =>
            {
                Some(InferredType::Pointer(pointee))
            }
            (OperandTypeClass::StringPointer(prefix), integer)
                if is_integer_class(integer) && when_false.null_pointer_constant =>
            {
                Some(InferredType::StringPointer(prefix))
            }
            (integer, OperandTypeClass::StringPointer(prefix))
                if is_integer_class(integer) && when_true.null_pointer_constant =>
            {
                Some(InferredType::StringPointer(prefix))
            }
            (integer, OperandTypeClass::Pointer(pointee))
                if is_integer_class(integer) && when_true.null_pointer_constant =>
            {
                Some(InferredType::Pointer(pointee))
            }
            (OperandTypeClass::Aggregate(left), OperandTypeClass::Aggregate(right))
                if self.same_unqualified_type(left, right) =>
            {
                Some(InferredType::Canonical(left))
            }
            _ => None,
        }
    }
}
