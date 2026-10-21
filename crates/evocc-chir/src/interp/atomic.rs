//! Accesses of `_Atomic` objects (EvoIR 10.1.21). A read is a `seq_cst`
//! atomic load and a simple assignment a `seq_cst` atomic store. A compound
//! assignment, `++` or `--` is one read-modify-write: an atomic load, the
//! ordinary computation with its obligations, and a compare-exchange that
//! retries with the observed value. One thread runs, so the first
//! compare-exchange succeeds; the events show the order of the accesses,
//! not inter-thread behavior. Initialization is not an atomic access.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    fn require_admitted_atomic_object(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<ValueType, InterpreterError> {
        if !crate::atomic_access_admitted(self.unit.types(), &self.target, ty) {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "atomic memory access",
                span,
            });
        }
        self.value_type(ty, span)
    }

    /// The C value of the bits or pointer that an atomic access observed.
    pub(super) fn atomic_object_value(
        &self,
        value: evocc_memory::AtomicValue,
        ty: ValueType,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        match (value, ty) {
            (evocc_memory::AtomicValue::Integer(raw), ValueType::Floating(kind)) => {
                evocc_types::floating::FloatBits::from_bits(kind, raw)
                    .map(CValue::Floating)
                    .ok_or(InterpreterError::UnsupportedExpression {
                        span,
                        feature: "atomic floating representation",
                    })
            }
            (value, ty) => self.atomic_result_value(value, ty, span),
        }
    }

    /// The representation that an atomic access stores for `value`, which
    /// already has the type of the object.
    pub(super) fn atomic_object_operand(
        &mut self,
        value: CValue,
        ty: ValueType,
        span: SourceSpan,
    ) -> Result<evocc_memory::AtomicValue, InterpreterError> {
        match value {
            CValue::Floating(value) => Ok(evocc_memory::AtomicValue::Integer(value.bits())),
            value => self.atomic_operand_value(value, ty, span),
        }
    }

    /// A `seq_cst` atomic load of the `_Atomic` object at `address`.
    pub(super) fn load_atomic_object(
        &mut self,
        address: Pointer,
        ty: TypeId,
        volatile: bool,
        layout: AccessLayout,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let value_type = self.require_admitted_atomic_object(ty, span)?;
        let (value, world) = self
            .memory
            .atomic_load(
                &self.world,
                address,
                layout,
                evocc_memory::AtomicOrder::SeqCst,
                volatile,
                matches!(value_type, ValueType::Pointer(_)),
            )
            .map_err(|error| self.memory_error(error, span))?;
        self.world = world;
        self.atomic_object_value(value, value_type, span)
    }

    /// A `seq_cst` atomic store of `value` converted to the type of the
    /// object. The converted value is the result of the assignment.
    pub(super) fn store_atomic_object(
        &mut self,
        address: Pointer,
        ty: TypeId,
        volatile: bool,
        value: CValue,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        let value_type = self.require_admitted_atomic_object(ty, span)?;
        let value = match value_type {
            ValueType::Floating(kind) => {
                let strict = self.strict_floating();
                CValue::Floating(floating::convert_runtime(
                    value,
                    kind,
                    span,
                    &mut self.world,
                    strict,
                )?)
            }
            _ => self.cast_runtime_value(value, value_type, span)?,
        };
        let stored = self.atomic_object_operand(value, value_type, span)?;
        let (memory, world) = self
            .memory
            .atomic_store(
                &self.world,
                address,
                layout,
                evocc_memory::AtomicOrder::SeqCst,
                volatile,
                stored,
            )
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        self.world = world;
        Ok(value)
    }

    /// One `seq_cst` read-modify-write of the `_Atomic` object at `address`.
    /// `compute` gives the new value from the old one. Returns the old and
    /// the new value of the successful compare-exchange.
    pub(super) fn update_atomic_object(
        &mut self,
        address: Pointer,
        ty: TypeId,
        volatile: bool,
        layout: AccessLayout,
        span: SourceSpan,
        mut compute: impl FnMut(&mut Self, CValue) -> Result<CValue, InterpreterError>,
    ) -> Result<(CValue, CValue), InterpreterError> {
        let value_type = self.require_admitted_atomic_object(ty, span)?;
        if matches!(value_type, ValueType::Floating(_)) {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "floating atomic update",
                span,
            });
        }
        let mut old = self.load_atomic_object(address, ty, volatile, layout, span)?;
        loop {
            let new = compute(self, old)?;
            let new = self.cast_runtime_value(new, value_type, span)?;
            let value = self.atomic_object_operand(new, value_type, span)?;
            let expected = self.atomic_object_operand(old, value_type, span)?;
            let (memory, world, outcome) = self
                .memory
                .atomic_rmw(
                    &self.world,
                    address,
                    layout,
                    evocc_memory::AtomicOrder::SeqCst,
                    volatile,
                    evocc_memory::AtomicRmwOp::CompareExchange,
                    value,
                    Some(expected),
                )
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            if outcome.exchanged {
                return Ok((old, new));
            }
            old = self.atomic_object_value(outcome.old, value_type, span)?;
        }
    }

    /// A compound assignment to an `_Atomic` integer or pointer object: one
    /// read-modify-write whose value is the new value.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_atomic_assignment(
        &mut self,
        operator: AssignmentOp,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        right: CValue,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        if !matches!(
            right,
            CValue::Integer(_) | CValue::Pointer { .. } | CValue::FunctionPointer { .. }
        ) {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "floating atomic update",
                span,
            });
        }
        let binary =
            assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                feature: "invalid compound assignment operator",
                span,
            })?;
        let (_, new) = self.update_atomic_object(
            address,
            ty,
            qualifiers.is_volatile,
            layout,
            span,
            |this, old| match old {
                CValue::Pointer { .. } | CValue::FunctionPointer { .. } => {
                    this.apply_machine_binary(binary, old, right, span)
                }
                old => {
                    let left = expect_machine_integer(old, span)?;
                    let right = expect_machine_integer(right, span)?;
                    this.runtime_integer_binary(binary, left, right, span)
                        .map(CValue::Integer)
                }
            },
        )?;
        Ok(new)
    }

    /// A generic form on memory: `value`, `expected` and `result` are the
    /// buffer pointers of `__atomic_store`, `__atomic_exchange`,
    /// `__atomic_load` and `__atomic_compare_exchange`. A compare-exchange
    /// takes the observed representation in a temporary and copies it to the
    /// expected buffer only when the comparison fails, as the lowering does.
    pub(super) fn apply_atomic_copy(
        &mut self,
        access: &crate::CHirAtomicAccess<'arena>,
        address: CValue,
        value: Option<CValue>,
        expected: Option<CValue>,
        result: Option<CValue>,
    ) -> Result<CValue, InterpreterError> {
        use evocc_memory::{AtomicCopyOp as C, AtomicRmwOp as R};
        let span = access.span;
        let pointer = |value: Option<CValue>| -> Result<Pointer, InterpreterError> {
            let value = value.ok_or(InterpreterError::UninitializedValue { span })?;
            expect_machine_pointer(value, span).map(|(pointer, _)| pointer)
        };
        let object = pointer(Some(address))?;
        let buffer = self.access_layout(access.access_ty, span)?;
        let order = access
            .constant_order
            .unwrap_or(evocc_memory::AtomicOrder::SeqCst);
        let (op, buffers, writeback) = match access.rmw {
            None if result.is_some() => (C::Load, vec![pointer(result)?], None),
            None => (C::Store, vec![pointer(value)?], None),
            Some((R::Exchange, _)) => (C::Exchange, vec![pointer(value)?, pointer(result)?], None),
            Some((R::CompareExchange, _)) => {
                let expected = pointer(expected)?;
                let observed = self.vector_temporary(access.access_ty, span)?;
                (
                    C::CompareExchange,
                    vec![expected, pointer(value)?, observed],
                    Some((expected, observed)),
                )
            }
            Some(_) => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "atomic arithmetic on memory",
                    span,
                })
            }
        };
        let (memory, world, exchanged) = self
            .memory
            .atomic_copy(
                &self.world,
                op,
                object,
                &buffers,
                buffer,
                order,
                access.volatile,
            )
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        self.world = world;
        if let Some((expected, observed)) = writeback.filter(|_| !exchanged) {
            self.memory = self
                .memory
                .copy_bytes(expected, observed, buffer)
                .map_err(|error| self.memory_error(error, span))?;
        }
        Ok(if op == C::CompareExchange {
            CValue::Integer(CInteger::new(u64::from(exchanged), CIntegerType::BOOL))
        } else {
            CValue::Integer(CInteger::int(0))
        })
    }
}
