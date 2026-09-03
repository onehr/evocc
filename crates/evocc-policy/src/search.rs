//! A deterministic search over the cost vector.
//!
//! Section 7.5 ("Cost is a policy") lets a producer extract under any cost
//! function that is deterministic for a given module, budget and policy name.
//! `policy.rs` in `evocc-eqopt` writes such a function out as a vector of
//! per-operator weights; this module searches that vector.
//!
//! **The algorithm, stated.** A (mu+lambda) evolutionary loop over integer
//! weight vectors, every step drawn from one seeded [`Rng`]:
//!
//! 1. The initial population is `population` vectors, each coordinate drawn
//!    uniformly from `0 ..= weight_max`. No hand-written function is planted
//!    in it, so a searched weighting that matches one has found it.
//! 2. Each generation evaluates every unevaluated member, sorts the whole
//!    population by `(fitness, coordinates)` -- the coordinates break ties, so
//!    the order is total and does not depend on evaluation order -- keeps the
//!    best `parents`, and makes `children` mutants, child `i` from parent
//!    `i % parents`. A mutation redraws one or two coordinates.
//! 3. After `generations` generations the best vector ever evaluated is the
//!    result; the best fitness after each generation is the trajectory.
//!
//! **Fitness** is the total dynamic interpreter step count after optimization
//! over the training corpus, which is the pre-backend fitness signal the day-9
//! contract names. A candidate whose report did not replay every certificate,
//! or refused any program, has no fitness at all: it is discarded and counted,
//! never scored as if it were merely bad.
//!
//! **The red line.** Training and held-out corpora are generated seed ranges,
//! disjoint by construction, and the harness's refusal of `.build/`,
//! `.evidence/` and the kernel cohort stands underneath both. The kernel
//! cohort never enters either.

use std::collections::BTreeMap;
use std::time::Instant;

use evocc_eqopt::{
    CostAxis, CostFunction, ExtractionPolicy, OperatorWeights, RegistryCeiling, SchedulePolicy,
};
use evocc_randgate::{GeneratorConfig, ProofChecker, Rng};

use crate::{evaluate, Corpus, PolicyReport};

/// The name every searched policy reports under.
pub const SEARCHED_POLICY_NAME: &str = "searched-weights";

/// The training corpus: `(first seed, count)`.
pub const TRAINING_RANGE: (u64, u64) = (1_000_000, 256);

/// The held-out corpus: `(first seed, count)`. Disjoint from
/// [`TRAINING_RANGE`] by construction, which `cost_search.rs` checks rather
/// than assumes.
pub const HELD_OUT_RANGE: (u64, u64) = (2_000_000, 256);

/// What the first search over this space found, recorded so it can be
/// re-measured rather than re-derived.
///
/// It is the result of [`SearchConfig::default`] over [`TRAINING_RANGE`] at
/// harness seed 7 and a 200000-step budget: training corpus digest
/// `831c4a20e3ce48ce46f6b01cde821563132d06dc5829de3613d87a573b316968`,
/// best training fitness 37183, 83 evaluations, 0 candidates discarded. The
/// search was shown no hand-written function, and what it found orders
/// `add` below `mul` below `shl` -- so on the held-out range it extracts
/// exactly what `prefer-add` extracts, program for program, which
/// `cost_search.rs` pins.
pub const SEARCHED_WEIGHTS: OperatorWeights =
    OperatorWeights::from_values([1, 1, 3, 7, 1, 6, 7, 4, 5, 1, 2, 3, 7]);

/// A policy that differs from the shipped ones only in its cost vector.
#[must_use]
pub fn searched_policy(weights: OperatorWeights) -> ExtractionPolicy {
    ExtractionPolicy::new(
        SEARCHED_POLICY_NAME,
        RegistryCeiling::V4,
        CostFunction::Vector(weights),
        SchedulePolicy::default(),
    )
}

/// The corpus the search trains on.
#[must_use]
pub fn training_corpus() -> Corpus {
    let (first, count) = TRAINING_RANGE;
    Corpus::generated(first, count, GeneratorConfig::default())
}

/// The corpus the searched weighting is judged on, once.
#[must_use]
pub fn held_out_corpus() -> Corpus {
    let (first, count) = HELD_OUT_RANGE;
    Corpus::generated(first, count, GeneratorConfig::default())
}

