//! The first search over cost weights.
//!
//! Section 7.5 makes the cost function a producer policy. This file holds what
//! a search over that policy has to satisfy before any number it produces
//! means anything: the vector form reproduces every shipped function byte for
//! byte, the search is a deterministic function of its seed, the corpus it
//! trains on is disjoint from the corpus it is judged on by construction, and
//! the harness's refusal of the acceptance corpora still stands underneath it.

use std::path::Path;

use evocc_eqopt::{
    optimize_scalar_module_with_policy, shipped_policies, CostAxis, CostFunction, OperatorWeights,
};
use evocc_evoir::render_module;
use evocc_policy::{
    candidate_fitness, evaluate, held_out_corpus, search_cost_weights, searched_policy,
    training_corpus, Aggregate, Corpus, CorpusRefusal, PolicyReport, ProgramMeasurement,
    ProgramOutcome, SearchConfig, TimingMode, HELD_OUT_RANGE, TRAINING_RANGE,
};
use evocc_proof::render_evp1;
use evocc_randgate::{generate_module, GeneratorConfig, ProofChecker};

const STEPS: u64 = 200_000;
const PINNED_SEEDS: u64 = 256;

fn checker() -> ProofChecker {
    ProofChecker::locate().expect("the workspace builds evo-proofcheck next to this test")
}

/// The producer's own output -- the optimized module and the certificate that
/// justifies it -- under a shipped cost function and under its vector form.
///
/// This is the strongest form of "the same policy": not that the reports agree
/// but that the bytes the producer emits agree, on 256 seeds, for all five.
#[test]
fn each_shipped_cost_function_emits_the_same_bytes_as_its_vector_form() {
    let modules = (0..PINNED_SEEDS)
        .map(|offset| generate_module(3_000_000 + offset, GeneratorConfig::default()))
        .collect::<Vec<_>>();
    for policy in shipped_policies() {
        let twin = policy.with_cost(CostFunction::Vector(policy.cost().weights()));
        for (offset, module) in modules.iter().enumerate() {
            let named = optimize_scalar_module_with_policy(module, &policy);
            let vector = optimize_scalar_module_with_policy(module, &twin);
            assert_eq!(
                render_module(&named.module),
                render_module(&vector.module),
                "{} and its vector form differ on seed {}",
                policy.name(),
                3_000_000 + offset as u64
            );
            assert_eq!(
                render_evp1(&named.certificate),
                render_evp1(&vector.certificate),
                "{} and its vector form certify differently on seed {}",
                policy.name(),
                3_000_000 + offset as u64
            );
        }
    }
}

/// The harness report agrees too, on every line but the one that names the
/// cost function -- which is the one line that is supposed to differ, because
/// the vector spells out what the name abbreviates.
#[test]
fn the_harness_report_is_identical_but_for_the_line_that_names_the_cost() {
    let corpus = Corpus::generated(3_100_000, 24, GeneratorConfig::default());
    let checker = checker();
    for policy in shipped_policies() {
        let twin = policy.with_cost(CostFunction::Vector(policy.cost().weights()));
        let named = evaluate(&policy, &corpus, 13, STEPS, &checker);
        let vector = evaluate(&twin, &corpus, 13, STEPS, &checker);
        let strip = |text: String| {
            text.lines()
                .filter(|line| !line.contains("\"costFunction\""))
                .map(str::to_owned)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            strip(named.render_json(TimingMode::Elided)),
            strip(vector.render_json(TimingMode::Elided)),
            "{} does not measure as its own vector",
            policy.name()
        );
        assert!(named
            .render_json(TimingMode::Elided)
            .contains("\"costFunction\""));
    }
}

