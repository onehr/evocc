//! EvoIR module execution.

use super::*;

/// Interpret one entry function in a validated module.
///
/// Function calls push explicit heap-backed frames. All instructions and
/// terminators in every frame debit the same step budget.
pub fn interpret_module(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    interpret_module_with_target(module, entry_name, entry_args, &target, limits)
}

/// Interpret one module entry against an explicit target data layout.
///
/// The state is created once here. Explicit call frames share the single state
/// owned by the module execution core; frames never construct or clone it.
pub fn interpret_module_with_target(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_module_with_fp_environment(
        module,
        entry_name,
        entry_args,
        target,
        limits,
        evocc_types::floating::FpEnvironment::default(),
    )
}

/// Explicit caller environment for one logical execution. Existing entry
/// points retain the default environment; recursive calls share this snapshot.
pub fn interpret_module_with_fp_environment(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    target: &TargetDataLayout,
    limits: InterpreterLimits,
    environment: evocc_types::floating::FpEnvironment,
) -> Result<InterpreterOutcome, InterpreterError> {
    let alignment = module
        .functions
        .iter()
        .map(|function| function.alignment)
        .max()
        .unwrap_or(4);
    let mut state = ExecutionState::for_target(target, alignment)?;
    state.world = state.world.with_fp_environment(environment);
    interpret_module_in_state(module, entry_name, entry_args, limits, state)
}

