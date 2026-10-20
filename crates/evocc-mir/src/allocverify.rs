//! Compatibility entry points for the independent shared allocation verifier.

use crate::{Allocated, MFunction, MTerm, Target};

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

/// Bind the retained input to the caller's actual pre-allocation program.
/// Native entry points use this form: an allocation cannot replace both sides
/// of the comparison and certify a different program.
pub fn verify_allocation_against(
    target: Target,
    input: &MFunction,
    allocated: &Allocated,
) -> Result<(), VerifierRefusal> {
    if &allocated.input != input {
        return Err(VerifierRefusal {
            code: "E4020",
            name: "allocation: verifier-refused",
            block: 0,
            detail: "retained allocation input differs from the caller's program".into(),
        });
    }
    verify_allocation_for(target, allocated)
}

/// Preserve the existing constant terminator query.
#[must_use]
pub const fn is_exit(term: &MTerm) -> bool {
    crate::isa::aarch64::insts::is_exit(term)
}
