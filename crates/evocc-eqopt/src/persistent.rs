//! Persistent equality state and transactional scalar optimization.

use std::collections::HashMap;

use evocc_egraph::{
    EClassId, Egraph, EgraphSnapshot, MergeOutcome, NodeId, OpId, Pattern, PatternChild,
    ProofContext, RuleSnapshot,
};
use evocc_evoir::{
    inst_results, render_target_module, validate_module, Inst, Module, Value, IMPLICIT_TARGET,
};
use evocc_proof::{parse_evp1, render_evp1, sha256, sha256_hex};
use evocc_types::EvoIRType;

use crate::evp1::{
    optimize_scalar_module_with_evp1_transaction_for_target, policy_for, CertifiedScalarEqOutcome,
};
use crate::scalar_eq::{
    ingest_scalar_inst, mask_raw, operator_id, scalar_operands, scalar_result, scalar_width,
    AppliedRewrite, EmittedOperand, Replacement, ScalarOperator,
};
use crate::{ExtractionPolicy, ScalarEqBudget, ScalarEqOutcome};

const FIRST_PERSISTENT_OP: OpId = 20_000;
const FIRST_INSTANCE_RULE: u32 = 100_000;
const REVIEWED_REGISTRY: &str = "p0.scalar.v1";
const FROZEN_LAYOUT_IDENTITY: [u8; 32] = [
    0x94, 0x60, 0xb0, 0x84, 0xc6, 0xb1, 0x10, 0x93, 0x53, 0x28, 0xce, 0xd6, 0xac, 0xde, 0xd2, 0xc8,
    0x69, 0xd6, 0x71, 0x8a, 0x6f, 0xdb, 0xfa, 0x69, 0x51, 0x6b, 0x98, 0x9c, 0xc1, 0xd5, 0x97, 0x98,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValueBinding {
    class: EClassId,
    node: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LeafKey {
    Binder {
        function: String,
        value: u32,
        ty: String,
    },
    Constant {
        ty: String,
        raw: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryRecord {
    instance: AppliedRewrite,
    lhs_class: EClassId,
    rhs_class: EClassId,
    result_class: EClassId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryAudit {
    function: String,
    block: u32,
    result: u32,
    rule: crate::ScalarRewrite,
    lhs_class: EClassId,
    rhs_class: EClassId,
    result_class: EClassId,
}

/// Detached state snapshot used to prove rollback and idempotence in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EqualityAudit {
    translation_unit_identity: [u8; 32],
    current_module_identity: [u8; 32],
    target_layout_identity: [u8; 32],
    revision: u64,
    ingested: bool,
    graph: EgraphSnapshot,
    values: Vec<(String, u32, EClassId)>,
    history: Vec<HistoryAudit>,
}

/// Equality information whose lifetime is the compilation, not one pass call.
///
/// The graph stores every representation admitted so far. `values` preserves
/// the EvoIR-to-e-class binding across extraction, while `history` binds every
/// successful union to a reviewed P0 rule instance. A persistent optimization
/// is staged on a clone and replaces this value only after both the low-level
/// certificate and the reviewed instances replay successfully.
#[derive(Clone)]
pub struct EqualityState {
    target: String,
    translation_unit_identity: [u8; 32],
    current_module_identity: [u8; 32],
    target_layout_identity: [u8; 32],
    canonical_input: String,
    revision: u64,
    ingested: bool,
    graph: Egraph,
    values: HashMap<(String, u32), ValueBinding>,
    leaves: HashMap<LeafKey, ValueBinding>,
    operators: HashMap<(OpId, u16), OpId>,
    next_op: OpId,
    next_rule: u32,
    history: Vec<HistoryRecord>,
}

impl EqualityState {
    /// Bind a fresh state to the first canonical module and frozen target
    /// layout. No e-node is added and revision zero is observable.
    pub fn for_module(module: &Module) -> Result<Self, String> {
        Self::for_target_module(module, IMPLICIT_TARGET)
    }

    /// Bind the complete text profile, not the host that runs the optimizer.
    /// Both admitted layouts use the same total integer rules; the explicit
    /// target remains part of every module and certificate identity.
    pub fn for_target_module(module: &Module, target: &str) -> Result<Self, String> {
        if !matches!(target, IMPLICIT_TARGET | "aarch64-apple-darwin") {
            return Err(format!("unsupported rewrite target `{target}`"));
        }
        validate_module(module)?;
        let canonical_input = render_target_module(module, target);
        let translation_unit_identity = sha256(canonical_input.as_bytes());
        Ok(Self {
            target: target.to_owned(),
            translation_unit_identity,
            current_module_identity: translation_unit_identity,
            target_layout_identity: if target == IMPLICIT_TARGET {
                FROZEN_LAYOUT_IDENTITY
            } else {
                sha256(b"aarch64-apple-darwin/evoir-layout-v1-long-double-binary64\n")
            },
            canonical_input,
            revision: 0,
            ingested: false,
            graph: Egraph::new(),
            values: HashMap::new(),
            leaves: HashMap::new(),
            operators: HashMap::new(),
            next_op: FIRST_PERSISTENT_OP,
            next_rule: FIRST_INSTANCE_RULE,
            history: Vec::new(),
        })
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn render(&self, module: &Module) -> String {
        render_target_module(module, &self.target)
    }

    pub(crate) fn matches_module(&self, module: &Module) -> bool {
        self.current_module_identity == sha256(self.render(module).as_bytes())
    }

    pub(crate) fn is_ingested(&self) -> bool {
        self.ingested
    }

    /// Seed block arguments, constants and P0 operations from the bound input.
    /// Re-ingestion of the same module is an idempotent no-op.
    pub fn ingest_module(&mut self, module: &Module) -> Result<(), String> {
        validate_module(module)?;
        if self.render(module) != self.canonical_input {
            return Err("module does not match EqualityState canonical identity".to_owned());
        }
        if self.ingested {
            return Ok(());
        }

        let mut staged = self.clone();
        staged.ingest_validated_module(module)?;
        staged.graph.rebuild();
        staged.ingested = true;
        staged.revision = 1;
        *self = staged;
        Ok(())
    }

    fn ingest_validated_module(&mut self, module: &Module) -> Result<(), String> {
        for function in &module.functions {
            for block in &function.blocks {
                for argument in &block.args {
                    if scalar_width(&argument.value.ty).is_none() {
                        continue;
                    }
                    let binding = self.leaf(LeafKey::Binder {
                        function: function.name.clone(),
                        value: argument.value.id,
                        ty: type_identity(&argument.value.ty),
                    })?;
                    self.bind_value(&function.name, &argument.value, binding)?;
                }
                for inst in &block.insts {
                    self.ingest_instruction(&function.name, inst)?;
                }
            }
        }
        Ok(())
    }

    /// Return the stable insertion handle associated with one EvoIR value.
    #[must_use]
    pub fn class_of(&mut self, function: &str, value: u32) -> Option<EClassId> {
        self.values
            .get(&(function.to_owned(), value))
            .map(|binding| self.graph.find(binding.class))
    }

    /// Resolve a stable insertion handle through the accumulated union-find.
    #[must_use]
    pub fn canonical_class(&mut self, class: EClassId) -> EClassId {
        self.graph.find(class)
    }

    /// Count all retained alternatives in an e-class after canonicalization.
    #[must_use]
    pub fn class_node_count(&mut self, class: EClassId) -> usize {
        let root = self.graph.find(class);
        self.graph
            .audit_snapshot()
            .nodes
            .iter()
            .filter(|node| node.owner == root)
            .count()
    }

    /// Replay both the exact e-graph certificate and the reviewed P0 semantic
    /// instance behind every optimizer-requested union.
    pub fn verify_history(&mut self) -> Result<(), String> {
        #[cfg(test)]
        fault::check(fault::Point::History)?;
        let certificate = self.graph.proof_certificate();
        let context = ProofContext::new(certificate.rules.clone()).map_err(|e| e.to_string())?;
        context.replay(&certificate).map_err(|e| e.to_string())?;

        for (index, record) in self.history.iter().enumerate() {
            if !record.instance.is_reviewed_instance() {
                return Err(format!(
                    "history entry {index} is not an instance of {REVIEWED_REGISTRY}"
                ));
            }
            let lhs = self.graph.find(record.lhs_class);
            let rhs = self.graph.find(record.rhs_class);
            let result = self.graph.find(record.result_class);
            if lhs != rhs || lhs != result {
                return Err(format!(
                    "history entry {index} does not reproduce the persistent partition"
                ));
            }
        }
        Ok(())
    }

    /// Return a complete structural snapshot. Calling this may compress paths
    /// but cannot change its own returned value.
    #[must_use]
    pub fn audit(&mut self) -> EqualityAudit {
        let mut values = self
            .values
            .iter()
            .map(|((function, value), binding)| {
                (function.clone(), *value, self.graph.find(binding.class))
            })
            .collect::<Vec<_>>();
        values.sort();
        let history = self
            .history
            .iter()
            .map(|record| HistoryAudit {
                function: record.instance.function.clone(),
                block: record.instance.block,
                result: record.instance.result.id,
                rule: record.instance.rule,
                lhs_class: record.lhs_class,
                rhs_class: record.rhs_class,
                result_class: record.result_class,
            })
            .collect();
        EqualityAudit {
            translation_unit_identity: self.translation_unit_identity,
            current_module_identity: self.current_module_identity,
            target_layout_identity: self.target_layout_identity,
            revision: self.revision,
            ingested: self.ingested,
            graph: self.graph.audit_snapshot(),
            values,
            history,
        }
    }

    fn ingest_instruction(&mut self, function: &str, inst: &Inst) -> Result<(), String> {
        if let Inst::Const(result, raw) = inst {
            let Some(binding) = self.constant(*raw, &result.ty)? else {
                return Ok(());
            };
            return self.bind_value(function, result, binding);
        }
        let Ok(operator) = ingest_scalar_inst(inst) else {
            return Ok(());
        };
        if operator == ScalarOperator::Const {
            return Ok(());
        }
        let Some(result) = scalar_result(inst) else {
            return Ok(());
        };
        let Some(width) = scalar_width(&result.ty) else {
            return Ok(());
        };
        let operands = scalar_operands(inst)
            .into_iter()
            .map(|value| self.values.get(&(function.to_owned(), value.id)).copied())
            .collect::<Option<Vec<_>>>();
        let Some(operands) = operands else {
            // Eligibility is witness-closed. A syntactically scalar operation
            // depending on poison, an effect, or an unsupported instruction is
            // outside P0 rather than a malformed module.
            return Ok(());
        };
        // The e-graph itself is untyped. Operand classes distinguish input
        // sorts, but conversions can have the same input and different result
        // sorts. Intern the result width as part of the operator identity.
        let op = self.typed_operator(operator, width)?;
        let inserted = self
            .graph
            .insert_with_node(op, operands.iter().map(|binding| binding.class).collect());
        self.bind_value(
            function,
            result,
            ValueBinding {
                class: inserted.eclass,
                node: inserted.node,
            },
        )
    }

    fn bind_value(
        &mut self,
        function: &str,
        value: &Value,
        binding: ValueBinding,
    ) -> Result<(), String> {
        let key = (function.to_owned(), value.id);
        if let Some(existing) = self.values.get(&key) {
            return if *existing == binding {
                Ok(())
            } else {
                Err(format!(
                    "function @{function} value %v{} has two persistent bindings",
                    value.id
                ))
            };
        }
        self.values.insert(key, binding);
        Ok(())
    }

    fn leaf(&mut self, key: LeafKey) -> Result<ValueBinding, String> {
        if let Some(binding) = self.leaves.get(&key) {
            return Ok(*binding);
        }
        let op = self.fresh_operator()?;
        let inserted = self.graph.insert_with_node(op, Vec::new());
        let binding = ValueBinding {
            class: inserted.eclass,
            node: inserted.node,
        };
        self.leaves.insert(key, binding);
        Ok(binding)
    }

    fn fresh_operator(&mut self) -> Result<OpId, String> {
        let op = self.next_op;
        self.next_op = op
            .checked_add(1)
            .ok_or_else(|| "persistent scalar operator space exhausted".to_owned())?;
        Ok(op)
    }

    fn typed_operator(&mut self, operator: ScalarOperator, width: u16) -> Result<OpId, String> {
        let key = (operator_id(operator), width);
        if let Some(&op) = self.operators.get(&key) {
            return Ok(op);
        }
        let op = self.fresh_operator()?;
        self.operators.insert(key, op);
        Ok(op)
    }

    fn constant(&mut self, raw: u64, ty: &EvoIRType) -> Result<Option<ValueBinding>, String> {
        // A width the P0 profile has no representation for binds no leaf; the
        // eligibility fixed point already excludes such a value, and the
        // constant environment is folded before that runs.
        let Some(raw) = mask_raw(raw, ty) else {
            return Ok(None);
        };
        self.leaf(LeafKey::Constant {
            ty: type_identity(ty),
            raw,
        })
        .map(Some)
    }

    fn apply(&mut self, instance: AppliedRewrite) -> Result<bool, String> {
        #[cfg(test)]
        fault::check(fault::Point::Apply)?;
        if !instance.is_reviewed_instance() {
            return Err(format!(
                "optimizer supplied a rewrite outside {REVIEWED_REGISTRY}: @{} %v{} {:?}",
                instance.function, instance.result.id, instance.rule
            ));
        }
        if instance.rule == crate::ScalarRewrite::GuardKnownTrue
            && !self
                .values
                .contains_key(&(instance.function.clone(), instance.result.id))
        {
            // Opaque to P0; introduced only after validating the g0 instance.
            // The ordinary persistent union/history still records its equality.
            let binding = self.leaf(LeafKey::Binder {
                function: instance.function.clone(),
                value: instance.result.id,
                ty: type_identity(&instance.result.ty),
            })?;
            self.bind_value(&instance.function, &instance.result, binding)?;
        }
        let lhs = self
            .values
            .get(&(instance.function.clone(), instance.result.id))
            .copied()
            .ok_or_else(|| {
                format!(
                    "@{} %v{} has no persistent root binding",
                    instance.function, instance.result.id
                )
            })?;
        let rhs = match &instance.replacement {
            Replacement::Alias(value) => self
                .values
                .get(&(instance.function.clone(), value.id))
                .copied()
                .ok_or_else(|| {
                    format!(
                        "@{} replacement %v{} has no persistent binding",
                        instance.function, value.id
                    )
                })?,
            Replacement::Constant(raw) => {
                self.constant(*raw, &instance.result.ty)?.ok_or_else(|| {
                    format!(
                        "@{} replacement constant has a width outside the profile",
                        instance.function
                    )
                })?
            }
            // Section 11.6 "Emitting right-hand sides": the node the rule
            // builds joins the persistent graph like any other, so a later
            // phase asking what else is equal to this value finds it. Its
            // operands are the bindings and the constants the pattern writes.
            Replacement::Emit(term) => {
                let mut children = Vec::new();
                let mut node = None;
                for operand in &term.operands {
                    let binding = match operand {
                        EmittedOperand::Bound(value) => self
                            .values
                            .get(&(instance.function.clone(), value.id))
                            .copied()
                            .ok_or_else(|| {
                                format!(
                                    "@{} emitted operand %v{} has no persistent binding",
                                    instance.function, value.id
                                )
                            })?,
                        EmittedOperand::Constant(raw) => {
                            self.constant(*raw, &term.result_ty)?.ok_or_else(|| {
                                format!(
                                    "@{} emitted constant has a width outside the profile",
                                    instance.function
                                )
                            })?
                        }
                    };
                    children.push(binding.class);
                }
                let width = scalar_width(&term.result_ty)
                    .ok_or_else(|| "emitted result sort is outside P0".to_owned())?;
                let op = self.typed_operator(term.operator, width)?;
                let inserted = self.graph.insert_with_node(op, children);
                node.replace(inserted.node);
                ValueBinding {
                    class: inserted.eclass,
                    node: node.ok_or_else(|| "emitted node was not inserted".to_owned())?,
                }
            }
        };
        let lhs_class = self.graph.find(lhs.class);
        let rhs_class = self.graph.find(rhs.class);
        if lhs_class == rhs_class {
            return Ok(false);
        }

        let lhs_snapshot = self
            .graph
            .node_snapshot(lhs.node)
            .ok_or_else(|| "persistent LHS node witness is missing".to_owned())?;
        let rhs_snapshot = self
            .graph
            .node_snapshot(rhs.node)
            .ok_or_else(|| "persistent RHS node witness is missing".to_owned())?;
        let rule = self.next_rule;
        self.next_rule = self
            .next_rule
            .checked_add(1)
            .ok_or_else(|| "persistent proof rule identity exhausted".to_owned())?;
        self.graph.register_rule(RuleSnapshot {
            id: rule,
            lhs: Pattern {
                op: lhs_snapshot.op,
                children: lhs_snapshot
                    .original_children
                    .iter()
                    .copied()
                    .map(PatternChild::Exact)
                    .collect(),
            },
            rhs: Pattern {
                op: rhs_snapshot.op,
                children: rhs_snapshot
                    .original_children
                    .iter()
                    .copied()
                    .map(PatternChild::Exact)
                    .collect(),
            },
        })?;
        let MergeOutcome::Merged(receipt) =
            self.graph
                .merge_rewrite(rule, lhs.node, rhs.node, Vec::new())?
        else {
            return Ok(false);
        };
        self.history.push(HistoryRecord {
            instance,
            lhs_class: receipt.lhs_pre,
            rhs_class: receipt.rhs_pre,
            result_class: receipt.result,
        });
        Ok(true)
    }

    fn remap_current_values(
        &mut self,
        value_remap: &HashMap<(String, u32), u32>,
    ) -> Result<(), String> {
        let old = std::mem::take(&mut self.values);
        let mut remapped = HashMap::new();
        for ((function, value), binding) in old {
            let Some(new_value) = value_remap.get(&(function.clone(), value)) else {
                continue;
            };
            let key = (function, *new_value);
            if remapped.insert(key.clone(), binding).is_some() {
                return Err(format!(
                    "certified value remap assigns @{} %v{} more than once",
                    key.0, key.1
                ));
            }
        }
        self.values = remapped;
        Ok(())
    }

    fn bind_new_definitions(&mut self, module: &Module) -> Result<(), String> {
        // Emitting RHSs introduce fresh definitions absent from the old-to-new
        // survivor map. Insert their actual post-extraction shapes using current
        // operand classes. Hash-consing reconnects them to the already-proved RHS
        // without a new asserted union or a second equality authority.
        for function in &module.functions {
            for inst in function.blocks.iter().flat_map(|block| &block.insts) {
                if inst_results(inst)
                    .iter()
                    .all(|value| !self.values.contains_key(&(function.name.clone(), value.id)))
                {
                    self.ingest_instruction(&function.name, inst)?;
                }
            }
        }
        self.graph.rebuild();
        Ok(())
    }
}

/// Commit one certified scalar optimization and its persistent equality
/// mutation as a single outer transaction. Any disagreement rolls both back.
#[must_use]
pub fn optimize_scalar_module_certified_persistent(
    module: &Module,
    state: &mut EqualityState,
    budget: ScalarEqBudget,
) -> CertifiedScalarEqOutcome {
    optimize_scalar_module_certified_persistent_with_policy(module, state, &policy_for(budget))
}

/// Policy-aware transaction used by both the driver and evaluation chain.
#[must_use]
pub fn optimize_scalar_module_certified_persistent_with_policy(
    module: &Module,
    state: &mut EqualityState,
    policy: &ExtractionPolicy,
) -> CertifiedScalarEqOutcome {
    let transaction =
        optimize_scalar_module_with_evp1_transaction_for_target(module, policy, state.target());
    let outcome = transaction.outcome;
    if outcome.report.fell_back {
        return outcome;
    }
    if outcome.certificate.regions.len() != transaction.events.len()
        || outcome.report.optimized_values != transaction.events.len()
    {
        return certified_persistent_fallback(
            module,
            state,
            outcome,
            "certificate/event count mismatch".into(),
        );
    }
    let proof = render_evp1(&outcome.certificate);
    if parse_evp1(proof.as_bytes()).as_ref() != Ok(&outcome.certificate) {
        return certified_persistent_fallback(
            module,
            state,
            outcome,
            "certificate text mismatch".into(),
        );
    }
    if !state.matches_module(module) {
        return certified_persistent_fallback(
            module,
            state,
            outcome,
            "persistent input identity mismatch".into(),
        );
    }
    if transaction.events.is_empty() {
        return outcome;
    }
    if !state.ingested {
        return certified_persistent_fallback(
            module,
            state,
            outcome,
            "unseeded persistent state".into(),
        );
    }

    let mut staged = state.clone();
    let mut changed = false;
    for event in transaction.events {
        match staged.apply(event) {
            Ok(applied) => changed |= applied,
            Err(detail) => return certified_persistent_fallback(module, state, outcome, detail),
        }
    }
    staged.graph.rebuild();
    if let Err(detail) = staged
        .remap_current_values(&transaction.value_remap)
        .and_then(|()| staged.bind_new_definitions(&outcome.module))
        .and_then(|()| staged.verify_history())
    {
        return certified_persistent_fallback(module, state, outcome, detail);
    }
    if changed {
        staged.revision = staged.revision.saturating_add(1);
    }
    staged.current_module_identity = sha256(staged.render(&outcome.module).as_bytes());
    *state = staged;
    outcome
}

fn certified_persistent_fallback(
    module: &Module,
    state: &EqualityState,
    mut outcome: CertifiedScalarEqOutcome,
    detail: String,
) -> CertifiedScalarEqOutcome {
    let input = state.render(module);
    let digest = sha256_hex(input.as_bytes());
    outcome.module = module.clone();
    outcome.certificate.input_digest = digest.clone();
    outcome.certificate.regions.clear();
    outcome.certificate.output_digest = digest;
    outcome.report.proof_replay_failed = true;
    outcome.report.fell_back = true;
    outcome.report.internal_error = Some(detail);
    outcome.report.regions_admitted = 0;
    outcome.report.graph_nodes = 0;
    outcome.report.epochs = 0;
    outcome.report.merges = 0;
    outcome.report.optimized_values = 0;
    outcome.report.receipts.clear();
    outcome
}

/// Compatibility view of the same certified persistent transaction. Certificate
/// creation is not independent replay; callers requiring publication authority
/// must still use the verified chain. Budget stops retain admitted regions.
#[must_use]
pub fn optimize_scalar_module_persistent(
    module: &Module,
    state: &mut EqualityState,
    budget: ScalarEqBudget,
) -> ScalarEqOutcome {
    let outcome = optimize_scalar_module_certified_persistent(module, state, budget);
    ScalarEqOutcome {
        module: outcome.module,
        report: outcome.report,
    }
}

fn type_identity(ty: &EvoIRType) -> String {
    format!("{ty:?}")
}

/// Test-only, thread-local injection at the real persistent mutation boundary.
/// No command-line flag, environment variable or release-build hook exists.
#[cfg(test)]
pub(crate) mod fault {
    use std::cell::Cell;
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Point {
        Apply,
        History,
    }
    thread_local! { static NEXT: Cell<Option<(Point, usize)>> = const { Cell::new(None) }; }
    pub struct Guard;
    pub fn after(point: Point, successful_calls: usize) -> Guard {
        NEXT.with(|next| assert!(next.replace(Some((point, successful_calls))).is_none()));
        Guard
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            NEXT.with(|next| next.set(None));
        }
    }
    pub fn check(point: Point) -> Result<(), String> {
        NEXT.with(|next| match next.get() {
            Some((p, 0)) if p == point => {
                next.set(None);
                Err("injected persistent-state failure".into())
            }
            Some((p, remaining)) if p == point => {
                next.set(Some((p, remaining - 1)));
                Ok(())
            }
            _ => Ok(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_layout_identity_tracks_hfa_transport_alignment() {
        assert_eq!(
            FROZEN_LAYOUT_IDENTITY,
            sha256(b"aarch64-unknown-linux-gnu/evoir-layout-v4-hfa-natural-alignment\n")
        );
    }
}
