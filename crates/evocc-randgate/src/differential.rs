//! Differential execution of one generated module before and after the
//! scalar equality optimizer, judged by the specification's equality criterion.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    interpret_module, render_module, validate_module, InterpreterLimits, Module, RuntimeValue,
};
use evocc_memory::{AccessLayout, MemoryState, ObjectOrigin, ObjectOwner, Pointer, VolatileEvent};
use evocc_proof::render_evp1;
use evocc_types::EvoIRType;

use crate::generate::{entry_arguments, generate_module, GeneratorConfig};
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
}

/// What one seed established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Both executions were defined and every comparison agreed.
    Compared {
        regions: usize,
        certificates_replayed: usize,
        optimizer_budget_exhausted: bool,
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

/// One complete before/after judgement of a single module.
struct Trial {
    before_text: String,
    after_text: String,
    proof_text: String,
    regions: usize,
    optimizer_budget_exhausted: bool,
    /// Set when either interpretation stopped on fuel. Section 10.4 makes that
    /// stop a limit of the host rather than a property of the program, so it is
    /// neither an undefined-behaviour discard nor a comparison result, whichever
    /// side ran out.
    fuel_exhausted: bool,
    input_undefined: Option<&'static str>,
    refusal: Option<(MismatchKind, String)>,
}

/// The one place that recognises a fuel stop, so both sides answer alike.
fn stopped_on_fuel(observation: &Observation) -> bool {
    matches!(observation, Observation::Undefined { code, .. } if *code == FUEL_EXHAUSTED)
}

fn judge(module: &Module, seed: u64, budget: GateBudget, checker: &ProofChecker) -> Trial {
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
                optimizer_budget_exhausted: false,
                fuel_exhausted: code == FUEL_EXHAUSTED,
                input_undefined: Some(code),
                refusal: None,
            };
        }
    };

    let outcome = optimize_scalar_module_with_evp1(module, budget.scalar);
    let after_text = render_module(&outcome.module);
    let proof_text = render_evp1(&outcome.certificate);
    let mut trial = Trial {
        before_text,
        after_text,
        proof_text,
        regions: outcome.certificate.regions.len(),
        optimizer_budget_exhausted: outcome.report.budget_exhausted,
        fuel_exhausted: false,
        input_undefined: None,
        refusal: None,
    };

    if outcome.report.proof_replay_failed {
        trial.refusal = Some((
            MismatchKind::ProducerReplayFailed,
            "the producer reported its own proof did not replay".to_owned(),
        ));
        return trial;
    }
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

    if let Err(error) = checker.check(&trial.before_text, &trial.after_text, &trial.proof_text) {
        trial.refusal = Some((MismatchKind::ProofReplay, error));
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
    let trial = judge(&module, seed, budget, checker);
    if trial.fuel_exhausted {
        return Verdict::InterpreterBudgetExhausted;
    }
    if let Some(code) = trial.input_undefined {
        return Verdict::DiscardedUndefined { code };
    }
    let Some((kind, detail)) = trial.refusal else {
        return Verdict::Compared {
            regions: trial.regions,
            certificates_replayed: 1,
            optimizer_budget_exhausted: trial.optimizer_budget_exhausted,
        };
    };

    // A reduction must keep refusing for the same reason. The kind alone is too
    // coarse: a reduction that removes every admitted region turns "this
    // optimization is not justified" into "this no-op claim does not hold",
    // which is a different defect, and a reproducer that has drifted onto one
    // sends its reader to the wrong place.
    let optimized = trial.regions != 0;
    let mut still_refuses = |candidate: &Module| {
        let candidate = judge(candidate, seed, budget, checker);
        candidate.input_undefined.is_none()
            && !candidate.fuel_exhausted
            && (candidate.regions != 0) == optimized
            && candidate
                .refusal
                .as_ref()
                .is_some_and(|(candidate_kind, _)| *candidate_kind == kind)
    };
    let minimal = shrink_module(&module, &mut still_refuses, 8);
    let minimal = judge(&minimal, seed, budget, checker);

    Verdict::Mismatch(Box::new(Mismatch {
        seed,
        kind,
        detail,
        before: minimal.before_text,
        after: minimal.after_text,
        proof: minimal.proof_text,
    }))
}
