//! Replay every sealed certificate triple through the standalone checker.
//!
//! `day8_corpus_replay.rs` measures a corpus it builds in a temporary directory
//! and throws away; nothing survives a run that a reader can open, and a
//! regression in the format shows up only as a corpus that stopped replaying.
//! `tests/fixtures/certificates/` keeps a handful of triples as bytes instead.
//! Two properties are worth having and this file asserts both: the checker
//! accepts every sealed triple, and every sealed `after.evo` and `proof.evp`
//! is still what the current optimizer produces from the sealed `before.evo`.
//! Without the second, a sealed fixture would be a snapshot of a producer
//! nobody runs any more, and a hand-repaired one would look exactly like a
//! passing one. `before.evo` itself is provenance: its README names the front
//! end commit that lowered `source.c`, and the front end may lower the same C
//! differently later without changing what the certificate is about, so the
//! sealed module is parsed under Section 9 rather than lowered again.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use evocc_eqopt::{optimize_scalar_module_with_policy, ExtractionPolicy};
use evocc_evoir::{parse_section9_module, render_module, validate_module};
use evocc_proof::{render_evp1, EvpCertificate, EvpEvent};

/// The smallest corpus the reviewed plan asks to be sealed. A run that found
/// fewer directories has lost fixtures rather than passed.
const MINIMUM_TRIPLES: usize = 2;

fn certificates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives under crates/")
        .join("tests/fixtures/certificates")
}

/// Every sealed directory, in name order so that a failure names the same
/// fixture on every host.
fn triples() -> Vec<PathBuf> {
    let mut directories = fs::read_dir(certificates())
        .expect("the sealed certificate fixtures are part of the repository")
        .map(|entry| entry.expect("read a fixture directory").path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    assert!(
        directories.len() >= MINIMUM_TRIPLES,
        "expected at least {MINIMUM_TRIPLES} sealed triples, found {}",
        directories.len()
    );
    directories
}

fn name(triple: &Path) -> String {
    triple
        .file_name()
        .expect("a fixture directory has a name")
        .to_string_lossy()
        .into_owned()
}

fn run_checker(before: &Path, after: &Path, proof: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([before, after, proof])
        .output()
        .expect("run standalone proof checker")
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// One `"key": integer` field of the sealed counts, wherever it appears.
///
/// The counts are the driver's own rendering rather than a general JSON
/// document, so a scanner for the one shape they use is enough and keeps this
/// test from depending on a parser the workspace does not have.
fn counted(counts: &str, key: &str) -> usize {
    let needle = format!("\"{key}\": ");
    let start = counts
        .find(&needle)
        .unwrap_or_else(|| panic!("counts.json has no {key} field"))
        + needle.len();
    let digits = counts[start..]
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .unwrap_or_default();
    digits
        .parse()
        .unwrap_or_else(|error| panic!("{key} is not a count: {error}"))
}

fn rewrites(certificate: &EvpCertificate) -> Vec<String> {
    certificate
        .regions
        .iter()
        .flat_map(|region| &region.events)
        .filter_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite.rule.clone()),
            _ => None,
        })
        .collect()
}

fn premises(certificate: &EvpCertificate) -> usize {
    certificate
        .regions
        .iter()
        .flat_map(|region| &region.events)
        .filter_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite.premises.len()),
            _ => None,
        })
        .sum()
}

#[test]
fn every_sealed_triple_replays_in_the_standalone_checker() {
    let triples = triples();
    let mut accepted = 0;
    for triple in &triples {
        let output = run_checker(
            &triple.join("before.evo"),
            &triple.join("after.evo"),
            &triple.join("proof.evp"),
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}: stdout={}, stderr={}",
            name(triple),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        accepted += 1;
    }
    assert_eq!(accepted, triples.len(), "the gate forbids skips");
    println!(
        "sealed certificate fixtures: accepted={accepted}/{}",
        triples.len()
    );
}

#[test]
fn every_sealed_triple_is_what_the_current_optimizer_produces() {
    for triple in triples() {
        let label = name(&triple);
        let sealed_before = read(&triple.join("before.evo"));
        let before = parse_section9_module(sealed_before.as_bytes())
            .expect("the sealed module parses under Section 9");
        validate_module(&before).expect("the fixture is valid EvoIR");
        let outcome = optimize_scalar_module_with_policy(&before, &ExtractionPolicy::default());
        validate_module(&outcome.module).expect("the optimized fixture is valid EvoIR");

        assert_eq!(
            sealed_before,
            render_module(&before),
            "{label}: before.evo is not in canonical form"
        );
        assert_eq!(
            read(&triple.join("after.evo")),
            render_module(&outcome.module),
            "{label}: after.evo is not what the current optimizer produces"
        );
        assert_eq!(
            read(&triple.join("proof.evp")),
            render_evp1(&outcome.certificate),
            "{label}: proof.evp is not what the current producer certifies"
        );

        let counts = read(&triple.join("counts.json"));
        assert_eq!(
            counted(&counts, "regions"),
            outcome.certificate.regions.len(),
            "{label}: the sealed region count"
        );
        assert_eq!(
            counted(&counts, "rewrite"),
            rewrites(&outcome.certificate).len(),
            "{label}: the sealed rewrite count"
        );
        assert_eq!(
            counted(&counts, "premises"),
            premises(&outcome.certificate),
            "{label}: the sealed premise count"
        );
    }
}

#[test]
fn the_sealed_corpus_covers_both_tiers_of_the_registry() {
    let mut tier_a = 0;
    let mut tier_b = 0;
    let mut recorded_premises = 0;
    for triple in triples() {
        let counts = read(&triple.join("counts.json"));
        recorded_premises += counted(&counts, "premises");
        for line in counts.lines() {
            let Some(rule) = line.trim().strip_prefix('"') else {
                continue;
            };
            if rule.starts_with("p0_") {
                tier_a += 1;
            } else if rule.starts_with("p1_") || rule.starts_with("p2_") {
                tier_b += 1;
            }
        }
    }
    assert!(
        tier_a > 0,
        "no sealed triple fires a syntactic rule; the corpus has become a no-op"
    );
    assert!(
        tier_b > 0,
        "no sealed triple fires a fact-driven rule; the known-bits analysis is unexercised"
    );
    assert!(
        recorded_premises > 0,
        "no sealed triple records a premise, so no checker recomputation is exercised"
    );
    println!(
        "sealed certificate fixtures: tierARuleIds={tier_a}, tierBRuleIds={tier_b}, premises={recorded_premises}"
    );
}
