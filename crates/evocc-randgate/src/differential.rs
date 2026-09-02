//! Differential execution of one generated module before and after the
//! scalar equality optimizer, judged by the specification's equality criterion.

use std::path::PathBuf;

use evocc_eqopt::ScalarEqBudget;
use evocc_evoir::{Module, RuntimeValue};
use evocc_memory::{ObjectOrigin, VolatileEvent};

use crate::generate::GeneratorConfig;

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
    Undefined { code: &'static str, detail: String },
}

/// Interpret `module`'s `@main` and record everything the equality criterion
/// compares.
#[must_use]
pub fn observe(module: &Module, arguments: &[RuntimeValue], step_budget: u64) -> Observation {
    let _ = (module, arguments, step_budget);
    todo!("observe one execution")
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
    DiscardedUndefined { code: &'static str },
    /// Interpreter fuel ran out before the program finished.
    InterpreterBudgetExhausted,
    Mismatch(Box<Mismatch>),
}

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

    /// Find the checker next to the running test or binary, building it when a
    /// single-package invocation has not produced it yet.
    pub fn locate() -> Result<Self, String> {
        todo!("locate evo-proofcheck")
    }

    /// The located binary.
    #[must_use]
    pub fn binary(&self) -> &PathBuf {
        &self.binary
    }

    /// Replay one certificate. `Ok(())` means the checker exited zero.
    pub fn check(&self, before: &str, after: &str, proof: &str) -> Result<(), String> {
        let _ = (before, after, proof);
        todo!("replay one certificate in {}", self.scratch.display())
    }
}

/// Generate, optimize, execute and compare the module named by `seed`.
#[must_use]
pub fn run_seed(
    seed: u64,
    config: GeneratorConfig,
    budget: GateBudget,
    checker: &ProofChecker,
) -> Verdict {
    let _ = (seed, config, budget, checker);
    todo!("one differential trial")
}
