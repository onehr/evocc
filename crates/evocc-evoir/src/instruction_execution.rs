//! EvoIR instruction execution.

use super::*;

pub(super) fn charge_step(
    steps: &mut u64,
    limits: InterpreterLimits,
    block: BlockId,
) -> Result<(), InterpreterError> {
    if *steps >= limits.step_budget {
        return Err(InterpreterError::StepBudgetExceeded {
            budget: limits.step_budget,
            steps: *steps,
            block,
        });
    }
    *steps += 1;
    Ok(())
}

pub(super) fn write_value(
    values: &mut HashMap<u32, InternalValue>,
    result: &Value,
    raw_bits: u128,
) {
    values.insert(result.id, InternalValue::Bits(raw_bits));
}

pub(super) fn write_internal(
    values: &mut HashMap<u32, InternalValue>,
    result: &Value,
    value: InternalValue,
) {
    values.insert(result.id, value);
}

pub(super) fn separate_anonymous_arguments(
    function: &Function,
    arguments: &[Value],
    marks: &[ParamMark],
    incoming: &mut Vec<InternalValue>,
) -> Vec<(EvoIRType, ParamMark, InternalValue)> {
    let named = function.params.len().saturating_sub(2);
    if !function.variadic {
        return Vec::new();
    }
    arguments[named..]
        .iter()
        .zip(incoming.drain(named..arguments.len()))
        .enumerate()
        .map(|(index, (argument, value))| {
            (
                argument.ty.clone(),
                marks.get(named + index).copied().unwrap_or(ParamMark::None),
                value,
            )
        })
        .collect()
}

