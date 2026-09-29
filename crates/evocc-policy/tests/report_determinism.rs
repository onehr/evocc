//! The report is a function of the policy, the corpus and the seed.
//!
//! Everything the harness prints except wall time is determined by those
//! three, so two runs of the same triple render the same bytes. Wall time is
//! not, which is why it is the one field a byte comparison elides rather than
//! a field the report omits.

use evocc_eqopt::policy_by_name;
use evocc_policy::{differing_programs, evaluate, Aggregate, Corpus, TimingMode};
use evocc_randgate::{GeneratorConfig, ProofChecker};

const STEPS: u64 = 200_000;

fn checker() -> ProofChecker {
    let checker =
        ProofChecker::locate().expect("the workspace builds evo-proofcheck next to this test");
    let expected = checker.identity_digest().unwrap();
    checker.with_expected_digest(expected)
}

#[test]
fn two_runs_of_one_triple_render_the_same_bytes() {
    let corpus = Corpus::generated(9_000, 24, GeneratorConfig::default());
    let policy = policy_by_name("instruction-count").expect("the default policy ships");
    let checker = checker();
    let first = evaluate(&policy, &corpus, 7, STEPS, &checker);
    let second = evaluate(&policy, &corpus, 7, STEPS, &checker);
    assert_eq!(
        first.render_json(TimingMode::Elided),
        second.render_json(TimingMode::Elided)
    );
    assert_eq!(first.corpus_digest, corpus.digest());
    assert!(first
        .render_json(TimingMode::Measured)
        .contains("\"optimizerWallNanos\""));
}

#[test]
fn a_different_seed_is_a_different_report() {
    let corpus = Corpus::generated(9_100, 8, GeneratorConfig::default());
    let policy = policy_by_name("instruction-count").expect("the default policy ships");
    let checker = checker();
    let first = evaluate(&policy, &corpus, 1, STEPS, &checker);
    let second = evaluate(&policy, &corpus, 2, STEPS, &checker);
    assert_ne!(
        first.render_json(TimingMode::Elided),
        second.render_json(TimingMode::Elided),
        "the seed names the entry arguments, so it must reach the report"
    );
}

#[test]
fn the_aggregate_is_the_sum_of_the_programs() {
    let corpus = Corpus::generated(9_200, 32, GeneratorConfig::default());
    let policy = policy_by_name("weighted-operators").expect("the weighted policy ships");
    let report = evaluate(&policy, &corpus, 3, STEPS, &checker());

    let recomputed = Aggregate::of(&report.programs);
    assert_eq!(recomputed, report.aggregate);
    assert_eq!(report.aggregate.programs, report.programs.len() as u64);
    assert_eq!(
        report.aggregate.programs,
        report.aggregate.measured
            + report.aggregate.undefined_input
            + report.aggregate.fuel_exhausted
            + report.aggregate.refused
    );
    let steps_before: u64 = report.programs.iter().map(|p| p.steps_before).sum();
    let regions: u64 = report.programs.iter().map(|p| p.regions).sum();
    assert_eq!(report.aggregate.steps_before, steps_before);
    assert_eq!(report.aggregate.regions, regions);
}

#[test]
fn every_certificate_replays() {
    let corpus = Corpus::generated(9_300, 32, GeneratorConfig::default());
    let checker = checker();
    for name in ["instruction-count", "weighted-operators"] {
        let policy = policy_by_name(name).expect("a shipped policy");
        let report = evaluate(&policy, &corpus, 5, STEPS, &checker);
        let (replayed, produced) = report.replay();
        assert_eq!(replayed, produced, "{name} lost a certificate");
        assert!(produced > 0, "{name} produced no certificate to replay");
        assert_eq!(report.aggregate.refused, 0, "{name} refused a program");
    }
}

#[test]
fn two_policies_over_one_corpus_are_comparable() {
    let corpus = Corpus::generated(9_400, 16, GeneratorConfig::default());
    let checker = checker();
    let default = evaluate(
        &policy_by_name("instruction-count").expect("the default policy ships"),
        &corpus,
        11,
        STEPS,
        &checker,
    );
    let control = evaluate(
        &policy_by_name("zero-multiply").expect("the control policy ships"),
        &corpus,
        11,
        STEPS,
        &checker,
    );
    let differing = differing_programs(&default, &control).expect("one corpus, two policies");
    assert!(
        differing > 0,
        "a policy that declines every multiply must change some program of a 16-program corpus"
    );

    let other_corpus = Corpus::generated(9_500, 16, GeneratorConfig::default());
    let elsewhere = evaluate(
        &policy_by_name("instruction-count").expect("the default policy ships"),
        &other_corpus,
        11,
        STEPS,
        &checker,
    );
    assert_eq!(
        differing_programs(&default, &elsewhere),
        None,
        "two different corpora are not comparable"
    );
}
