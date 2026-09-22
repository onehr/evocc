use crate::*;
pub use evocc_memory::{AtomicOrder, AtomicRmwOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicOp {
    Load(Value),
    Store(Value),
    Rmw {
        result: Value,
        operation: AtomicRmwOp,
        value: Value,
        expected: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicAccess {
    pub op: AtomicOp,
    pub address: Value,
    pub order: AtomicOrder,
    pub volatile: bool,
    pub mem_in: Value,
    pub world_in: Value,
    pub mem_out: Value,
    pub world_out: Value,
}

impl AtomicAccess {
    pub fn is_load(&self) -> bool {
        matches!(self.op, AtomicOp::Load(_))
    }
    pub fn value(&self) -> &Value {
        match &self.op {
            AtomicOp::Load(value) | AtomicOp::Store(value) => value,
            AtomicOp::Rmw { result, .. } => result,
        }
    }
    pub fn results(&self) -> Vec<&Value> {
        let mut values = match &self.op {
            AtomicOp::Load(value) | AtomicOp::Rmw { result: value, .. } => vec![value],
            _ => vec![],
        };
        values.extend([&self.mem_out, &self.world_out]);
        values
    }
    pub fn results_mut(&mut self) -> Vec<&mut Value> {
        let mut values = match &mut self.op {
            AtomicOp::Load(value) | AtomicOp::Rmw { result: value, .. } => vec![value],
            _ => vec![],
        };
        values.extend([&mut self.mem_out, &mut self.world_out]);
        values
    }
    pub fn values_mut(&mut self) -> Vec<&mut Value> {
        let mut values = match &mut self.op {
            AtomicOp::Load(value) | AtomicOp::Store(value) => vec![value],
            AtomicOp::Rmw {
                result,
                value,
                expected,
                ..
            } => {
                let mut values = vec![result, value];
                values.extend(expected.iter_mut());
                values
            }
        };
        values.extend([
            &mut self.address,
            &mut self.mem_in,
            &mut self.world_in,
            &mut self.mem_out,
            &mut self.world_out,
        ]);
        values
    }
    pub fn operands(&self) -> Vec<&Value> {
        let mut values = vec![&self.mem_in, &self.world_in, &self.address];
        if let AtomicOp::Store(value) = &self.op {
            values.push(value);
        }
        if let AtomicOp::Rmw {
            value, expected, ..
        } = &self.op
        {
            values.push(value);
            values.extend(expected.iter());
        }
        values
    }
    pub fn operands_mut(&mut self) -> Vec<&mut Value> {
        let mut values = vec![&mut self.mem_in, &mut self.world_in, &mut self.address];
        match &mut self.op {
            AtomicOp::Load(_) => {}
            AtomicOp::Store(value) => values.push(value),
            AtomicOp::Rmw {
                value, expected, ..
            } => {
                values.push(value);
                values.extend(expected.iter_mut());
            }
        }
        values
    }
    pub fn layout(&self) -> Result<AccessLayout, String> {
        let size = match self.value().ty {
            EvoIRType::Bits(1 | 8) => 1,
            EvoIRType::Bits(16) => 2,
            EvoIRType::Bits(32) => 4,
            EvoIRType::Bits(64) | EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => 8,
            _ => return Err("E3062: invalid-atomic-access: type".into()),
        };
        Ok(AccessLayout::new(size, size as u32))
    }
    pub fn validate(&self) -> Result<(), String> {
        self.layout()?;
        if let AtomicOp::Rmw {
            result,
            operation,
            value,
            expected,
        } = &self.op
        {
            if result.ty == EvoIRType::Bits(1)
                || value.ty != result.ty
                || expected.as_ref().is_some_and(|v| v.ty != result.ty)
                || (*operation == AtomicRmwOp::CompareExchange) != expected.is_some()
                || (matches!(result.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
                    && !matches!(
                        operation,
                        AtomicRmwOp::Exchange | AtomicRmwOp::CompareExchange
                    ))
            {
                return Err("E3062: invalid-atomic-access: RMW operands".into());
            }
        } else if !self.order.valid_for(self.is_load()) {
            return Err("E3062: invalid-atomic-access: order".into());
        }
        expect_pointer(&self.address.ty, "atomic address", 0)?;
        expect_day7_effect_tokens(
            &self.mem_in,
            &self.world_in,
            &self.mem_out,
            &self.world_out,
            "atomic access",
            0,
        )
    }
}

pub(crate) fn execute_atomic(
    access: &AtomicAccess,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let token = read_value(values, &access.mem_in, block)?;
    let _ = read_value(values, &access.world_in, block)?;
    let address = read_pointer(values, &access.address, block)?;
    let layout = matching_guard(state, token, address, block)?;
    match &access.op {
        AtomicOp::Rmw {
            result,
            operation,
            value,
            expected,
        } => {
            let value = atomic_operand(values, value, block)?;
            let expected = expected
                .as_ref()
                .map(|value| atomic_operand(values, value, block))
                .transpose()?;
            let (memory, world, outcome) = state
                .memory
                .atomic_rmw(
                    &state.world,
                    address,
                    layout,
                    access.order,
                    access.volatile,
                    *operation,
                    value,
                    expected,
                )
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.memory = memory;
            state.world = world;
            let value = match outcome.old {
                evocc_memory::AtomicValue::Integer(raw) => InternalValue::Bits(raw),
                evocc_memory::AtomicValue::Pointer(pointer) => match (&result.ty, pointer) {
                    (EvoIRType::Ptr(_), PointerValue::Null) => {
                        InternalValue::Pointer(Pointer::NULL)
                    }
                    (EvoIRType::Ptr(_), PointerValue::Object(pointer)) => {
                        InternalValue::Pointer(pointer)
                    }
                    (EvoIRType::Ptr(_), PointerValue::Exposed(address)) => {
                        InternalValue::Pointer(Pointer::from_address(address))
                    }
                    _ => InternalValue::FunctionPointer(pointer),
                },
            };
            write_internal(values, result, value);
        }
        AtomicOp::Load(result) => {
            let (value, world) = state
                .memory
                .atomic_load(
                    &state.world,
                    address,
                    layout,
                    access.order,
                    access.volatile,
                    matches!(result.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }),
                )
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.world = world;
            let value = match value {
                evocc_memory::AtomicValue::Integer(raw) => InternalValue::Bits(raw),
                evocc_memory::AtomicValue::Pointer(value) => match (&result.ty, value) {
                    (EvoIRType::Ptr(_), PointerValue::Null) => {
                        InternalValue::Pointer(Pointer::NULL)
                    }
                    (EvoIRType::Ptr(_), PointerValue::Object(pointer)) => {
                        InternalValue::Pointer(pointer)
                    }
                    (EvoIRType::Ptr(_), PointerValue::Exposed(value)) => {
                        InternalValue::Pointer(Pointer::from_address(value))
                    }
                    _ => InternalValue::FunctionPointer(value),
                },
            };
            write_internal(values, result, value);
        }
        AtomicOp::Store(value) => {
            let value = match read_internal(values, value, block)? {
                InternalValue::Vector(_) => {
                    return Err(InterpreterError::InvalidFunction {
                        message: "vector value at scalar atomic access".into(),
                    })
                }
                InternalValue::Bits(raw) => evocc_memory::AtomicValue::Integer(raw),
                InternalValue::Pointer(pointer) => {
                    evocc_memory::AtomicValue::Pointer(PointerValue::object(pointer))
                }
                InternalValue::FunctionPointer(value) => evocc_memory::AtomicValue::Pointer(value),
                InternalValue::Uninitialized => {
                    return Err(InterpreterError::UninitializedRead { block })
                }
            };
            let (memory, world) = state
                .memory
                .atomic_store(
                    &state.world,
                    address,
                    layout,
                    access.order,
                    access.volatile,
                    value,
                )
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.memory = memory;
            state.world = world;
        }
    }
    let memory_token = state.fresh_token()?;
    let world_token = state.fresh_token()?;
    write_value(values, &access.mem_out, memory_token);
    write_value(values, &access.world_out, world_token);
    Ok(())
}

fn atomic_operand(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<evocc_memory::AtomicValue, InterpreterError> {
    match read_internal(values, value, block)? {
        InternalValue::Bits(raw) => Ok(evocc_memory::AtomicValue::Integer(raw)),
        InternalValue::Pointer(pointer) => Ok(evocc_memory::AtomicValue::Pointer(
            PointerValue::object(pointer),
        )),
        InternalValue::FunctionPointer(pointer) => Ok(evocc_memory::AtomicValue::Pointer(pointer)),
        InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
        _ => Err(InterpreterError::InvalidFunction {
            message: "invalid scalar atomic operand".into(),
        }),
    }
}
