//! The Section 5 lowering from extracted EvoIR to pre-allocation EvoMIR.

use crate::MModule;

/// One EvoIR form this revision does not lower, reported at the instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub function: String,
    pub block: u32,
    pub index: usize,
    pub code: &'static str,
    pub name: &'static str,
    pub detail: String,
}

/// Lower a whole module, or report every refusal it contains.
pub fn lower_module(_module: &evocc_evoir::Module) -> Result<MModule, Vec<Refusal>> {
    todo!("EvoMIR Section 5 lowering")
}
