//! Shared driver/evaluator orchestration. The producer never grants VERIFIED.
//! A trusted wrapper supplies independent replay; candidate data cannot supply
//! callbacks. No caller-owned persistent state is committed before the whole
//! chain succeeds, including replay of its terminal zero-region transaction.

use evocc_evoir::{render_module, validate_module, Module};
use evocc_proof::{render_evp1, sha256_hex};

use crate::{
    optimize_scalar_module_certified_persistent_with_policy, CertifiedScalarEqOutcome,
    EqualityState, ExtractionPolicy, ScalarEqReport,
};

macro_rules! labels {
    ($name:ident { $($variant:ident => $label:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const fn label(self) -> &'static str {
                match self { $(Self::$variant => $label),+ }
            }
        }
    };
}

labels!(TransactionResult { Changed => "CHANGED", Unchanged => "UNCHANGED", RolledBackError => "ROLLED_BACK_ERROR" });
labels!(TransactionStop { Normal => "NORMAL", PolicyBudget => "POLICY_BUDGET", InternalError => "INTERNAL_ERROR" });
labels!(CertificateState { Emitted => "EMITTED", None => "NONE" });
labels!(Verification { NotChecked => "NOT_CHECKED", Verified => "VERIFIED", Rejected => "REJECTED" });
labels!(RejectionReason {
    None => "NONE", CertificateMalformed => "CERTIFICATE_MALFORMED",
    ProofInvalid => "PROOF_INVALID", CheckerTimeout => "CHECKER_TIMEOUT",
    CheckerUnavailable => "CHECKER_UNAVAILABLE", CheckerError => "CHECKER_ERROR",
    CertificateMissing => "CERTIFICATE_MISSING", BundleMismatch => "BUNDLE_MISMATCH",
    ProducerError => "PRODUCER_ERROR",
});
labels!(ChainStop { NoChange => "NO_CHANGE", TransactionLimit => "TRANSACTION_LIMIT", TransactionBudget => "TRANSACTION_BUDGET", Error => "ERROR" });

impl RejectionReason {
    pub fn from_checker_outcome(outcome: evocc_proof::CheckerOutcome) -> Result<(), Self> {
        use evocc_proof::CheckerOutcome as C;
        Err(match outcome {
            C::Valid => return Ok(()),
            C::Malformed => Self::CertificateMalformed,
            C::Invalid => Self::ProofInvalid,
            C::Timeout => Self::CheckerTimeout,
            C::Unavailable => Self::CheckerUnavailable,
            C::Error => Self::CheckerError,
        })
    }
    /// EvoIR 11.4. Signal termination and unexpected statuses are not claims
    /// checked and refused. Timeout/unavailability are assigned by the wrapper.
    pub const fn from_checker_exit(code: Option<i32>) -> Self {
        match code {
            Some(0) => Self::None,
            Some(2) => Self::CertificateMalformed,
            Some(3) => Self::ProofInvalid,
            _ => Self::CheckerError,
        }
    }
}

