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

pub mod axes;
pub mod search;

pub use axes::{axis_sensitivity, AxisSensitivity, AxisSweep};
pub use search::{
    candidate_fitness, held_out_corpus, search_cost_weights, searched_policy, training_corpus,
    SearchConfig, SearchEvaluation, SearchOutcome, HELD_OUT_RANGE, SEARCHED_POLICY_NAME,
    SEARCHED_WEIGHTS, TRAINING_RANGE,
};

use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};

use evocc_eqopt::{
    run_scalar_chain_with_checker, ChainResources, EqualityState, ExtractionPolicy,
    TransactionResult, Verification,
};
use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, validate_module, InterpreterError,
    InterpreterLimits, InterpreterOutcome, Module, RuntimeValue,
};
use evocc_proof::sha256_hex;
use evocc_randgate::{entry_arguments, generate_module, DefinedRun, GeneratorConfig, ProofChecker};

/// Section 10.4: the diagnostic the interpreter reports when fuel runs out.
const FUEL_EXHAUSTED: &str = "E3008";

/// Directory prefixes and exact paths no corpus may come from.
///
/// `.build` and `.evidence` are where acceptance evidence is frozen, and the
/// kernel cohort is the acceptance corpus itself. A policy measured against
/// any of them would be tuned against the corpus that is meant to judge it.
const FORBIDDEN_PREFIXES: [&str; 2] = [".build", ".evidence"];
const FORBIDDEN_SUFFIX: [&str; 2] = ["tests", "contracts"];
const FORBIDDEN_FILE: &str = "day5-kernel-cohort.json";

/// Where a corpus came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorpusSource {
    /// Modules the randomized generator names, one per seed.
    Generated { first_seed: u64, count: u64 },
    /// Every `.evo` file directly under one directory, in file-name order.
    Directory(PathBuf),
}