pub(super) fn interpret_module_in_state(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    state: ExecutionState,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_module_execution(
        module, entry_name, entry_args, limits, state, false, &mut None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn interpret_module_execution(
    module: &Module,
    entry_name: &str,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
    mut state: ExecutionState,
    program: bool,
    site: &mut Option<ExecutionSite>,
) -> Result<InterpreterOutcome, InterpreterError> {
    validate_module(module).map_err(|message| InterpreterError::InvalidModule { message })?;
    let mut function_indices: HashMap<&str, usize> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect();
    for alias in &module.aliases {
        if let Some(index) = function_indices.get(alias.target.as_str()).copied() {
            function_indices.insert(alias.name.as_str(), index);
        }
    }
    state.functions = module
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .chain(
            module
                .declares
                .iter()
                .map(|function| function.name.as_str()),
        )
        .enumerate()
        .map(|(index, name)| {
            let identity = u64::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(1))
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "function provenance identity exhausted".to_owned(),
                })?;
            Ok((name.to_owned(), identity))
        })
        .collect::<Result<_, _>>()?;
    state.blocks = block_identities(module, state.functions.len())?;
    // Section 3.7: an activation address is fresh, and a fresh identity is one
    // no function and no block already holds.
    state.next_activation = u64::try_from(state.functions.len() + state.blocks.len())
        .ok()
        .and_then(|used| used.checked_add(1))
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "activation address identity exhausted".to_owned(),
        })?;
    for alias in &module.aliases {
        if let Some(identity) = state.functions.get(&alias.target).copied() {
            state.functions.insert(alias.name.clone(), identity);
        }
    }
    initialize_globals(module, &mut state)?;
    let entry_index = function_indices.get(entry_name).copied().ok_or_else(|| {
        InterpreterError::MissingEntryFunction {
            name: entry_name.to_owned(),
        }
    })?;
    let entry_function = &module.functions[entry_index];
    let entry_block =
        entry_function
            .blocks
            .first()
            .ok_or_else(|| InterpreterError::InvalidModule {
                message: format!("function '{}' has no entry block", entry_function.name),
            })?;
    if entry_args.len() != entry_block.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: entry_block.args.len(),
            actual: entry_args.len(),
        });
    }
    let incoming = entry_args
        .iter()
        .zip(&entry_block.args)
        .enumerate()
        .map(|(index, (value, argument))| {
            normalize_runtime_input(value, &argument.value.ty, index, entry_block.id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut roots = lifecycle::program_entries(module, entry_index, program);
    let mut root_kind = lifecycle::EntryKind::Main;
    let mut program_result = Vec::new();
    let mut frames = Vec::new();
    let mut steps = 0;

    loop {
        if frames.is_empty() {
            let Some((index, kind)) = roots.pop_front() else {
                return Ok(InterpreterOutcome {
                    return_values: program_result,
                    steps,
                    memory: state.memory,
                    world: state.world,
                });
            };
            root_kind = kind;
            *site = None;
            let function = &module.functions[index];
            let block = function.blocks[0].id;
            let values = if kind == lifecycle::EntryKind::Main {
                incoming.clone()
            } else {
                vec![InternalValue::Bits(0), InternalValue::Bits(0)]
            };
            let (frame, scope) = begin_function_memory(&mut state, function, block)?;
            let activation = state.begin_activation()?;
            frames.push(ModuleFrame::new(
                index,
                block,
                values,
                Vec::new(),
                None,
                frame,
                scope,
                activation,
            ));
        }
        let frame_index =
            frames
                .len()
                .checked_sub(1)
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "interpreter frame stack became empty".to_owned(),
                })?;
        let frame = &frames[frame_index];
        *site = Some(ExecutionSite {
            function: frame.function_index,
            block: frame.block,
            instruction: frame.next_instruction,
        });
        if !frames[frame_index].entered {
            enter_module_frame(module, &mut frames[frame_index])?;
        }
        let action = module_frame_action(module, &frames[frame_index])?;
        let block_id = frames[frame_index].block;
        charge_step(&mut steps, limits, block_id)?;

        match action {
            FrameAction::Instruction(instruction) => {
                frames[frame_index].next_instruction += 1;
                // A second return needs an external jump such as `longjmp`,
                // which no interpreter executes. A returns-twice call to a
                // module function therefore returns once here, as a call.
                if let Inst::Call {
                    results,
                    callee,
                    args,
                    arg_marks,
                    returns_twice: _,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                {
                    let mut call_arguments =
                        read_call_values(&args, block_id, &frames[frame_index].values)?;
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &mem_in,
                        block_id,
                    )?);
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &world_in,
                        block_id,
                    )?);
                    if module
                        .declares
                        .iter()
                        .any(|declaration| declaration.name == callee)
                    {
                        if program
                            && matches!(callee.as_str(), "exit" | "_Exit" | "_exit" | "quick_exit")
                        {
                            let status =
                                args.first()
                                    .ok_or_else(|| InterpreterError::InvalidModule {
                                        message: "termination needs an exit status".into(),
                                    })?;
                            let code = read_value(&frames[frame_index].values, status, block_id)?;
                            program_result = entry_function
                                .ret_tys
                                .iter()
                                .map(|ty| {
                                    RuntimeValue::wide(
                                        ty.clone(),
                                        if matches!(ty, EvoIRType::Bits(_)) {
                                            code
                                        } else {
                                            0
                                        },
                                    )
                                })
                                .collect();
                            roots.retain(|(_, kind)| {
                                callee == "exit" && *kind == lifecycle::EntryKind::Finalizer
                            });
                            for frame in frames.drain(..).rev() {
                                if let Some(frame) = frame.memory_frame {
                                    state.memory =
                                        state.memory.end_frame(frame).map_err(|error| {
                                            InterpreterError::MemoryAccess {
                                                block: block_id,
                                                error,
                                            }
                                        })?;
                                }
                            }
                            continue;
                        }
                        let returned = execute_external_call(
                            &mut state,
                            &callee,
                            call_arguments,
                            &results,
                            block_id,
                        )?;
                        bind_call_return(
                            &mut frames,
                            CallReturnBinding {
                                results,
                                mem_out,
                                world_out,
                            },
                            returned,
                        )?;
                        continue;
                    }
                    let callee_index =
                        function_indices
                            .get(callee.as_str())
                            .copied()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: format!("call targets missing function '{callee}'"),
                            })?;
                    let callee_entry =
                        module.functions[callee_index]
                            .blocks
                            .first()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: format!("callee '{callee}' has no entry block"),
                            })?;
                    let (memory_frame, memory_scope) = begin_function_memory(
                        &mut state,
                        &module.functions[callee_index],
                        block_id,
                    )?;
                    let anonymous = separate_anonymous_arguments(
                        &module.functions[callee_index],
                        &args,
                        &arg_marks,
                        &mut call_arguments,
                    );
                    let callee_activation = state.begin_activation()?;
                    frames.push(ModuleFrame::new(
                        callee_index,
                        callee_entry.id,
                        call_arguments,
                        anonymous,
                        Some(CallReturnBinding {
                            results,
                            mem_out,
                            world_out,
                        }),
                        memory_frame,
                        memory_scope,
                        callee_activation,
                    ));
                } else if let Inst::IndirectCall {
                    results,
                    callee,
                    args,
                    arg_marks,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                {
                    let callee_value =
                        read_internal(&frames[frame_index].values, &callee, block_id)?;
                    let callee_value = if let InternalValue::FunctionPointer(pointer) = callee_value
                    {
                        let pointer = if let PointerValue::Exposed(address) = pointer {
                            state
                                .functions
                                .values()
                                .copied()
                                .map(PointerValue::Function)
                                .find(|candidate| {
                                    state.memory.pointer_address(*candidate) == Ok(address)
                                })
                                .unwrap_or(pointer)
                        } else {
                            pointer
                        };
                        InternalValue::FunctionPointer(
                            state
                                .memory
                                .resolve_function_pointer(pointer)
                                .map_err(|_| InterpreterError::CallThroughNonFunctionPointer {
                                    block: block_id,
                                })?,
                        )
                    } else {
                        callee_value
                    };
                    let identity = match callee_value {
                        InternalValue::FunctionPointer(PointerValue::Function(identity)) => {
                            identity
                        }
                        InternalValue::FunctionPointer(PointerValue::Null) => {
                            return Err(InterpreterError::CallThroughNullFunctionPointer {
                                block: block_id,
                            });
                        }
                        InternalValue::FunctionPointer(PointerValue::Object(_))
                        | InternalValue::FunctionPointer(PointerValue::Exposed(_))
                        | InternalValue::Pointer(_)
                        | InternalValue::Vector(_)
                        | InternalValue::Bits(_) => {
                            return Err(InterpreterError::CallThroughNonFunctionPointer {
                                block: block_id,
                            });
                        }
                        InternalValue::Uninitialized => {
                            return Err(InterpreterError::InvalidPointerRepresentation {
                                block: block_id,
                            });
                        }
                    };
                    let identity_index = identity
                        .checked_sub(1)
                        .and_then(|identity| usize::try_from(identity).ok());
                    if let Some(declaration) = identity_index
                        .and_then(|index| index.checked_sub(module.functions.len()))
                        .and_then(|index| module.declares.get(index))
                    {
                        if function_pointer_type(&declaration.signature())
                            .map_err(|message| InterpreterError::InvalidModule { message })?
                            != callee.ty
                        {
                            return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                                block: block_id,
                            });
                        }
                        let mut arguments =
                            read_call_values(&args, block_id, &frames[frame_index].values)?;
                        arguments.push(read_internal(
                            &frames[frame_index].values,
                            &mem_in,
                            block_id,
                        )?);
                        arguments.push(read_internal(
                            &frames[frame_index].values,
                            &world_in,
                            block_id,
                        )?);
                        let returned = execute_external_call(
                            &mut state,
                            &declaration.name,
                            arguments,
                            &results,
                            block_id,
                        )?;
                        bind_call_return(
                            &mut frames,
                            CallReturnBinding {
                                results,
                                mem_out,
                                world_out,
                            },
                            returned,
                        )?;
                        continue;
                    }
                    let callee_index = identity
                        .checked_sub(1)
                        .and_then(|identity| usize::try_from(identity).ok())
                        .filter(|index| *index < module.functions.len())
                        .ok_or(InterpreterError::CallThroughNonFunctionPointer {
                            block: block_id,
                        })?;
                    let dynamic_type = function_pointer_type(&module.functions[callee_index])
                        .map_err(|message| InterpreterError::InvalidModule { message })?;
                    if dynamic_type != callee.ty {
                        return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                            block: block_id,
                        });
                    }
                    let mut call_arguments =
                        read_call_values(&args, block_id, &frames[frame_index].values)?;
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &mem_in,
                        block_id,
                    )?);
                    call_arguments.push(read_internal(
                        &frames[frame_index].values,
                        &world_in,
                        block_id,
                    )?);
                    let callee_entry =
                        module.functions[callee_index]
                            .blocks
                            .first()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: format!(
                                    "indirect callee '{}' has no entry block",
                                    module.functions[callee_index].name
                                ),
                            })?;
                    let (memory_frame, memory_scope) = begin_function_memory(
                        &mut state,
                        &module.functions[callee_index],
                        block_id,
                    )?;
                    let anonymous = separate_anonymous_arguments(
                        &module.functions[callee_index],
                        &args,
                        &arg_marks,
                        &mut call_arguments,
                    );
                    let callee_activation = state.begin_activation()?;
                    frames.push(ModuleFrame::new(
                        callee_index,
                        callee_entry.id,
                        call_arguments,
                        anonymous,
                        Some(CallReturnBinding {
                            results,
                            mem_out,
                            world_out,
                        }),
                        memory_frame,
                        memory_scope,
                        callee_activation,
                    ));
                } else if let Inst::VaAccess(access) = &instruction {
                    let frame = &mut frames[frame_index];
                    execute_variadic_access(
                        access,
                        &frame.anonymous,
                        block_id,
                        &mut frame.values,
                        &mut state,
                    )?;
                } else {
                    let memory_frame = frames[frame_index].memory_frame;
                    let memory_scope = frames[frame_index].memory_scope;
                    let activation = frames[frame_index].activation;
                    let name = module.functions[frames[frame_index].function_index]
                        .name
                        .clone();
                    execute_instruction(
                        &instruction,
                        &name,
                        block_id,
                        &mut frames[frame_index].values,
                        &mut state,
                        memory_frame,
                        memory_scope,
                        activation,
                    )?;
                }
            }
            FrameAction::Terminator(terminator) => match terminator {
                Inst::IndirectBr { target, edges } => {
                    let function = &module.functions[frames[frame_index].function_index].name;
                    let edge = select_indirect_edge(
                        &state,
                        function,
                        block_id,
                        &target,
                        &edges,
                        &frames[frame_index].values,
                    )?;
                    let incoming =
                        copy_values(&edge.arguments, block_id, &frames[frame_index].values)?;
                    set_module_frame_block(&mut frames[frame_index], edge.target, incoming);
                }
                Inst::Br(target, arguments) => {
                    let incoming = copy_values(&arguments, block_id, &frames[frame_index].values)?;
                    set_module_frame_block(&mut frames[frame_index], target, incoming);
                }
                Inst::CondBr(
                    condition,
                    true_block,
                    true_arguments,
                    false_block,
                    false_arguments,
                ) => {
                    let condition = read_value(&frames[frame_index].values, &condition, block_id)?;
                    let (target, arguments) = if condition != 0 {
                        (true_block, true_arguments)
                    } else {
                        (false_block, false_arguments)
                    };
                    let incoming = copy_values(&arguments, block_id, &frames[frame_index].values)?;
                    set_module_frame_block(&mut frames[frame_index], target, incoming);
                }
                Inst::Ret(returned) => {
                    let returned_values =
                        copy_values(&returned, block_id, &frames[frame_index].values)?;
                    // A callee may fall off its end and return poison. Binding
                    // that result is transport, not a read: a discarding caller
                    // must still observe the callee's effects without a trap.
                    let returned_types = module.functions[frames[frame_index].function_index]
                        .ret_tys
                        .clone();
                    let completed =
                        frames
                            .pop()
                            .ok_or_else(|| InterpreterError::InvalidModule {
                                message: "return observed with no active frame".to_owned(),
                            })?;
                    if let Some(memory_frame) = completed.memory_frame {
                        state.memory = state.memory.end_frame(memory_frame).map_err(|error| {
                            InterpreterError::MemoryAccess {
                                block: block_id,
                                error,
                            }
                        })?;
                    }
                    if let Some(binding) = completed.return_binding {
                        bind_call_return(&mut frames, binding, returned_values)?;
                    } else {
                        if root_kind == lifecycle::EntryKind::Main {
                            program_result =
                                public_return_values(returned_values, returned_types, block_id)?;
                        }
                    }
                }
                Inst::AsmGoto { .. } => return Err(assembly_error(block_id)),
                Inst::Trap(reason) => return Err(trap_error(reason, block_id)),
                _ => {
                    return Err(InterpreterError::InvalidModule {
                        message: format!("block {block_id} has an invalid terminator"),
                    });
                }
            },
        }
    }
}

