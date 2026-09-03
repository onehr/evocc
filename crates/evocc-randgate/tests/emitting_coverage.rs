//! Whether the campaign's generator reaches the emitting rules.
//!
//! A rule the campaign never fires is a rule whose side condition, whose
//! builder and whose reconstruction in the checker are all unmeasured, however
//! carefully they are written. Which spelling of a doubling survives is the
//! policy's choice (Section 7.5), so a rule is counted as reached when some
//! shipped policy keeps a region that fires it: under one cost function every
//! spelling ties and none of the four could ever appear.

use std::collections::{BTreeMap, BTreeSet};

use evocc_eqopt::{optimize_scalar_module_with_policy, shipped_policies};
use evocc_randgate::{generate_module, GeneratorConfig};

const PROGRAMS: u64 = 256;

fn config() -> GeneratorConfig {
    GeneratorConfig::default()
}

/// Which emitting rules one policy fires over the first `PROGRAMS` seeds, and
/// how often.
fn fired_by(policy: &evocc_eqopt::ExtractionPolicy) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for seed in 0..PROGRAMS {
        let module = generate_module(seed, config());
        let outcome = optimize_scalar_module_with_policy(&module, policy);
        for region in &outcome.certificate.regions {
            for event in &region.events {
                if let evocc_proof::EvpEvent::Rewrite(rewrite) = event {
                    if rewrite.rule.starts_with("p2_") {
                        *counts.entry(rewrite.rule.clone()).or_default() += 1;
                    }
                }
            }
        }
    }
    counts
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

/// How many multiplies, additions and shifts a module holds.
fn spellings(module: &evocc_evoir::Module) -> (u64, u64, u64) {
    let mut counts = (0, 0, 0);
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.insts {
                match instruction {
                    evocc_evoir::Inst::Mul(..) => counts.0 += 1,
                    evocc_evoir::Inst::Add(..) => counts.1 += 1,
                    evocc_evoir::Inst::Shl(..) => counts.2 += 1,
                    _ => {}
                }
            }
        }
    }
    counts
}

/// Which spelling of a doubling each shipped policy extracts.
///
/// Section 7.5: the cost function decides, and this is the measurement that
/// says so. Counting the operators of the module a policy hands back answers
/// the question a fired rule does not: a region records every merge its
/// witness closure produced, including one whose own root the policy declined,
/// so what a policy extracted is read off the module and not off the events.
#[test]
fn each_policy_extracts_the_spelling_its_costs_prefer() {
    let mut measured = Vec::new();
    let mut baseline = (0_u64, 0_u64, 0_u64);
    for policy in shipped_policies() {
        let mut after = (0_u64, 0_u64, 0_u64);
        baseline = (0, 0, 0);
        for seed in 0..PROGRAMS {
            let module = generate_module(seed, config());
            let before = spellings(&module);
            baseline = (
                baseline.0 + before.0,
                baseline.1 + before.1,
                baseline.2 + before.2,
            );
            let outcome = optimize_scalar_module_with_policy(&module, &policy);
            let counts = spellings(&outcome.module);
            after = (after.0 + counts.0, after.1 + counts.1, after.2 + counts.2);
        }
        println!(
            "{}: mul={} add={} shl={}",
            policy.name(),
            after.0,
            after.1,
            after.2
        );
        measured.push((policy.name(), after));
    }
    println!(
        "before: mul={} add={} shl={}",
        baseline.0, baseline.1, baseline.2
    );
    let of = |name: &str| {
        measured
            .iter()
            .find(|(policy, _)| *policy == name)
            .map(|(_, counts)| *counts)
            .expect("every shipped policy was measured")
    };
    // Under instruction count every spelling of a doubling costs one, so every
    // one of them ties and is declined; the multiplies that do disappear are
    // the ones an earlier tier folds or reduces. Under a weighted multiply the
    // doublings go too, which is the whole difference between the two.
    assert!(
        of("weighted-operators").0 < of("instruction-count").0,
        "a weighted multiply rewrites the doublings instruction count keeps: {:?} against {:?}",
        of("weighted-operators"),
        of("instruction-count")
    );
    // The control charges nothing for a multiply, so no region rooted at one
    // is ever a strict improvement and every multiply survives.
    assert_eq!(
        of("zero-multiply").0,
        baseline.0,
        "a free multiply is never worth replacing"
    );
    // A shift is dearer than an addition here and cheaper there, and the two
    // policies move the same programs in opposite directions.
    assert!(
        of("prefer-add").2 < of("prefer-shift").2,
        "prefer-shift ends with more shifts than prefer-add: {:?} against {:?}",
        of("prefer-add"),
        of("prefer-shift")
    );
    assert!(
        of("prefer-shift").1 < of("prefer-add").1,
        "prefer-add ends with more additions than prefer-shift"
    );
}
