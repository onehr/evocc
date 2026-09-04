//! Differential execution of one generated module before and after the
//! scalar equality optimizer, judged by the specification's equality criterion.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    interpret_module, render_module, validate_module, InterpreterLimits, Module, RuntimeValue,
};
use evocc_memory::{AccessLayout, MemoryState, ObjectOrigin, ObjectOwner, Pointer, VolatileEvent};
use evocc_mir::{
    compile_module, lane, lower_module, native_corpus_entry, validate_module as validate_machine,
    MirError, MirLimits, Toolchain,
};
use evocc_proof::{render_evp1, sha256_hex};
use evocc_types::EvoIRType;

use crate::generate::{entry_arguments, generate_module, GeneratorConfig, GeneratorMode};
use crate::shrink::shrink_module;

/// The two independent ceilings one trial runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateBudget {
    /// The bounded scheduler's node and epoch ceilings.
    pub scalar: ScalarEqBudget,
    /// Interpreter fuel charged across every frame of one execution.
    pub step_budget: u64,
}

impl Default for GateBudget {
    fn default() -> Self {
        Self {
            scalar: ScalarEqBudget::new(20_000, 256),
            step_budget: 200_000,
        }
    }
}

/// One static object's surviving bytes, keyed by its cross-run stable origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticObservation {
    pub origin: ObjectOrigin,
    /// One entry per byte: `Some(byte)` when initialized, `None` otherwise.
    pub bytes: Vec<Option<u8>>,
}

/// Everything one defined execution makes observable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinedRun {
    /// Ordinary return values, with the memory and world tokens dropped: the
    /// token identities are host bookkeeping, not observations.
    pub returns: Vec<RuntimeValue>,
    pub volatile: Vec<VolatileEvent>,
    pub statics: Vec<StaticObservation>,
}

/// The result of interpreting one module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Defined(DefinedRun),
    /// The interpreter reported a typed diagnostic; equivalence is modulo this.
    Undefined {
        code: &'static str,
        detail: String,
    },
}

/// The diagnostic the interpreter reports when its fuel runs out.
const FUEL_EXHAUSTED: &str = "E3008";

/// Interpret `module`'s `@main` and record everything the equality criterion
/// compares.
#[must_use]
pub fn observe(module: &Module, arguments: &[RuntimeValue], step_budget: u64) -> Observation {
    match interpret_module(
        module,
        "main",
        arguments,
        InterpreterLimits::new(step_budget),
    ) {
        Ok(outcome) => Observation::Defined(DefinedRun {
            returns: outcome
                .return_values
                .iter()
                .filter(|value| !matches!(value.ty, EvoIRType::Mem(_) | EvoIRType::World))
                .cloned()
                .collect(),
            volatile: outcome.world.volatile_trace().to_vec(),
            statics: static_observations(&outcome.memory),
        }),
        Err(error) => Observation::Undefined {
            code: error.code(),
            detail: error.message(),
        },
    }
}

/// The bytes of every object that outlives the entry frame.
///
/// Automatic objects are tombstoned at return, so only static storage carries
/// an observation across the function boundary. A byte that is still
/// uninitialized is recorded as such rather than as a value, because reading it
/// would itself be the fault the comparison is trying to detect.
fn static_observations(memory: &MemoryState) -> Vec<StaticObservation> {
    let byte = AccessLayout::new(1, 1);
    let mut observations = Vec::new();
    for info in memory.objects() {
        if matches!(info.owner, ObjectOwner::Automatic { .. }) || !info.lifetime.is_alive() {
            continue;
        }
        let mut bytes = Vec::new();
        for offset in 0..info.layout.size_bytes {
            let value = i64::try_from(offset)
                .ok()
                .and_then(|offset| Pointer::new(info.id, offset).ok())
                .filter(|address| memory.is_range_initialized(*address, byte) == Ok(true))
                .and_then(|address| memory.load_bytes(address, byte).ok())
                .and_then(|sequence| sequence.bytes().first().copied());
            bytes.push(value);
        }
        observations.push(StaticObservation {
            origin: info.origin,
            bytes,
        });
    }
    observations.sort_by_key(|observation| (observation.origin.site, observation.origin.instance));
    observations
}

