//! Minimization of a refusing module.

use evocc_evoir::Module;

/// Delete instructions, blocks and functions for as long as `still_refuses`
/// keeps answering true and the reduced module still validates.
///
/// The predicate is the only authority on whether a reduction preserved the
/// failure; the shrinker never reasons about why the module failed.
#[must_use]
pub fn shrink_module(
    module: &Module,
    still_refuses: &mut dyn FnMut(&Module) -> bool,
    max_rounds: u32,
) -> Module {
    let _ = (module, still_refuses, max_rounds);
    todo!("greedy module minimization")
}
