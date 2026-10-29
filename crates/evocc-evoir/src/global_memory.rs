//! EvoIR global memory.

use super::*;

pub(super) fn initialize_globals(
    module: &Module,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    for (index, object) in module.extern_objects.iter().enumerate() {
        let (memory, pointer) = state
            .memory
            .register_external(u64::MAX - index as u64, object.size, object.align)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        state.memory = memory;
        state.globals.insert(object.symbol.clone(), pointer);
    }
    for global in &module.globals {
        if module.tls.iter().any(|t| t.symbol == global.symbol) {
            state
                .tls_templates
                .insert(global.symbol.clone(), global.clone());
            continue;
        }
        let (memory, pointer) = match global.kind {
            GlobalKind::StringLiteral => {
                let bytes = ByteSequence::try_from_bytes(global.bytes.clone())
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                state
                    .memory
                    .allocate_string_literal(global.origin, global.layout, &bytes)
                    .map_err(|error| InterpreterError::MemorySetup { error })?
            }
            GlobalKind::File | GlobalKind::FunctionStatic | GlobalKind::CompoundLiteral => state
                .memory
                .allocate_static(global.origin, global.layout, InitialState::Uninitialized)
                .map_err(|error| InterpreterError::MemorySetup { error })?,
        };
        state.memory = memory;
        if global.kind != GlobalKind::StringLiteral {
            initialize_global_bytes(global, pointer, state)?;
        }
        state.globals.insert(global.symbol.clone(), pointer);
    }
    for alias in &module.aliases {
        if let Some(template) = state.tls_templates.get(&alias.target).cloned() {
            state.tls_templates.insert(alias.name.clone(), template);
        }
        if let Some(pointer) = state.globals.get(&alias.target).copied() {
            state.globals.insert(alias.name.clone(), pointer);
        }
    }
    for global in &module.globals {
        if state.tls_templates.contains_key(&global.symbol) {
            continue;
        }
        initialize_global_relocations(global, state.globals[&global.symbol], state)?;
        if global.read_only && global.kind != GlobalKind::StringLiteral {
            state.memory = state
                .memory
                .freeze_static(state.globals[&global.symbol])
                .map_err(|error| InterpreterError::MemorySetup { error })?;
        }
    }
    Ok(())
}

pub(super) fn initialize_global_bytes(
    global: &GlobalObject,
    pointer: Pointer,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let mut start = 0_usize;
    while start < global.initialized.len() {
        if !global.initialized[start] {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < global.initialized.len() && global.initialized[end] {
            end += 1;
        }
        let offset = i64::try_from(start).map_err(|_| InterpreterError::InvalidModule {
            message: format!("global '{}' offset exceeds i64", global.symbol),
        })?;
        let address = pointer
            .checked_offset(offset)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        let size = u64::try_from(end - start).map_err(|_| InterpreterError::InvalidModule {
            message: format!("global '{}' range exceeds u64", global.symbol),
        })?;
        let bytes = ByteSequence::try_from_static_initializer(global.bytes[start..end].to_vec())
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        state.memory = state
            .memory
            .store_bytes(address, AccessLayout::new(size, 1), &bytes)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        start = end;
    }
    Ok(())
}

pub(super) fn initialize_global_relocations(
    global: &GlobalObject,
    base: Pointer,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    for relocation in &global.relocs {
        if let Some(difference) = &relocation.difference {
            let mut address = |symbol: &str| -> Result<u64, InterpreterError> {
                let identity = symbol
                    .split_once("^b")
                    .and_then(|(name, block)| {
                        block
                            .parse::<BlockId>()
                            .ok()
                            .and_then(|block| state.blocks.get(&(name.to_owned(), block)))
                    })
                    .copied()
                    .ok_or_else(|| InterpreterError::InvalidModule {
                        message: "relative relocation names an absent block".into(),
                    })?;
                let (memory, address) = state
                    .memory
                    .expose_pointer(PointerValue::Function(identity))
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                state.memory = memory;
                Ok(address)
            };
            let raw = address(&relocation.symbol)?
                .wrapping_sub(address(&difference.base)?)
                .wrapping_add_signed(relocation.addend);
            let destination = base
                .checked_offset(relocation.offset as i64)
                .map_err(|error| InterpreterError::MemorySetup { error })?;
            state.memory = state
                .memory
                .store_integer(
                    destination,
                    AccessLayout::new(relocation.size_bytes(), 1),
                    u128::from(raw),
                )
                .map_err(|error| InterpreterError::MemorySetup { error })?;
            continue;
        }
        let pointer = if let Some(pointer) = state.globals.get(&relocation.symbol) {
            PointerValue::object(
                pointer
                    .checked_offset(relocation.addend)
                    .map_err(|error| InterpreterError::MemorySetup { error })?,
            )
        } else if let Some(identity) = state.functions.get(&relocation.symbol) {
            if relocation.addend != 0 {
                return Err(InterpreterError::InvalidModule {
                    message: "function relocation has a nonzero addend".into(),
                });
            }
            PointerValue::Function(*identity)
        } else if let Some((name, block)) = relocation.symbol.split_once("^b") {
            let identity = block
                .parse::<BlockId>()
                .ok()
                .and_then(|block| state.blocks.get(&(name.to_owned(), block)))
                .copied()
                .ok_or_else(|| InterpreterError::InvalidModule {
                    message: "relocation names an absent block".into(),
                })?;
            let value = PointerValue::Function(identity);
            if relocation.addend == 0 {
                value
            } else {
                // A nonzero code addend is an integer address, not the
                // identity of another block or an object allocation.
                let address = state
                    .memory
                    .pointer_address(value)
                    .map_err(|error| InterpreterError::MemorySetup { error })?;
                PointerValue::object(Pointer::from_address(
                    address.wrapping_add_signed(relocation.addend),
                ))
            }
        } else {
            return Err(InterpreterError::InvalidModule {
                message: "relocation names an absent symbol".into(),
            });
        };
        let destination = base
            .checked_offset(relocation.offset as i64)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        if relocation.exposed {
            (state.memory, _) = state
                .memory
                .expose_pointer(pointer)
                .map_err(|error| InterpreterError::MemorySetup { error })?;
        }
        state.memory = state
            .memory
            // A data relocation patches bytes, including packed fields;
            // the subsequent load/store carries its own access alignment.
            .store_pointer_value(destination, AccessLayout::new(8, 1), pointer)
            .map_err(|error| InterpreterError::MemorySetup { error })?;
    }
    Ok(())
}