/// Which comparison refused the optimizer's claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MismatchKind {
    /// A transaction of a run names an input digest that is not the previous
    /// transaction's output digest (8.11).
    BrokenChain,
    /// The optimizer's output module does not validate.
    OptimizedModuleInvalid,
    /// The producer itself reported that its proof did not replay.
    ProducerReplayFailed,
    /// A defined execution became undefined, or the reverse.
    TrapBehaviour,
    ReturnValues,
    VolatileTrace,
    StaticMemory,
    /// `evo-proofcheck` refused the certificate.
    ProofReplay,
    /// The Section 5 lowering produced EvoMIR its own Section 2 verifier
    /// refuses.
    MachineModuleInvalid,
    /// The machine lane stopped where the EvoIR lane did not, or the reverse.
    MachineTrapBehaviour,
    MachineReturnValues,
    MachineVolatileTrace,
    MachineStaticMemory,
    /// The fourth lane: the assembled program's exit status is not the eight
    /// bits the EvoIR interpreter's result carries.
    NativeExitStatus,
    /// The fourth lane: a stage of the native pipeline refused a module the
    /// first three lanes agreed on.
    NativeStopped,
}

/// A refused claim together with the smallest input that still refuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub seed: u64,
    pub kind: MismatchKind,
    pub detail: String,
    /// Canonical Section 9 text of the minimized input module.
    pub before: String,
    /// Canonical Section 9 text of the minimized optimizer output.
    pub after: String,
    /// The EVP1 certificate for the minimized input.
    pub proof: String,
    /// Whether the fourth lane assembled, linked and ran the program before
    /// the comparison refused, so a campaign can count a native run that
    /// disagreed alongside the ones that agreed.
    pub native_run: bool,
}

/// What one seed established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Both executions were defined and every comparison agreed.
    Compared {
        regions: usize,
        certificates_replayed: usize,
        optimizer_budget_exhausted: bool,
        /// Regions a fresh transaction over the optimized module still admits.
        /// Section 11.3 calls these a quality metric of the registry rather
        /// than a defect, so they are counted and never refused.
        residual_regions: usize,
        /// Transactions the run kept. Zero when the first admitted nothing.
        transactions: usize,
        stopped_at_ceiling: bool,
        /// Regions carrying at least one rewrite that read a fact, and how many
        /// facts each such rewrite read. Section 11.6 gives every fact-driven
        /// rule a fixed premise count, so the distribution says which side
        /// conditions the campaign actually exercised.
        premised_regions: usize,
        premise_counts: BTreeMap<usize, u64>,
        /// How many congruence events each transaction recorded, and the
        /// regions whose root is replaced on the strength of one with no
        /// rewrite of its own -- what closing congruence bought.
        congruence_counts: BTreeMap<usize, u64>,
        congruent_roots: usize,
        /// Premises that assert a bit about a class a seed owns, and the passes
        /// the Section 8.10 seed fixed point took in each transaction.
        premises_asserting_a_seed: usize,
        seed_fact_work_items: BTreeMap<usize, u64>,
        /// The third lane. Either the optimized module lowered and the EvoMIR
        /// interpreter agreed with the other two, or the Section 5 table
        /// declined it and these codes say which rows it wanted.
        lowered: bool,
        refusal_codes: Vec<&'static str>,
        machine_compared: bool,
        machine_budget_exhausted: bool,
        /// The fourth lane: whether the host ran the program, and the reason it
        /// did not when it did not.
        native_run: bool,
        native_skipped: Option<&'static str>,
    },
    /// The input program was undefined; the specification's equality is modulo
    /// undefined behaviour, so nothing is compared.
    DiscardedUndefined {
        code: &'static str,
    },
    /// Interpreter fuel ran out before the program finished.
    InterpreterBudgetExhausted,
    Mismatch(Box<Mismatch>),
}

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

/// A handle on the standalone `evo-proofcheck` binary.
#[derive(Debug)]
pub struct ProofChecker {
    binary: PathBuf,
    scratch: PathBuf,
}

