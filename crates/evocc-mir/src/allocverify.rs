//! Compatibility entry points for the independent shared allocation verifier.

use crate::{Allocated, MTerm, Target};

pub use crate::generic::allocverify::VerifierRefusal;

/// Verify the existing target's allocated machine form without allocator facts.
///
/// As with `allocate`, the target-free entry point uses the Darwin ABI table.
pub fn verify_allocation(allocated: &Allocated) -> Result<(), VerifierRefusal> {
    verify_allocation_for(Target::Darwin, allocated)
}

/// Verify against the boundary and call-clobber facts of `target`'s ABI.
pub fn verify_allocation_for(target: Target, allocated: &Allocated) -> Result<(), VerifierRefusal> {
    crate::generic::allocverify::verify_allocation(
        &crate::isa::aarch64::allocation::Aarch64Allocation::for_target(target),
        allocated,
    )
}

/// Preserve the existing constant terminator query.
#[must_use]
pub const fn is_exit(term: &MTerm) -> bool {
    crate::isa::aarch64::insts::is_exit(term)
}
