//! EvoIR function execution.

use super::*;

/// Interpret a validated function with explicit entry arguments and limits.
///
/// Each non-terminator instruction and each terminator costs one step. The
/// implementation advances an explicit block program counter, so loops and
/// backedges consume bounded host stack.
pub fn interpret_function(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    interpret_function_with_target(func, entry_args, &target, limits)
}

/// Interpret one function against an explicit target data layout.
///
/// This is the authoritative target-aware entry point. It constructs exactly
/// one memory/world state pair for the complete top-level invocation.
pub fn interpret_function_with_target(
    func: &Function,
    entry_args: &[RuntimeValue],
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let mut state = ExecutionState::for_target(target, func.alignment)?;
    state.functions.insert(func.name.clone(), 1);
    for (index, block) in func.blocks.iter().enumerate() {
        state
            .blocks
            .insert((func.name.clone(), block.id), index as u64 + 2);
    }
    state.next_activation = func.blocks.len() as u64 + 2;
    interpret_function_in_state(func, entry_args, limits, state)
}

/// One provenance identity per block, numbered after the function identities.
///
/// Section 3.7 asks only that two `block_addr` of the same block be equal and
/// of different blocks unequal; a dense numbering in declaration order is the
/// deterministic way to say that, and starting past the function identities
/// keeps a block address from ever equalling a function address.
pub(super) fn block_identities(
    module: &Module,
    functions: usize,
) -> Result<HashMap<(String, BlockId), u64>, InterpreterError> {
    let mut identities = HashMap::new();
    let mut next = u64::try_from(functions)
        .ok()
        .and_then(|count| count.checked_add(1))
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "block provenance identity exhausted".to_owned(),
        })?;
    for function in &module.functions {
        for block in &function.blocks {
            identities.insert((function.name.clone(), block.id), next);
            next = next
                .checked_add(1)
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "block provenance identity exhausted".to_owned(),
                })?;
        }
    }
    Ok(identities)
}

pub(super) fn interpret_function_in_state(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    mut state: ExecutionState,
) -> Result<InterpreterOutcome, InterpreterError> {
    validate_ssa(func).map_err(|message| InterpreterError::InvalidFunction { message })?;
    let entry = func
        .blocks
        .first()
        .ok_or_else(|| InterpreterError::InvalidFunction {
            message: format!("function '{}' has no entry block", func.name),
        })?;
    if entry_args.len() != entry.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: entry.args.len(),
            actual: entry_args.len(),
        });
    }

    let blocks: HashMap<BlockId, &Block> =
        func.blocks.iter().map(|block| (block.id, block)).collect();
    let mut values = HashMap::new();
    let mut incoming = entry_args
        .iter()
        .zip(&entry.args)
        .enumerate()
        .map(|(index, (value, argument))| {
            normalize_runtime_input(value, &argument.value.ty, index, entry.id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut program_counter = entry.id;
    let mut steps = 0;
    let (memory_frame, memory_scope) = begin_function_memory(&mut state, func, entry.id)?;
    let activation = state.begin_activation()?;

    loop {
        let block =
            blocks
                .get(&program_counter)
                .copied()
                .ok_or(InterpreterError::MissingBlock {
                    block: program_counter,
                })?;
        if incoming.len() != block.args.len() {
            return Err(InterpreterError::EntryArgumentCount {
                expected: block.args.len(),
                actual: incoming.len(),
            });
        }
        for (argument, runtime_value) in block.args.iter().zip(incoming.drain(..)) {
            values.insert(argument.value.id, runtime_value);
        }

        for inst in &block.insts {
            charge_step(&mut steps, limits, block.id)?;
            execute_instruction(
                inst,
                &func.name,
                block.id,
                &mut values,
                &mut state,
                memory_frame,
                memory_scope,
                activation,
            )?;
        }

        charge_step(&mut steps, limits, block.id)?;
        match &block.terminator {
            Inst::IndirectBr { target, edges } => {
                let edge =
                    select_indirect_edge(&state, &func.name, block.id, target, edges, &values)?;
                incoming = copy_values(&edge.arguments, block.id, &values)?;
                program_counter = edge.target;
            }
            Inst::Br(target, arguments) => {
                incoming = copy_values(arguments, block.id, &values)?;
                program_counter = *target;
            }
            Inst::CondBr(condition, true_block, true_arguments, false_block, false_arguments) => {
                let condition_value = read_value(&values, condition, block.id)?;
                if condition_value != 0 {
                    incoming = copy_values(true_arguments, block.id, &values)?;
                    program_counter = *true_block;
                } else {
                    incoming = copy_values(false_arguments, block.id, &values)?;
                    program_counter = *false_block;
                }
            }
            Inst::Ret(returned) => {
                let internal = copy_values(returned, block.id, &values)?;
                if let Some(memory_frame) = memory_frame {
                    state.memory = state.memory.end_frame(memory_frame).map_err(|error| {
                        InterpreterError::MemoryAccess {
                            block: block.id,
                            error,
                        }
                    })?;
                }
                let return_values = public_return_values(
                    internal,
                    returned.iter().map(|value| value.ty.clone()).collect(),
                    block.id,
                )?;
                return Ok(InterpreterOutcome {
                    return_values,
                    steps,
                    memory: state.memory,
                    world: state.world,
                });
            }
            Inst::AsmGoto { .. } => return Err(assembly_error(block.id)),
            Inst::Trap(reason) => return Err(trap_error(*reason, block.id)),
            _ => {
                return Err(InterpreterError::InvalidFunction {
                    message: format!("block {} has a non-terminator", block.id),
                });
            }
        }
    }
}

/// Explicit compatibility spelling for [`interpret_function`].
pub fn eval_function(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_function(func, entry_args, limits)
}