impl CorpusSource {
    /// The one-line description the report prints.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Generated { first_seed, count } => format!("generated:{first_seed}:{count}"),
            Self::Directory(path) => format!("directory:{}", path.display()),
        }
    }
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
        match self {
            Self::ForbiddenPath { path, reason } => {
                format!("refused corpus path {}: {reason}", path.display())
            }
            Self::DeniedDigest { name, digest } => {
                format!("refused module {name}: digest {digest} is on the corpus denylist")
            }
            Self::Unreadable { path, detail } => {
                format!("could not read {}: {detail}", path.display())
            }
            Self::NotEvoIR { path, detail } => {
                format!(
                    "{} is not a Section 9 EvoIR module: {detail}",
                    path.display()
                )
            }
        }
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
    pub fn generated(first_seed: u64, count: u64, config: GeneratorConfig) -> Self {
        let programs = (0..count)
            .map(|offset| {
                let seed = first_seed.wrapping_add(offset);
                let module = generate_module(seed, config);
                let text = render_module(&module);
                CorpusProgram {
                    name: format!("seed-{seed:020}"),
                    digest: sha256_hex(text.as_bytes()),
                    module,
                    text,
                }
            })
            .collect::<Vec<_>>();
        Self {
            digest: corpus_digest(&programs),
            source: CorpusSource::Generated { first_seed, count },
            programs,
        }
    }

    /// Every `.evo` file directly under `directory`, in file-name order.
    ///
    /// Refuses the directory outright when its path is forbidden, and refuses
    /// any module whose digest the denylist names.
    pub fn from_directory(
        directory: &Path,
        denylist: Option<&Path>,
    ) -> Result<Self, CorpusRefusal> {
        Self::admit_path(directory)?;
        let denied = match denylist {
            Some(path) => Self::read_denylist(path)?,
            None => Vec::new(),
        };
        let mut paths = Vec::new();
        let entries = fs::read_dir(directory).map_err(|error| CorpusRefusal::Unreadable {
            path: directory.to_path_buf(),
            detail: error.to_string(),
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| CorpusRefusal::Unreadable {
                path: directory.to_path_buf(),
                detail: error.to_string(),
            })?;
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "evo") {
                paths.push(path);
            }
        }
        paths.sort();

        let mut programs = Vec::new();
        for path in paths {
            Self::admit_path(&path)?;
            let bytes = fs::read(&path).map_err(|error| CorpusRefusal::Unreadable {
                path: path.clone(),
                detail: error.to_string(),
            })?;
            let module =
                parse_section9_module(&bytes).map_err(|detail| CorpusRefusal::NotEvoIR {
                    path: path.clone(),
                    detail,
                })?;
            validate_module(&module).map_err(|detail| CorpusRefusal::NotEvoIR {
                path: path.clone(),
                detail,
            })?;
            let text = render_module(&module);
            let digest = sha256_hex(text.as_bytes());
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            if denied.iter().any(|entry| entry == &digest) {
                return Err(CorpusRefusal::DeniedDigest { name, digest });
            }
            programs.push(CorpusProgram {
                name,
                module,
                text,
                digest,
            });
        }
        Ok(Self {
            digest: corpus_digest(&programs),
            source: CorpusSource::Directory(directory.to_path_buf()),
            programs,
        })
    }

    /// Whether the harness will read a corpus from `path` at all.
    ///
    /// The judgement is on path components rather than on the rendered string,
    /// so `buildings/.buildx` is not `.build` and a leading `./` changes
    /// nothing.
    pub fn admit_path(path: &Path) -> Result<(), CorpusRefusal> {
        let components = path
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for forbidden in FORBIDDEN_PREFIXES {
            if components.iter().any(|part| part == forbidden) {
                return Err(CorpusRefusal::ForbiddenPath {
                    path: path.to_path_buf(),
                    reason: format!(
                        "{forbidden} holds frozen acceptance evidence and never feeds a policy"
                    ),
                });
            }
        }
        if components.last().is_some_and(|last| last == FORBIDDEN_FILE)
            && components
                .len()
                .checked_sub(FORBIDDEN_SUFFIX.len() + 1)
                .is_some_and(|start| components[start..components.len() - 1] == FORBIDDEN_SUFFIX)
        {
            return Err(CorpusRefusal::ForbiddenPath {
                path: path.to_path_buf(),
                reason: "the kernel cohort is an acceptance corpus and never enters a policy's \
                         fitness"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// The digests a denylist file names, or an empty set when it is absent.
    ///
    /// The file is a JSON object with one `digests` array of lowercase hex
    /// strings. It is read with the same small scanner the harness uses for
    /// nothing else, because the workspace carries no JSON dependency and a
    /// denylist that silently failed to parse would be worse than none.
    pub fn read_denylist(path: &Path) -> Result<Vec<String>, CorpusRefusal> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path).map_err(|error| CorpusRefusal::Unreadable {
            path: path.to_path_buf(),
            detail: error.to_string(),
        })?;
        let mut digests = Vec::new();
        let mut rest = text.as_str();
        while let Some(open) = rest.find('"') {
            rest = &rest[open + 1..];
            let Some(close) = rest.find('"') else {
                return Err(CorpusRefusal::Unreadable {
                    path: path.to_path_buf(),
                    detail: "an unterminated string".to_owned(),
                });
            };
            let token = &rest[..close];
            rest = &rest[close + 1..];
            if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                digests.push(token.to_owned());
            }
        }
        digests.sort();
        digests.dedup();
        Ok(digests)
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

fn corpus_digest(programs: &[CorpusProgram]) -> String {
    let mut joined = String::new();
    for program in programs {
        joined.push_str(&program.text);
    }
    sha256_hex(joined.as_bytes())
}

/// Instructions the module holds, terminators excluded.
///
/// A terminator is one per block and no rewrite of this profile can remove
/// one, so counting them would add a constant to both sides of every
/// comparison and hide the movement the measurement is about.
#[must_use]
pub fn instruction_count(module: &Module) -> u64 {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .map(|block| block.insts.len() as u64)
        .sum()
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

impl ProgramOutcome {
    fn tag(&self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::UndefinedInput { .. } => "undefinedInput",
            Self::FuelExhausted => "fuelExhausted",
            Self::Refused { .. } => "refused",
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::Measured | Self::FuelExhausted => String::new(),
            Self::UndefinedInput { code } => code.clone(),
            Self::Refused { detail } => detail.clone(),
        }
    }
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
    /// None means no chain was attempted, not a measured zero-cost chain.
    pub chain_resources: Option<ChainResources>,
    pub chain_record: Option<String>,
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
    pub chain_resources: ChainResources,
}

impl Aggregate {
    /// The sums implied by `programs`, recomputed rather than accumulated.
    #[must_use]
    pub fn of(programs: &[ProgramMeasurement]) -> Self {
        let mut total = Self {
            programs: programs.len() as u64,
            ..Self::default()
        };
        for program in programs {
            match &program.outcome {
                ProgramOutcome::Measured => total.measured += 1,
                ProgramOutcome::UndefinedInput { .. } => total.undefined_input += 1,
                ProgramOutcome::FuelExhausted => total.fuel_exhausted += 1,
                ProgramOutcome::Refused { .. } => total.refused += 1,
            }
            if program.regions != 0 {
                total.programs_optimized += 1;
            }
            total.steps_before += program.steps_before;
            total.steps_after += program.steps_after;
            total.instructions_before += program.instructions_before;
            total.instructions_after += program.instructions_after;
            total.regions += program.regions;
            total.transactions += program.transactions;
            total.certificates += program.certificates;
            total.certificates_replayed += program.certificates_replayed;
            total.optimizer_wall_nanos += program.optimizer_wall_nanos;
            if let Some(resources) = program.chain_resources {
                total.chain_resources.total_wall_nanos += resources.total_wall_nanos;
                total.chain_resources.checker_wall_nanos += resources.checker_wall_nanos;
                total.chain_resources.identity_wall_nanos += resources.identity_wall_nanos;
            }
        }
        total
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
    pub fn render_json(&self, timing: TimingMode) -> String {
        let wall = |nanos: u128| match timing {
            TimingMode::Measured => nanos.to_string(),
            TimingMode::Elided => "null".to_owned(),
        };
        let schedule = self.policy.schedule();
        let mut text = String::new();
        let _ = writeln!(text, "{{");
        let _ = writeln!(text, "  \"schema\": \"evocc-policy-report-v3\",");
        let _ = writeln!(text, "  \"policy\": {{");
        let _ = writeln!(text, "    \"name\": {},", json_string(self.policy.name()));
        let _ = writeln!(
            text,
            "    \"registry\": {},",
            json_string(self.policy.registry().name())
        );
        let _ = writeln!(
            text,
            "    \"costFunction\": {},",
            json_string(&format!("{:?}", self.policy.cost()))
        );
        let _ = writeln!(text, "    \"schedule\": {{");
        let _ = writeln!(text, "      \"maxNodes\": {},", schedule.max_nodes);
        let _ = writeln!(text, "      \"maxEpochs\": {},", schedule.max_epochs);
        let _ = writeln!(
            text,
            "      \"maxTransactions\": {}",
            schedule.max_transactions
        );
        let _ = writeln!(text, "    }}");
        let _ = writeln!(text, "  }},");
        let _ = writeln!(
            text,
            "  \"corpusSource\": {},",
            json_string(&self.corpus_source.describe())
        );
        let _ = writeln!(
            text,
            "  \"corpusDigest\": {},",
            json_string(&self.corpus_digest)
        );
        let _ = writeln!(text, "  \"seed\": {},", self.seed);
        let _ = writeln!(text, "  \"stepBudget\": {},", self.step_budget);
        let _ = writeln!(text, "  \"programs\": [");
        for (index, program) in self.programs.iter().enumerate() {
            let comma = if index + 1 == self.programs.len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(text, "    {{");
            let _ = writeln!(text, "      \"name\": {},", json_string(&program.name));
            let _ = writeln!(
                text,
                "      \"inputDigest\": {},",
                json_string(&program.input_digest)
            );
            let _ = writeln!(
                text,
                "      \"outputDigest\": {},",
                json_string(&program.output_digest)
            );
            let _ = writeln!(
                text,
                "      \"outcome\": {},",
                json_string(program.outcome.tag())
            );
            let _ = writeln!(
                text,
                "      \"outcomeDetail\": {},",
                json_string(&program.outcome.detail())
            );
            let _ = writeln!(text, "      \"stepsBefore\": {},", program.steps_before);
            let _ = writeln!(text, "      \"stepsAfter\": {},", program.steps_after);
            let _ = writeln!(
                text,
                "      \"instructionsBefore\": {},",
                program.instructions_before
            );
            let _ = writeln!(
                text,
                "      \"instructionsAfter\": {},",
                program.instructions_after
            );
            let _ = writeln!(text, "      \"regions\": {},", program.regions);
            let _ = writeln!(text, "      \"transactions\": {},", program.transactions);
            let _ = writeln!(text, "      \"certificates\": {},", program.certificates);
            let _ = writeln!(
                text,
                "      \"certificatesReplayed\": {},",
                program.certificates_replayed
            );
            let _ = writeln!(
                text,
                "      \"optimizerWallNanos\": {},",
                wall(program.optimizer_wall_nanos)
            );
            let _ = writeln!(
                text,
                "      \"chainResources\": {},",
                resources_json(
                    program.chain_resources,
                    program.optimizer_wall_nanos,
                    timing
                )
            );
            let _ = writeln!(
                text,
                "      \"chainRecord\": {}",
                program.chain_record.as_deref().unwrap_or("null")
            );
            let _ = writeln!(text, "    }}{comma}");
        }
        let _ = writeln!(text, "  ],");
        let total = &self.aggregate;
        let _ = writeln!(text, "  \"aggregate\": {{");
        let _ = writeln!(text, "    \"programs\": {},", total.programs);
        let _ = writeln!(text, "    \"measured\": {},", total.measured);
        let _ = writeln!(text, "    \"undefinedInput\": {},", total.undefined_input);
        let _ = writeln!(text, "    \"fuelExhausted\": {},", total.fuel_exhausted);
        let _ = writeln!(text, "    \"refused\": {},", total.refused);
        let _ = writeln!(
            text,
            "    \"programsOptimized\": {},",
            total.programs_optimized
        );
        let _ = writeln!(text, "    \"stepsBefore\": {},", total.steps_before);
        let _ = writeln!(text, "    \"stepsAfter\": {},", total.steps_after);
        let _ = writeln!(
            text,
            "    \"instructionsBefore\": {},",
            total.instructions_before
        );
        let _ = writeln!(
            text,
            "    \"instructionsAfter\": {},",
            total.instructions_after
        );
        let _ = writeln!(text, "    \"regions\": {},", total.regions);
        let _ = writeln!(text, "    \"transactions\": {},", total.transactions);
        let _ = writeln!(text, "    \"certificates\": {},", total.certificates);
        let _ = writeln!(
            text,
            "    \"certificatesReplayed\": {},",
            total.certificates_replayed
        );
        let _ = writeln!(
            text,
            "    \"optimizerWallNanos\": {},",
            wall(total.optimizer_wall_nanos)
        );
        let _ = writeln!(
            text,
            "    \"chainResources\": {}",
            resources_json(
                Some(total.chain_resources),
                total.optimizer_wall_nanos,
                timing
            )
        );
        let _ = writeln!(text, "  }}");
        let _ = writeln!(text, "}}");
        text
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

fn resources_json(resources: Option<ChainResources>, producer: u128, timing: TimingMode) -> String {
    let Some(resources) = resources else {
        return "null".into();
    };
    let wall = |n: u128| match timing {
        TimingMode::Measured => n.to_string(),
        TimingMode::Elided => "null".into(),
    };
    format!("{{\"scope\":\"shared-chain-v1\",\"totalWallNanos\":{},\"checkerWallNanos\":{},\"identityWallNanos\":{},\"wrapperWallNanos\":{}}}",
        wall(resources.total_wall_nanos), wall(resources.checker_wall_nanos),
        wall(resources.identity_wall_nanos), wall(resources.wrapper_wall_nanos(producer)))
}

pub(crate) fn json_string(value: &str) -> String {
    let mut text = String::with_capacity(value.len() + 2);
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                let _ = write!(text, "\\u{:04x}", control as u32);
            }
            other => text.push(other),
        }
    }
    text.push('"');
    text
}

/// Which programs' optimized module text differs between two reports.
///
/// The two reports must name the same corpus, or the comparison is meaningless
/// and this returns `None`. The names come back in corpus order, so a caller
/// that wants the union over several comparisons can take one.
#[must_use]
pub fn differing_program_names<'a>(
    left: &'a PolicyReport,
    right: &PolicyReport,
) -> Option<Vec<&'a str>> {
    if left.corpus_digest != right.corpus_digest || left.programs.len() != right.programs.len() {
        return None;
    }
    let mut differing = Vec::new();
    for (one, other) in left.programs.iter().zip(&right.programs) {
        if one.name != other.name {
            return None;
        }
        if one.output_digest != other.output_digest {
            differing.push(one.name.as_str());
        }
    }
    Some(differing)
}

