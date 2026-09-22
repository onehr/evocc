use crate::*;

pub use evocc_types::TlsModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsSymbol {
    pub symbol: String,
    pub model: TlsModel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsAddress {
    pub result: Value,
    pub symbol: String,
    pub mem_in: Value,
    pub world_in: Value,
    pub mem_out: Value,
    pub world_out: Value,
}
impl TlsAddress {
    pub fn results(&self) -> Vec<&Value> {
        vec![&self.result, &self.mem_out, &self.world_out]
    }
    pub fn operands(&self) -> Vec<&Value> {
        vec![&self.mem_in, &self.world_in]
    }
    pub fn values_mut(&mut self) -> Vec<&mut Value> {
        vec![
            &mut self.result,
            &mut self.mem_in,
            &mut self.world_in,
            &mut self.mem_out,
            &mut self.world_out,
        ]
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.result.ty != EvoIRType::Ptr(AddressSpace::Generic) {
            return Err("tls_addr returns ptr<generic>".into());
        }
        expect_day7_effect_tokens(
            &self.mem_in,
            &self.world_in,
            &self.mem_out,
            &self.world_out,
            "tls_addr",
            0,
        )
    }
}

pub(crate) fn execute(
    access: &TlsAddress,
    block: BlockId,
    values: &mut HashMap<u32, InternalValue>,
    state: &mut ExecutionState,
) -> Result<(), InterpreterError> {
    let global = state
        .tls_templates
        .get(&access.symbol)
        .cloned()
        .ok_or_else(|| InterpreterError::MemoryAccess {
            block,
            error: MemoryFault::ExternalObject {
                object: state
                    .globals
                    .get(&access.symbol)
                    .copied()
                    .unwrap_or(Pointer::NULL)
                    .object(),
            },
        })?;
    let (memory, pointer, fresh) = state
        .memory
        .allocate_thread_local(global.origin, global.layout, InitialState::Uninitialized)
        .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
    state.memory = memory;
    if fresh {
        initialize_global_bytes(&global, pointer, state)?;
        initialize_global_relocations(&global, pointer, state)?;
        if global.read_only {
            state.memory = state
                .memory
                .freeze_static(pointer)
                .map_err(|error| InterpreterError::MemoryAccess { block, error })?;
        }
    }
    values.insert(access.result.id, InternalValue::Pointer(pointer));
    values.insert(access.mem_out.id, InternalValue::Bits(state.fresh_token()?));
    values.insert(
        access.world_out.id,
        InternalValue::Bits(state.fresh_token()?),
    );
    Ok(())
}