/// The closed state/reason table, independent of cross-field identity checks.
pub fn valid_transaction_state(
    result: TransactionResult,
    stop: TransactionStop,
    certificate: CertificateState,
    verification: Verification,
    reason: RejectionReason,
) -> bool {
    let rollback = result == TransactionResult::RolledBackError;
    if rollback != (stop == TransactionStop::InternalError) {
        return false;
    }
    match (certificate, verification, reason) {
        (CertificateState::None, Verification::NotChecked, RejectionReason::None) => rollback,
        (CertificateState::None, Verification::Rejected, RejectionReason::CertificateMissing) => {
            !rollback
        }
        (
            CertificateState::Emitted,
            Verification::NotChecked | Verification::Verified,
            RejectionReason::None,
        ) => true,
        (
            CertificateState::Emitted,
            Verification::Rejected,
            RejectionReason::CertificateMalformed
            | RejectionReason::ProofInvalid
            | RejectionReason::CheckerTimeout
            | RejectionReason::CheckerUnavailable
            | RejectionReason::CheckerError
            | RejectionReason::BundleMismatch,
        ) => true,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofBundle {
    pub before: String,
    pub after: String,
    pub proof: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionRecord {
    pub index: usize,
    pub input_digest: String,
    pub output_digest: String,
    pub proof_digest: String,
    pub result: TransactionResult,
    pub stop: TransactionStop,
    pub certificate_state: CertificateState,
    pub verification: Verification,
    pub rejection_reason: RejectionReason,
    pub report: ScalarEqReport,
    pub budget: crate::ScalarEqBudget,
    pub bundle: ProofBundle,
}

impl TransactionRecord {
    pub fn validate(&self) -> Result<(), String> {
        if !valid_transaction_state(
            self.result,
            self.stop,
            self.certificate_state,
            self.verification,
            self.rejection_reason,
        ) {
            return Err("illegal transaction state/reason combination".into());
        }
        if self.certificate_state != CertificateState::Emitted
            || self.input_digest != sha256_hex(self.bundle.before.as_bytes())
            || self.output_digest != sha256_hex(self.bundle.after.as_bytes())
            || self.proof_digest != sha256_hex(self.bundle.proof.as_bytes())
        {
            return Err("missing or mismatched retained proof bundle".into());
        }
        let changed = self.bundle.before != self.bundle.after;
        if (self.result == TransactionResult::Changed) != changed
            || (self.result == TransactionResult::Changed) != (self.report.optimized_values > 0)
            || self.report.regions_admitted != self.report.optimized_values
            || (self.result == TransactionResult::RolledBackError)
                != (self.report.fell_back || self.report.proof_replay_failed)
        {
            return Err(
                "transaction result disagrees with retained output/producer telemetry".into(),
            );
        }
        let t = &self.report.telemetry;
        let hit = t.search_nodes_hit
            || t.search_events_hit
            || t.admission_nodes_hit
            || t.admission_regions_hit;
        if hit != self.report.budget_exhausted
            || (self.stop == TransactionStop::PolicyBudget && !hit)
            || (self.stop == TransactionStop::Normal && hit)
            || t.roots_eligible
                .saturating_add(t.roots_tainted)
                .saturating_add(t.roots_unlisted)
                .saturating_add(t.roots_excluded_dependency)
                != t.roots_total
            || t.roots_visited > t.roots_total
            || t.roots_proposed > t.roots_visited
            || t.admission_regions > t.roots_proposed
            || t.admission_regions > self.budget.max_epochs
            || t.admission_proof_nodes > self.budget.max_nodes
            || (t.search_nodes_hit && t.search_bridge_nodes <= self.budget.max_nodes)
            || (t.search_events_hit && t.search_rewrite_events < self.budget.max_epochs)
            || (t.admission_regions_hit && t.admission_regions < self.budget.max_epochs)
            || (t.admission_nodes_hit
                && t.admission_proof_nodes
                    .saturating_add(t.admission_next_nodes)
                    <= self.budget.max_nodes)
            || (self.result != TransactionResult::RolledBackError
                && t.admission_regions != self.report.optimized_values)
        {
            return Err("inconsistent scoped budget/root telemetry".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedScalarChain {
    pub module: Module,
    pub policy: ExtractionPolicy,
    pub checker_digest: Option<String>,
    pub checker_timeout_ms: Option<u128>,
    pub chain_id: String,
    pub policy_digest: String,
    pub input_digest: String,
    pub output_digest: String,
    pub max_transactions: usize,
    pub stop: ChainStop,
    pub transactions: Vec<TransactionRecord>,
    /// Measurement only; excluded from the deterministic chain digest.
    pub optimizer_wall_nanos: u128,
    pub resources: ChainResources,
}

/// Disjoint wall-clock scopes, deliberately outside certificate/chain identity.
/// Replay includes process startup and scratch I/O; identity includes both full
/// executable reads/hashes. The residual is orchestration, not producer work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChainResources {
    pub total_wall_nanos: u128,
    pub checker_wall_nanos: u128,
    pub identity_wall_nanos: u128,
}

impl ChainResources {
    pub fn wrapper_wall_nanos(self, producer_wall_nanos: u128) -> Result<u128, String> {
        self.total_wall_nanos
            .checked_sub(producer_wall_nanos)
            .and_then(|remaining| remaining.checked_sub(self.checker_wall_nanos))
            .and_then(|remaining| remaining.checked_sub(self.identity_wall_nanos))
            .ok_or_else(|| {
                format!(
                    "inconsistent chain wall scopes: total={}, producer={}, checker={}, identity={}",
                    self.total_wall_nanos,
                    producer_wall_nanos,
                    self.checker_wall_nanos,
                    self.identity_wall_nanos
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainFailure {
    pub detail: String,
    pub policy: ExtractionPolicy,
    pub input_digest: String,
    pub checker_digest: Option<String>,
    pub checker_timeout_ms: Option<u128>,
    /// A chain-boundary reason; never replaces a transaction's verdict.
    pub rejection_reason: RejectionReason,
    pub verified_prefix: usize,
    pub transactions: Vec<TransactionRecord>,
    pub optimizer_wall_nanos: u128,
    pub resources: ChainResources,
}

impl VerifiedScalarChain {
    /// Canonical, sorted-key JSON with no source text or nondeterministic timing.
    pub fn record_json(&self) -> String {
        let policy = policy_json(&self.policy);
        let txs = self
            .transactions
            .iter()
            .map(transaction_json)
            .collect::<Vec<_>>()
            .join(",");
        let checker = self
            .checker_digest
            .as_ref()
            .map_or_else(|| "null".to_string(), |d| format!("\"{d}\""));
        let timeout = self
            .checker_timeout_ms
            .map_or_else(|| "null".to_string(), |n| n.to_string());
        format!("{{\"attempted_transactions\":{},\"chain_id\":\"{}\",\"checker_digest\":{checker},\"checker_timeout_ms\":{timeout},\"input_digest\":\"{}\",\"max_transactions\":{},\"output_digest\":\"{}\",\"policy\":{policy},\"policy_digest\":\"{}\",\"publication\":\"VERIFIED\",\"schema\":\"evocc-verified-chain-v1\",\"stop\":\"{}\",\"transactions\":[{txs}],\"verified_prefix\":{}}}", self.transactions.len(),self.chain_id,self.input_digest,self.max_transactions,self.output_digest,self.policy_digest,self.stop.label(),self.transactions.len())
    }

    pub fn record_digest(&self) -> String {
        sha256_hex(self.record_json().as_bytes())
    }

    /// Refuse existing destinations. A receipt appears only after all bundles
    /// are written; on error only this newly owned directory is removed.
    pub fn write_bundle(&self, destination: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        if self.checker_digest.is_none() || self.checker_timeout_ms.is_none_or(|n| n == 0) {
            return Err(
                "standalone checker identity and deadline are required for publication".into(),
            );
        }
        write_records(
            destination,
            &self.transactions,
            &self.record_json(),
            self.resources,
            self.optimizer_wall_nanos,
            true,
        )
    }

    /// Structural publication checks supplement (never replace) independent EVP
    /// replay. Reordering, truncation and codegen input drift are rejected here.
    pub fn validate(&self) -> Result<(), String> {
        if self.transactions.is_empty() || self.transactions.len() > self.max_transactions {
            return Err("invalid attempted chain length".into());
        }
        if self.policy_digest != self.policy.identity_digest()
            || self.max_transactions != self.policy.schedule().max_transactions
        {
            return Err("policy/transaction ceiling identity mismatch".into());
        }
        let id = sha256_hex(
            format!(
                "scalar-chain-v1\n{}\n{}\n",
                self.input_digest, self.policy_digest
            )
            .as_bytes(),
        );
        if id != self.chain_id {
            return Err("chain identity mismatch".into());
        }
        let mut previous = &self.input_digest;
        for (index, transaction) in self.transactions.iter().enumerate() {
            transaction.validate()?;
            if transaction.index != index
                || &transaction.input_digest != previous
                || transaction.budget != self.policy.schedule().budget()
                || transaction.verification != Verification::Verified
                || transaction.result == TransactionResult::RolledBackError
            {
                return Err("unverified or discontinuous chain".into());
            }
            if index + 1 < self.transactions.len()
                && (transaction.result != TransactionResult::Changed
                    || transaction.stop != TransactionStop::Normal)
            {
                return Err("chain continued after a terminal transaction".into());
            }
            previous = &transaction.output_digest;
        }
        let last = self.transactions.last().unwrap();
        let expected_stop = if last.stop == TransactionStop::PolicyBudget {
            ChainStop::TransactionBudget
        } else if last.result == TransactionResult::Unchanged {
            ChainStop::NoChange
        } else {
            ChainStop::TransactionLimit
        };
        if self.stop != expected_stop
            || (self.stop == ChainStop::TransactionLimit
                && self.transactions.len() != self.max_transactions)
            || previous != &self.output_digest
            || sha256_hex(render_module(&self.module).as_bytes()) != self.output_digest
        {
            return Err("chain stop or final codegen identity mismatch".into());
        }
        Ok(())
    }
}

/// One trusted orchestration entry point. The callback is host integration,
/// never a policy field; production callers must use the standalone checker.
pub fn run_verified_scalar_chain(
    module: &Module,
    state: &mut EqualityState,
    policy: &ExtractionPolicy,
    checker: impl FnMut(&ProofBundle) -> Result<(), RejectionReason>,
) -> Result<VerifiedScalarChain, Box<ChainFailure>> {
    run_chain(
        module,
        state,
        policy,
        checker,
        optimize_scalar_module_certified_persistent_with_policy,
    )
}

/// Production wrapper used by both entry points. Freeze/recheck the actual
/// checker executable; a callback-only test run cannot publish a proof bundle.
pub fn run_scalar_chain_with_checker(
    module: &Module,
    state: &mut EqualityState,
    policy: &ExtractionPolicy,
    checker: &evocc_proof::StandaloneProofChecker,
) -> Result<VerifiedScalarChain, Box<ChainFailure>> {
    let started = std::time::Instant::now();
    let mut identity_wall_nanos = 0;
    let result = run_with_checker(module, state, policy, checker, &mut identity_wall_nanos);
    finish_resources(result, started, identity_wall_nanos)
}

fn finish_resources(
    mut result: Result<VerifiedScalarChain, Box<ChainFailure>>,
    started: std::time::Instant,
    identity_wall_nanos: u128,
) -> Result<VerifiedScalarChain, Box<ChainFailure>> {
    let resources = match &mut result {
        Ok(run) => &mut run.resources,
        Err(failure) => &mut failure.resources,
    };
    resources.identity_wall_nanos = identity_wall_nanos;
    resources.total_wall_nanos = started.elapsed().as_nanos();
    result
}

fn run_with_checker(
    module: &Module,
    state: &mut EqualityState,
    policy: &ExtractionPolicy,
    checker: &evocc_proof::StandaloneProofChecker,
    identity_wall_nanos: &mut u128,
) -> Result<VerifiedScalarChain, Box<ChainFailure>> {
    let identity_started = std::time::Instant::now();
    let admitted_identity = checker.admitted_identity();
    *identity_wall_nanos += identity_started.elapsed().as_nanos();
    let identity = admitted_identity
        .and_then(|identity| {
            if !(1..=300_000).contains(&checker.timeout().as_millis()) {
                return Err("checker deadline exceeds the admitted 1..300000 ms profile".into());
            }
            Ok(identity)
        })
        .map_err(|detail| ChainFailure {
            detail,
            policy: *policy,
            input_digest: sha256_hex(render_module(module).as_bytes()),
            checker_digest: None,
            checker_timeout_ms: Some(checker.timeout().as_millis()),
            rejection_reason: if checker.binary().is_file() {
                RejectionReason::BundleMismatch
            } else {
                RejectionReason::CheckerUnavailable
            },
            verified_prefix: 0,
            transactions: Vec::new(),
            optimizer_wall_nanos: 0,
            resources: ChainResources::default(),
        })?;
    let (mut run, staged) = run_chain_staged(
        module,
        state.clone(),
        policy,
        |b| {
            RejectionReason::from_checker_outcome(
                checker.check_typed(&b.before, &b.after, &b.proof),
            )
        },
        optimize_scalar_module_certified_persistent_with_policy,
    )
    .map_err(|mut failure| {
        failure.checker_digest = Some(identity.clone());
        failure.checker_timeout_ms = Some(checker.timeout().as_millis());
        failure
    })?;
    let identity_started = std::time::Instant::now();
    let final_identity = checker.identity_digest();
    *identity_wall_nanos += identity_started.elapsed().as_nanos();
    if final_identity.as_ref() != Ok(&identity) {
        return Err(Box::new(ChainFailure {
            detail: "checker executable changed during replay".into(),
            policy: *policy,
            input_digest: run.input_digest,
            checker_digest: Some(identity),
            checker_timeout_ms: Some(checker.timeout().as_millis()),
            rejection_reason: RejectionReason::BundleMismatch,
            verified_prefix: run.transactions.len(),
            transactions: run.transactions,
            optimizer_wall_nanos: run.optimizer_wall_nanos,
            resources: run.resources,
        }));
    }
    run.checker_digest = Some(identity);
    run.checker_timeout_ms = Some(checker.timeout().as_millis());
    *state = staged;
    Ok(run)
}

fn run_chain(
    module: &Module,
    state: &mut EqualityState,
    policy: &ExtractionPolicy,
    checker: impl FnMut(&ProofBundle) -> Result<(), RejectionReason>,
    producer: impl FnMut(&Module, &mut EqualityState, &ExtractionPolicy) -> CertifiedScalarEqOutcome,
) -> Result<VerifiedScalarChain, Box<ChainFailure>> {
    let started = std::time::Instant::now();
    let result =
        run_chain_staged(module, state.clone(), policy, checker, producer).map(|(run, staged)| {
            *state = staged;
            run
        });
    finish_resources(result, started, 0)
}

/// Own the single chain staging copy. Return it without committing: production
/// must still recheck the checker identity after the last successful replay.
fn run_chain_staged(
    module: &Module,
    mut staged: EqualityState,
    policy: &ExtractionPolicy,
    mut checker: impl FnMut(&ProofBundle) -> Result<(), RejectionReason>,
    mut producer: impl FnMut(&Module, &mut EqualityState, &ExtractionPolicy) -> CertifiedScalarEqOutcome,
) -> Result<(VerifiedScalarChain, EqualityState), Box<ChainFailure>> {
    let mut transactions = Vec::new();
    let mut verified_prefix = 0;
    let mut optimizer_wall_nanos = 0;
    let mut resources = ChainResources::default();
    let input_digest = sha256_hex(render_module(module).as_bytes());
    macro_rules! fail {
        ($message:expr) => {
            return Err(Box::new(ChainFailure {
                detail: $message.to_string(),
                policy: *policy,
                input_digest,
                checker_digest: None,
                checker_timeout_ms: None,
                rejection_reason: transactions.last().map_or(
                    RejectionReason::ProducerError,
                    |t: &TransactionRecord| {
                        if t.rejection_reason != RejectionReason::None {
                            t.rejection_reason
                        } else {
                            RejectionReason::ProducerError
                        }
                    },
                ),
                verified_prefix,
                transactions,
                optimizer_wall_nanos,
                resources,
            }));
        };
    }
    let schedule = policy.schedule();
    if schedule.max_transactions == 0
        || schedule.max_transactions > 8
        || schedule.max_nodes > 100_000
        || schedule.max_epochs > 1_024
    {
        fail!("policy schedule exceeds the admitted scalar profile");
    }
    if let Err(error) = validate_module(module) {
        fail!(error);
    }
    if !staged.matches_module(module) {
        fail!("persistent state belongs to another module");
    }
    if !staged.is_ingested() {
        if let Err(error) = staged.ingest_module(module) {
            fail!(error);
        }
    }
    let policy_digest = policy.identity_digest();
    let chain_id =
        sha256_hex(format!("scalar-chain-v1\n{input_digest}\n{policy_digest}\n").as_bytes());
    let mut current = module.clone();
    let mut stop = ChainStop::TransactionLimit;
    for index in 0..schedule.max_transactions {
        let before = render_module(&current);
        let started = std::time::Instant::now();
        let outcome = producer(&current, &mut staged, policy);
        optimizer_wall_nanos += started.elapsed().as_nanos();
        let after = render_module(&outcome.module);
        let proof = render_evp1(&outcome.certificate);
        let rollback = outcome.report.fell_back || outcome.report.proof_replay_failed;
        let result = if rollback {
            TransactionResult::RolledBackError
        } else if before != after {
            TransactionResult::Changed
        } else {
            TransactionResult::Unchanged
        };
        let tx_stop = if rollback {
            TransactionStop::InternalError
        } else if outcome.report.budget_exhausted {
            TransactionStop::PolicyBudget
        } else {
            TransactionStop::Normal
        };
        let mut record = TransactionRecord {
            index,
            input_digest: outcome.certificate.input_digest.clone(),
            output_digest: outcome.certificate.output_digest.clone(),
            proof_digest: sha256_hex(proof.as_bytes()),
            result,
            stop: tx_stop,
            certificate_state: CertificateState::Emitted,
            verification: Verification::NotChecked,
            rejection_reason: RejectionReason::None,
            report: outcome.report,
            budget: schedule.budget(),
            bundle: ProofBundle {
                before,
                after,
                proof,
            },
        };
        if let Err(detail) = record.validate() {
            record.verification = Verification::Rejected;
            record.rejection_reason = RejectionReason::BundleMismatch;
            transactions.push(record);
            fail!(detail);
        }
        let checker_started = std::time::Instant::now();
        let verdict = checker(&record.bundle);
        resources.checker_wall_nanos += checker_started.elapsed().as_nanos();
        match verdict {
            Ok(()) => record.verification = Verification::Verified,
            Err(reason) => {
                record.verification = Verification::Rejected;
                record.rejection_reason = if matches!(
                    reason,
                    RejectionReason::None
                        | RejectionReason::CertificateMissing
                        | RejectionReason::ProducerError
                ) {
                    RejectionReason::CheckerError
                } else {
                    reason
                };
            }
        }
        let accepted = record.verification == Verification::Verified && !rollback;
        let detail = record.report.internal_error.clone().unwrap_or_else(|| {
            "producer or independent verification rejected the chain".to_owned()
        });
        transactions.push(record);
        if !accepted {
            fail!(detail);
        }
        verified_prefix += 1;
        current = outcome.module;
        if tx_stop == TransactionStop::PolicyBudget {
            stop = ChainStop::TransactionBudget;
            break;
        }
        if result == TransactionResult::Unchanged {
            stop = ChainStop::NoChange;
            break;
        }
    }
    let run = VerifiedScalarChain {
        output_digest: sha256_hex(render_module(&current).as_bytes()),
        module: current,
        chain_id,
        policy: *policy,
        checker_digest: None,
        checker_timeout_ms: None,
        policy_digest,
        input_digest,
        max_transactions: schedule.max_transactions,
        stop,
        transactions,
        optimizer_wall_nanos,
        resources,
    };
    if let Err(detail) = run.validate() {
        return Err(Box::new(ChainFailure {
            detail,
            policy: *policy,
            input_digest: run.input_digest,
            checker_digest: None,
            checker_timeout_ms: None,
            rejection_reason: RejectionReason::BundleMismatch,
            verified_prefix,
            transactions: run.transactions,
            optimizer_wall_nanos,
            resources,
        }));
    }
    Ok((run, staged))
}

impl ChainFailure {
    pub fn record_json(&self) -> String {
        let txs = self
            .transactions
            .iter()
            .map(transaction_json)
            .collect::<Vec<_>>()
            .join(",");
        let policy = policy_json(&self.policy);
        let policy_digest = self.policy.identity_digest();
        let chain_id = sha256_hex(
            format!("scalar-chain-v1\n{}\n{policy_digest}\n", self.input_digest).as_bytes(),
        );
        let checker = self
            .checker_digest
            .as_ref()
            .map_or_else(|| "null".to_string(), |d| format!("\"{d}\""));
        let timeout = self
            .checker_timeout_ms
            .map_or_else(|| "null".to_string(), |n| n.to_string());
        format!("{{\"attempted_transactions\":{},\"chain_id\":\"{chain_id}\",\"checker_digest\":{checker},\"checker_timeout_ms\":{timeout},\"input_digest\":\"{}\",\"max_transactions\":{},\"output_digest\":null,\"policy\":{policy},\"policy_digest\":\"{policy_digest}\",\"publication\":\"REJECTED\",\"rejection_reason\":\"{}\",\"schema\":\"evocc-verified-chain-v1\",\"stop\":\"ERROR\",\"transactions\":[{txs}],\"verified_prefix\":{}}}",self.transactions.len(),self.input_digest,self.policy.schedule().max_transactions,self.rejection_reason.label(),self.verified_prefix)
    }
    pub fn write_bundle(&self, destination: &std::path::Path) -> Result<(), String> {
        write_records(
            destination,
            &self.transactions,
            &self.record_json(),
            self.resources,
            self.optimizer_wall_nanos,
            false,
        )
    }
}

fn policy_json(policy: &ExtractionPolicy) -> String {
    let schedule = policy.schedule();
    let weights = policy
        .cost()
        .weights()
        .values()
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"cost_weights\":[{weights}],\"counter_mapping\":\"legacy-coupled-v1\",\"extractor\":\"selected-pair-v1\",\"registry\":\"{}\",\"registry_digest\":\"{}\",\"schedule\":{{\"max_epochs\":{},\"max_nodes\":{},\"max_transactions\":{}}}}}",policy.registry().name(),crate::evp1::registry_ceiling_digest(policy.registry()),schedule.max_epochs,schedule.max_nodes,schedule.max_transactions)
}

fn transaction_json(t: &TransactionRecord) -> String {
    let s = &t.report.telemetry;
    let mut hits = Vec::new();
    for (scope, unit, consumed, limit, next, hit) in [
        (
            "search.bridge_nodes",
            "bridge_nodes",
            s.search_bridge_nodes,
            t.budget.max_nodes,
            0,
            s.search_nodes_hit,
        ),
        (
            "search.rewrite_events",
            "rewrite_events",
            s.search_rewrite_events,
            t.budget.max_epochs,
            1,
            s.search_events_hit,
        ),
        (
            "admission.accepted_regions",
            "regions",
            s.admission_regions,
            t.budget.max_epochs,
            1,
            s.admission_regions_hit,
        ),
        (
            "admission.proof_nodes",
            "proof_nodes",
            s.admission_proof_nodes,
            t.budget.max_nodes,
            s.admission_next_nodes,
            s.admission_nodes_hit,
        ),
    ] {
        if hit {
            hits.push(format!("{{\"consumed\":{consumed},\"limit\":{limit},\"next\":{next},\"reason\":\"{}\",\"scope\":\"{scope}\",\"unit\":\"{unit}\"}}",if next == 0 {"CONSUMPTION_EXCEEDED"} else {"NEXT_ADMISSION_EXCEEDS"}));
        }
    }
    let hits = hits.join(",");
    let roots = format!("{{\"accepted\":{},\"declined_by_reason\":{{\"policy\":{},\"witness\":{}}},\"eligible\":{},\"excluded_by_reason\":{{\"dependency\":{},\"taint\":{},\"unlisted\":{}}},\"proposed\":{},\"total\":{},\"unvisited\":{},\"visited\":{}}}", t.report.optimized_values,s.declined_policy,s.declined_witness,s.roots_eligible,s.roots_excluded_dependency,s.roots_tainted,s.roots_unlisted,s.roots_proposed,s.roots_total,s.roots_total.saturating_sub(s.roots_visited),s.roots_visited);
    let evp = evocc_proof::parse_evp1(t.bundle.proof.as_bytes())
        .ok()
        .and_then(|c| c.budget)
        .map_or_else(
            || "null".into(),
            |b| {
                format!(
                    "{{\"epoch_marker\":{},\"fuel\":{},\"fuel_bound\":{},\"n\":{},\"nodes\":{}}}",
                    b.epochs.consumed, b.fuel.consumed, b.fuel.bound, b.n, b.nodes.consumed
                )
            },
        );
    let counters = format!("{{\"admission\":{{\"next_nodes\":{},\"nodes_hit\":{},\"proof_nodes\":{},\"regions\":{},\"regions_hit\":{}}},\"evp\":{evp},\"matching_work\":null,\"search\":{{\"bridge_nodes\":{},\"nodes_hit\":{},\"rewrite_events\":{},\"rewrite_events_hit\":{}}}}}",s.admission_next_nodes,s.admission_nodes_hit,s.admission_proof_nodes,s.admission_regions,s.admission_regions_hit,s.search_bridge_nodes,s.search_nodes_hit,s.search_rewrite_events,s.search_events_hit);
    format!("{{\"budget_hits\":[{hits}],\"certificate_state\":\"{}\",\"index\":{},\"input_digest\":\"{}\",\"output_digest\":\"{}\",\"proof_digest\":\"{}\",\"rejection_reason\":\"{}\",\"result\":\"{}\",\"roots\":{roots},\"scoped_counters\":{counters},\"stop\":\"{}\",\"verification\":\"{}\"}}",t.certificate_state.label(),t.index,t.input_digest,t.output_digest,t.proof_digest,t.rejection_reason.label(),t.result.label(),t.stop.label(),t.verification.label())
}

fn write_records(
    destination: &std::path::Path,
    transactions: &[TransactionRecord],
    record: &str,
    resources: ChainResources,
    producer_wall_nanos: u128,
    published: bool,
) -> Result<(), String> {
    // Validate public telemetry before creating a supposedly complete bundle.
    let wrapper_wall_nanos = resources.wrapper_wall_nanos(producer_wall_nanos)?;
    write_new_bundle(destination, record, |destination| {
        if published {
            if let (Some(first), Some(last)) = (transactions.first(), transactions.last()) {
                std::fs::write(destination.join("input.evo"), &first.bundle.before)?;
                std::fs::write(destination.join("output.evo"), &last.bundle.after)?;
            }
        }
        std::fs::write(destination.join("resources.json"), format!(
            "{{\"schema\":\"evocc-chain-resources-v1\",\"producer_wall_ns\":{},\"checker_wall_ns\":{},\"identity_wall_ns\":{},\"wrapper_wall_ns\":{},\"total_wall_ns\":{}}}",
            producer_wall_nanos, resources.checker_wall_nanos, resources.identity_wall_nanos,
            wrapper_wall_nanos, resources.total_wall_nanos))?;
        for t in transactions {
            std::fs::write(
                destination.join(format!("{:03}.before.evo", t.index)),
                &t.bundle.before,
            )?;
            std::fs::write(
                destination.join(format!("{:03}.after.evo", t.index)),
                &t.bundle.after,
            )?;
            std::fs::write(
                destination.join(format!("{:03}.proof.evp", t.index)),
                &t.bundle.proof,
            )?;
            // Additive diagnostics do not change EVP/chain semantics. A
            // measurement consumer must pin this file separately as telemetry.
            let decisions = t.report.policy_decisions.iter().map(|d| {
                let axis = |a: Option<crate::CostAxis>| a.map_or("null".into(), |a| format!("\"{}\"", a.name()));
                format!("{{\"function_index\":{},\"block\":{},\"root\":{},\"root_axis\":{},\"replacement_axis\":{},\"root_cost\":{},\"replacement_cost\":{},\"registry_admitted\":{},\"kept\":{}}}", d.function_index,d.block,d.root,axis(d.root_axis),axis(d.replacement_axis),d.root_cost,d.replacement_cost,d.registry_admitted,d.kept)
            }).collect::<Vec<_>>().join(",");
            std::fs::write(destination.join(format!("{:03}.policy.json", t.index)),
                format!("{{\"schema\":\"evocc-selected-pairs-v1\",\"authority\":\"PRODUCER_TELEMETRY_NOT_PROVED\",\"input_digest\":\"{}\",\"decisions\":[{decisions}]}}",t.input_digest))?;
        }
        Ok(())
    })
}

/// Variant A provenance: exact identity, zero transactions, no asserted EVP
/// proof or checker success. It does not disable existing lowering passes.
pub fn write_identity_chain(module: &Module, destination: &std::path::Path) -> Result<(), String> {
    validate_module(module)?;
    let input = render_module(module);
    let digest = sha256_hex(input.as_bytes());
    let chain_id = sha256_hex(format!("identity-chain-v1\n{digest}\n").as_bytes());
    let record = format!("{{\"attempted_transactions\":0,\"chain_id\":\"{chain_id}\",\"input_digest\":\"{digest}\",\"mode\":\"eqsat-off\",\"output_digest\":\"{digest}\",\"publication\":\"IDENTITY_VERIFIED\",\"schema\":\"evocc-identity-chain-v1\",\"transactions\":[]}}");
    write_new_bundle(destination, &record, |destination| {
        std::fs::write(destination.join("input.evo"), &input)?;
        std::fs::write(destination.join("output.evo"), &input)
    })
}

fn write_new_bundle(
    destination: &std::path::Path,
    record: &str,
    contents: impl FnOnce(&std::path::Path) -> std::io::Result<()>,
) -> Result<(), String> {
    use std::fs;
    fs::create_dir(destination).map_err(|e| format!("create new proof directory: {e}"))?;
    let result = (|| -> std::io::Result<()> {
        contents(destination)?;
        fs::write(destination.join("chain.json.pending"), record)?;
        fs::rename(
            destination.join("chain.json.pending"),
            destination.join("chain.json"),
        )
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(destination);
        return Err(format!("proof bundle was not published: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent::fault;

    #[test]
    fn recovery_proof_does_not_erase_real_apply_or_history_failure() {
        let module = evocc_evoir::parse_section9_module(br#"func @main(%arg0: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>):
    %v1 = const.bits<32> 0
    %v2 = add.bits<32> %v0, %v1
    %v3 = xor.bits<32> %v2, %v1
    ret %v3
}
"#).unwrap();
        let checker = evocc_proof::StandaloneProofChecker::locate().unwrap();
        let expected = checker.identity_digest().unwrap(); // Trust this test's workspace build.
        let checker = checker.with_expected_digest(expected);
        for point in [fault::Point::Apply, fault::Point::History] {
            let mut state = EqualityState::for_module(&module).unwrap();
            let original = state.audit();
            // Fail the second apply, after the first mutation has been staged.
            let _fault = fault::after(point, usize::from(point == fault::Point::Apply));
            let failure = run_scalar_chain_with_checker(
                &module,
                &mut state,
                &ExtractionPolicy::default(),
                &checker,
            )
            .unwrap_err();
            assert_eq!(failure.rejection_reason, RejectionReason::ProducerError);
            assert_eq!(state.audit(), original);
            assert_eq!(failure.verified_prefix, 0);
            assert_eq!(failure.transactions.len(), 1);
            let transaction = &failure.transactions[0];
            assert_eq!(transaction.result, TransactionResult::RolledBackError);
            assert_eq!(transaction.stop, TransactionStop::InternalError);
            assert_eq!(
                transaction.verification,
                Verification::Verified,
                "recovery proof is valid: {failure:?}"
            );
            assert_eq!(transaction.report.optimized_values, 0);
            assert!(transaction.report.telemetry.admission_regions > 0);
            assert_eq!(transaction.bundle.before, transaction.bundle.after);
        }
    }
}