pub(super) fn execute_external_call(
    state: &mut ExecutionState,
    name: &str,
    mut arguments: Vec<InternalValue>,
    results: &[Value],
    block: BlockId,
) -> Result<Vec<InternalValue>, InterpreterError> {
    let _world = arguments
        .pop()
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "external call lacks world".into(),
        })?;
    let _memory = arguments
        .pop()
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "external call lacks memory".into(),
        })?;
    let arguments =
        arguments
            .into_iter()
            .map(|value| match value {
                // The library boundary of 10.1.14 takes C `int` and `size_t`
                // arguments only; a wider value cannot name a library argument.
                InternalValue::Bits(value) => u64::try_from(value)
                    .map(LibraryValue::Integer)
                    .map_err(|_| InterpreterError::MemoryAccess {
                        block,
                        error: MemoryFault::ExternalFunction { name: name.into() },
                    }),
                InternalValue::Pointer(value) => Ok(LibraryValue::Pointer(value)),
                _ => Err(InterpreterError::MemoryAccess {
                    block,
                    error: MemoryFault::ExternalFunction { name: name.into() },
                }),
            })
            .collect::<Result<Vec<_>, _>>()?;
    let (next, world, result) = state
        .memory
        .call_library_with_world(
            &state.world,
            evocc_memory::FenvRuntime::Aarch64Glibc,
            name,
            &arguments,
        )
        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
    state.memory = next;
    state.world = world;
    let result = match result {
        LibraryValue::Integer(value) => match results {
            [result @ Value {
                ty: EvoIRType::Bits(_),
                ..
            }] => InternalValue::Bits(
                IntegerKernel::for_value(result, block)?.normalize(u128::from(value)),
            ),
            _ => {
                return Err(InterpreterError::MemoryAccess {
                    block,
                    error: MemoryFault::ExternalFunction { name: name.into() },
                })
            }
        },
        LibraryValue::Pointer(value) => InternalValue::Pointer(value),
    };
    Ok(vec![
        result,
        InternalValue::Bits(state.fresh_token()?),
        InternalValue::Bits(state.fresh_token()?),
    ])
}