impl ProofChecker {
    /// Use an explicit checker binary and scratch directory.
    #[must_use]
    pub fn new(binary: PathBuf, scratch: PathBuf) -> Self {
        Self { binary, scratch }
    }

    /// Find the checker next to the running test or binary.
    ///
    /// Nothing here reaches into the optimizer: the checker is a separate
    /// process, which is the entire point of replaying certificates through it.
    pub fn locate() -> Result<Self, String> {
        let mut directory =
            std::env::current_exe().map_err(|error| format!("current executable: {error}"))?;
        directory.pop();
        if directory.ends_with("deps") {
            directory.pop();
        }
        let binary = directory.join(format!("evo-proofcheck{}", std::env::consts::EXE_SUFFIX));
        if !binary.is_file() {
            return Err(format!(
                "{} does not exist; build the workspace so the standalone checker is present",
                binary.display()
            ));
        }
        let scratch = std::env::temp_dir().join(format!(
            "evocc-randgate-{}-{}",
            std::process::id(),
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&scratch)
            .map_err(|error| format!("create {}: {error}", scratch.display()))?;
        Ok(Self { binary, scratch })
    }

    /// The located binary.
    #[must_use]
    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    /// Replay one certificate. `Ok(())` means the checker exited zero.
    pub fn check(&self, before: &str, after: &str, proof: &str) -> Result<(), String> {
        let ordinal = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
        let before_path = self.scratch.join(format!("{ordinal:012}.before.evo"));
        let after_path = self.scratch.join(format!("{ordinal:012}.after.evo"));
        let proof_path = self.scratch.join(format!("{ordinal:012}.proof.evp"));
        fs::write(&before_path, before).map_err(|error| error.to_string())?;
        fs::write(&after_path, after).map_err(|error| error.to_string())?;
        fs::write(&proof_path, proof).map_err(|error| error.to_string())?;
        let output = Command::new(&self.binary)
            .args([&before_path, &after_path, &proof_path])
            .output()
            .map_err(|error| format!("run {}: {error}", self.binary.display()))?;
        let _ = fs::remove_file(&before_path);
        let _ = fs::remove_file(&after_path);
        let _ = fs::remove_file(&proof_path);
        match output.status.code() {
            Some(0) => Ok(()),
            Some(2) => Err("evo-proofcheck reported malformed input (status 2)".to_owned()),
            Some(3) => Err("evo-proofcheck refused the certificate (status 3)".to_owned()),
            other => Err(format!("evo-proofcheck exited with {other:?}")),
        }
    }
}

/// Section 8.11: a run is a chain of at most this many transactions over one
/// module. The ceiling exists so a run that never settles still ends.
pub const MAX_TRANSACTIONS: usize = 8;

/// One complete before/after judgement of a single module.
struct Trial {
    before_text: String,
    after_text: String,
    proof_text: String,
    regions: usize,
    premised_regions: usize,
    premise_counts: BTreeMap<usize, u64>,
    congruence_counts: BTreeMap<usize, u64>,
    congruent_roots: usize,
    premises_asserting_a_seed: usize,
    seed_fact_work_items: BTreeMap<usize, u64>,
    residual_regions: usize,
    /// Transactions the run executed and kept, each of which admitted at least
    /// one region. The transaction that admits nothing is what ends the run and
    /// changes no module, so it is the residual measurement rather than a step.
    transactions: usize,
    stopped_at_ceiling: bool,
    optimizer_budget_exhausted: bool,
    /// Set when either interpretation stopped on fuel. Section 10.4 makes that
    /// stop a limit of the host rather than a property of the program, so it is
    /// neither an undefined-behaviour discard nor a comparison result, whichever
    /// side ran out.
    fuel_exhausted: bool,
    input_undefined: Option<&'static str>,
    refusal: Option<(MismatchKind, String)>,
    /// The third lane. `lowered` is set when every function of the optimized
    /// module had a row in the Section 5 table; otherwise `refusal_codes`
    /// holds the distinct Section 10 codes that stopped it, and nothing is
    /// compared. `machine_compared` is set when the EvoMIR interpreter ran to
    /// completion and its observations were judged.
    lowered: bool,
    refusal_codes: Vec<&'static str>,
    machine_compared: bool,
    machine_budget_exhausted: bool,
    /// The fourth lane. `native_run` is set when the host assembled, linked and
    /// ran the optimized module; `native_skipped` names, in one word, why it
    /// did not.
    native_run: bool,
    native_skipped: Option<&'static str>,
}

/// The one place that recognises a fuel stop, so both sides answer alike.
fn stopped_on_fuel(observation: &Observation) -> bool {
    matches!(observation, Observation::Undefined { code, .. } if *code == FUEL_EXHAUSTED)
}

fn judge(
    module: &Module,
    seed: u64,
    config: GeneratorConfig,
    budget: GateBudget,
    checker: &ProofChecker,
) -> Trial {
    let before_text = render_module(module);
    let arguments = entry_arguments(module, seed);
    let before = observe(module, &arguments, budget.step_budget);
    let before_run = match before {
        Observation::Defined(run) => run,
        Observation::Undefined { code, .. } => {
            return Trial {
                before_text,
                after_text: String::new(),
                proof_text: String::new(),
                regions: 0,
                premised_regions: 0,
                premise_counts: BTreeMap::new(),
                congruence_counts: BTreeMap::new(),
                congruent_roots: 0,
                premises_asserting_a_seed: 0,
                seed_fact_work_items: BTreeMap::new(),
                residual_regions: 0,
                transactions: 0,
                stopped_at_ceiling: false,
                optimizer_budget_exhausted: false,
                fuel_exhausted: code == FUEL_EXHAUSTED,
                input_undefined: Some(code),
                refusal: None,
                lowered: false,
                refusal_codes: Vec::new(),
                machine_compared: false,
                machine_budget_exhausted: false,
                native_run: false,
                native_skipped: None,
            };
        }
    };

    let mut trial = Trial {
        before_text: before_text.clone(),
        after_text: before_text.clone(),
        proof_text: String::new(),
        regions: 0,
        premised_regions: 0,
        premise_counts: BTreeMap::new(),
        congruence_counts: BTreeMap::new(),
        congruent_roots: 0,
        premises_asserting_a_seed: 0,
        seed_fact_work_items: BTreeMap::new(),
        residual_regions: 0,
        transactions: 0,
        stopped_at_ceiling: false,
        optimizer_budget_exhausted: false,
        fuel_exhausted: false,
        input_undefined: None,
        refusal: None,
        lowered: false,
        refusal_codes: Vec::new(),
        machine_compared: false,
        machine_budget_exhausted: false,
        native_run: false,
        native_skipped: None,
    };

    // The run of Section 8.11. Each transaction takes the previous output, each
    // certificate is verified alone, and the chain is verified by the digests
    // the certificates already carry: a producer that optimized a module other
    // than the one it was handed breaks the chain even when every certificate
    // is individually sound.
    let mut current = module.clone();
    let mut current_text = before_text;
    let mut chained_from = sha256_hex(current_text.as_bytes());

    for step in 0..MAX_TRANSACTIONS {
        let outcome = optimize_scalar_module_with_evp1(&current, budget.scalar);
        let admitted = outcome.certificate.regions.len();
        if admitted == 0 {
            // Nothing left to admit: this transaction ends the run and leaves
            // the module alone, which is what a residual of zero means.
            trial.residual_regions = 0;
            return finish(
                trial,
                &before_run,
                &current,
                &arguments,
                budget,
                seed,
                config.mode,
            );
        }
        if step + 1 == MAX_TRANSACTIONS {
            trial.stopped_at_ceiling = true;
        }

        let after_text = render_module(&outcome.module);
        let proof_text = render_evp1(&outcome.certificate);
        trial.regions += admitted;
        for region in &outcome.certificate.regions {
            let mut premised = false;
            // Seed classes are a region's first identifiers (11.3), so a
            // premise below that bound with a bit in its mask is a claim about
            // a value the region treats as opaque -- what the Section 8.10 seed
            // fixed point is for. A premise over a top fact claims nothing and
            // is not counted.
            let seeds = u32::try_from(region.seeds.len()).unwrap_or(u32::MAX);
            for event in &region.events {
                if let evocc_proof::EvpEvent::Rewrite(rewrite) = event {
                    trial.premises_asserting_a_seed += rewrite
                        .premises
                        .iter()
                        .filter(|premise| premise.class < seeds && premise.mask != 0)
                        .count();
                    if !rewrite.premises.is_empty() {
                        premised = true;
                        *trial
                            .premise_counts
                            .entry(rewrite.premises.len())
                            .or_default() += 1;
                    }
                }
            }
            if premised {
                trial.premised_regions += 1;
            }
        }
        *trial
            .congruence_counts
            .entry(outcome.report.congruences)
            .or_default() += 1;
        trial.congruent_roots += outcome.report.congruent_roots;
        *trial
            .seed_fact_work_items
            .entry(outcome.report.seed_fact_work_items)
            .or_default() += 1;
        trial.transactions += 1;
        trial.optimizer_budget_exhausted |= outcome.report.budget_exhausted;
        trial.after_text = after_text.clone();
        trial.proof_text = proof_text.clone();

        if outcome.report.proof_replay_failed {
            trial.refusal = Some((
                MismatchKind::ProducerReplayFailed,
                "the producer reported its own proof did not replay".to_owned(),
            ));
            return trial;
        }
        if outcome.certificate.input_digest != chained_from {
            trial.refusal = Some((
                MismatchKind::BrokenChain,
                format!(
                    "transaction {step} names input digest {} after {chained_from}",
                    outcome.certificate.input_digest
                ),
            ));
            return trial;
        }
        chained_from = outcome.certificate.output_digest.clone();

        if let Err(error) = validate_module(&outcome.module) {
            trial.refusal = Some((MismatchKind::OptimizedModuleInvalid, error));
            return trial;
        }
        let after_run = observe(&outcome.module, &arguments, budget.step_budget);
        if stopped_on_fuel(&after_run) {
            trial.fuel_exhausted = true;
            return trial;
        }
        if let Some(refusal) = compare(&before_run, &after_run) {
            trial.refusal = Some(refusal);
            return trial;
        }
        if let Err(error) = checker.check(&current_text, &after_text, &proof_text) {
            trial.refusal = Some((MismatchKind::ProofReplay, error));
            return trial;
        }

        current = outcome.module;
        current_text = after_text;
    }

    // The ceiling stopped the run, so what a further transaction would admit is
    // the residual this run leaves behind.
    trial.residual_regions = optimize_scalar_module_with_evp1(&current, budget.scalar)
        .certificate
        .regions
        .len();
    finish(
        trial,
        &before_run,
        &current,
        &arguments,
        budget,
        seed,
        config.mode,
    )
}

/// The step budget the machine lane runs under, as a multiple of the EvoIR
/// lane's.
///
/// One EvoIR instruction becomes several EvoMIR instructions -- an extension,
/// a divide and its `msub`, an address pair -- and every one of them charges a
/// step, so a machine execution of the same program takes more fuel than the
/// EvoIR execution it is compared with. The multiple is generous enough that a
/// program the EvoIR lane finished is not stopped here by arithmetic.
const MACHINE_FUEL_MULTIPLE: u64 = 16;

/// The third lane: the EvoMIR interpreter on the lowering of the optimized
/// module, judged against the same observations as the other two.
///
/// EvoMIR Section 9.1: "The randomized semantic gate of the middle end gains a
/// third lane: EvoIR before, EvoIR after optimization, EvoMIR after lowering,
/// all three agreeing or the program is a mismatch to shrink. This lane exists
/// before any native run, so lowering defects are found without an assembler in
/// the loop."
///
/// A refusal is counted, not compared: Section 1 makes a form outside the
/// Section 5 table fail closed, so a module the lowering declines is a
/// measurement of the table's reach rather than a disagreement.
fn finish(
    mut trial: Trial,
    before: &DefinedRun,
    optimized: &Module,
    arguments: &[RuntimeValue],
    budget: GateBudget,
    seed: u64,
    mode: GeneratorMode,
) -> Trial {
    let machine = match lower_module(optimized) {
        Ok(machine) => machine,
        Err(refusals) => {
            let mut codes: Vec<&'static str> =
                refusals.iter().map(|refusal| refusal.code).collect();
            codes.sort_unstable();
            codes.dedup();
            trial.refusal_codes = codes;
            trial.native_skipped = Some("loweringRefused");
            return trial;
        }
    };
    trial.lowered = true;
    if let Err(error) = validate_machine(&machine) {
        trial.refusal = Some((MismatchKind::MachineModuleInvalid, error));
        return trial;
    }

    let result_types: Vec<EvoIRType> = optimized
        .functions
        .iter()
        .find(|function| function.name == "main")
        .map(|function| {
            function
                .ret_tys
                .iter()
                .filter(|ty| !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    let limits = MirLimits::new(budget.step_budget.saturating_mul(MACHINE_FUEL_MULTIPLE));
    let run = match lane::observe_entry(&machine, "main", arguments, &result_types, limits) {
        Ok(run) => run,
        Err(MirError::BudgetExhausted) => {
            trial.machine_budget_exhausted = true;
            return trial;
        }
        Err(error) => {
            trial.refusal = Some((
                MismatchKind::MachineTrapBehaviour,
                format!(
                    "a defined execution became {} {} on the machine",
                    error.code(),
                    error.message()
                ),
            ));
            return trial;
        }
    };

    trial.machine_compared = true;
    if before.returns != run.returns {
        trial.refusal = Some((
            MismatchKind::MachineReturnValues,
            format!("{:?} became {:?}", before.returns, run.returns),
        ));
        return trial;
    }
    if before.volatile != run.world.volatile_trace() {
        trial.refusal = Some((
            MismatchKind::MachineVolatileTrace,
            format!(
                "{} volatile events became {}",
                before.volatile.len(),
                run.world.volatile_trace().len()
            ),
        ));
        return trial;
    }
    let statics = static_observations(&run.memory);
    if before.statics != statics {
        trial.refusal = Some((
            MismatchKind::MachineStaticMemory,
            "surviving static memory differs on the machine".to_owned(),
        ));
        return trial;
    }
    native_lane(trial, before, optimized, seed, mode)
}

/// The host toolchain, located once for the whole process.
///
/// A campaign is a thousand or twenty thousand programs; asking the host for
/// its version once each would be a thousand processes spent on a constant.
/// The answer includes the reason there is none, so a skipped lane is a
/// recorded reason rather than a silence.
fn toolchain() -> Result<&'static Toolchain, &'static str> {
    static LOCATED: OnceLock<Result<Toolchain, String>> = OnceLock::new();
    LOCATED
        .get_or_init(Toolchain::locate)
        .as_ref()
        .map_err(|_| "hostToolchainAbsent")
}

/// The fourth lane: the optimized module assembled, linked and run on the host.
///
/// EvoMIR Section 9: "then the machine lane of the randomized gate gains a
/// fourth, native lane where the host toolchain is present", over the corpus of
/// Section 9's "The native corpus", whose programs "are compared by exit status
/// modulo 256, the eight bits a process status carries". Every reason not to
/// run it is counted, so the lane's silence is never mistaken for its assent.
fn native_lane(
    mut trial: Trial,
    before: &DefinedRun,
    optimized: &Module,
    seed: u64,
    mode: GeneratorMode,
) -> Trial {
    if mode != GeneratorMode::NativeMain {
        trial.native_skipped = Some("notTheNativeCorpus");
        return trial;
    }
    if let Err(reason) = native_corpus_entry(optimized) {
        trial.native_skipped = Some(reason);
        return trial;
    }
    let toolchain = match toolchain() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            trial.native_skipped = Some(reason);
            return trial;
        }
    };
    let text = match compile_module(optimized) {
        Ok(text) => text,
        Err(stop) => {
            trial.refusal = Some((
                MismatchKind::NativeStopped,
                format!("{} stopped: {}", stop.component(), stop.detail()),
            ));
            return trial;
        }
    };
    let observed = match toolchain.exit_status(&format!("gate{seed:x}"), &text) {
        Ok(status) => status,
        Err(stop) => {
            trial.refusal = Some((
                MismatchKind::NativeStopped,
                format!("{} stopped: {}", stop.component(), stop.detail()),
            ));
            return trial;
        }
    };
    trial.native_run = true;

    let expected = before
        .returns
        .first()
        .map_or(0, |value| (value.raw_bits & 0xff) as u8);
    if observed != expected {
        trial.refusal = Some((
            MismatchKind::NativeExitStatus,
            format!("the interpreter said {expected}, the host said {observed}"),
        ));
    }
    trial
}

/// The Section 7 comparison: same value, same observable effects, same trap
/// behaviour. A memory or world token identity is deliberately not compared;
/// it is a fresh SSA name for the same abstract state.
fn compare(before: &DefinedRun, after: &Observation) -> Option<(MismatchKind, String)> {
    let after = match after {
        Observation::Undefined { code, detail } => {
            return Some((
                MismatchKind::TrapBehaviour,
                format!("a defined execution became undefined: {code} {detail}"),
            ));
        }
        Observation::Defined(run) => run,
    };
    if before.returns != after.returns {
        return Some((
            MismatchKind::ReturnValues,
            format!("{:?} became {:?}", before.returns, after.returns),
        ));
    }
    if before.volatile != after.volatile {
        return Some((
            MismatchKind::VolatileTrace,
            format!(
                "{} volatile events became {}",
                before.volatile.len(),
                after.volatile.len()
            ),
        ));
    }
    if before.statics != after.statics {
        return Some((
            MismatchKind::StaticMemory,
            "surviving static memory differs".to_owned(),
        ));
    }
    None
}

/// Generate, optimize, execute and compare the module named by `seed`.
#[must_use]
pub fn run_seed(
    seed: u64,
    config: GeneratorConfig,
    budget: GateBudget,
    checker: &ProofChecker,
) -> Verdict {
    let module = generate_module(seed, config);
    let trial = judge(&module, seed, config, budget, checker);
    if trial.fuel_exhausted {
        return Verdict::InterpreterBudgetExhausted;
    }
    if let Some(code) = trial.input_undefined {
        return Verdict::DiscardedUndefined { code };
    }
    let Some((kind, detail)) = trial.refusal else {
        return Verdict::Compared {
            regions: trial.regions,
            certificates_replayed: trial.transactions,
            optimizer_budget_exhausted: trial.optimizer_budget_exhausted,
            premised_regions: trial.premised_regions,
            premise_counts: trial.premise_counts.clone(),
            congruence_counts: trial.congruence_counts.clone(),
            congruent_roots: trial.congruent_roots,
            premises_asserting_a_seed: trial.premises_asserting_a_seed,
            seed_fact_work_items: trial.seed_fact_work_items.clone(),
            residual_regions: trial.residual_regions,
            transactions: trial.transactions,
            stopped_at_ceiling: trial.stopped_at_ceiling,
            lowered: trial.lowered,
            refusal_codes: trial.refusal_codes.clone(),
            machine_compared: trial.machine_compared,
            machine_budget_exhausted: trial.machine_budget_exhausted,
            native_run: trial.native_run,
            native_skipped: trial.native_skipped,
        };
    };

    // A reduction must keep refusing for the same reason. The kind alone is too
    // coarse: a reduction that removes every admitted region turns "this
    // optimization is not justified" into "this no-op claim does not hold",
    // which is a different defect, and a reproducer that has drifted onto one
    // sends its reader to the wrong place.
    let optimized = trial.regions != 0;
    let native_run = trial.native_run;
    let mut still_refuses = |candidate: &Module| {
        let candidate = judge(candidate, seed, config, budget, checker);
        candidate.input_undefined.is_none()
            && !candidate.fuel_exhausted
            && (candidate.regions != 0) == optimized
            && candidate
                .refusal
                .as_ref()
                .is_some_and(|(candidate_kind, _)| *candidate_kind == kind)
    };
    let minimal = shrink_module(&module, &mut still_refuses, 8);
    let minimal = judge(&minimal, seed, config, budget, checker);

    Verdict::Mismatch(Box::new(Mismatch {
        seed,
        kind,
        detail,
        before: minimal.before_text,
        after: minimal.after_text,
        proof: minimal.proof_text,
        native_run,
    }))
}