/// How many programs' optimized module text differs between two reports.
#[must_use]
pub fn differing_programs(left: &PolicyReport, right: &PolicyReport) -> Option<u64> {
    differing_program_names(left, right).map(|names| names.len() as u64)
}

/// Evaluate `policy` over `corpus`, replaying every certificate through
/// `checker`.
#[must_use]
pub fn evaluate(
    policy: &ExtractionPolicy,
    corpus: &Corpus,
    seed: u64,
    step_budget: u64,
    checker: &ProofChecker,
) -> PolicyReport {
    let programs = corpus
        .programs()
        .iter()
        .map(|program| measure(policy, program, seed, step_budget, checker))
        .collect::<Vec<_>>();
    PolicyReport {
        policy: *policy,
        corpus_source: corpus.source().clone(),
        corpus_digest: corpus.digest().to_owned(),
        seed,
        step_budget,
        aggregate: Aggregate::of(&programs),
        programs,
    }
}

/// One program's run: the Section 8.11 chain of transactions the policy's
/// schedule permits, judged before and after by the Section 7 criterion.
fn measure(
    policy: &ExtractionPolicy,
    program: &CorpusProgram,
    seed: u64,
    step_budget: u64,
    checker: &ProofChecker,
) -> ProgramMeasurement {
    measure_with_interpreter(
        policy,
        program,
        seed,
        step_budget,
        checker,
        interpret_module,
    )
}

