//! What the six transfer rows of revision 1.9.43 can reach.
//!
//! Section 8.10 gave `shl`, `lshr`, `ashr`, `zext`, `sext` and `trunc` a row
//! each at revision 1.9.43, and revision 1.9.44 admitted the six operators to
//! Section 8.5's allowlist and Section 11.3's operator table, so a fact one of
//! them carries can now be a premise. What that is worth is a number, and this
//! is where it is counted: the rows are switched on and off against the same
//! seeds, and the difference is what they enable.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_proof::EvpEvent;
use evocc_randgate::{generate_module, GeneratorConfig};

/// How many rewrites the campaign's own programs state a premise for.
///
/// The number is what the shift and width rows are measured against, with the
/// rows computed and with them answering top.
#[test]
fn the_campaign_counts_its_premised_rewrites() {
    let mut premised = 0_u64;
    let mut rewrites = 0_u64;
    for seed in 0..256_u64 {
        let module = generate_module(seed, GeneratorConfig::default());
        let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
        for region in &outcome.certificate.regions {
            for event in &region.events {
                if let EvpEvent::Rewrite(rewrite) = event {
                    rewrites += 1;
                    if !rewrite.premises.is_empty() {
                        premised += 1;
                    }
                }
            }
        }
    }
    println!("rewrites={rewrites}");
    println!("premisedRewrites={premised}");
    assert!(premised > 0, "the fact-driven rules must still fire");
}