#[test]
fn the_search_is_a_function_of_its_seed() {
    let corpus = Corpus::generated(3_200_000, 8, GeneratorConfig::default());
    let checker = checker();
    let config = SearchConfig {
        seed: 0x5EED_C057,
        population: 4,
        parents: 2,
        children: 2,
        generations: 2,
        weight_max: 3,
    };
    let first = search_cost_weights(&config, &corpus, 17, STEPS, &checker);
    let second = search_cost_weights(&config, &corpus, 17, STEPS, &checker);
    assert_eq!(first.best, second.best, "the same seed found two weightings");
    assert_eq!(first.best_fitness, second.best_fitness);
    assert_eq!(first.trajectory, second.trajectory);
    assert_eq!(first.evaluations, second.evaluations);
    assert_eq!(first.discarded, second.discarded);
    assert_eq!(
        first.trajectory.len(),
        config.generations + 1,
        "the trajectory records the initial population and every generation"
    );
    assert!(
        first.trajectory.windows(2).all(|pair| pair[1] <= pair[0]),
        "a (mu+lambda) search keeps its parents, so best fitness never rises: {:?}",
        first.trajectory
    );
    assert_eq!(
        first.discarded, 0,
        "no candidate lost a certificate: {:?}",
        first.discards
    );

    let elsewhere = SearchConfig {
        seed: 0x5EED_C058,
        ..config
    };
    let other = search_cost_weights(&elsewhere, &corpus, 17, STEPS, &checker);
    assert_ne!(
        other.trajectory, first.trajectory,
        "a different seed must walk a different path"
    );
}

#[test]
fn a_candidate_that_loses_a_certificate_is_not_a_fitness() {
    let corpus = Corpus::generated(3_300_000, 4, GeneratorConfig::default());
    let policy = searched_policy(OperatorWeights::uniform(1));
    let good = evaluate(&policy, &corpus, 19, STEPS, &checker());
    let fitness = candidate_fitness(&good).expect("a clean report is a fitness");
    assert_eq!(fitness, good.aggregate.steps_after);

    let lost = PolicyReport {
        aggregate: Aggregate {
            certificates_replayed: good.aggregate.certificates.saturating_sub(1),
            ..good.aggregate
        },
        ..good.clone()
    };
    assert!(
        candidate_fitness(&lost).is_err(),
        "a report that replayed fewer certificates than it produced is not a fitness"
    );

    let refused = PolicyReport {
        programs: vec![ProgramMeasurement {
            outcome: ProgramOutcome::Refused {
                detail: "a planted refusal".to_owned(),
            },
            ..good.programs[0].clone()
        }],
        aggregate: Aggregate {
            refused: 1,
            ..good.aggregate
        },
        ..good
    };
    assert!(
        candidate_fitness(&refused).is_err(),
        "a refused program is not a fitness either"
    );
}

#[test]
fn training_and_held_out_corpora_are_disjoint_by_construction() {
    let (training_first, training_count) = TRAINING_RANGE;
    let (held_out_first, held_out_count) = HELD_OUT_RANGE;
    assert!(training_count > 0 && held_out_count > 0);
    assert!(
        training_first + training_count <= held_out_first
            || held_out_first + held_out_count <= training_first,
        "the two seed ranges overlap, so held-out is not held out"
    );

    let training = training_corpus();
    let held_out = held_out_corpus();
    assert_eq!(training.programs().len() as u64, training_count);
    assert_eq!(held_out.programs().len() as u64, held_out_count);
    assert_ne!(training.digest(), held_out.digest());
    assert_eq!(
        training.digest(),
        training_corpus().digest(),
        "a corpus named by a seed range is the same corpus every time"
    );

    let training_names = training
        .programs()
        .iter()
        .map(|program| program.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for program in held_out.programs() {
        assert!(
            !training_names.contains(&program.name),
            "{} is in both corpora",
            program.name
        );
    }
}

#[test]
fn the_search_corpora_cannot_come_from_an_acceptance_path() {
    for path in [
        ".build/corpus",
        ".evidence/day9",
        "tests/contracts/day5-kernel-cohort.json",
    ] {
        let refusal = Corpus::from_directory(Path::new(path), None)
            .expect_err("the acceptance corpora never enter a policy's fitness");
        assert!(matches!(refusal, CorpusRefusal::ForbiddenPath { .. }));
    }
    // The generated corpora the search uses read no path at all, which is why
    // no path refusal can be bypassed by naming one of them.
    assert!(matches!(
        training_corpus().source(),
        evocc_policy::CorpusSource::Generated { .. }
    ));
}

#[test]
fn a_searched_policy_names_itself_and_its_registry() {
    let weights = OperatorWeights::uniform(1).with(CostAxis::Mul, 6);
    let policy = searched_policy(weights);
    assert_eq!(policy.name(), evocc_policy::SEARCHED_POLICY_NAME);
    assert_eq!(policy.cost(), CostFunction::Vector(weights));
    assert_eq!(
        policy.registry(),
        shipped_policies()[0].registry(),
        "the search changes cost, never the registry"
    );
    assert_eq!(
        policy.schedule(),
        shipped_policies()[0].schedule(),
        "the search changes cost, never the schedule"
    );
}
