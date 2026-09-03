//! The policy evaluation harness.
//!
//! Section 7.5 at revision 1.9.50 makes the extraction cost function and the
//! Section 8.11 schedule a producer policy the checker never verifies. This
//! crate is the instrument that measures one such policy: it takes a policy, a
//! corpus of EvoIR modules and a seed, optimizes every module under that
//! policy, interprets it before and after, replays every certificate through
//! the standalone `evo-proofcheck` binary, and reports what it measured as
//! deterministic JSON.
//!
//! Nothing here learns anything. It is the measuring instrument only.
//!
//! **The corpus red line.** The kernel cohort is an acceptance corpus and
//! never enters the fitness of any policy. The harness refuses, in code, any
//! corpus path under `.build/`, under `.evidence/`, or equal to
//! `tests/contracts/day5-kernel-cohort.json`, and refuses any module whose
//! canonical text digest appears in a denylist file.

use std::path::{Path, PathBuf};

use evocc_eqopt::ExtractionPolicy;
use evocc_evoir::Module;
use evocc_randgate::{GeneratorConfig, ProofChecker};

/// Where a corpus came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusSource {
    /// Modules the randomized generator names, one per seed.
    Generated { first_seed: u64, count: u64 },
    /// Every `.evo` file directly under one directory, in file-name order.
    Directory(PathBuf),
}

/// One module of a corpus, with the canonical Section 9 text it hashes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusProgram {
    pub name: String,
    pub module: Module,
    pub text: String,
    pub digest: String,
}

/// Why the harness would not accept a corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusRefusal {
    /// The path is inside a build or evidence tree, or is the kernel cohort.
    ForbiddenPath { path: PathBuf, reason: String },
    /// The module's canonical text digest is on the denylist.
    DeniedDigest { name: String, digest: String },
    /// The path or the denylist could not be read.
    Unreadable { path: PathBuf, detail: String },
    /// A corpus file is not a Section 9 EvoIR module.
    NotEvoIR { path: PathBuf, detail: String },
}

impl CorpusRefusal {
    /// The one-line refusal a caller prints.
    #[must_use]
    pub fn message(&self) -> String {
        todo!("the harness does not report refusals yet")
    }
}

/// A loaded corpus and the digest that names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corpus {
    source: CorpusSource,
    programs: Vec<CorpusProgram>,
    digest: String,
}

impl Corpus {
    /// The modules the generator names for `first_seed .. first_seed + count`.
    ///
    /// A generated corpus reads no path, so no path refusal can apply to it.
    #[must_use]
    pub fn generated(_first_seed: u64, _count: u64, _config: GeneratorConfig) -> Self {
        todo!("the harness does not build a generated corpus yet")
    }

    /// Every `.evo` file directly under `directory`, in file-name order.
    ///
    /// Refuses the directory outright when its path is forbidden, and refuses
    /// any module whose digest the denylist names.
    pub fn from_directory(
        _directory: &Path,
        _denylist: Option<&Path>,
    ) -> Result<Self, CorpusRefusal> {
        todo!("the harness does not load a corpus from disk yet")
    }

    /// Whether the harness will read a corpus from `path` at all.
    pub fn admit_path(_path: &Path) -> Result<(), CorpusRefusal> {
        todo!("the harness does not enforce the corpus red line yet")
    }

    /// The digests a denylist file names, or an empty set when it is absent.
    pub fn read_denylist(_path: &Path) -> Result<Vec<String>, CorpusRefusal> {
        todo!("the harness does not read a denylist yet")
    }

    #[must_use]
    pub fn source(&self) -> &CorpusSource {
        &self.source
    }

    #[must_use]
    pub fn programs(&self) -> &[CorpusProgram] {
        &self.programs
    }

    /// SHA-256 over the canonical module texts, in corpus order.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// What happened to one program under one policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramOutcome {
    /// Both executions were defined and agreed.
    Measured,
    /// The input program was already undefined; Section 7 equality is modulo
    /// undefined behaviour, so it is counted and not compared.
    UndefinedInput { code: String },
    /// Section 10.4 diagnostic E3008: the interpreter ran out of fuel.
    FuelExhausted,
    /// The harness refused the run: the executions disagreed, the output did
    /// not validate, the digest chain broke, or a certificate did not replay.
    Refused { detail: String },
}

/// Everything one program contributes to the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramMeasurement {
    pub name: String,
    pub input_digest: String,
    pub output_digest: String,
    pub outcome: ProgramOutcome,
    pub steps_before: u64,
    pub steps_after: u64,
    pub instructions_before: u64,
    pub instructions_after: u64,
    pub regions: u64,
    pub transactions: u64,
    pub certificates: u64,
    pub certificates_replayed: u64,
    pub optimizer_wall_nanos: u128,
}

/// The sums the report closes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Aggregate {
    pub programs: u64,
    pub measured: u64,
    pub undefined_input: u64,
    pub fuel_exhausted: u64,
    pub refused: u64,
    pub programs_optimized: u64,
    pub steps_before: u64,
    pub steps_after: u64,
    pub instructions_before: u64,
    pub instructions_after: u64,
    pub regions: u64,
    pub transactions: u64,
    pub certificates: u64,
    pub certificates_replayed: u64,
    pub optimizer_wall_nanos: u128,
}

impl Aggregate {
    /// The sums implied by `programs`, recomputed rather than accumulated.
    #[must_use]
    pub fn of(_programs: &[ProgramMeasurement]) -> Self {
        todo!("the harness does not aggregate yet")
    }
}

/// Whether wall time is written into a rendered report.
///
/// Everything else the report holds is a function of the policy, the corpus
/// and the seed. Wall time is not, so a caller that wants a byte-comparable
/// artifact elides it and a caller that wants the measurement keeps it. One
/// renderer serves both; the mode only chooses the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingMode {
    Measured,
    Elided,
}

/// One complete evaluation of one policy over one corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyReport {
    pub policy: ExtractionPolicy,
    pub corpus_source: CorpusSource,
    pub corpus_digest: String,
    pub seed: u64,
    pub step_budget: u64,
    pub programs: Vec<ProgramMeasurement>,
    pub aggregate: Aggregate,
}

impl PolicyReport {
    /// The report as JSON, keys in a fixed order.
    #[must_use]
    pub fn render_json(&self, _timing: TimingMode) -> String {
        todo!("the harness does not render a report yet")
    }

    /// Certificates replayed over certificates produced, as a pair rather than
    /// a ratio: the requirement is that the two numbers are equal.
    #[must_use]
    pub fn replay(&self) -> (u64, u64) {
        (
            self.aggregate.certificates_replayed,
            self.aggregate.certificates,
        )
    }
}

/// Programs whose optimized module text differs between two reports.
///
/// The two reports must name the same corpus, or the comparison is meaningless
/// and this returns `None`.
#[must_use]
pub fn differing_programs(_left: &PolicyReport, _right: &PolicyReport) -> Option<u64> {
    todo!("the harness does not compare two policies yet")
}

/// Evaluate `policy` over `corpus`, replaying every certificate through
/// `checker`.
#[must_use]
pub fn evaluate(
    _policy: &ExtractionPolicy,
    _corpus: &Corpus,
    _seed: u64,
    _step_budget: u64,
    _checker: &ProofChecker,
) -> PolicyReport {
    todo!("the harness does not evaluate a policy yet")
}
