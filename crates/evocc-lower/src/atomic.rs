//! Accesses of `_Atomic` objects (EvoIR 10.1.21). A read is a `seq_cst`
//! `atomic.load` and a simple assignment a `seq_cst` `atomic.store`. A
//! compound assignment, `++` or `--` is one `seq_cst` read-modify-write: an
//! atomic load, then a loop that computes the new value with the obligations
//! of the ordinary computation and stores it with `atomic.rmw.cmpxchg`. A
//! failed comparison takes the observed value and retries. A floating object
//! moves through `bits<32>` or `bits<64>` without conversion. The generic
//! `__atomic` forms on memory lower to `atomic.copy` (EvoIR 10.1.14).

use super::*;

/// The computation of an update from the old value and the evaluated right
/// operand, emitted in the retry block.
pub(super) type AtomicCompute<'s, 'a> = dyn FnMut(
        &mut FunctionLowerer<'a>,
        ExprValue,
        Option<ExprValue>,
        &mut State,
    ) -> Result<ExprValue, LowerError>
    + 's;

impl<'a> FunctionLowerer<'a> {
    /// The EvoIR type that an atomic access of a value of type `value_ty`
    /// transfers: floating values move as their bits.
    fn atomic_access_type(value_ty: &EvoIRType) -> EvoIRType {
        match value_ty {
            EvoIRType::F32 => EvoIRType::Bits(32),
            EvoIRType::F64 => EvoIRType::Bits(64),
            other => other.clone(),
        }
    }