fn measure_with_interpreter(
    policy: &ExtractionPolicy,
    program: &CorpusProgram,
    seed: u64,
    step_budget: u64,
    checker: &ProofChecker,
    mut execute: impl FnMut(
        &Module,
        &str,
        &[RuntimeValue],
        InterpreterLimits,
    ) -> Result<InterpreterOutcome, InterpreterError>,
) -> ProgramMeasurement {
    let mut measurement = ProgramMeasurement {
        name: program.name.clone(),
        input_digest: program.digest.clone(),
        output_digest: program.digest.clone(),
        outcome: ProgramOutcome::Measured,
        steps_before: 0,
        steps_after: 0,
        instructions_before: instruction_count(&program.module),
        instructions_after: instruction_count(&program.module),
        regions: 0,
        transactions: 0,
        certificates: 0,
        certificates_replayed: 0,
        optimizer_wall_nanos: 0,
        chain_resources: None,
        chain_record: None,
    };
    let arguments = entry_arguments(&program.module, seed);
    let limits = InterpreterLimits::new(step_budget);

    let before_run = match execute(&program.module, "main", &arguments, limits) {
        Ok(outcome) => outcome,
        Err(error) => {
            measurement.outcome = if error.code() == FUEL_EXHAUSTED {
                ProgramOutcome::FuelExhausted
            } else {
                ProgramOutcome::UndefinedInput {
                    code: error.code().to_owned(),
                }
            };
            return measurement;
        }
    };
    measurement.steps_before = before_run.steps;
    measurement.steps_after = before_run.steps;
    let before_observation = DefinedRun::from(&before_run);
    drop(before_run);

    let mut state = match EqualityState::for_module(&program.module) {
        Ok(state) => state,
        Err(detail) => {
            measurement.outcome = ProgramOutcome::Refused { detail };
            return measurement;
        }
    };
    let chain = match run_scalar_chain_with_checker(&program.module, &mut state, policy, checker) {
        Ok(chain) => chain,
        Err(failure) => {
            measurement.transactions = failure.transactions.len() as u64;
            measurement.certificates = failure.transactions.len() as u64;
            measurement.certificates_replayed = failure
                .transactions
                .iter()
                .filter(|t| t.verification == Verification::Verified)
                .count() as u64;
            measurement.optimizer_wall_nanos = failure.optimizer_wall_nanos;
            measurement.chain_resources = Some(failure.resources);
            measurement.chain_record = Some(failure.record_json());
            measurement.outcome = ProgramOutcome::Refused {
                detail: failure.detail,
            };
            return measurement;
        }
    };
    measurement.transactions = chain.transactions.len() as u64;
    measurement.certificates = measurement.transactions;
    measurement.certificates_replayed = measurement.transactions;
    measurement.regions = chain
        .transactions
        .iter()
        .map(|t| t.report.optimized_values as u64)
        .sum();
    measurement.optimizer_wall_nanos = chain.optimizer_wall_nanos;
    measurement.chain_resources = Some(chain.resources);
    measurement.chain_record = Some(chain.record_json());
    // Every certificate, including the terminal no-op, was independently replayed.
    // Identical bytes with identical inputs reuse the last execution. Every
    // changed module still gets its own complete step/observation check.
    for transaction in &chain.transactions {
        if transaction.result == TransactionResult::Unchanged {
            debug_assert_eq!(transaction.bundle.before, transaction.bundle.after);
            debug_assert_eq!(measurement.output_digest, transaction.output_digest);
            continue;
        }
        let after = parse_section9_module(transaction.bundle.after.as_bytes())
            .expect("verified chain parses");
        let after_run = match execute(&after, "main", &arguments, limits) {
            Ok(run) => run,
            Err(error) if error.code() == FUEL_EXHAUSTED => {
                measurement.outcome = ProgramOutcome::FuelExhausted;
                return measurement;
            }
            Err(error) => {
                measurement.outcome = ProgramOutcome::Refused {
                    detail: format!(
                        "a defined execution became undefined: {} {}",
                        error.code(),
                        error.message()
                    ),
                };
                return measurement;
            }
        };
        let after_observation = DefinedRun::from(&after_run);
        if after_observation != before_observation {
            measurement.outcome = ProgramOutcome::Refused {
                detail: "the optimized module is not observationally equal to the input".to_owned(),
            };
            return measurement;
        }

        measurement.steps_after = after_run.steps;
        measurement.instructions_after = instruction_count(&after);
        measurement.output_digest = transaction.output_digest.clone();
    }
    measurement
}