pub(super) fn enter_module_frame(
    module: &Module,
    frame: &mut ModuleFrame,
) -> Result<(), InterpreterError> {
    let function = module.functions.get(frame.function_index).ok_or_else(|| {
        InterpreterError::InvalidModule {
            message: format!("missing function index {}", frame.function_index),
        }
    })?;
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == frame.block)
        .ok_or(InterpreterError::MissingBlock { block: frame.block })?;
    if frame.incoming.len() != block.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: block.args.len(),
            actual: frame.incoming.len(),
        });
    }
    for (argument, runtime_value) in block.args.iter().zip(frame.incoming.drain(..)) {
        frame.values.insert(argument.value.id, runtime_value);
    }
    frame.next_instruction = 0;
    frame.entered = true;
    Ok(())
}

pub(super) fn module_frame_action(
    module: &Module,
    frame: &ModuleFrame,
) -> Result<FrameAction, InterpreterError> {
    let function = module.functions.get(frame.function_index).ok_or_else(|| {
        InterpreterError::InvalidModule {
            message: format!("missing function index {}", frame.function_index),
        }
    })?;
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == frame.block)
        .ok_or(InterpreterError::MissingBlock { block: frame.block })?;
    Ok(if frame.next_instruction < block.insts.len() {
        FrameAction::Instruction(block.insts[frame.next_instruction].clone())
    } else {
        FrameAction::Terminator(block.terminator.clone())
    })
}

