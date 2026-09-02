//! Campaign counters. Every number here is produced by a run, never predicted.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::differential::{run_seed, GateBudget, Mismatch, ProofChecker, Verdict};
use crate::generate::GeneratorConfig;

/// How many minimized reproducers one report keeps. A campaign that refuses a
/// thousand times has one defect, not a thousand, and the seed list below
/// names every one of them.
const RETAINED_REPRODUCERS: usize = 8;

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
    /// Regions a fresh transaction over an optimized module still admits, and
    /// the programs that have any. Section 11.3 names these a measured quality
    /// metric of the registry: Section 11.6 lets a left-hand `const_like` match
    /// only a constant already in the before witness closure, so a constant an
    /// earlier transaction derived becomes matchable only in the next one.
    pub residual_regions: u64,
    pub residual_programs: u64,
    /// Transactions executed across every run, and how many runs took each
    /// length. A run is Section 8.11's chain of transactions over one module.
    pub transactions: u64,
    pub run_lengths: BTreeMap<usize, u64>,
    /// Runs cut off at `max_transactions` rather than by admitting nothing.
    pub runs_at_ceiling: u64,
    pub certificates_replayed: u64,
    pub mismatched: u64,
    pub mismatches: Vec<Mismatch>,
    /// Every seed that refused, in campaign order.
    pub mismatched_seeds: Vec<u64>,
    /// Diagnostic codes that caused a discard, with their counts, sorted by
    /// code so the line is stable between runs.
    pub undefined_codes: Vec<(&'static str, u64)>,
}

impl CampaignReport {
    /// The measurement table, one `key=value` per line.
    #[must_use]
    pub fn render(&self) -> String {
        let codes = self
            .undefined_codes
            .iter()
            .map(|(code, count)| format!("{code}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        let seeds = self
            .mismatched_seeds
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut text = String::new();
        let _ = writeln!(text, "firstSeed={}", self.first_seed);
        let _ = writeln!(text, "count={}", self.count);
        let _ = writeln!(text, "generated={}", self.generated);
        let _ = writeln!(text, "discardedUndefined={}", self.discarded_undefined);
        let _ = writeln!(
            text,
            "interpreterBudgetExhausted={}",
            self.interpreter_budget_exhausted
        );
        let _ = writeln!(
            text,
            "optimizerBudgetExhausted={}",
            self.optimizer_budget_exhausted
        );
        let _ = writeln!(text, "compared={}", self.compared);
        let _ = writeln!(text, "optimized={}", self.optimized);
        let _ = writeln!(text, "regions={}", self.regions);
        let _ = writeln!(text, "residualRegions={}", self.residual_regions);
        let _ = writeln!(text, "residualPrograms={}", self.residual_programs);
        let lengths = self
            .run_lengths
            .iter()
            .map(|(length, count)| format!("{length}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(text, "transactions={}", self.transactions);
        let _ = writeln!(text, "runLengths={lengths}");
        let _ = writeln!(text, "runsAtCeiling={}", self.runs_at_ceiling);
        let _ = writeln!(text, "certificatesReplayed={}", self.certificates_replayed);
        let _ = writeln!(text, "mismatched={}", self.mismatched);
        let _ = writeln!(text, "mismatchedSeeds={seeds}");
        let _ = writeln!(text, "undefinedCodes={codes}");
        text
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
    let mut report = CampaignReport {
        first_seed,
        count,
        ..CampaignReport::default()
    };
    let mut codes: BTreeMap<&'static str, u64> = BTreeMap::new();

    for offset in 0..count {
        let seed = first_seed.wrapping_add(offset);
        report.generated += 1;
        match run_seed(seed, config, budget, checker) {
            Verdict::Compared {
                regions,
                certificates_replayed,
                optimizer_budget_exhausted,
                residual_regions,
            } => {
                report.compared += 1;
                report.regions += regions as u64;
                report.residual_regions += residual_regions as u64;
                if residual_regions != 0 {
                    report.residual_programs += 1;
                }
                report.certificates_replayed += certificates_replayed as u64;
                if regions > 0 {
                    report.optimized += 1;
                }
                if optimizer_budget_exhausted {
                    report.optimizer_budget_exhausted += 1;
                }
            }
            Verdict::DiscardedUndefined { code } => {
                report.discarded_undefined += 1;
                *codes.entry(code).or_default() += 1;
            }
            Verdict::InterpreterBudgetExhausted => {
                report.interpreter_budget_exhausted += 1;
            }
            Verdict::Mismatch(mismatch) => {
                report.mismatched += 1;
                report.mismatched_seeds.push(seed);
                if report.mismatches.len() < RETAINED_REPRODUCERS {
                    report.mismatches.push(*mismatch);
                }
            }
        }
    }

    report.undefined_codes = codes.into_iter().collect();
    report
}
