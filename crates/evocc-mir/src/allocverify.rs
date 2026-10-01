//! Compatibility entry points for the independent shared allocation verifier.

use crate::{Allocated, MTerm};

pub use crate::generic::allocverify::VerifierRefusal;

/// Verify the existing target's allocated machine form without allocator facts.
pub fn verify_allocation(allocated: &Allocated) -> Result<(), VerifierRefusal> {
    crate::generic::allocverify::verify_allocation(
        &crate::isa::aarch64::allocation::Aarch64Allocation,
        allocated,
    )
}

/// Preserve the existing constant terminator query.
#[must_use]
pub const fn is_exit(term: &MTerm) -> bool {
    crate::isa::aarch64::insts::is_exit(term)
}
