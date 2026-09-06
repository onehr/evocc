//! EvoIR 10.1.19 makes `bits<128>` an executed scalar type. The P0 equality
//! profile does not reason about it: Section 11 bounds every root and witness
//! to "`bits<N>` with `1 <= N <= 64`", and adds that "silently truncating a
//! wider constant to the host's integer width is invalid".
//!
//! Exclusion is therefore the correct answer. Aborting is not: the exclusion
//! fixed point already refuses a `bits<128>` value, but three paths read a
//! constant's width before eligibility is consulted, and the width function
//! they reach is total.

use evocc_eqopt::{
    optimize_scalar_module, optimize_scalar_module_certified_persistent, EqualityState,
    ScalarEqBudget,
};
use evocc_evoir::{parse_section9_module, render_module, Module};

/// A module whose entry block holds one `bits<128>` constant beside ordinary
/// 32-bit work the profile does reason about.
const WIDE_CONSTANT: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<128> 5
    %v4 = const.bits<32> 0
    %v5 = add.bits<32> %v0, %v4
    %v6 = trunc.bits<32> %v3
    %v7 = add.bits<32> %v5, %v6
    ret %v7, %v1, %v2
}
";

/// The same width reached through arithmetic rather than a constant.
const WIDE_ARITHMETIC: &str = "\
func @main(%arg0: bits<64>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: mem<all>, %v2: world):
    %v3 = zext.bits<128> %v0
    %v4 = mul.bits<128> %v3, %v3
    %v5 = const.bits<128> 64
    %v6 = lshr.bits<128> %v4, %v5
    %v7 = trunc.bits<64> %v6
    ret %v7, %v1, %v2
}
";

fn module(text: &str) -> Module {
    parse_section9_module(text.as_bytes()).expect("canonical EvoIR")
}

#[test]
fn a_wide_constant_is_declined_rather_than_reached() {
    // The transient sweep reads every `Const`'s width before the exclusion
    // fixed point is consulted.
    for text in [WIDE_CONSTANT, WIDE_ARITHMETIC] {
        let source = module(text);
        let outcome = optimize_scalar_module(&source, ScalarEqBudget::new(128, 8));
        assert!(
            render_module(&outcome.module).contains("bits<128>"),
            "the wide value survives the optimizer unchanged"
        );
    }
}

#[test]
fn the_persistent_equality_state_ingests_a_wide_constant_without_aborting() {
    // `EqualityState::ingest_module` folds every `Const` into its constant
    // environment before any allowlist runs.
    for text in [WIDE_CONSTANT, WIDE_ARITHMETIC] {
        let source = module(text);
        let mut state =
            EqualityState::for_module(&source).expect("a canonical module opens a state");
        state
            .ingest_module(&source)
            .expect("a wide constant is data the state may hold");
    }
}

#[test]
fn the_certified_path_produces_a_certificate_the_checker_replays() {
    // The producer and the independent checker must agree that the wide
    // value is excluded: the certificate names no region over it, and the
    // checker never sees a `bits<128>` sort.
    for text in [WIDE_CONSTANT, WIDE_ARITHMETIC] {
        let source = module(text);
        let mut state =
            EqualityState::for_module(&source).expect("a canonical module opens a state");
        let outcome = optimize_scalar_module_certified_persistent(
            &source,
            &mut state,
            ScalarEqBudget::new(128, 8),
        );
        for region in &outcome.certificate.regions {
            for seed in &region.seeds {
                assert_ne!(
                    seed.sort, "bits<128>",
                    "no region seeds a sort the profile does not reason about"
                );
            }
        }
    }
}