    pub(super) fn require_admitted_atomic_object(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<(), LowerError> {
        if evocc_chir::atomic_access_admitted(self.unit.types(), self.target, ty) {
            Ok(())
        } else {
            Err(LowerError::UnsupportedExpression {
                span,
                feature: "atomic access outside the admitted scalar widths",
            })
        }
    }

    fn emit_atomic_access(
        &mut self,
        op: evocc_evoir::AtomicOp,
        address: Value,
        volatile: bool,
        layout: AccessLayout,
        state: &mut State,
    ) -> Result<(), LowerError> {
        self.emit_atomic_access_with_order(
            op,
            address,
            evocc_evoir::AtomicOrder::SeqCst,
            volatile,
            layout,
            state,
        )
    }

    fn emit_atomic_access_with_order(
        &mut self,
        op: evocc_evoir::AtomicOp,
        address: Value,
        order: evocc_evoir::AtomicOrder,
        volatile: bool,
        layout: AccessLayout,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let guarded = self.fresh_value(EvoIRType::Mem(Region::All));
        self.emit(
            state,
            Inst::AccessInBounds {
                mem_in,
                pointer: address.clone(),
                mem_out: guarded.clone(),
                layout,
            },
        )?;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Atomic(evocc_evoir::AtomicAccess {
                op,
                address,
                order,
                volatile,
                mem_in: guarded,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }),
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(())
    }

    /// Convert the bits of an atomic access back to a floating value.
    fn floating_from_atomic_bits(
        &mut self,
        bits: Value,
        value_ty: EvoIRType,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        if evocc_evoir::floating_kind(&value_ty).is_none() {
            return Ok(bits);
        }
        let result = self.fresh_value(value_ty);
        self.emit(
            state,
            Inst::Floating(evocc_evoir::FloatingInst {
                result: result.clone(),
                op: evocc_evoir::FloatingOp::FromBits,
                arguments: vec![bits],
            }),
        )?;
        Ok(result)
    }

    /// A `seq_cst` atomic load of the `_Atomic` object of type `ty`.
    pub(super) fn emit_atomic_object_load(
        &mut self,
        address: Value,
        ty: TypeId,
        volatile: bool,
        layout: AccessLayout,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.require_admitted_atomic_object(ty, span)?;
        let (value_ty, integer) = self.c_value_type(ty, span)?;
        let bits = self.fresh_value(Self::atomic_access_type(&value_ty));
        self.emit_atomic_access(
            evocc_evoir::AtomicOp::Load(bits.clone()),
            address,
            volatile,
            layout,
            state,
        )?;
        let value = self.floating_from_atomic_bits(bits, value_ty, state)?;
        Ok(ExprValue { value, integer })
    }

    /// A `seq_cst` atomic store of `value`, already converted to the type
    /// `ty` of the object. The value is the result of the assignment.
    pub(super) fn emit_atomic_object_store(
        &mut self,
        address: Value,
        value: ExprValue,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.require_admitted_atomic_object(ty, span)?;
        let layout = self.access_layout(ty, span)?;
        let stored = if evocc_evoir::floating_kind(&value.value.ty).is_some() {
            let bits = self.fresh_value(Self::atomic_access_type(&value.value.ty));
            self.emit(
                state,
                Inst::Floating(evocc_evoir::FloatingInst {
                    result: bits.clone(),
                    op: evocc_evoir::FloatingOp::ToBits,
                    arguments: vec![value.value.clone()],
                }),
            )?;
            bits
        } else {
            value.value.clone()
        };
        self.emit_atomic_access(
            evocc_evoir::AtomicOp::Store(stored),
            address,
            volatile,
            layout,
            state,
        )?;
        Ok(value)
    }

    /// One `seq_cst` read-modify-write of the `_Atomic` object of type `ty`:
    /// an atomic load, then a retry block that computes the new value from
    /// the old one and `operand`, and stores it with a compare-exchange. A
    /// failed comparison branches back with the observed value. Returns the
    /// old and the new value that the successful exchange used.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_atomic_object_update(
        &mut self,
        address: Value,
        ty: TypeId,
        volatile: bool,
        operand: Option<ExprValue>,
        span: SourceSpan,
        state: &mut State,
        compute: &mut AtomicCompute<'_, 'a>,
    ) -> Result<(ExprValue, ExprValue), LowerError> {
        self.require_admitted_atomic_object(ty, span)?;
        let layout = self.access_layout(ty, span)?;
        let (value_ty, integer) = self.c_value_type(ty, span)?;
        if evocc_evoir::floating_kind(&value_ty).is_some() {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "floating atomic update",
            });
        }
        let address_key = self.retain_temporary(address.clone(), state);
        let operand_key = operand.map(|operand| {
            let key = self.retain_temporary(operand.value, state);
            state.env.get_mut(&key).unwrap().integer = Some(operand.integer);
            key
        });
        let old = self.emit_atomic_object_load(address, ty, volatile, layout, span, state)?;
        let old_key = self.retain_temporary(old.value, state);
        state.env.get_mut(&old_key).unwrap().integer = Some(integer);
        let live = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) | EnvKey::Address(symbol) => Some(*symbol),
                _ => None,
            })
            .collect();
        let keys = self.keys_for(state, &live, &[]);
        let header = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        self.branch(state, header.block, &keys)?;
        let header_block = header.block;
        let mut current = header;
        let old = ExprValue {
            value: current.env[&old_key].value.clone(),
            integer,
        };
        let operand = operand_key.map(|key| ExprValue {
            value: current.env[&key].value.clone(),
            integer: current.env[&key].integer.expect("retained operand type"),
        });
        let new = compute(self, old, operand, &mut current)?;
        if new.value.ty != value_ty {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "atomic update result disagrees with its object",
            });
        }
        let address = current.env[&address_key].value.clone();
        let expected = current.env[&old_key].value.clone();
        let observed = self.fresh_value(value_ty);
        self.emit_atomic_access(
            evocc_evoir::AtomicOp::Rmw {
                result: observed.clone(),
                operation: evocc_evoir::AtomicRmwOp::CompareExchange,
                value: new.value.clone(),
                expected: Some(expected.clone()),
            },
            address,
            volatile,
            layout,
            &mut current,
        )?;
        let left = self.cast(
            ExprValue {
                value: observed.clone(),
                integer,
            },
            CIntegerType::ULONG,
            &mut current,
        )?;
        let right = self.cast(
            ExprValue {
                value: expected,
                integer,
            },
            CIntegerType::ULONG,
            &mut current,
        )?;
        let equal = self.checked_compare(&current, CmpOp::Eq, &left.value, &right.value)?;
        // Equal values make the observed value the old one on both edges.
        current.env.get_mut(&old_key).unwrap().value = observed;
        let new_key = self.retain_temporary(new.value, &mut current);
        current.env.get_mut(&new_key).unwrap().integer = Some(new.integer);
        let done_keys = self.keys_for(&current, &live, &[]);
        let done = self.new_block_from(&current, &done_keys, BlockRole::Ordinary)?;
        let done_arguments = self.edge_values(&current, &done_keys)?;
        let retry_arguments = self.edge_values(&current, &keys)?;
        self.terminate(
            &current,
            conditional_branch(
                equal,
                None,
                done.block,
                done_arguments,
                header_block,
                retry_arguments,
            ),
        )?;
        *state = done;
        let old = ExprValue {
            value: state.env.remove(&old_key).expect("joined old value").value,
            integer,
        };
        let new = ExprValue {
            value: state.env.remove(&new_key).expect("joined new value").value,
            integer: new.integer,
        };
        state.env.remove(&address_key);
        if let Some(key) = operand_key {
            state.env.remove(&key);
        }
        Ok((old, new))
    }

    /// An assignment to an `_Atomic` object whose place and right operand
    /// are evaluated. A simple assignment is one atomic store; a compound
    /// assignment is one read-modify-write whose value is the new value.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_atomic_assignment(
        &mut self,
        operator: AssignmentOp,
        ty: TypeId,
        volatile: bool,
        address: Value,
        right: ExprValue,
        assigned: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if operator == AssignmentOp::Simple {
            let value = self.coerce_to_object_type(right, Some(assigned), ty, span, state)?;
            return self.emit_atomic_object_store(address, value, ty, volatile, span, state);
        }
        if evocc_evoir::floating_kind(&right.value.ty).is_some()
            || evocc_chir::vector::expression_type(self.unit.types(), assigned)
                .is_some_and(|ty| evocc_chir::complex::rank(self.unit.types(), ty).is_some())
        {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "floating atomic update",
            });
        }
        let (_, new) = self.emit_atomic_object_update(
            address,
            ty,
            volatile,
            Some(right),
            span,
            state,
            &mut |this, old, right, state| {
                let right = right.expect("compound assignment operand");
                if matches!(old.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
                    this.compute_pointer_assignment(operator, old, right, ty, span, state)
                } else {
                    let destination = canonical_integer(this.unit.types(), this.target, ty, span)?;
                    this.compute_compound_assignment(operator, old, right, destination, span, state)
                }
            },
        )?;
        Ok(new)
    }

    /// Prefix or postfix `++` or `--` of an `_Atomic` object: one
    /// read-modify-write. A prefix form yields the new value, a postfix
    /// form the old one.
    pub(super) fn lower_atomic_update(
        &mut self,
        operator: UpdateOp,
        ty: TypeId,
        volatile: bool,
        address: Value,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let increment = matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement);
        let (old, new) = self.emit_atomic_object_update(
            address,
            ty,
            volatile,
            None,
            span,
            state,
            &mut |this, old, _, state| {
                if matches!(old.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
                    let one = this.constant(state, CIntegerType::INT, 1)?;
                    let operator = if increment {
                        AssignmentOp::Add
                    } else {
                        AssignmentOp::Subtract
                    };
                    return this.compute_pointer_assignment(operator, old, one, ty, span, state);
                }
                let promoted_type = integer_promotion(old.integer);
                let promoted = this.cast(old, promoted_type, state)?;
                let one = this.constant(state, promoted_type, 1)?;
                let computed = this.emit_signed_arithmetic(
                    state,
                    if increment {
                        BinaryOp::Add
                    } else {
                        BinaryOp::Sub
                    },
                    promoted,
                    one,
                    promoted_type,
                )?;
                let destination = canonical_integer(this.unit.types(), this.target, ty, span)?;
                this.cast(computed, destination, state)
            },
        )?;
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                new
            } else {
                old
            },
        )
    }

    /// A generic form on memory (EvoIR 10.1.14): one `atomic.copy`
    /// instruction on the object, with ordinary buffers of `access_ty`. A
    /// compare-exchange takes the observed representation in a temporary
    /// object and copies it to the expected buffer only when the comparison
    /// fails, so a successful comparison never writes that buffer.
    pub(super) fn lower_atomic_copy(
        &mut self,
        access: &evocc_chir::CHirAtomicAccess<'a>,
        object: Value,
        value: Option<Value>,
        expected: Option<Value>,
        result: Option<Value>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        use evocc_chir::AtomicRmwOp as R;
        use evocc_evoir::AtomicCopyOp as C;
        let span = access.span;
        let buffer = self.access_layout(access.access_ty, span)?;
        let missing = || LowerError::InvalidControlFlow {
            span,
            reason: "atomic buffer operand is missing",
        };
        let (op, mut buffers) = match access.rmw {
            None if result.is_some() => (C::Load, vec![result.ok_or_else(missing)?]),
            None => (C::Store, vec![value.ok_or_else(missing)?]),
            Some((R::Exchange, _)) => (
                C::Exchange,
                vec![value.ok_or_else(missing)?, result.ok_or_else(missing)?],
            ),
            Some((R::CompareExchange, _)) => (
                C::CompareExchange,
                vec![expected.ok_or_else(missing)?, value.ok_or_else(missing)?],
            ),
            Some(_) => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "atomic arithmetic on memory",
                })
            }
        };
        let observed = if op == C::CompareExchange {
            let origin = self.synthetic_origin()?;
            let observed = self.emit_alloca_with_layout(buffer, origin, state)?;
            buffers.push(observed.clone());
            Some(observed)
        } else {
            None
        };
        let success = (op == C::CompareExchange).then(|| self.fresh_value(EvoIRType::Bits(1)));
        self.emit_atomic_access_with_order(
            evocc_evoir::AtomicOp::Copy {
                op,
                buffers: buffers.clone(),
                result: success.clone(),
                layout: buffer,
            },
            object,
            access
                .constant_order
                .unwrap_or(evocc_evoir::AtomicOrder::SeqCst),
            access.volatile,
            AccessLayout::new(buffer.size_bytes, buffer.size_bytes as u32),
            state,
        )?;
        let (Some(success), Some(observed)) = (success, observed) else {
            return self.constant(state, CIntegerType::INT, 0);
        };
        // A failed comparison copies the observed representation back.
        let mut carried = Vec::new();
        for value in [success, buffers[0].clone(), observed] {
            carried.push(self.retain_temporary(value, state));
        }
        let keys = self.expression_join_keys(state, &[]);
        let done = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut failed = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let done_arguments = self.edge_values(state, &keys)?;
        let failed_arguments = self.edge_values(state, &keys)?;
        let success = state.env[&carried[0]].value.clone();
        self.terminate(
            state,
            conditional_branch(
                success,
                None,
                done.block,
                done_arguments,
                failed.block,
                failed_arguments,
            ),
        )?;
        let expected = failed.env[&carried[1]].value.clone();
        let observed = failed.env[&carried[2]].value.clone();
        self.emit_copy(expected, observed, access.access_ty, span, &mut failed)?;
        self.branch(&failed, done.block, &keys)?;
        *state = done;
        let success = state
            .env
            .remove(&carried[0])
            .expect("joined comparison")
            .value;
        state.env.remove(&carried[1]);
        let observed = state
            .env
            .remove(&carried[2])
            .expect("joined temporary")
            .value;
        self.end_pointer_lifetime(observed, state)?;
        Ok(ExprValue {
            value: success,
            integer: CIntegerType::BOOL,
        })
    }
}
