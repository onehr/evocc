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
    axis_sensitivity, candidate_fitness, evaluate, held_out_corpus, search_cost_weights,
    searched_policy, training_corpus, Aggregate, Corpus, CorpusRefusal, PolicyReport,
    ProgramMeasurement, ProgramOutcome, SearchConfig, TimingMode, HELD_OUT_RANGE, SEARCHED_WEIGHTS,
    TRAINING_RANGE,
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
    assert_eq!(
        first.best, second.best,
        "the same seed found two weightings"
    );
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
    let visited = |outcome: &evocc_policy::SearchOutcome| {
        outcome
            .history
            .iter()
            .map(|step| step.weights.render())
            .collect::<Vec<_>>()
    };
    assert_ne!(
        visited(&other),
        visited(&first),
        "a different seed must visit different vectors"
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

/// The measured result of the first search, kept as a test rather than as a
/// sentence: the weighting the search found was never shown a hand-written
/// function, orders `add` below `mul` below `shl`, and extracts on the
/// held-out range exactly what `prefer-add` extracts.
#[test]
fn the_searched_weighting_extracts_what_prefer_add_extracts() {
    let searched = searched_policy(SEARCHED_WEIGHTS);
    let hand_written = evocc_eqopt::policy_by_name("prefer-add").expect("a shipped policy");
    assert_ne!(
        SEARCHED_WEIGHTS,
        hand_written.cost().weights(),
        "the search did not simply reproduce the vector; it reproduced the behaviour"
    );
    assert!(
        SEARCHED_WEIGHTS.get(CostAxis::Add) < SEARCHED_WEIGHTS.get(CostAxis::Mul)
            && SEARCHED_WEIGHTS.get(CostAxis::Mul) < SEARCHED_WEIGHTS.get(CostAxis::Shl),
        "the ordering that decides a doubling is what the search found: {}",
        SEARCHED_WEIGHTS.render()
    );

    let (first, _) = HELD_OUT_RANGE;
    for offset in 0..64 {
        let module = generate_module(first + offset, GeneratorConfig::default());
        assert_eq!(
            render_module(&optimize_scalar_module_with_policy(&module, &searched).module),
            render_module(&optimize_scalar_module_with_policy(&module, &hand_written).module),
            "the searched weighting and prefer-add part on held-out seed {}",
            first + offset
        );
    }
}

/// The coordinate sweep measures the space, so its own arithmetic has to hold
/// before its counts mean anything: the baseline's own weight changes nothing,
/// an axis's union is bounded by the corpus and by the sum over its range, and
/// the axes that matter are the ones with a nonzero union.
#[test]
fn the_axis_sweep_counts_a_union_and_not_a_sum() {
    let corpus = Corpus::generated(3_400_000, 24, GeneratorConfig::default());
    let baseline = evocc_eqopt::policy_by_name("prefer-add")
        .expect("a shipped policy")
        .cost()
        .weights();
    let sweep = axis_sensitivity(baseline, 3, &corpus, 23, STEPS, &checker());

    assert_eq!(sweep.axes.len(), CostAxis::ALL.len());
    assert_eq!(sweep.baseline, baseline);
    assert_eq!(sweep.corpus_digest, corpus.digest());
    assert!(
        sweep.discards.is_empty(),
        "a swept point lost a certificate: {:?}",
        sweep.discards
    );
    // Every axis holds its baseline weight somewhere in 0..=3, and that point
    // is the baseline itself, so it is never evaluated again.
    assert_eq!(sweep.repeats, CostAxis::ALL.len() as u64);
    assert_eq!(
        sweep.evaluations,
        1 + CostAxis::ALL.len() as u64 * 3,
        "one baseline and three moved points per axis"
    );

    for axis in &sweep.axes {
        assert_eq!(axis.baseline_weight, baseline.get(axis.axis));
        assert_eq!(axis.by_weight.len(), 4);
        let at_baseline = axis
            .by_weight
            .iter()
            .find(|(weight, _)| *weight == axis.baseline_weight)
            .expect("the baseline weight is in the range");
        assert_eq!(at_baseline.1, 0, "the baseline cannot differ from itself");
        let sum: u64 = axis.by_weight.iter().map(|(_, changed)| changed).sum();
        assert!(
            axis.programs_changed <= sum,
            "{}: a union of {} exceeds the sum {sum}",
            axis.axis.name(),
            axis.programs_changed
        );
        assert!(axis.programs_changed <= corpus.programs().len() as u64);
        assert_eq!(
            axis.programs_changed == 0,
            sum == 0,
            "{}: an axis changes nothing exactly when every weight changes nothing",
            axis.axis.name()
        );
    }

    assert_eq!(
        sweep.axes_that_matter(),
        sweep
            .axes
            .iter()
            .filter(|axis| axis.programs_changed != 0)
            .count() as u64
    );
    // A zero weight declines a whole rule family (7.5), so it moves programs
    // for almost any operator that appears at all; that is a different claim
    // from an axis the registry lets a policy decide something with. The two
    // counts are therefore reported separately and the positive one is never
    // the larger.
    assert!(
        sweep.axes_that_matter_at_a_positive_weight() <= sweep.axes_that_matter(),
        "an axis that matters at a positive weight matters"
    );
    for axis in &sweep.axes {
        if axis.matters_at_a_positive_weight() {
            assert_ne!(axis.programs_changed, 0);
        }
    }
    let rendered = sweep.render_json();
    assert!(
        rendered.contains("\"axesThatMatter\"")
            && rendered.contains("\"axesThatMatterAtAPositiveWeight\""),
        "the sweep reports both counts it exists to produce"
    );
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
