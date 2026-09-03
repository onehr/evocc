//! Section 7's linear scan and the allocated form it produces.

use std::collections::BTreeMap;

use crate::MBlock;

/// Section 6: "`x0` to `x17` are caller-saved", less the two intra-procedure
/// scratch registers, which Section 2 keeps for emitted sequences.
pub const CALLER_SAVED: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// Section 6: "`x19` to `x28` ... are callee-saved".
pub const CALLEE_SAVED: [u8; 10] = [19, 20, 21, 22, 23, 24, 25, 26, 27, 28];

/// One allocated function: the same blocks with every virtual register
/// assigned, no block arguments, and a frame slot for each spilled value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocated {
    pub name: String,
    pub blocks: Vec<MBlock>,
    /// Virtual register identity to physical register number.
    pub assignment: BTreeMap<u32, u8>,
    /// How many virtual registers went to the frame.
    pub spilled: usize,
    /// Eight-byte slots in the spill area.
    pub frame_slots: u32,
    /// Callee-saved registers the body uses, which the prologue must save.
    pub used_callee_saved: Vec<u8>,
}

/// Assign a physical register to every virtual register of an out-of-SSA
/// function, spilling to the frame when a class runs out.
pub fn allocate(_function: &crate::MFunction) -> Result<Allocated, String> {
    todo!("Section 7 linear scan")
}

/// Render an allocated function in the post-allocation spelling of Appendix A.
#[must_use]
pub fn render_allocated(_allocated: &Allocated) -> String {
    todo!("the post-allocation textual form")
}
