//! Compatibility entry points for the shared SSA destruction algorithm.
//!
//! The existing machine validator supplies source-shape admission only; target
//! instruction facts do not choose aliases, copies, critical edges or DCE.

use crate::{MFunction, RegClass, VReg};

pub use crate::generic::ssa::{SCRATCH_BASE, SCRATCH_LIMIT};

#[must_use]
pub fn out_of_ssa(function: &MFunction) -> MFunction {
    out_of_ssa_with_dce(function, true)
}

/// The unchanged attribution switch for shared baseline dead-computation removal.
#[must_use]
pub fn out_of_ssa_with_dce(function: &MFunction, eliminate_dead: bool) -> MFunction {
    let forwarding_input_valid = crate::validate::validate_function(function).is_ok();
    crate::generic::ssa::out_of_ssa_with_dce(function, eliminate_dead, forwarding_input_valid)
}

pub(crate) fn unused_parameters(function: &MFunction) -> Vec<bool> {
    crate::generic::ssa::unused_parameters(function)
}

/// Producer telemetry from the same baseline demand analysis, not an audit.
#[must_use]
pub fn dead_computation_count(function: &MFunction) -> usize {
    crate::generic::ssa::dead_computation_count(function)
}

/// Preserve the public constant classification of cycle-breaking identities.
#[must_use]
pub const fn is_scratch(reg: VReg) -> bool {
    reg.id >= SCRATCH_BASE && reg.id < SCRATCH_LIMIT && !matches!(reg.class, RegClass::Flags)
}