/// The fitness of one evaluated candidate, or why it has none.
///
/// The requirement is the contract's, not a threshold: every certificate the
/// candidate produced must have replayed through the standalone checker, and
/// no program may have been refused. A candidate that fails either is not a
/// worse policy, it is not a measurement.
pub fn candidate_fitness(report: &PolicyReport) -> Result<u64, String> {
    let (replayed, produced) = report.replay();
    if replayed != produced {
        return Err(format!("replayed {replayed} of {produced} certificates",));
    }
    if report.aggregate.refused != 0 {
        let detail = report
            .programs
            .iter()
            .find_map(|program| match &program.outcome {
                crate::ProgramOutcome::Refused { detail } => {
                    Some(format!("{}: {detail}", program.name))
                }
                _ => None,
            })
            .unwrap_or_else(|| "a refusal without a detail".to_owned());
        return Err(format!(
            "{} refused programs, the first being {detail}",
            report.aggregate.refused
        ));
    }
    Ok(report.aggregate.steps_after)
}

/// The parameters of one search run, all of them written down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchConfig {
    /// The seed of the search's own random source.
    pub seed: u64,
    /// How many vectors the initial population holds.
    pub population: usize,
    /// mu: how many survive a generation.
    pub parents: usize,
    /// lambda: how many mutants a generation makes.
    pub children: usize,
    /// How many generations run after the initial population.
    pub generations: usize,
    /// The inclusive upper bound of every coordinate.
    pub weight_max: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            seed: 0x5EED_C057,
            population: 12,
            parents: 4,
            children: 8,
            generations: 10,
            weight_max: 7,
        }
    }
}

impl SearchConfig {
    /// The one-line statement of the search a report prints.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "(mu+lambda) seed={} population={} mu={} lambda={} generations={} weights=0..={}",
            self.seed,
            self.population,
            self.parents,
            self.children,
            self.generations,
            self.weight_max
        )
    }
}

/// What one candidate cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEvaluation {
    pub weights: OperatorWeights,
    pub generation: usize,
    /// `Ok` is the fitness; `Err` is why the candidate was discarded.
    pub fitness: Result<u64, String>,
}

/// Everything one search run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOutcome {
    pub config: SearchConfig,
    pub corpus_digest: String,
    pub best: OperatorWeights,
    pub best_fitness: u64,
    /// Best fitness after the initial population and after each generation.
    pub trajectory: Vec<u64>,
    /// Harness evaluations actually performed.
    pub evaluations: u64,
    /// Candidates served from the memo because an identical vector had already
    /// been evaluated. They cost nothing and are counted separately so that
    /// `evaluations` is the honest price of the search.
    pub repeats: u64,
    /// Candidates discarded because they did not replay every certificate.
    pub discarded: u64,
    /// One line per discarded candidate.
    pub discards: Vec<String>,
    /// Every evaluation, in the order it was performed.
    pub history: Vec<SearchEvaluation>,
    pub wall_nanos: u128,
}

/// Draw one uniformly random weight vector.
fn draw(rng: &mut Rng, weight_max: u64) -> OperatorWeights {
    let mut weights = OperatorWeights::uniform(0);
    for axis in CostAxis::ALL {
        weights = weights.with(axis, rng.below(weight_max + 1));
    }
    weights
}

/// Redraw one or two coordinates of `parent`.
fn mutate(rng: &mut Rng, parent: OperatorWeights, weight_max: u64) -> OperatorWeights {
    let redraws = 1 + rng.below(2);
    let mut child = parent;
    for _ in 0..redraws {
        let axis = CostAxis::ALL[rng.below(CostAxis::ALL.len() as u64) as usize];
        child = child.with(axis, rng.below(weight_max + 1));
    }
    child
}

