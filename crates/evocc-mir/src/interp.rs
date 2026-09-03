//! The EvoMIR interpreter: executable semantics for every Section 4 form.

use evocc_memory::{MemoryState, Pointer, PointerValue, WorldState};

use crate::MModule;

/// Resource limits for deterministic EvoMIR execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirLimits {
    pub step_budget: u64,
}

impl MirLimits {
    #[must_use]
    pub const fn new(step_budget: u64) -> Self {
        Self { step_budget }
    }
}

/// One machine value.
///
/// A register holds either raw bits or a provenance-carrying pointer: the
/// EvoMIR interpreter uses the object model of EvoIR Section 5, because
/// Section 9.1 requires it to agree with the EvoIR interpreter on results and
/// on the volatile trace, both of which name objects by origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirValue {
    Int(u64),
    Ptr(Pointer),
    Fn(PointerValue),
}

/// Successful EvoMIR execution.
#[derive(Debug, Clone)]
pub struct MirOutcome {
    pub result: Option<MirValue>,
    pub steps: u64,
    pub memory: MemoryState,
    pub world: WorldState,
}

/// A defined failure of EvoMIR execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirError {
    /// A `brk` was reached; the immediate names the EvoIR reason.
    Trap { imm: u16 },
    /// The step budget was exhausted.
    BudgetExhausted,
    /// A memory fault, reported with the EvoIR diagnostic of Section 6.1.
    Memory { code: &'static str, detail: String },
    /// The module could not be executed at all.
    Invalid { detail: String },
}

/// Interpret a module's named entry function.
pub fn interpret_module(
    _module: &MModule,
    _entry: &str,
    _args: &[MirValue],
    _limits: MirLimits,
) -> Result<MirOutcome, MirError> {
    todo!("EvoMIR Section 4 interpreter")
}
