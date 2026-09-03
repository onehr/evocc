//! Running a lowered function on the inputs the EvoIR interpreter was given.
//!
//! Section 9.1's first instrument compares two interpreters on the same
//! program, so the argument and result shapes have to cross the boundary. This
//! module is the only place that translates between an EvoIR `RuntimeValue`
//! and an EvoMIR register, and it is shared by the hand-written fixtures and
//! by the semantic gate's third lane so the two cannot drift.

use evocc_evoir::RuntimeValue;
use evocc_memory::{MemoryState, WorldState};
use evocc_types::EvoIRType;

use crate::{MModule, MirError, MirLimits};

/// One defined EvoMIR execution, shaped so the Section 7 equality criterion
/// can be applied to it directly.
#[derive(Debug, Clone)]
pub struct MirRun {
    pub returns: Vec<RuntimeValue>,
    pub memory: MemoryState,
    pub world: WorldState,
    pub steps: u64,
}

/// Execute `entry` in a lowered module on the EvoIR entry arguments.
///
/// `result_types` are the entry function's ordinary EvoIR result types, memory
/// and world excluded. They are needed because Section 6 passes and returns a
/// `bits<N>` with `N < 64` "in the low bits of its register with the upper bits
/// unspecified": the register alone does not say how much of it is the value.
pub fn observe_entry(
    _module: &MModule,
    _entry: &str,
    _entry_args: &[RuntimeValue],
    _result_types: &[EvoIRType],
    _limits: MirLimits,
) -> Result<MirRun, MirError> {
    todo!("EvoMIR lane entry")
}
