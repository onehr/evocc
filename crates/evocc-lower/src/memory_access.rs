//! Lowering memory access.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_atomic(
        &mut self,
        access: &evocc_chir::CHirAtomicAccess<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        // Arguments can contain calls and joins. Keep evaluated operands in
        // the same explicit edge environment used by ordinary calls.
        let mut keys = Vec::new();
        for operand in access.operands() {
            let value = self.lower_expr(operand, state)?;
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: value.value,
                    integer: Some(value.integer),
                },
            );
            keys.push(key);
        }
        let mut values = keys
            .into_iter()
            .map(|key| {
                state
                    .env
                    .remove(&key)
                    .ok_or(LowerError::InvalidControlFlow {
                        span: access.span,
                        reason: "atomic operand lost across control flow",
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        // The orders and the weak flag were evaluated once each; a dynamic
        // order only selects seq_cst.
        if access.failure.is_some() {
            values.pop();
        }
        let _evaluated_order = values.pop();
        if access.weak.is_some() {
            values.pop();
        }
        let result_buffer = if access.result.is_some() {
            values.pop().map(|bound| bound.value)
        } else {
            None
        };
        let stored = if access.value.is_some() {
            values.pop().map(|bound| bound.value)
        } else {
            None
        };
        let expected = if access.expected.is_some() {
            values.pop().map(|bound| bound.value)
        } else {
            None
        };
        let address = values.remove(0).value;
        if access.memory {
            return self.lower_atomic_copy(access, address, stored, expected, result_buffer, state);
        }
        let (value_ty, integer) = self.c_value_type(access.access_ty, access.span)?;
        // A floating object moves as its bits (EvoIR 10.1.21), so a
        // compare-exchange compares object representations.
        let (ty, bits_integer) = match value_ty {
            EvoIRType::F32 => (EvoIRType::Bits(32), CIntegerType::UINT),
            EvoIRType::F64 => (EvoIRType::Bits(64), CIntegerType::ULONG),
            _ => (value_ty.clone(), integer),
        };
        let stored = match stored {
            Some(value) => Some(self.floating_to_atomic_bits(value, state)?),
            None => None,
        };
        // `__atomic_compare_exchange_n` reads the expected value through its
        // pointer with an ordinary load before the atomic operation.
        let expected_object = if access.writeback {
            expected.clone()
        } else {
            None
        };
        let expected = match &expected_object {
            Some(pointer) => {
                let loaded = self
                    .emit_memory_load(pointer.clone(), access.access_ty, false, access.span, state)?
                    .value;
                Some(self.floating_to_atomic_bits(loaded, state)?)
            }
            None => expected,
        };
        let result = (stored.is_none() || access.rmw.is_some()).then(|| self.fresh_value(ty));
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let guarded = self.fresh_value(EvoIRType::Mem(Region::All));
        let layout = self.access_layout(access.access_ty, access.span)?;
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
        let op = if let Some((operation, _)) = access.rmw {
            evocc_evoir::AtomicOp::Rmw {
                result: result.clone().expect("RMW result"),
                operation,
                value: stored.clone().expect("RMW value"),
                expected: expected.clone(),
            }
        } else if let Some(value) = &stored {
            evocc_evoir::AtomicOp::Store(value.clone())
        } else {
            evocc_evoir::AtomicOp::Load(result.clone().expect("atomic load result"))
        };
        self.emit(
            state,
            Inst::Atomic(evocc_evoir::AtomicAccess {
                op,
                address,
                order: access
                    .constant_order
                    .unwrap_or(evocc_evoir::AtomicOrder::SeqCst),
                volatile: access.volatile,
                mem_in: guarded,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }),
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        if let Some(value) = result {
            use evocc_chir::{AtomicRmwOp as R, AtomicRmwReturn as K};
            let (value, integer) = match access.rmw {
                Some((_, K::Success)) => {
                    let observed = value.clone();
                    let old = self.cast(
                        ExprValue {
                            value,
                            integer: bits_integer,
                        },
                        CIntegerType::ULONG,
                        state,
                    )?;
                    let expected = self.cast(
                        ExprValue {
                            value: expected.expect("CAS expected"),
                            integer: bits_integer,
                        },
                        CIntegerType::ULONG,
                        state,
                    )?;
                    let equal =
                        self.checked_compare(state, CmpOp::Eq, &old.value, &expected.value)?;
                    if let Some(pointer) = expected_object {
                        // A failed comparison stores the observed value back.
                        let observed =
                            self.floating_from_atomic_bits(observed, value_ty.clone(), state)?;
                        self.store_on_failure(
                            equal.clone(),
                            pointer,
                            ExprValue {
                                value: observed,
                                integer,
                            },
                            access,
                            state,
                        )
                        .map(|equal| (equal, CIntegerType::BOOL))?
                    } else {
                        (equal, CIntegerType::BOOL)
                    }
                }
                Some((operation @ (R::Add | R::Sub), K::New))
                    if matches!(value.ty, EvoIRType::Ptr(_)) =>
                {
                    // Pointer forms add or subtract bytes as an address; the
                    // atomic object itself keeps its provenance.
                    let operand = stored.expect("RMW value");
                    let address = self.fresh_value(EvoIRType::Bits(64));
                    self.emit(
                        state,
                        Inst::PointerCast {
                            result: address.clone(),
                            operand: value.clone(),
                        },
                    )?;
                    let inst = if operation == R::Add {
                        Inst::Add
                    } else {
                        Inst::Sub
                    };
                    let moved = self.emit_raw_binary(state, inst, &address, &operand)?;
                    let pointer = self.fresh_value(value.ty.clone());
                    self.emit(
                        state,
                        Inst::PointerCast {
                            result: pointer.clone(),
                            operand: moved,
                        },
                    )?;
                    (pointer, integer)
                }
                Some((operation, K::New)) => {
                    let operand = stored.expect("RMW value");
                    let inst = match operation {
                        R::Add => Inst::Add,
                        R::Sub => Inst::Sub,
                        R::And | R::Nand => Inst::And,
                        R::Or => Inst::Or,
                        R::Xor => Inst::Xor,
                        _ => unreachable!("new-result arithmetic operation"),
                    };
                    let mut result = self.emit_raw_binary(state, inst, &value, &operand)?;
                    if operation == R::Nand {
                        let ones = self.wide_constant(state, integer, u128::MAX)?.value;
                        result = self.emit_raw_binary(state, Inst::Xor, &result, &ones)?;
                    }
                    (result, integer)
                }
                _ => (
                    self.floating_from_atomic_bits(value, value_ty.clone(), state)?,
                    integer,
                ),
            };
            Ok(ExprValue { value, integer })
        } else {
            self.constant(state, CIntegerType::INT, 0)
        }
    }

    /// An implicit `asm volatile` barrier with a `memory` clobber, no
    /// operands and the current effect tokens. Both interpreters report
    /// `E3021` on reaching it; native code emits the template.
    pub(super) fn emit_barrier(
        &mut self,
        template: &str,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let assembly = InlineAssembly {
            template: template.into(),
            volatile: true,
            outputs: vec![],
            inputs: vec![],
            results: vec![],
            clobbers: vec!["memory".into()],
            mem_in: self.token(state, EnvKey::Memory)?.value,
            world_in: self.token(state, EnvKey::World)?.value,
            mem_out: self.fresh_value(EvoIRType::Mem(Region::All)),
            world_out: self.fresh_value(EvoIRType::World),
        };
        assembly
            .validate(&[])
            .map_err(|reason| LowerError::AsmOperand { span, reason })?;
        self.replace_effect_tokens(
            state,
            assembly.mem_out.clone(),
            Some(assembly.world_out.clone()),
        );
        self.emit(state, Inst::Asm(assembly))
    }

    /// Store `observed` through `pointer` only when `equal` is false, then
    /// join. Returns `equal` as it reaches the join.
    pub(super) fn store_on_failure(
        &mut self,
        equal: Value,
        pointer: Value,
        observed: ExprValue,
        access: &evocc_chir::CHirAtomicAccess<'a>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let integer = observed.integer;
        let mut carried = Vec::new();
        for value in [equal.clone(), pointer, observed.value] {
            let key = self.retain_temporary(value, state);
            state.env.get_mut(&key).unwrap().integer = Some(integer);
            carried.push(key);
        }
        let keys = self.expression_join_keys(state, &[]);
        let done = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut failed = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let done_arguments = self.edge_values(state, &keys)?;
        let failed_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            conditional_branch(
                equal,
                None,
                done.block,
                done_arguments,
                failed.block,
                failed_arguments,
            ),
        )?;
        let pointer = failed.env[&carried[1]].value.clone();
        let observed = ExprValue {
            value: failed.env[&carried[2]].value.clone(),
            integer,
        };
        self.emit_memory_store(
            pointer,
            observed,
            access.access_ty,
            false,
            access.span,
            &mut failed,
        )?;
        self.branch(&failed, done.block, &keys)?;
        *state = done;
        let equal = state
            .env
            .remove(&carried[0])
            .expect("joined comparison")
            .value;
        for key in &carried[1..] {
            state.env.remove(key);
        }
        Ok(equal)
    }

    pub(super) fn emit_memory_load(
        &mut self,
        address: Value,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let layout = self.access_layout(ty, span)?;
        self.emit_memory_load_with_layout(address, ty, volatile, span, state, layout)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_memory_load_with_layout(
        &mut self,
        address: Value,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
        layout: AccessLayout,
    ) -> Result<ExprValue, LowerError> {
        // A read of an `_Atomic` object is a `seq_cst` atomic load.
        if self
            .unit
            .types()
            .get(ty)
            .is_some_and(|canonical| canonical.qualifiers.is_atomic)
        {
            return self.emit_atomic_object_load(address, ty, volatile, layout, span, state);
        }
        let (result_ty, integer) = self.c_value_type(ty, span)?;
        self.emit_typed_memory_load(address, result_ty, integer, volatile, state, layout)
    }

    pub(super) fn emit_typed_memory_load(
        &mut self,
        address: Value,
        result_ty: EvoIRType,
        integer: CIntegerType,
        volatile: bool,
        state: &mut State,
        layout: AccessLayout,
    ) -> Result<ExprValue, LowerError> {
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
        self.replace_effect_tokens(state, guarded.clone(), None);
        let world_in = self.token(state, EnvKey::World)?.value;
        let result = self.fresh_value(result_ty);
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Load {
                result: result.clone(),
                address,
                mem_in: guarded,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                volatile,
                mask: None,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    pub(super) fn emit_memory_store(
        &mut self,
        address: Value,
        value: ExprValue,
        ty: TypeId,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let layout = self.access_layout(ty, span)?;
        self.emit_memory_store_with_layout(address, value, layout, volatile, state)
    }

    pub(super) fn emit_memory_store_with_layout(
        &mut self,
        address: Value,
        value: ExprValue,
        layout: AccessLayout,
        volatile: bool,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
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
        self.replace_effect_tokens(state, guarded.clone(), None);
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Store {
                address,
                value: value.value.clone(),
                mem_in: guarded,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                volatile,
                mask: None,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(value)
    }
}