#[cfg(test)]
mod measurement_tests {
    use super::*;

    fn program() -> CorpusProgram {
        let module = parse_section9_module(include_bytes!(
            "../../../tests/fixtures/eqsat-performance/identity-tail.evo"
        ))
        .unwrap();
        let text = render_module(&module);
        CorpusProgram {
            name: "identity-tail".into(),
            digest: sha256_hex(text.as_bytes()),
            text,
            module,
        }
    }

    fn checker() -> ProofChecker {
        let checker = ProofChecker::locate().unwrap();
        let identity = checker.identity_digest().unwrap();
        checker.with_expected_digest(identity)
    }

    #[test]
    fn one_execution_per_distinct_module_but_every_certificate_replayed() {
        let program = program();
        for (weights, expected_runs, expected_proofs) in [(1, 2, 2), (0, 1, 1)] {
            let policy = ExtractionPolicy::default().with_cost(evocc_eqopt::CostFunction::Vector(
                evocc_eqopt::OperatorWeights::uniform(weights),
            ));
            let mut runs = 0;
            let result = measure_with_interpreter(
                &policy,
                &program,
                7,
                1000,
                &checker(),
                |module, name, args, limits| {
                    runs += 1;
                    interpret_module(module, name, args, limits)
                },
            );
            assert_eq!(result.outcome, ProgramOutcome::Measured);
            assert_eq!(runs, expected_runs);
            assert_eq!(result.certificates_replayed, expected_proofs);
            assert_eq!(result.certificates_replayed, result.certificates);
            let resources = result.chain_resources.unwrap();
            assert!(resources.checker_wall_nanos > 0);
            assert!(resources.identity_wall_nanos > 0);
            assert_eq!(
                resources.total_wall_nanos,
                result.optimizer_wall_nanos
                    + resources.checker_wall_nanos
                    + resources.identity_wall_nanos
                    + resources.wrapper_wall_nanos(result.optimizer_wall_nanos)
            );
            assert!(result
                .chain_record
                .unwrap()
                .contains("\"result\":\"UNCHANGED\""));
        }
    }

    #[test]
    fn reused_execution_still_checks_the_changed_observation() {
        let mut runs = 0;
        let result = measure_with_interpreter(
            &ExtractionPolicy::default(),
            &program(),
            7,
            1000,
            &checker(),
            |module, name, args, limits| {
                runs += 1;
                let mut outcome = interpret_module(module, name, args, limits)?;
                if runs == 2 {
                    outcome.return_values.clear();
                }
                Ok(outcome)
            },
        );
        assert_eq!(runs, 2);
        assert!(matches!(result.outcome, ProgramOutcome::Refused { .. }));
    }

    #[test]
    fn undefined_or_fuel_exhausted_input_never_enters_the_optimizer() {
        let result = measure(&ExtractionPolicy::default(), &program(), 7, 0, &checker());
        assert_eq!(result.outcome, ProgramOutcome::FuelExhausted);
        assert_eq!(result.certificates, 0);
        assert!(result.chain_record.is_none());
        assert!(result.chain_resources.is_none());
    }
}