pub(super) fn set_module_frame_block(
    frame: &mut ModuleFrame,
    block: BlockId,
    incoming: Vec<InternalValue>,
) {
    frame.block = block;
    frame.incoming = incoming;
    frame.entered = false;
}

pub(super) fn bind_call_return(
    frames: &mut [ModuleFrame],
    binding: CallReturnBinding,
    returned: Vec<InternalValue>,
) -> Result<(), InterpreterError> {
    let expected = binding.results.len() + 2;
    if returned.len() != expected {
        return Err(InterpreterError::InvalidModule {
            message: format!(
                "callee returned {} values to a call expecting {expected}",
                returned.len()
            ),
        });
    }
    let caller = frames
        .last_mut()
        .ok_or_else(|| InterpreterError::InvalidModule {
            message: "callee return has no caller frame".to_owned(),
        })?;
    for (result, value) in binding.results.iter().zip(&returned) {
        caller.values.insert(result.id, *value);
    }
    caller
        .values
        .insert(binding.mem_out.id, returned[binding.results.len()]);
    caller
        .values
        .insert(binding.world_out.id, returned[binding.results.len() + 1]);
    Ok(())
}

pub(super) fn public_return_values(
    values: Vec<InternalValue>,
    types: Vec<EvoIRType>,
    block: BlockId,
) -> Result<Vec<RuntimeValue>, InterpreterError> {
    values
        .into_iter()
        .zip(types)
        .enumerate()
        .map(|(index, (value, ty))| match value {
            InternalValue::Bits(raw_bits) => Ok(RuntimeValue::wide(ty, raw_bits)),
            InternalValue::Pointer(_)
            | InternalValue::FunctionPointer(_)
            | InternalValue::Vector(_) => {
                Err(InterpreterError::UnsupportedRuntimeType { index, ty })
            }
            InternalValue::Uninitialized => Err(InterpreterError::UninitializedRead { block }),
        })
        .collect()
}
