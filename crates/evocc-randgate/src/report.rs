//! Campaign counters. Every number here is produced by a run, never predicted.

use crate::differential::{GateBudget, Mismatch, ProofChecker};
use crate::generate::GeneratorConfig;

/// The measured outcome of a contiguous run of seeds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CampaignReport {
    pub first_seed: u64,
    pub count: u64,
    pub generated: u64,
    pub discarded_undefined: u64,
    pub interpreter_budget_exhausted: u64,
    pub optimizer_budget_exhausted: u64,
    pub compared: u64,
    pub optimized: u64,
    pub regions: u64,
    pub certificates_replayed: u64,
    pub mismatched: u64,
    pub mismatches: Vec<Mismatch>,
    /// Diagnostic codes that caused a discard, with their counts, sorted by
    /// code so the line is stable between runs.
    pub undefined_codes: Vec<(&'static str, u64)>,
}

impl CampaignReport {
    /// The measurement table, one `key=value` per line.
    #[must_use]
    pub fn render(&self) -> String {
        todo!("render the measurement table")
    }
}

/// Run `count` consecutive seeds starting at `first_seed`.
#[must_use]
pub fn run_campaign(
    first_seed: u64,
    count: u64,
    config: GeneratorConfig,
    budget: GateBudget,
    checker: &ProofChecker,
) -> CampaignReport {
    let _ = (first_seed, count, config, budget, checker);
    todo!("run one campaign")
}