pub(super) fn execute_variadic_access(
    access: &VariadicAccess,
    anonymous: &[(EvoIRType, ParamMark, InternalValue)],
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let fault = |error| InterpreterError::MemoryAccess { block, error };
    let _ = read_value(values, &access.mem_in, block)?;
    let _ = read_value(values, &access.world_in, block)?;
    let list = read_pointer(values, &access.list, block)?;
    let identity = match &access.op {
        VariadicOp::Start => Some(state.argument_lists.start(anonymous.to_vec())),
        VariadicOp::End => None,
        VariadicOp::Arg(result) => {
            let identity = state.memory.argument_list(list).map_err(fault)?;
            let (ty, mark, value) = state.argument_lists.read(identity).map_err(fault)?;
            if ty != &result.ty || *mark != ParamMark::None {
                return Err(fault(MemoryFault::InvalidArgumentList));
            }
            write_internal(values, result, *value);
            Some(state.argument_lists.advance(identity).map_err(fault)?)
        }
        VariadicOp::ArgRecord { destination, shape } => {
            let identity = state.memory.argument_list(list).map_err(fault)?;
            let (_, mark, value) = state.argument_lists.read(identity).map_err(fault)?;
            if *mark != ParamMark::ByVal(*shape) {
                return Err(fault(MemoryFault::InvalidArgumentList));
            }
            let InternalValue::Pointer(source) = *value else {
                return Err(fault(MemoryFault::InvalidArgumentList));
            };
            let destination = read_pointer(values, destination, block)?;
            state.memory = state
                .memory
                .copy_bytes(
                    destination,
                    source,
                    AccessLayout::new(shape.size, shape.align),
                )
                .map_err(fault)?;
            Some(state.argument_lists.advance(identity).map_err(fault)?)
        }
    };
    state.memory = state
        .memory
        .with_argument_list(list, identity)
        .map_err(fault)?;
    let mem = state.fresh_token()?;
    let world = state.fresh_token()?;
    write_value(values, &access.mem_out, mem);
    write_value(values, &access.world_out, world);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_instruction(
    inst: &Inst,
    function: &str,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
    memory_frame: Option<FrameId>,
    memory_scope: Option<ScopeId>,
    activation: ActivationAddresses,
) -> Result<(), InterpreterError> {
    match inst {
        Inst::Vector(operation) => vector::execute(operation, block, values, state)?,
        Inst::Floating(operation) => {
            if operation.is_complex_helper() && state.compiler_rt_complex {
                return Err(InterpreterError::ComplexHelperUnavailable { block });
            }
            if operation.needs_environment() {
                state
                    .world
                    .check_fixed_fp_environment()
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            }
            floating::execute(operation, block, values)?;
        }
        Inst::StrictFloating(operation) => {
            if operation.instruction.is_complex_helper() && state.compiler_rt_complex {
                return Err(InterpreterError::ComplexHelperUnavailable { block });
            }
            strict_floating::execute(operation, block, values, state)?
        }
        Inst::VaAccess(access) => execute_variadic_access(access, &[], block, values, state)?,
        Inst::Atomic(access) => atomic::execute_atomic(access, block, values, state)?,
        Inst::TlsAddr(access) => tls::execute(access, block, values, state)?,
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            let raw = read_value(values, operand, block)?;
            let EvoIRType::Bits(width) = result.ty else {
                unreachable!("validated bit type")
            };
            // The value model is 128 bits wide (revision 1.9.61); the counts
            // are answered at the instruction's own width.
            let result_bits: u128 = match op {
                BitOp::LeadingZeros => u128::from(raw.leading_zeros() - (128 - u32::from(width))),
                BitOp::TrailingZeros => u128::from(raw.trailing_zeros().min(u32::from(width))),
                BitOp::Population => u128::from(raw.count_ones()),
                BitOp::ByteSwap => raw.swap_bytes() >> (128 - width),
            };
            write_value(values, result, result_bits);
        }
        Inst::Const(result, immediate) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            write_value(values, result, kernel.normalize(u128::from(*immediate)));
        }
        Inst::Uninitialized(result) => {
            values.insert(result.id, InternalValue::Uninitialized);
        }
        Inst::Add(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.add(left, right));
        }
        Inst::Sub(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.sub(left, right));
        }
        Inst::Mul(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.mul(left, right));
        }
        Inst::SRem(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.srem(left, right, block)?);
        }
        Inst::URem(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.urem(left, right, block)?);
        }
        Inst::Shl(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.shl(left, right));
        }
        Inst::AShr(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.ashr(left, right));
        }
        Inst::LShr(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.lshr(left, right));
        }
        Inst::And(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.normalize(left & right));
        }
        Inst::Or(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.normalize(left | right));
        }
        Inst::Xor(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(values, result, kernel.normalize(left ^ right));
        }
        Inst::SignedAddDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_add_defined(left, right)),
            );
        }
        Inst::SignedSubDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_sub_defined(left, right)),
            );
        }
        Inst::SignedMulDefined(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_mul_defined(left, right)),
            );
        }
        Inst::DivisorNonZero(result, divisor) => {
            let kernel = IntegerKernel::for_value(divisor, block)?;
            let divisor = read_value(values, divisor, block)?;
            write_value(values, result, u128::from(kernel.normalize(divisor) != 0));
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            write_value(
                values,
                result,
                u128::from(kernel.signed_div_rem_no_overflow(left, right)),
            );
        }
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let _shifted = read_value(values, shifted, block)?;
            let amount_raw = read_value(values, amount, block)?;
            let shifted_kernel = IntegerKernel::for_value(shifted, block)?;
            let amount_kernel = IntegerKernel::for_value(amount, block)?;
            write_value(
                values,
                result,
                u128::from(shifted_kernel.shift_count_defined(
                    amount_kernel,
                    amount_raw,
                    *amount_signed,
                )),
            );
        }
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let shifted_kernel = IntegerKernel::for_value(shifted, block)?;
            let amount_kernel = IntegerKernel::for_value(amount, block)?;
            let shifted = read_value(values, shifted, block)?;
            let amount = read_value(values, amount, block)?;
            write_value(
                values,
                result,
                u128::from(shifted_kernel.signed_shl_defined(
                    shifted,
                    amount_kernel,
                    amount,
                    *amount_signed,
                )),
            );
        }
        Inst::UbAssume(condition, reason) => {
            if read_value(values, condition, block)? == 0 {
                return Err(InterpreterError::UndefinedBehavior {
                    block,
                    diagnostic: reason.diagnostic(),
                    message: reason.message(),
                });
            }
        }
        Inst::SDiv(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let quotient = kernel.sdiv(left, right, block)?;
            write_value(values, result, quotient);
        }
        Inst::UDiv(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let quotient = kernel.udiv(left, right, block)?;
            write_value(values, result, quotient);
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            if matches!(lhs.ty, EvoIRType::Ptr(_)) {
                let left = read_pointer_value(values, lhs, block)?;
                let right = read_pointer_value(values, rhs, block)?;
                let left = state
                    .memory
                    .pointer_address(left)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let right = state
                    .memory
                    .pointer_address(right)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let comparison = match operation {
                    CmpOp::Eq => left == right,
                    CmpOp::Ne => left != right,
                    _ => {
                        return Err(InterpreterError::InvalidFunction {
                            message: format!("block {block} uses ordered comparison on pointers"),
                        });
                    }
                };
                write_value(values, result, u128::from(comparison));
                return Ok(());
            }
            if matches!(lhs.ty, EvoIRType::FnPtr { .. }) {
                let left = read_pointer_value(values, lhs, block)?;
                let right = read_pointer_value(values, rhs, block)?;
                let left = state
                    .memory
                    .pointer_address(left)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let right = state
                    .memory
                    .pointer_address(right)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                let comparison = match operation {
                    CmpOp::Eq => left == right,
                    CmpOp::Ne => left != right,
                    _ => {
                        return Err(InterpreterError::InvalidFunction {
                            message: format!(
                                "block {block} uses ordered comparison on function pointers"
                            ),
                        });
                    }
                };
                write_value(values, result, u128::from(comparison));
                return Ok(());
            }
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let comparison = match operation {
                CmpOp::Eq => kernel.normalize(left) == kernel.normalize(right),
                CmpOp::Ne => kernel.normalize(left) != kernel.normalize(right),
                CmpOp::SLt => kernel.signed(left) < kernel.signed(right),
                CmpOp::SLe => kernel.signed(left) <= kernel.signed(right),
                CmpOp::ULt => kernel.normalize(left) < kernel.normalize(right),
                CmpOp::ULe => kernel.normalize(left) <= kernel.normalize(right),
            };
            write_value(values, result, u128::from(comparison));
        }
        Inst::SExt(result, source, _) => {
            let source_kernel = IntegerKernel::for_value(source, block)?;
            let target_kernel = IntegerKernel::for_value(result, block)?;
            let raw = read_value(values, source, block)?;
            write_value(
                values,
                result,
                target_kernel.encode_signed(source_kernel.signed(raw)),
            );
        }
        Inst::ZExt(result, source, _) | Inst::Trunc(result, source, _) => {
            let source_kernel = IntegerKernel::for_value(source, block)?;
            let target_kernel = IntegerKernel::for_value(result, block)?;
            let raw = read_value(values, source, block)?;
            write_value(
                values,
                result,
                target_kernel.normalize(source_kernel.normalize(raw)),
            );
        }
        Inst::PointerCast { result, operand } => {
            let value = if result.ty == EvoIRType::Bits(64) {
                let pointer = match read_internal(values, operand, block)? {
                    InternalValue::Pointer(pointer) => PointerValue::object(pointer),
                    InternalValue::FunctionPointer(pointer) => pointer,
                    _ => return Err(InterpreterError::InvalidPointerRepresentation { block }),
                };
                let (memory, address) = state
                    .memory
                    .expose_pointer(pointer)
                    .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                state.memory = memory;
                InternalValue::Bits(u128::from(address))
            } else {
                // `ptr.from_int` takes a `bits<64>` (Section 3.7); the value model is 128 bits wide.
                let address = u64::try_from(read_value(values, operand, block)?)
                    .map_err(|_| InterpreterError::InvalidPointerRepresentation { block })?;
                if matches!(result.ty, EvoIRType::FnPtr { .. }) {
                    InternalValue::FunctionPointer(if address == 0 {
                        PointerValue::Null
                    } else {
                        PointerValue::Exposed(address)
                    })
                } else {
                    InternalValue::Pointer(Pointer::from_address(address))
                }
            };
            write_internal(values, result, value);
        }
        Inst::PtrNull(result) => match result.ty {
            EvoIRType::FnPtr { .. } => write_internal(
                values,
                result,
                InternalValue::FunctionPointer(PointerValue::Null),
            ),
            _ => write_internal(values, result, InternalValue::Pointer(Pointer::NULL)),
        },
        Inst::GlobalAddr { result, symbol } => {
            let pointer = state.globals.get(symbol).copied().ok_or_else(|| {
                InterpreterError::InvalidModule {
                    message: format!("global_addr refers to missing global '{symbol}'"),
                }
            })?;
            write_internal(values, result, InternalValue::Pointer(pointer));
        }
        Inst::FuncAddr { result, symbol } => {
            let identity = state.functions.get(symbol).copied().ok_or_else(|| {
                InterpreterError::InvalidModule {
                    message: format!("func_addr refers to missing function '{symbol}'"),
                }
            })?;
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(identity)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
            );
        }
        Inst::BlockAddr {
            result,
            block: target,
        } => {
            let identity = state
                .blocks
                .get(&(function.to_owned(), *target))
                .copied()
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: format!(
                        "block_addr refers to missing block {target} of function '{function}'"
                    ),
                })?;
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(identity)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
            );
        }
        // Section 3.7 (revision 1.9.72): "the interpreters give each activation
        // a fresh value for each". The identity is the memory model's opaque
        // non-object provenance: it compares equal to itself, `ptr.to_int`
        // exposes it to a distinct address that names no object, and every load
        // and store through it is `E3013`.
        Inst::ReturnAddress { result } => {
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(activation.return_address)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
            );
        }
        Inst::FrameAddress { result, depth } => {
            if *depth != 0 {
                return Err(InterpreterError::FrameAddressUnavailable {
                    block,
                    depth: *depth,
                });
            }
            write_internal(
                values,
                result,
                InternalValue::FunctionPointer(
                    PointerValue::function(activation.frame_address)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
            );
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            origin,
            ..
        }
        | Inst::DynamicAlloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            origin,
            ..
        } => {
            let layout = match inst {
                Inst::Alloca { layout, .. } => *layout,
                Inst::DynamicAlloca { size, align, .. } => {
                    AccessLayout::new(read_value(values, size, block)? as u64, *align)
                }
                _ => unreachable!("allocation instruction"),
            };
            let _ = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let memory_frame = memory_frame.ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("alloca in block {block} lacks a function memory frame"),
            })?;
            let memory_scope = memory_scope.ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("alloca in block {block} lacks a function memory scope"),
            })?;
            let (memory, pointer) = state
                .memory
                .allocate_automatic(memory_frame, memory_scope, *origin, layout)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.memory = memory;
            let memory_token = state.fresh_token()?;
            let world_token = state.fresh_token()?;
            write_internal(values, result, InternalValue::Pointer(pointer));
            write_value(values, mem_out, memory_token);
            write_value(values, world_out, world_token);
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let memory_frame = memory_frame.ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("lifetime.end in block {block} lacks a function memory frame"),
            })?;
            let pointer = read_pointer(values, pointer, block)?;
            state.memory = state
                .memory
                .end_object_lifetime(memory_frame, pointer)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            let memory_token = state.fresh_token()?;
            let world_token = state.fresh_token()?;
            write_value(values, mem_out, memory_token);
            write_value(values, world_out, world_token);
        }
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let pointer = read_pointer(values, pointer, block)?;
            state
                .memory
                .check_pointer(pointer)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            let memory_token = state.fresh_token()?;
            write_value(values, mem_out, memory_token);
        }
        Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            layout,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let pointer = read_pointer(values, pointer, block)?;
            state
                .memory
                .check_access(pointer, *layout)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            let memory_token = state.fresh_token()?;
            state.access_guards.insert(memory_token, (pointer, *layout));
            write_value(values, mem_out, memory_token);
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
            mask,
        } => {
            let memory_token = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let address = read_pointer(values, address, block)?;
            let layout = matching_guard(state, memory_token, address, block)?;
            let loaded = match (&result.ty, mask) {
                (EvoIRType::Bits(_), Some(mask)) if *volatile => {
                    let (raw, world) = state
                        .memory
                        .volatile_load_integer_bits(&state.world, address, layout, *mask)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    state.world = world;
                    InternalValue::Bits(raw)
                }
                (EvoIRType::Bits(_), Some(mask)) => InternalValue::Bits(
                    state
                        .memory
                        .load_integer_bits(address, layout, *mask)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?,
                ),
                (_, Some(_)) => {
                    return Err(InterpreterError::UnsupportedInstruction {
                        block,
                        operation: "masked load type",
                    });
                }
                (ty, None) => match ty {
                    EvoIRType::Vec(..) => {
                        let (representation, world) = state
                            .memory
                            .load_representation(&state.world, address, layout, *volatile)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.world = world;
                        state.save_vector(representation)
                    }
                    EvoIRType::Bits(_)
                    | EvoIRType::F16
                    | EvoIRType::BF16
                    | EvoIRType::F32
                    | EvoIRType::F64
                    | EvoIRType::F128 => {
                        let raw = if *volatile {
                            let (raw, world) = state
                                .memory
                                .volatile_load_integer(&state.world, address, layout)
                                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                            state.world = world;
                            raw
                        } else {
                            state
                                .memory
                                .load_integer(address, layout)
                                .map_err(|error| InterpreterError::MemoryAccess { block, error })?
                        };
                        InternalValue::Bits(raw)
                    }
                    EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => {
                        let pointer = if *volatile {
                            let (pointer, world) = state
                                .memory
                                .volatile_load_pointer_value(&state.world, address, layout)
                                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                            state.world = world;
                            pointer
                        } else {
                            state
                                .memory
                                .load_pointer_value(address, layout)
                                .map_err(|error| InterpreterError::MemoryAccess { block, error })?
                        };
                        match (&result.ty, pointer) {
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
                        }
                    }
                    _ => {
                        return Err(InterpreterError::UnsupportedInstruction {
                            block,
                            operation: "load type",
                        });
                    }
                },
            };
            let next_memory_token = state.fresh_token()?;
            let next_world_token = state.fresh_token()?;
            write_internal(values, result, loaded);
            write_value(values, mem_out, next_memory_token);
            write_value(values, world_out, next_world_token);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
            mask,
        } => {
            let memory_token = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let address = read_pointer(values, address, block)?;
            let layout = matching_guard(state, memory_token, address, block)?;
            let stored = read_internal(values, value, block)?;
            match (stored, mask) {
                (InternalValue::Bits(raw), Some(mask)) if *volatile => {
                    let (memory, world) = state
                        .memory
                        .volatile_store_integer_bits(&state.world, address, layout, raw, *mask)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    state.memory = memory;
                    state.world = world;
                }
                (InternalValue::Bits(raw), Some(mask)) => {
                    state.memory = state
                        .memory
                        .store_integer_bits(address, layout, raw, *mask)
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                }
                (InternalValue::Uninitialized, _) => {
                    return Err(InterpreterError::UninitializedRead { block });
                }
                (_, Some(_)) => {
                    return Err(InterpreterError::UnsupportedInstruction {
                        block,
                        operation: "masked store value",
                    });
                }
                (InternalValue::Vector(identity), None) => {
                    let (memory, world) = state
                        .memory
                        .store_representation(
                            &state.world,
                            address,
                            layout,
                            &state.vectors[identity],
                            *volatile,
                        )
                        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    state.memory = memory;
                    state.world = world;
                }
                (InternalValue::Bits(raw), None) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_integer(&state.world, address, layout, raw)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory = state
                            .memory
                            .store_integer(address, layout, raw)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    }
                }
                (InternalValue::Pointer(pointer), None) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_pointer(&state.world, address, layout, pointer)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory = state
                            .memory
                            .store_pointer(address, layout, pointer)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    }
                }
                (InternalValue::FunctionPointer(pointer), None) => {
                    if *volatile {
                        let (memory, world) = state
                            .memory
                            .volatile_store_pointer_value(&state.world, address, layout, pointer)
                            .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                        state.memory = memory;
                        state.world = world;
                    } else {
                        state.memory =
                            state
                                .memory
                                .store_pointer_value(address, layout, pointer)
                                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
                    }
                }
            }
            let next_memory_token = state.fresh_token()?;
            let next_world_token = state.fresh_token()?;
            write_value(values, mem_out, next_memory_token);
            write_value(values, world_out, next_world_token);
        }
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            volatile,
        } => {
            let _ = read_value(values, mem_in, block)?;
            let _ = read_value(values, world_in, block)?;
            let destination = read_pointer(values, destination, block)?;
            let source = read_pointer(values, source, block)?;
            let (memory, world) = state
                .memory
                .copy_with_volatile(&state.world, destination, source, *layout, *volatile)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            state.memory = memory;
            state.world = world;
            let next_memory_token = state.fresh_token()?;
            let next_world_token = state.fresh_token()?;
            write_value(values, mem_out, next_memory_token);
            write_value(values, world_out, next_world_token);
        }
        Inst::Asm(_) | Inst::AsmGoto { .. } => return Err(assembly_error(block)),
        Inst::Call { .. } | Inst::IndirectCall { .. } => {
            return Err(InterpreterError::UnsupportedInstruction {
                block,
                operation: "call",
            });
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => {
            let base = read_pointer(values, base, block)?;
            let raw = read_value(values, index, block)?;
            let EvoIRType::Bits(width) = index.ty else {
                return Err(InterpreterError::InvalidFunction {
                    message: format!("pointer offset index in block {block} is not bits"),
                });
            };
            let pointer = base
                .checked_scaled_offset(raw, width, *signedness, *direction, *scale)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
            write_internal(values, result, InternalValue::Pointer(pointer));
        }
        Inst::Br(..)
        | Inst::CondBr(..)
        | Inst::IndirectBr { .. }
        | Inst::Ret(..)
        | Inst::Trap(..) => {
            return Err(InterpreterError::InvalidFunction {
                message: format!("terminator appears in instructions of block {block}"),
            });
        }
    }
    Ok(())
}
