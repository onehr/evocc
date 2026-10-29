//! Lowering predicates.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn common_branch_values(
        &mut self,
        left: ExprValue,
        right: ExprValue,
        left_state: &mut State,
        right_state: &mut State,
    ) -> Result<(ExprValue, ExprValue, CIntegerType), LowerError> {
        if evocc_evoir::floating_kind(&left.value.ty).is_some()
            || evocc_evoir::floating_kind(&right.value.ty).is_some()
        {
            let halves = [&left.value.ty, &right.value.ty]
                .into_iter()
                .filter_map(|ty| evocc_evoir::floating_kind(ty).filter(|kind| kind.is_half()))
                .collect::<Vec<_>>();
            let kind = if left.value.ty == EvoIRType::F128 || right.value.ty == EvoIRType::F128 {
                evocc_types::FloatKind::F128
            } else if left.value.ty == EvoIRType::F64 || right.value.ty == EvoIRType::F64 {
                evocc_types::FloatKind::F64
            } else if left.value.ty == EvoIRType::F32 || right.value.ty == EvoIRType::F32 {
                // A binary32 arm is either `float` or an evaluated half
                // operation; the other arm converts to binary32 too.
                evocc_types::FloatKind::F32
            } else if halves.len() == 2 && halves[0] == halves[1] {
                // Two half values of one format: the conditional keeps the
                // format, whose values binary32 holds exactly.
                halves[0]
            } else {
                // A half conditional evaluates in binary32, as in GCC 14: an
                // integer arm converts to binary32 directly (EvoIR 3.13.6).
                evocc_types::FloatKind::F32
            };
            return Ok((
                self.cast_floating(left, kind, left_state)?,
                self.cast_floating(right, kind, right_state)?,
                CIntegerType::ULONG,
            ));
        }
        if left.value.ty == right.value.ty
            && matches!(left.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
        {
            return Ok((left, right, CIntegerType::ULONG));
        }
        // The arms of a conditional of one vector type select a vector value
        // as it is; the front end gave both arms that type.
        if left.value.ty == right.value.ty && matches!(left.value.ty, EvoIRType::Vec(..)) {
            let integer = left.integer;
            return Ok((left, right, integer));
        }
        let common = arithmetic_common_type(left.integer, right.integer);
        Ok((
            self.cast(left, common, left_state)?,
            self.cast(right, common, right_state)?,
            common,
        ))
    }

    pub(super) fn known_expression_truth(
        &self,
        condition: &CHirExpr<'_>,
        arms: &[&CHirExpr<'_>],
    ) -> Option<bool> {
        let truth = self.result_after_evaluation(condition)?.truth();
        if !self.address_labels.is_empty() {
            let mut references = FunctionReferences::default();
            for arm in arms {
                collect_references_expr(arm, &mut references);
            }
            if references
                .defined_labels
                .iter()
                .any(|label| self.address_labels.contains_key(label))
            {
                return None;
            }
        }
        Some(truth)
    }

    pub(super) fn expression_has_external_entry(&self, expression: &CHirExpr<'_>) -> bool {
        let mut references = FunctionReferences::default();
        collect_references_expr(expression, &mut references);
        references.has_case_label
            || references.defined_labels.iter().any(|label| {
                self.address_labels.contains_key(label)
                    || self.goto_counts.get(label).copied().unwrap_or(0)
                        > references.goto_counts.get(label).copied().unwrap_or(0)
            })
    }

    // Used only where lowering still evaluates the entire condition. In
    // particular, loops which omit evaluation of a source constant must not
    // use this fact: a constant-return call can still evaluate effectful args.
    pub(super) fn result_after_evaluation(&self, expression: &CHirExpr<'_>) -> Option<CInteger> {
        let values = self.possible_results_after_evaluation(expression)?;
        match values.as_slice() {
            [value] => Some(*value),
            _ => None,
        }
    }

    // Boolean knowledge is not a numeric pointer representation. An address
    // of an ordinary declared object/function is nonnull, but an unresolved
    // weak symbol or an arbitrary pointer expression is not such a proof.
    // The caller still lowers the original expression and its effects.
    pub(super) fn known_pointer_truth(&self, expression: &CHirExpr<'_>) -> Option<bool> {
        if null_pointer_constant(self.unit, self.target, expression) {
            return Some(false);
        }
        let symbol = match expression {
            CHirExpr::Cast(ty, operand, _)
                if matches!(self.unit.types().get(*ty)?.kind, TypeKind::Pointer(_)) =>
            {
                return self.known_pointer_truth(operand);
            }
            CHirExpr::Unary(UnaryOp::AddrOf, CHirExpr::Ident { symbol, .. }, _) => *symbol,
            CHirExpr::Ident { symbol, facts, .. }
                if matches!(
                    self.unit.types().get(facts.ty())?.kind,
                    TypeKind::Function { .. } | TypeKind::Array { .. }
                ) =>
            {
                *symbol
            }
            CHirExpr::StringLit(..) | CHirExpr::LabelAddress { .. } => return Some(true),
            _ => return None,
        };
        let linkage = self.unit.symbol_linkage().ok()?;
        (!linkage.weak.contains(&symbol) && !linkage.weak.contains(&linkage.target(symbol)))
            .then_some(true)
    }

    pub(super) fn possible_results_after_evaluation(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<Vec<CInteger>> {
        if self.unit.runtime_floating_environment() {
            return None;
        }
        if let Some(value) = self
            .comparison_partition_result(expression)
            .or_else(|| self.floating_partition_result(expression))
            .or_else(|| self.short_circuit_range_result(expression))
        {
            return Some(vec![CInteger::int(i32::from(value))]);
        }
        if let Some(value) = evocc_chir::constant::eval_absolute_address_constant(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            self.target,
        ) {
            return Some(vec![value]);
        }
        let results = |expression| self.possible_results_after_evaluation(expression);
        let mut values = match expression {
            CHirExpr::MemoryRead { place, .. } => vec![self.fresh_literal_read(place)?],
            CHirExpr::Call(CHirExpr::Ident { symbol, .. }, _, _) => {
                vec![self.functions.get(symbol)?.constant_result?]
            }
            CHirExpr::CompoundLit(ty, initializer, _) => {
                if self.unit.types().get(*ty)?.qualifiers.is_volatile {
                    return None;
                }
                let integer =
                    canonical_integer(self.unit.types(), self.target, *ty, expression.span())
                        .ok()?;
                match initializer.plan().actions() {
                    [InitAction::StoreScalar { path, value, .. }] if path.is_root() => {
                        results(value)?
                            .into_iter()
                            .map(|value| value.cast(integer))
                            .collect()
                    }
                    [] if initializer.plan().zero_fills_unwritten() => {
                        vec![CInteger::new(0, integer)]
                    }
                    _ => return None,
                }
            }
            CHirExpr::Comma(_, right, _) => results(right)?,
            CHirExpr::Cast(ty, operand, _) => {
                let integer =
                    canonical_integer(self.unit.types(), self.target, *ty, expression.span())
                        .ok()?;
                results(operand)?
                    .into_iter()
                    .map(|value| value.cast(integer))
                    .collect()
            }
            CHirExpr::Unary(UnaryOp::Not, operand, _)
                if self.known_pointer_truth(operand).is_some() =>
            {
                vec![CInteger::int(i32::from(
                    !self.known_pointer_truth(operand)?,
                ))]
            }
            CHirExpr::Unary(operator, operand, _) => results(operand)?
                .into_iter()
                .map(|value| evocc_chir::integer::eval_unary_integer(*operator, value).ok())
                .collect::<Option<Vec<_>>>()?,
            CHirExpr::Binary(op @ (BinaryOp::And | BinaryOp::Or), left, right, _) => {
                let left = results(left)?;
                let mut values = Vec::new();
                for value in left {
                    let truth = value.truth();
                    if (*op == BinaryOp::And && !truth) || (*op == BinaryOp::Or && truth) {
                        values.push(CInteger::int(i32::from(truth)));
                    } else {
                        values.extend(
                            results(right)?
                                .into_iter()
                                .map(|value| CInteger::int(i32::from(value.truth()))),
                        );
                    }
                }
                values
            }
            CHirExpr::Binary(operator, left, right, _) => {
                if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) {
                    let other = if null_pointer_constant(self.unit, self.target, left) {
                        Some(*right)
                    } else if null_pointer_constant(self.unit, self.target, right) {
                        Some(*left)
                    } else {
                        None
                    };
                    if let Some(truth) = other.and_then(|e| self.known_pointer_truth(e)) {
                        return Some(vec![CInteger::int(i32::from(
                            truth == (*operator == BinaryOp::Ne),
                        ))]);
                    }
                }
                if matches!(
                    operator,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                ) && [left, right].iter().all(|operand| {
                    evocc_chir::PointerTypeDesc::from_expression(
                        self.unit.symbols(),
                        self.unit.types(),
                        operand,
                    )
                    .is_ok()
                }) {
                    let address = |expression| {
                        symbolic_address(
                            self.unit,
                            self.target,
                            expression,
                            false,
                            self.allocation_origins,
                            &self.string_origins,
                            &BTreeMap::new(),
                        )
                        .ok()
                    };
                    if let (Some((a, x, false)), Some((b, y, false))) =
                        (address(left), address(right))
                    {
                        if a == b {
                            return Some(vec![eval_binary_integer(
                                *operator,
                                CInteger::new(x as u64, CIntegerType::LONG),
                                CInteger::new(y as u64, CIntegerType::LONG),
                            )
                            .ok()?]);
                        }
                    }
                }
                let left = results(left)?;
                let right = results(right)?;
                let mut values = Vec::new();
                for a in left {
                    for &b in &right {
                        values.push(eval_binary_integer(*operator, a, b).ok()?);
                    }
                }
                values
            }
            CHirExpr::Ternary(condition, yes, no, _) => {
                let ty = evocc_chir::constant::integer_expression_type(
                    expression,
                    self.unit.types(),
                    self.unit.symbols(),
                )?;
                let conditions =
                    results(condition).unwrap_or_else(|| vec![CInteger::int(0), CInteger::int(1)]);
                let mut values = Vec::new();
                if conditions.iter().any(|value| value.truth()) {
                    values.extend(results(yes)?);
                }
                if conditions.iter().any(|value| !value.truth()) {
                    values.extend(results(no)?);
                }
                values.into_iter().map(|value| value.cast(ty)).collect()
            }
            _ => return None,
        };
        values.sort_by_key(|value| value.raw_bits());
        values.dedup();
        // Finite alternatives, not path enumeration or an unbounded evaluator.
        (values.len() <= 8).then_some(values)
    }

    // A pair of comparisons partitions the SAME stable integer operands into
    // less/equal/greater. This proves only the result after evaluation: normal
    // short-circuit execution and poison consumption are still lowered above.
    pub(super) fn comparison_partition_result(&self, expression: &CHirExpr<'_>) -> Option<bool> {
        let CHirExpr::Binary(logical @ (BinaryOp::And | BinaryOp::Or), left, right, _) = expression
        else {
            return None;
        };
        let operand = |expression: &CHirExpr<'_>| {
            let CHirExpr::Ident { symbol, .. } = expression else {
                return None;
            };
            if self.memory_symbols.contains(symbol) || self.global_registers.contains_key(symbol) {
                return None;
            }
            let place = ScalarPlace::bind(self.unit.symbols(), self.unit.types(), *symbol).ok()?;
            let ty = self.unit.types().get(place.ty())?;
            if ty.qualifiers.is_volatile || ty.qualifiers.is_atomic {
                return None;
            }
            Some((
                *symbol,
                canonical_integer(
                    self.unit.types(),
                    self.target,
                    place.ty(),
                    expression.span(),
                )
                .ok()?,
            ))
        };
        let predicate = |expression: &CHirExpr<'_>| {
            let CHirExpr::Binary(op, left, right, _) = expression else {
                return None;
            };
            let mut mask = match op {
                BinaryOp::Lt => 1u8,
                BinaryOp::Eq => 2,
                BinaryOp::Gt => 4,
                BinaryOp::Le => 3,
                BinaryOp::Ne => 5,
                BinaryOp::Ge => 6,
                _ => return None,
            };
            let (mut a, at) = operand(left)?;
            let (mut b, bt) = operand(right)?;
            if a > b {
                std::mem::swap(&mut a, &mut b);
                mask = (mask & 2) | ((mask & 1) << 2) | ((mask & 4) >> 2);
            }
            Some((a, b, usual_arithmetic_type(at, bt), mask))
        };
        let (a, b, ty, x) = predicate(left)?;
        let (c, d, other, y) = predicate(right)?;
        if (a, b, ty) != (c, d, other) {
            return None;
        }
        match if *logical == BinaryOp::And {
            x & y
        } else {
            x | y
        } {
            0 => Some(false),
            7 => Some(true),
            _ => None,
        }
    }

    // Prove a fresh literal or a private immutable object's initialized
    // representation. The real load and initialization are retained.
    // Applying the existing scalar/bit-field write plan to complementary
    // initial bytes makes every unwritten bit disagree, so padding cannot
    // accidentally become a known zero. Relocated/pointer bytes are excluded.
    pub(super) fn fresh_literal_read(&self, place: &MemoryPlace<'_>) -> Option<CInteger> {
        let qualifiers = place.access_qualifiers();
        if qualifiers.is_volatile || qualifiers.is_atomic {
            return None;
        }
        let (ty, initializer, mut offset) = match place.root() {
            PlaceRoot::Object { symbol } if self.constant_objects.contains(symbol) => {
                let object = self.object_declarations.get(symbol)?;
                (object.ty, object.initializer.as_ref()?, 0)
            }
            PlaceRoot::PointerExpression { pointer, .. } => {
                literal_address(self.unit.types(), pointer)?
            }
            PlaceRoot::CompoundLiteral {
                expression: CHirExpr::CompoundLit(ty, init, _),
                ..
            } => (*ty, init, 0),
            _ => return None,
        };
        if self.unit.types().get(ty)?.qualifiers.is_volatile {
            return None;
        }
        for projection in place.projections() {
            let delta = match projection {
                PlaceProjection::Index(index) => {
                    let value = constant_integer(self.unit, self.target, index.index())?;
                    if value.ty().is_signed() && value.mathematical_value() < 0 {
                        return None;
                    }
                    u64::try_from(value.raw_bits()).ok()?.checked_mul(
                        self.target
                            .layout_of(self.unit.types(), index.element_ty())
                            .ok()?
                            .size_bytes,
                    )?
                }
                PlaceProjection::Field(field) => {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), field.tag())
                        .ok()?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|entry| entry.field_index == field.field_index())?;
                    if field.bit_field.is_some() {
                        return None;
                    }
                    field.byte_offset
                }
            };
            offset = offset.checked_add(delta)?;
        }
        let integer = canonical_integer(
            self.unit.types(),
            self.target,
            place.value_ty(),
            initializer.span(),
        )
        .ok()?;
        let layout = self.target.layout_of(self.unit.types(), ty).ok()?;
        let size = usize::try_from(layout.size_bytes).ok()?;
        // This bounds compile-time work, not source object size or acceptance.
        if size > 4096 {
            return None;
        }
        let start = usize::try_from(offset).ok()?;
        let width = usize::from(integer.bits()).div_ceil(8);
        let end = start.checked_add(width)?;
        if end > size {
            return None;
        }
        let mut first = vec![0x55; size];
        let mut second = vec![0xaa; size];
        for bytes in [&mut first, &mut second] {
            let mut relocs = Vec::new();
            if initializer.plan().zero_fills_unwritten() {
                self.zero_constant_subobjects(ty, initializer, bytes)?;
            }
            for action in initializer.plan().actions() {
                if !matches!(
                    action,
                    InitAction::ZeroScalar { .. }
                        | InitAction::StoreScalar { .. }
                        | InitAction::StoreString { .. }
                ) {
                    return None;
                }
                apply_static_initializer(
                    self.unit,
                    self.target,
                    ty,
                    action,
                    bytes,
                    &mut relocs,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .ok()?;
            }
            if !relocs.is_empty() {
                return None;
            }
        }
        if first[start..end] != second[start..end] {
            return None;
        }
        let mut raw = 0u128;
        for (i, byte) in first[start..end].iter().enumerate() {
            let index = match self.target.byte_order() {
                evocc_types::ByteOrder::Little => i,
                evocc_types::ByteOrder::Big => width - 1 - i,
            };
            raw |= u128::from(*byte) << (index * 8);
        }
        Some(CInteger::from_raw(raw, integer))
    }

    pub(super) fn zero_constant_subobjects(
        &self,
        ty: TypeId,
        initializer: &ObjectInitializer<'_>,
        bytes: &mut [u8],
    ) -> Option<()> {
        let actions = initializer.plan().actions();
        let mut pending = vec![(ty, SubobjectPath::root())];
        let mut visited = 0;
        while let Some((current, path)) = pending.pop() {
            visited += 1;
            if visited > 8192 {
                return None;
            }
            if actions
                .iter()
                .any(|action| path.steps().starts_with(action.path().steps()))
            {
                continue;
            }
            match &self.unit.types().get(current)?.kind {
                TypeKind::Array {
                    element,
                    bound: ArrayBound::Fixed(count),
                } => {
                    if *count > 8192 {
                        return None;
                    }
                    pending.extend(
                        (0..*count).map(|index| (*element, path.clone().with_index(index))),
                    );
                }
                TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                    let union = matches!(self.unit.types().get(current)?.kind, TypeKind::Union(_));
                    let TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) = &self.unit.types().tag(*tag)?.state
                    else {
                        return None;
                    };
                    let selected = union.then(|| {
                        actions.iter().find_map(|action| {
                            let suffix = action.path().steps().strip_prefix(path.steps())?;
                            match suffix.first()? {
                                SubobjectStep::Field { field_index } => Some(*field_index),
                                _ => None,
                            }
                        })
                    });
                    for (index, field) in fields.iter().enumerate() {
                        if field.name.is_none() && field.bit_field.is_some() {
                            continue;
                        }
                        if selected.flatten().is_some_and(|selected| selected != index) {
                            continue;
                        }
                        pending.push((field.ty, path.clone().with_field(index)));
                        if union {
                            break;
                        }
                    }
                }
                TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_) => {
                    apply_static_initializer(
                        self.unit,
                        self.target,
                        ty,
                        &InitAction::zero_scalar(path, current),
                        bytes,
                        &mut Vec::new(),
                        &BTreeMap::new(),
                        &BTreeMap::new(),
                        &BTreeMap::new(),
                    )
                    .ok()?;
                }
                // Optional byte proof, not a second initializer. Leave
                // unsupported subobjects (and a flexible array's nonexistent
                // elements) unknown in the two complementary buffers. They
                // cannot invalidate knowledge of a separately initialized
                // member, or become fabricated zero/pointer representations.
                _ => {}
            }
        }
        Some(())
    }

    pub(super) fn lower_condition(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let value = self.lower_expr(expression, state)?;
        self.lower_evaluated_condition(expression, value, state)
    }

    pub(super) fn lower_evaluated_condition(
        &mut self,
        expression: &'a CHirExpr<'a>,
        value: ExprValue,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        // Materialize a proven truth only after performing the expression.
        // Its loads, calls and obligations still run; the truth can now cross
        // a short-circuit join just like any other constant SSA value.
        if self.optimize {
            if let Some(known) = self.result_after_evaluation(expression) {
                return Ok(self
                    .constant(state, CIntegerType::BOOL, u64::from(known.truth()))?
                    .value);
            }
        }
        if let Some(ty) = evocc_chir::vector::expression_type(self.unit.types(), expression)
            .filter(|ty| evocc_chir::complex::admitted(self.unit.types(), *ty))
        {
            let value = self.complex_operand(value, expression, state)?;
            return self.complex_truth(value, ty, expression.span(), state);
        }
        self.lower_condition_value(value, state)
    }

    pub(super) fn lower_condition_value(
        &mut self,
        value: ExprValue,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        if evocc_evoir::floating_kind(&value.value.ty).is_some() {
            return self.floating_truth(value, state);
        }
        if value.value.ty == EvoIRType::Bits(1) {
            return Ok(value.value);
        }
        self.compare_with_zero(value.value, CmpOp::Ne, state)
    }

    pub(super) fn compare_with_zero(
        &mut self,
        value: Value,
        comparison: CmpOp,
        state: &State,
    ) -> Result<Value, LowerError> {
        let zero = self.fresh_value(value.ty.clone());
        let constant = if matches!(value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
            Inst::PtrNull(zero.clone())
        } else {
            Inst::Const(zero.clone(), 0)
        };
        self.emit(state, constant)?;
        let condition = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ICmp(condition.clone(), comparison, value, zero),
        )?;
        Ok(condition)
    }
}
