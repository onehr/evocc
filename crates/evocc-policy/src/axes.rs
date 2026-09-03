//! What each weight axis is worth, measured rather than argued.
//!
//! The search over cost found that under `p0.scalar.v4` the only thing a cost
//! function decides is which spelling of a doubling survives, because the
//! doubling triangle is the only rule family whose right-hand side emits an
//! instruction. That is a claim about the *space*, not about one search, so it
//! is measured directly: hold a baseline vector fixed, move one axis across
//! its whole range, and count the programs whose optimized module changes. An
//! axis that never changes a program is an axis this registry gives a policy
//! no use for.
//!
//! Nothing here learns anything and nothing here is optimized. It is one
//! coordinate sweep around a fixed point, run through the same harness every
//! other measurement uses, over a generated corpus the acceptance corpora
//! never enter.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::time::Instant;

use evocc_eqopt::{CostAxis, OperatorWeights};
use evocc_randgate::ProofChecker;

use crate::search::{candidate_fitness, searched_policy};
use crate::{differing_program_names, evaluate, json_string, Corpus};

/// What one axis is worth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisSensitivity {
    pub axis: CostAxis,
    /// The weight this axis holds in the baseline.
    pub baseline_weight: u64,
    /// Programs whose output differs from the baseline for at least one weight
    /// on this axis: the union over the range, not a sum over it.
    pub programs_changed: u64,
    /// `(weight, programs changed)` for every weight in the range. The
    /// baseline's own weight is in the list and is necessarily zero.
    pub by_weight: Vec<(u64, u64)>,
}

/// The whole sweep, and what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisSweep {
    pub baseline: OperatorWeights,
    pub corpus_digest: String,
    pub weight_max: u64,
    pub axes: Vec<AxisSensitivity>,
    /// Harness evaluations actually performed, the baseline included.
    pub evaluations: u64,
    /// Points served from the memo: every axis holds its baseline weight
    /// somewhere in its range, and that point is the baseline itself.
    pub repeats: u64,
    /// Points that did not replay every certificate. A sweep is a
    /// measurement, so one of these voids its cell rather than filling it.
    pub discards: Vec<String>,
    pub wall_nanos: u128,
}

impl AxisSensitivity {
    /// Whether this axis changed a program at some weight above zero.
    ///
    /// Zero is a different kind of move from every other weight: Section 7.5
    /// notes that a zero cost makes no region rooted at that operator a strict
    /// improvement, so setting an axis to zero declines a whole rule family
    /// rather than choosing between two spellings of one term. Nearly every
    /// axis changes programs that way, which says only that the operator
    /// appears. An axis that also changes a program at a positive weight is an
    /// axis the registry gives a policy a real choice on.
    #[must_use]
    pub fn matters_at_a_positive_weight(&self) -> bool {
        self.by_weight
            .iter()
            .any(|(weight, changed)| *weight != 0 && *changed != 0)
    }
}

impl AxisSweep {
    /// Axes that changed at least one program at any weight, zero included.
    #[must_use]
    pub fn axes_that_matter(&self) -> u64 {
        self.axes
            .iter()
            .filter(|axis| axis.programs_changed != 0)
            .count() as u64
    }

    /// Axes that changed at least one program at a weight above zero.
    #[must_use]
    pub fn axes_that_matter_at_a_positive_weight(&self) -> u64 {
        self.axes
            .iter()
            .filter(|axis| axis.matters_at_a_positive_weight())
            .count() as u64
    }