/// Run the search stated at the top of this module.
///
/// `seed` and `step_budget` are the harness's, not the search's: they name the
/// entry arguments and the interpreter fuel every candidate is measured under,
/// and they are held fixed so that two candidates differ only in cost.
#[must_use]
pub fn search_cost_weights(
    config: &SearchConfig,
    corpus: &Corpus,
    seed: u64,
    step_budget: u64,
    checker: &ProofChecker,
) -> SearchOutcome {
    let started = Instant::now();
    let mut rng = Rng::from_seed(config.seed);
    let mut scores: BTreeMap<[u64; 13], Result<u64, String>> = BTreeMap::new();
    let mut outcome = SearchOutcome {
        config: *config,
        corpus_digest: corpus.digest().to_owned(),
        best: OperatorWeights::uniform(1),
        best_fitness: u64::MAX,
        trajectory: Vec::new(),
        evaluations: 0,
        repeats: 0,
        discarded: 0,
        discards: Vec::new(),
        history: Vec::new(),
        wall_nanos: 0,
    };

    let mut population = (0..config.population)
        .map(|_| draw(&mut rng, config.weight_max))
        .collect::<Vec<_>>();

    for generation in 0..=config.generations {
        for candidate in &population {
            let key = candidate.values();
            if scores.contains_key(&key) {
                outcome.repeats += 1;
                continue;
            }
            let report = evaluate(
                &searched_policy(*candidate),
                corpus,
                seed,
                step_budget,
                checker,
            );
            outcome.evaluations += 1;
            let fitness = candidate_fitness(&report);
            if let Err(reason) = &fitness {
                outcome.discarded += 1;
                outcome
                    .discards
                    .push(format!("{}: {reason}", candidate.render()));
            } else if let Ok(value) = &fitness {
                // The same order the population is ranked by, so "best" means
                // one thing in this module: fitness first, coordinates to
                // break a tie. Many vectors reach the same fitness, and a tie
                // broken by evaluation order would make the answer depend on
                // the walk rather than on the space.
                if (*value, candidate.values()) < (outcome.best_fitness, outcome.best.values()) {
                    outcome.best_fitness = *value;
                    outcome.best = *candidate;
                }
            }
            outcome.history.push(SearchEvaluation {
                weights: *candidate,
                generation,
                fitness: fitness.clone(),
            });
            scores.insert(key, fitness);
        }
        outcome.trajectory.push(outcome.best_fitness);

        if generation == config.generations {
            break;
        }
        // Rank the whole population: fitness first, coordinates to break
        // ties. A discarded candidate sorts after every scored one and is
        // never a parent.
        population.sort_by(|left, right| {
            let rank = |weights: &OperatorWeights| match scores.get(&weights.values()) {
                Some(Ok(fitness)) => (0_u8, *fitness),
                _ => (1_u8, u64::MAX),
            };
            rank(left)
                .cmp(&rank(right))
                .then_with(|| left.values().cmp(&right.values()))
        });
        population.dedup();
        population.truncate(config.parents.max(1));
        let parents = population.clone();
        for index in 0..config.children {
            let parent = parents[index % parents.len()];
            population.push(mutate(&mut rng, parent, config.weight_max));
        }
    }

    outcome.wall_nanos = started.elapsed().as_nanos();
    outcome
}

impl SearchOutcome {
    /// The run as deterministic JSON, wall time excepted.
    #[must_use]
    pub fn render_json(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::new();
        let _ = writeln!(text, "{{");
        let _ = writeln!(text, "  \"search\": \"{}\",", self.config.describe());
        let _ = writeln!(text, "  \"corpusDigest\": \"{}\",", self.corpus_digest);
        let _ = writeln!(text, "  \"best\": \"{}\",", self.best.render());
        let _ = writeln!(text, "  \"bestFitness\": {},", self.best_fitness);
        let trajectory = self
            .trajectory
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(text, "  \"trajectory\": [{trajectory}],");
        let _ = writeln!(text, "  \"evaluations\": {},", self.evaluations);
        let _ = writeln!(text, "  \"repeats\": {},", self.repeats);
        let _ = writeln!(text, "  \"discarded\": {},", self.discarded);
        let _ = writeln!(text, "  \"discards\": [");
        for (index, discard) in self.discards.iter().enumerate() {
            let comma = if index + 1 == self.discards.len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(text, "    {}{comma}", crate::json_string(discard));
        }
        let _ = writeln!(text, "  ],");
        let _ = writeln!(text, "  \"wallNanos\": {}", self.wall_nanos);
        let _ = writeln!(text, "}}");
        text
    }
}
