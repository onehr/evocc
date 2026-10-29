//! C-HIR interpreter atomic values.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn apply_atomic(
        &mut self,
        access: &crate::CHirAtomicAccess<'arena>,
        address: CValue,
        stored: Option<CValue>,
        expected: Option<CValue>,
    ) -> Result<CValue, InterpreterError> {
        let span = access.span;
        let (address, _) = expect_machine_pointer(address, span)?;
        let layout = self.access_layout(access.access_ty, span)?;
        let order = access
            .constant_order
            .unwrap_or(evocc_memory::AtomicOrder::SeqCst);
        let ty = self.value_type(access.access_ty, span)?;
        if let Some((operation, result_kind)) = access.rmw {
            let stored = stored.ok_or(InterpreterError::UnsupportedType {
                ty: access.access_ty,
                span,
            })?;
            // GNU `__atomic` add and subtract on a pointer count bytes.
            let pointer_offset = matches!(ty, ValueType::Pointer(_))
                && matches!(
                    operation,
                    evocc_memory::AtomicRmwOp::Add | evocc_memory::AtomicRmwOp::Sub
                );
            // The compare-exchange of `__atomic_compare_exchange_n` reads the
            // expected value through its pointer with an ordinary load.
            let expected_object = if access.writeback {
                let (pointer, _) = expect_machine_pointer(
                    expected.ok_or(InterpreterError::UninitializedValue { span })?,
                    span,
                )?;
                Some(pointer)
            } else {
                None
            };
            let expected = match expected_object {
                Some(pointer) => Some(self.load_memory_value(
                    pointer,
                    access.access_ty,
                    Qualifiers::default(),
                    span,
                )?),
                None => expected,
            };
            let outcome = if pointer_offset {
                let CValue::Integer(bytes) = stored else {
                    return Err(InterpreterError::UninitializedValue { span });
                };
                self.memory.atomic_pointer_offset(
                    &self.world,
                    address,
                    layout,
                    order,
                    access.volatile,
                    operation,
                    bytes.raw_bits() as u64,
                )
            } else {
                // A floating object moves as its bits, so a compare-exchange
                // compares object representations.
                let value = self.atomic_object_operand(stored, ty, span)?;
                let expected = expected
                    .map(|value| self.atomic_object_operand(value, ty, span))
                    .transpose()?;
                self.memory.atomic_rmw(
                    &self.world,
                    address,
                    layout,
                    order,
                    access.volatile,
                    operation,
                    value,
                    expected,
                )
            };
            let (memory, world, result) =
                outcome.map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            if let Some(pointer) = expected_object.filter(|_| !result.exchanged) {
                // A failed comparison stores the observed value back.
                let observed = self.atomic_object_value(result.old, ty, span)?;
                self.store_memory_value(
                    pointer,
                    access.access_ty,
                    Qualifiers::default(),
                    observed,
                    span,
                )?;
            }
            return match result_kind {
                crate::AtomicRmwReturn::Old => self.atomic_object_value(result.old, ty, span),
                crate::AtomicRmwReturn::New => self.atomic_object_value(result.new, ty, span),
                crate::AtomicRmwReturn::Success => Ok(CValue::Integer(CInteger::new(
                    u64::from(result.exchanged),
                    CIntegerType::BOOL,
                ))),
            };
        }
        if let Some(stored) = stored {
            // The front end converted the value to the type of the object; a
            // floating value moves as its bits.
            let stored = match stored {
                CValue::Floating(value) if matches!(ty, ValueType::Floating(_)) => {
                    CValue::Floating(value)
                }
                stored => self.cast_runtime_value(stored, ty, span)?,
            };
            let stored = match stored {
                CValue::Integer(value) => evocc_memory::AtomicValue::Integer(value.raw_bits()),
                CValue::Floating(value) => evocc_memory::AtomicValue::Integer(value.bits()),
                CValue::Pointer { value, .. } => {
                    evocc_memory::AtomicValue::Pointer(PointerValue::object(value))
                }
                CValue::FunctionPointer { value, .. } => evocc_memory::AtomicValue::Pointer(value),
                CValue::Uninitialized(_) => {
                    return Err(InterpreterError::UninitializedValue { span })
                }
                _ => {
                    return Err(InterpreterError::UnsupportedType {
                        ty: access.access_ty,
                        span,
                    })
                }
            };
            let (memory, world) = self
                .memory
                .atomic_store(&self.world, address, layout, order, access.volatile, stored)
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            Ok(CValue::Integer(CInteger::int(0)))
        } else {
            let (value, world) = self
                .memory
                .atomic_load(
                    &self.world,
                    address,
                    layout,
                    order,
                    access.volatile,
                    matches!(ty, ValueType::Pointer(_)),
                )
                .map_err(|error| self.memory_error(error, span))?;
            self.world = world;
            match (value, ty) {
                (evocc_memory::AtomicValue::Integer(raw), ValueType::Integer(ty)) => {
                    Ok(CValue::Integer(CInteger::from_raw(raw, ty)))
                }
                (evocc_memory::AtomicValue::Pointer(value), ValueType::Pointer(ty)) => {
                    if pointer_type_is_function(self.unit.types(), ty) {
                        return Ok(CValue::FunctionPointer { value, ty });
                    }
                    let value = match value {
                        PointerValue::Null => Pointer::NULL,
                        PointerValue::Object(value) => value,
                        PointerValue::Exposed(value) => Pointer::from_address(value),
                        PointerValue::Function(_) => {
                            return Ok(CValue::FunctionPointer { value, ty })
                        }
                    };
                    Ok(CValue::Pointer { value, ty })
                }
                (value, ValueType::Floating(_)) => self.atomic_object_value(value, ty, span),
                _ => Err(InterpreterError::UnsupportedType {
                    ty: access.access_ty,
                    span,
                }),
            }
        }
    }

    pub(super) fn atomic_operand_value(
        &mut self,
        value: CValue,
        ty: ValueType,
        span: SourceSpan,
    ) -> Result<evocc_memory::AtomicValue, InterpreterError> {
        match self.cast_runtime_value(value, ty, span)? {
            CValue::Integer(value) => Ok(evocc_memory::AtomicValue::Integer(value.raw_bits())),
            CValue::Pointer { value, .. } => Ok(evocc_memory::AtomicValue::Pointer(
                PointerValue::object(value),
            )),
            CValue::FunctionPointer { value, .. } => Ok(evocc_memory::AtomicValue::Pointer(value)),
            _ => Err(InterpreterError::UninitializedValue { span }),
        }
    }

    pub(super) fn atomic_result_value(
        &self,
        value: evocc_memory::AtomicValue,
        ty: ValueType,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        match (value, ty) {
            (evocc_memory::AtomicValue::Integer(raw), ValueType::Integer(ty)) => {
                Ok(CValue::Integer(CInteger::from_raw(raw, ty)))
            }
            (evocc_memory::AtomicValue::Pointer(value), ValueType::Pointer(ty)) => {
                if pointer_type_is_function(self.unit.types(), ty) {
                    return Ok(CValue::FunctionPointer { value, ty });
                }
                let value = match value {
                    PointerValue::Null => Pointer::NULL,
                    PointerValue::Object(pointer) => pointer,
                    PointerValue::Exposed(address) => Pointer::from_address(address),
                    PointerValue::Function(_) => return Ok(CValue::FunctionPointer { value, ty }),
                };
                Ok(CValue::Pointer { value, ty })
            }
            _ => Err(InterpreterError::UnsupportedExpression {
                span,
                feature: "atomic result type disagrees with access",
            }),
        }
    }
}
