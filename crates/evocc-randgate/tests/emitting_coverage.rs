//! Whether the campaign's generator reaches the emitting rules.
//!
//! A rule the campaign never fires is a rule whose side condition, whose
//! builder and whose reconstruction in the checker are all unmeasured, however
//! carefully they are written. Which spelling of a doubling survives is the
//! policy's choice (Section 7.5), so a rule is counted as reached when some
//! shipped policy keeps a region that fires it: under one cost function every
//! spelling ties and none of the four could ever appear.

use std::collections::BTreeSet;

use evocc_eqopt::{optimize_scalar_module_with_policy, shipped_policies};
use evocc_randgate::{generate_module, GeneratorConfig};

const PROGRAMS: u64 = 256;

fn config() -> GeneratorConfig {
    GeneratorConfig::default()
}

/// Every rule any shipped policy fires over the first `PROGRAMS` seeds.
fn fired() -> BTreeSet<String> {
    let mut fired = BTreeSet::new();
    for seed in 0..PROGRAMS {
        let module = generate_module(seed, config());
        for policy in shipped_policies() {
            let outcome = optimize_scalar_module_with_policy(&module, &policy);
            for region in &outcome.certificate.regions {
                for event in &region.events {
                    if let evocc_proof::EvpEvent::Rewrite(rewrite) = event {
                        fired.insert(rewrite.rule.clone());
                    }
                }
            }
        }
    }
    fired
}

#[test]
fn the_campaign_reaches_every_emitting_rule() {
    let fired = fired();
    println!("firedRules={}", fired.len());
    let emitting = [
        "p2_300_mul_two_rhs",
        "p2_301_mul_two_lhs",
        "p2_302_add_self_shl",
        "p2_303_shl_one_add",
    ];
    let reached = emitting
        .iter()
        .filter(|rule| fired.contains(**rule))
        .copied()
        .collect::<Vec<_>>();
    println!("emittingReached={}", reached.join(","));
    let missing = emitting
        .iter()
        .filter(|rule| !fired.contains(**rule))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "{} never fired over {PROGRAMS} programs",
        missing.join(",")
    );
}
