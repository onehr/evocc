//! Section 7: out-of-SSA and linear-scan register allocation.

use crate::MFunction;

/// Remove block arguments by placing each edge's parallel copy in its
/// predecessor, splitting critical edges first.
#[must_use]
pub fn out_of_ssa(_function: &MFunction) -> MFunction {
    todo!("Section 7 out-of-SSA")
}
