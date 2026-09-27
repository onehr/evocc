//! EvoIR Section 3.7, revision 1.9.72: "Both are pure, consume no tokens, are
//! not in the Section 8.5 allowlist".
//!
//! Absence from the allowlist is a decline, not an abort. The profile must open
//! a state over a module holding `return_address` and `frame_address`, ingest
//! it, optimize it, and leave both forms standing, exactly as it does for the
//! other pointer-producing forms of Section 3.7.

use evocc_eqopt::{
    optimize_scalar_module, optimize_scalar_module_certified_persistent, EqualityState,
    ScalarEqBudget,
};
use evocc_evoir::{parse_section9_module, render_module, Module};

/// Both activation addresses beside ordinary 32-bit work the profile does
/// reason about, so a decline is visible as work done elsewhere.
const ACTIVATION_ADDRESSES: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<64>, bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = return_address : ptr<generic>
    %v4 = frame_address : ptr<generic>
    %v5 = ptr.to_int %v3
    %v6 = ptr.to_int %v4
    %v7 = xor.bits<64> %v5, %v6
    %v8 = const.bits<32> 0
    %v9 = add.bits<32> %v0, %v8
    ret %v7, %v9, %v1, %v2
}
";

fn module() -> Module {
    parse_section9_module(ACTIVATION_ADDRESSES.as_bytes()).expect("canonical EvoIR")
}

#[test]
fn the_optimizer_declines_both_forms_and_leaves_them_standing() {
    let source = module();
    let outcome = optimize_scalar_module(&source, ScalarEqBudget::new(128, 8));
    let text = render_module(&outcome.module);
    assert!(
        text.contains("= return_address : ptr<generic>"),
        "the return address survives the optimizer unchanged:\n{text}"
    );
    assert!(
        text.contains("= frame_address : ptr<generic>"),
        "and so does the frame address:\n{text}"
    );
}

#[test]
fn the_persistent_equality_state_ingests_them_without_aborting() {
    let source = module();
    let mut state = EqualityState::for_module(&source).expect("a canonical module opens a state");
    state
        .ingest_module(&source)
        .expect("an activation address is data the state may hold");
}

#[test]
fn the_certified_path_seeds_no_region_over_an_activation_address() {
    let source = module();
    let mut state = EqualityState::for_module(&source).expect("a canonical module opens a state");
    let outcome = optimize_scalar_module_certified_persistent(
        &source,
        &mut state,
        ScalarEqBudget::new(128, 8),
    );
    for region in &outcome.certificate.regions {
        for seed in &region.seeds {
            assert!(
                !seed.sort.starts_with("ptr"),
                "no region seeds a pointer sort the profile does not reason about"
            );
        }
    }
}