    /// The sweep as deterministic JSON, wall time excepted.
    #[must_use]
    pub fn render_json(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(text, "{{");
        let _ = writeln!(text, "  \"baseline\": \"{}\",", self.baseline.render());
        let _ = writeln!(text, "  \"corpusDigest\": \"{}\",", self.corpus_digest);
        let _ = writeln!(text, "  \"weightMax\": {},", self.weight_max);
        let _ = writeln!(text, "  \"axes\": [");
        for (index, axis) in self.axes.iter().enumerate() {
            let comma = if index + 1 == self.axes.len() {
                ""
            } else {
                ","
            };
            let by_weight = axis
                .by_weight
                .iter()
                .map(|(weight, changed)| format!("[{weight}, {changed}]"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                text,
                "    {{ \"axis\": \"{}\", \"baselineWeight\": {}, \"programsChanged\": {}, \
                 \"mattersAtAPositiveWeight\": {}, \"byWeight\": [{by_weight}] }}{comma}",
                axis.axis.name(),
                axis.baseline_weight,
                axis.programs_changed,
                axis.matters_at_a_positive_weight()
            );
        }
        let _ = writeln!(text, "  ],");
        let _ = writeln!(text, "  \"axesThatMatter\": {},", self.axes_that_matter());
        let _ = writeln!(
            text,
            "  \"axesThatMatterAtAPositiveWeight\": {},",
            self.axes_that_matter_at_a_positive_weight()
        );
        let _ = writeln!(text, "  \"evaluations\": {},", self.evaluations);
        let _ = writeln!(text, "  \"repeats\": {},", self.repeats);
        let _ = writeln!(text, "  \"discarded\": {},", self.discards.len());
        let _ = writeln!(text, "  \"discards\": [");
        for (index, discard) in self.discards.iter().enumerate() {
            let comma = if index + 1 == self.discards.len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(text, "    {}{comma}", json_string(discard));
        }
        let _ = writeln!(text, "  ],");
        let _ = writeln!(text, "  \"wallNanos\": {}", self.wall_nanos);
        let _ = writeln!(text, "}}");
        text
    }
}

/// Move each axis of `baseline` across `0 ..= weight_max` on its own and count
/// the programs each move changes.
#[must_use]
pub fn axis_sensitivity(
    baseline: OperatorWeights,
    weight_max: u64,
    corpus: &Corpus,
    seed: u64,
    step_budget: u64,
    checker: &ProofChecker,
) -> AxisSweep {
    let started = Instant::now();
    let mut sweep = AxisSweep {
        baseline,
        corpus_digest: corpus.digest().to_owned(),
        weight_max,
        axes: Vec::new(),
        evaluations: 1,
        repeats: 0,
        discards: Vec::new(),
        wall_nanos: 0,
    };
    let reference = evaluate(
        &searched_policy(baseline),
        corpus,
        seed,
        step_budget,
        checker,
    );
    if let Err(reason) = candidate_fitness(&reference) {
        sweep
            .discards
            .push(format!("{} (baseline): {reason}", baseline.render()));
    }

    for axis in CostAxis::ALL {
        let baseline_weight = baseline.get(axis);
        let mut by_weight = Vec::new();
        let mut changed_anywhere: BTreeSet<String> = BTreeSet::new();
        for weight in 0..=weight_max {
            if weight == baseline_weight {
                sweep.repeats += 1;
                by_weight.push((weight, 0));
                continue;
            }
            let moved = baseline.with(axis, weight);
            let report = evaluate(&searched_policy(moved), corpus, seed, step_budget, checker);
            sweep.evaluations += 1;
            if let Err(reason) = candidate_fitness(&report) {
                sweep.discards.push(format!("{}: {reason}", moved.render()));
                by_weight.push((weight, 0));
                continue;
            }
            let names = differing_program_names(&reference, &report)
                .expect("one corpus, two policies, so the two reports are comparable");
            by_weight.push((weight, names.len() as u64));
            changed_anywhere.extend(names.into_iter().map(str::to_owned));
        }
        sweep.axes.push(AxisSensitivity {
            axis,
            baseline_weight,
            programs_changed: changed_anywhere.len() as u64,
            by_weight,
        });
    }

    sweep.wall_nanos = started.elapsed().as_nanos();
    sweep
}
