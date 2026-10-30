//! EvoIR runtime values.

use super::*;

pub(super) fn select_indirect_edge<'a>(
    state: &ExecutionState,
    function: &str,
    block: BlockId,
    target: &Value,
    edges: &'a [Edge],
    values: &HashMap<u32, InternalValue>,
) -> Result<&'a Edge, InterpreterError> {
    let error = || InterpreterError::InvalidIndirectBranchTarget { block };
    let pointer = match read_internal(values, target, block)? {
        InternalValue::FunctionPointer(pointer) => pointer,
        InternalValue::Pointer(pointer) => PointerValue::Exposed(
            state
                .memory
                .pointer_address(PointerValue::object(pointer))
                .map_err(|_| error())?,
        ),
        InternalValue::Uninitialized => {
            return Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::Bits(_) | InternalValue::Vector(_) => return Err(error()),
    };
    let pointer = state
        .memory
        .resolve_function_pointer(pointer)
        .map_err(|_| error())?;
    let PointerValue::Function(identity) = pointer else {
        return Err(error());
    };
    edges
        .iter()
        .find(|edge| state.blocks.get(&(function.to_owned(), edge.target)) == Some(&identity))
        .ok_or_else(error)
}

pub(super) fn read_value(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<u128, InterpreterError> {
    match values
        .get(&value.id)
        .copied()
        .ok_or(InterpreterError::MissingValue {
            block,
            value: value.id,
        })? {
        InternalValue::Bits(raw_bits) => Ok(raw_bits),
        InternalValue::Pointer(_)
        | InternalValue::FunctionPointer(_)
        | InternalValue::Vector(_) => Err(InterpreterError::InvalidFunction {
            message: format!(
                "block {block} consumes pointer v{} as integer bits",
                value.id
            ),
        }),
        InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
    }
}

pub(super) fn read_internal(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<InternalValue, InterpreterError> {
    values
        .get(&value.id)
        .copied()
        .ok_or(InterpreterError::MissingValue {
            block,
            value: value.id,
        })
}

pub(super) fn read_pointer(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<Pointer, InterpreterError> {
    match read_internal(values, value, block)? {
        InternalValue::Pointer(pointer) => Ok(pointer),
        InternalValue::Uninitialized => {
            Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::FunctionPointer(_) => {
            Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::Bits(_) | InternalValue::Vector(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes non-pointer v{} as pointer",
                    value.id
                ),
            })
        }
    }
}

pub(super) fn read_pointer_value(
    values: &HashMap<u32, InternalValue>,
    value: &Value,
    block: BlockId,
) -> Result<PointerValue, InterpreterError> {
    match read_internal(values, value, block)? {
        InternalValue::FunctionPointer(pointer) => Ok(pointer),
        InternalValue::Pointer(pointer) => Ok(PointerValue::object(pointer)),
        InternalValue::Uninitialized => {
            Err(InterpreterError::InvalidPointerRepresentation { block })
        }
        InternalValue::Bits(_) | InternalValue::Vector(_) => {
            Err(InterpreterError::InvalidFunction {
                message: format!(
                    "block {block} consumes non-pointer v{} as a pointer value",
                    value.id
                ),
            })
        }
    }
}

pub(super) fn matching_guard(
    state: &ExecutionState,
    memory_token: u128,
    pointer: Pointer,
    block: BlockId,
) -> Result<AccessLayout, InterpreterError> {
    let (guarded_pointer, layout) =
        state
            .access_guards
            .get(&memory_token)
            .copied()
            .ok_or_else(|| InterpreterError::InvalidModule {
                message: format!(
                    "block {block} load/store consumes memory token without access_in_bounds"
                ),
            })?;
    if guarded_pointer != pointer {
        return Err(InterpreterError::InvalidModule {
            message: format!("block {block} load/store pointer does not match its access guard"),
        });
    }
    Ok(layout)
}

pub(super) fn copy_values(
    arguments: &[Value],
    block: BlockId,
    values: &HashMap<u32, InternalValue>,
) -> Result<Vec<InternalValue>, InterpreterError> {
    arguments
        .iter()
        .map(|argument| {
            values
                .get(&argument.id)
                .copied()
                .ok_or(InterpreterError::MissingValue {
                    block,
                    value: argument.id,
                })
        })
        .collect()
}

pub(super) fn read_call_values(
    arguments: &[Value],
    block: BlockId,
    values: &HashMap<u32, InternalValue>,
) -> Result<Vec<InternalValue>, InterpreterError> {
    arguments
        .iter()
        .map(|argument| match read_internal(values, argument, block)? {
            InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
            value => Ok(value),
        })
        .collect()
}

pub(super) fn normalize_runtime_input(
    value: &RuntimeValue,
    expected: &EvoIRType,
    index: usize,
    block: BlockId,
) -> Result<InternalValue, InterpreterError> {
    validate_value_type(&value.ty)
        .map_err(|message| InterpreterError::InvalidEntryArgument { index, message })?;
    if &value.ty != expected {
        return Err(InterpreterError::EntryArgumentType {
            index,
            expected: Box::new(expected.clone()),
            actual: Box::new(value.ty.clone()),
        });
    }
    match expected {
        EvoIRType::F16 | EvoIRType::BF16 | EvoIRType::F32 | EvoIRType::F64 | EvoIRType::F128 => {
            let kind = floating_kind(expected).unwrap();
            evocc_types::floating::FloatBits::from_bits(kind, value.value()).ok_or_else(|| {
                InterpreterError::InvalidEntryArgument {
                    index,
                    message: "floating representation exceeds format width".into(),
                }
            })?;
            Ok(InternalValue::Bits(value.value()))
        }
        EvoIRType::Bits(_) => Ok(InternalValue::Bits(
            IntegerKernel::for_type(expected, block)?.normalize(value.value()),
        )),
        EvoIRType::Mem(_) | EvoIRType::World => Ok(InternalValue::Bits(value.value())),
        _ => Err(InterpreterError::UnsupportedRuntimeType {
            index,
            ty: expected.clone(),
        }),
    }
}
