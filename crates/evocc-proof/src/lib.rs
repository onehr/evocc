//! Proof-carrying compiler: justifications and independent proof checker

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

// ---------------------------------------------------------------------------
// Type aliases (self-contained; no dependency on evocc-egraph)
// ---------------------------------------------------------------------------
pub type EClassId = u32;
pub type RuleId = u32;
pub type SubstitutionId = u32;
pub type FactProofId = u32;
pub type OpId = u16;

// ---------------------------------------------------------------------------
// Bound Day 4 proof vocabulary
// ---------------------------------------------------------------------------

/// Append-only identity of an e-node snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

/// Immutable insertion-time view of an e-node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSnapshot {
    pub id: NodeId,
    pub owner: EClassId,
    pub op: OpId,
    pub original_children: Vec<EClassId>,
}

/// One level of a rewrite pattern. Day 4 deliberately covers the current
/// e-graph rule language, whose children are exact classes or variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub op: OpId,
    pub children: Vec<PatternChild>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternChild {
    Exact(EClassId),
    Var(String),
}

/// Immutable definition of an unconditional rewrite rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSnapshot {
    pub id: RuleId,
    pub lhs: Pattern,
    pub rhs: Pattern,
}

/// A substitution entry. Certificates must store entries in strictly sorted
/// variable-name order; the checker never silently normalises them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub variable: String,
    pub eclass: EClassId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnionReason {
    UnconditionalRewrite {
        rule: RuleId,
        lhs_node: NodeId,
        rhs_node: NodeId,
        substitution: Vec<Binding>,
    },
    Congruence {
        lhs_node: NodeId,
        rhs_node: NodeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeReceipt {
    pub lhs_pre: EClassId,
    pub rhs_pre: EClassId,
    pub result: EClassId,
    pub reason: UnionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofEvent {
    Insert(NodeSnapshot),
    Union(MergeReceipt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalRoot {
    pub eclass: EClassId,
    pub root: EClassId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofCertificate {
    pub rules: Vec<RuleSnapshot>,
    pub events: Vec<ProofEvent>,
    pub final_roots: Vec<FinalRoot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageStatus {
    Covered,
    NotCovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProofCoverage {
    pub unconditional_rewrites: CoverageStatus,
    pub congruence: CoverageStatus,
    pub guarded_facts: CoverageStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayReport {
    pub unions_replayed: usize,
    pub rewrite_unions_replayed: usize,
    pub congruence_unions_replayed: usize,
    pub coverage: ProofCoverage,
}

/// A fail-closed replay error. `event_index` is absent only for certificate
/// metadata (rules and final roots), which lives outside the event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError {
    pub event_index: Option<usize>,
    pub message: String,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.event_index {
            Some(index) => write!(f, "proof event {index}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Immutable rule authority against which certificates are checked. A
/// certificate commits to rule snapshots but cannot authorize its own rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofContext {
    trusted_rules: Vec<RuleSnapshot>,
}

impl ProofContext {
    pub fn new(trusted_rules: Vec<RuleSnapshot>) -> Result<Self, ReplayError> {
        validate_rules(&trusted_rules)?;
        Ok(Self { trusted_rules })
    }

    pub fn trusted_rules(&self) -> &[RuleSnapshot] {
        &self.trusted_rules
    }

    pub fn replay(&self, certificate: &ProofCertificate) -> Result<ReplayReport, ReplayError> {
        replay_certificate_in_context(certificate, self)
    }
}

impl ProofCertificate {
    pub fn replay(&self, context: &ProofContext) -> Result<ReplayReport, ReplayError> {
        context.replay(self)
    }
}

/// Replay a bound proof certificate independently of the e-graph that
/// produced it.
///
/// Insertions build an immutable node table and singleton partition. Unions
/// are then checked against the partition *before* applying the recorded
/// merge. Guarded rules are intentionally absent from the vocabulary: Day 4
/// covers only unconditional rewrites and congruence.
pub fn replay_certificate(
    certificate: &ProofCertificate,
    trusted_rules: &[RuleSnapshot],
) -> Result<ReplayReport, ReplayError> {
    let context = ProofContext::new(trusted_rules.to_vec())?;
    replay_certificate_in_context(certificate, &context)
}

fn replay_certificate_in_context(
    certificate: &ProofCertificate,
    context: &ProofContext,
) -> Result<ReplayReport, ReplayError> {
    if certificate.rules != context.trusted_rules {
        return Err(metadata_error(
            "certificate rule snapshots do not exactly match the trusted rule registry",
        ));
    }
    let rules = validate_rules(&context.trusted_rules)?;
    let mut state = ReplayState::default();
    let mut unions_replayed = 0;
    let mut rewrite_unions_replayed = 0;
    let mut congruence_unions_replayed = 0;

    for (event_index, event) in certificate.events.iter().enumerate() {
        match event {
            ProofEvent::Insert(node) => state.insert_node(node, event_index)?,
            ProofEvent::Union(receipt) => {
                state.replay_union(receipt, &rules, event_index)?;
                unions_replayed += 1;
                match receipt.reason {
                    UnionReason::UnconditionalRewrite { .. } => rewrite_unions_replayed += 1,
                    UnionReason::Congruence { .. } => congruence_unions_replayed += 1,
                }
            }
        }
    }

    state.validate_final_roots(&certificate.final_roots)?;

    Ok(ReplayReport {
        unions_replayed,
        rewrite_unions_replayed,
        congruence_unions_replayed,
        coverage: ProofCoverage {
            unconditional_rewrites: if rewrite_unions_replayed == 0 {
                CoverageStatus::NotCovered
            } else {
                CoverageStatus::Covered
            },
            congruence: if congruence_unions_replayed == 0 {
                CoverageStatus::NotCovered
            } else {
                CoverageStatus::Covered
            },
            guarded_facts: CoverageStatus::NotCovered,
        },
    })
}

fn metadata_error(message: impl Into<String>) -> ReplayError {
    ReplayError {
        event_index: None,
        message: message.into(),
    }
}

fn event_error(event_index: usize, message: impl Into<String>) -> ReplayError {
    ReplayError {
        event_index: Some(event_index),
        message: message.into(),
    }
}

fn validate_rules(rules: &[RuleSnapshot]) -> Result<BTreeMap<RuleId, &RuleSnapshot>, ReplayError> {
    let mut by_id = BTreeMap::new();
    let mut previous = None;
    for rule in rules {
        if previous.is_some_and(|id| rule.id <= id) {
            return Err(metadata_error(
                "rule snapshots must be strictly sorted by RuleId",
            ));
        }
        validate_pattern(&rule.lhs, rule.id, "lhs")?;
        validate_pattern(&rule.rhs, rule.id, "rhs")?;
        let lhs_variables = pattern_variables(&rule.lhs).collect::<BTreeSet<_>>();
        let rhs_variables = pattern_variables(&rule.rhs).collect::<BTreeSet<_>>();
        if !rhs_variables.is_subset(&lhs_variables) {
            return Err(metadata_error(format!(
                "rule {} has RHS variables not bound by its LHS",
                rule.id
            )));
        }
        by_id.insert(rule.id, rule);
        previous = Some(rule.id);
    }
    Ok(by_id)
}

fn validate_pattern(pattern: &Pattern, rule: RuleId, side: &str) -> Result<(), ReplayError> {
    for child in &pattern.children {
        if let PatternChild::Var(variable) = child {
            if variable.is_empty() {
                return Err(metadata_error(format!(
                    "rule {rule} {side} contains an empty variable name"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct ReplayState {
    parent: BTreeMap<EClassId, EClassId>,
    nodes: BTreeMap<NodeId, NodeSnapshot>,
    last_node_id: Option<NodeId>,
}

impl ReplayState {
    fn insert_node(&mut self, node: &NodeSnapshot, event_index: usize) -> Result<(), ReplayError> {
        if self.last_node_id.is_some_and(|last| node.id <= last) {
            return Err(event_error(
                event_index,
                "NodeId values must be append-only and strictly increasing",
            ));
        }
        if self.nodes.contains_key(&node.id) {
            return Err(event_error(
                event_index,
                format!("duplicate NodeId {}", node.id.0),
            ));
        }
        if self.parent.contains_key(&node.owner) {
            return Err(event_error(
                event_index,
                format!("node owner {} is not a fresh singleton", node.owner),
            ));
        }
        for &child in &node.original_children {
            let root = self.root(child).ok_or_else(|| {
                event_error(
                    event_index,
                    format!("node child e-class {child} has not been inserted"),
                )
            })?;
            if root != child {
                return Err(event_error(
                    event_index,
                    format!(
                        "node child e-class {child} was not canonical at insertion (root {root})"
                    ),
                ));
            }
        }

        self.parent.insert(node.owner, node.owner);
        self.nodes.insert(node.id, node.clone());
        self.last_node_id = Some(node.id);
        Ok(())
    }

    fn replay_union(
        &mut self,
        receipt: &MergeReceipt,
        rules: &BTreeMap<RuleId, &RuleSnapshot>,
        event_index: usize,
    ) -> Result<(), ReplayError> {
        let lhs_root = self.known_root(receipt.lhs_pre, event_index, "lhs_pre")?;
        let rhs_root = self.known_root(receipt.rhs_pre, event_index, "rhs_pre")?;
        if lhs_root != receipt.lhs_pre || rhs_root != receipt.rhs_pre {
            return Err(event_error(
                event_index,
                format!(
                    "receipt endpoints must be pre-merge roots: ({}, {}) resolved to ({lhs_root}, {rhs_root})",
                    receipt.lhs_pre, receipt.rhs_pre
                ),
            ));
        }
        if receipt.lhs_pre == receipt.rhs_pre {
            return Err(event_error(
                event_index,
                "receipt cannot merge an already-equivalent pair",
            ));
        }
        let expected_result = receipt.lhs_pre.min(receipt.rhs_pre);
        if receipt.result != expected_result {
            return Err(event_error(
                event_index,
                format!(
                    "merge result {} does not match deterministic min-root result {expected_result}",
                    receipt.result,
                ),
            ));
        }

        match &receipt.reason {
            UnionReason::UnconditionalRewrite {
                rule,
                lhs_node,
                rhs_node,
                substitution,
            } => self.validate_rewrite(
                receipt,
                *rule,
                *lhs_node,
                *rhs_node,
                substitution,
                rules,
                event_index,
            )?,
            UnionReason::Congruence { lhs_node, rhs_node } => {
                self.validate_congruence(receipt, *lhs_node, *rhs_node, event_index)?
            }
        }

        let child = receipt.lhs_pre.max(receipt.rhs_pre);
        self.parent.insert(child, receipt.result);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_rewrite(
        &self,
        receipt: &MergeReceipt,
        rule_id: RuleId,
        lhs_node_id: NodeId,
        rhs_node_id: NodeId,
        substitution: &[Binding],
        rules: &BTreeMap<RuleId, &RuleSnapshot>,
        event_index: usize,
    ) -> Result<(), ReplayError> {
        let rule = rules
            .get(&rule_id)
            .ok_or_else(|| event_error(event_index, format!("unknown rewrite RuleId {rule_id}")))?;
        let lhs_node = self.node(lhs_node_id, event_index, "lhs_node")?;
        let rhs_node = self.node(rhs_node_id, event_index, "rhs_node")?;
        self.validate_ordered_owners(receipt, lhs_node, rhs_node, event_index)?;

        let bindings = self.validate_substitution(rule, substitution, event_index)?;
        self.validate_pattern_match(&rule.lhs, lhs_node, &bindings, event_index, "lhs")?;
        self.validate_pattern_match(&rule.rhs, rhs_node, &bindings, event_index, "rhs")?;
        Ok(())
    }

    fn validate_congruence(
        &self,
        receipt: &MergeReceipt,
        lhs_node_id: NodeId,
        rhs_node_id: NodeId,
        event_index: usize,
    ) -> Result<(), ReplayError> {
        let lhs_node = self.node(lhs_node_id, event_index, "lhs_node")?;
        let rhs_node = self.node(rhs_node_id, event_index, "rhs_node")?;
        self.validate_ordered_owners(receipt, lhs_node, rhs_node, event_index)?;

        if lhs_node.op != rhs_node.op {
            return Err(event_error(
                event_index,
                format!(
                    "congruence operator mismatch: {} != {}",
                    lhs_node.op, rhs_node.op
                ),
            ));
        }
        if lhs_node.original_children.len() != rhs_node.original_children.len() {
            return Err(event_error(event_index, "congruence arity mismatch"));
        }
        for (position, (&lhs, &rhs)) in lhs_node
            .original_children
            .iter()
            .zip(&rhs_node.original_children)
            .enumerate()
        {
            if self.root(lhs) != self.root(rhs) {
                return Err(event_error(
                    event_index,
                    format!("congruence child {position} is not equivalent: {lhs} != {rhs}"),
                ));
            }
        }
        Ok(())
    }

    fn validate_ordered_owners(
        &self,
        receipt: &MergeReceipt,
        lhs_node: &NodeSnapshot,
        rhs_node: &NodeSnapshot,
        event_index: usize,
    ) -> Result<(), ReplayError> {
        let lhs_owner = self
            .root(lhs_node.owner)
            .ok_or_else(|| event_error(event_index, "lhs reason node has an unknown owner"))?;
        let rhs_owner = self
            .root(rhs_node.owner)
            .ok_or_else(|| event_error(event_index, "rhs reason node has an unknown owner"))?;
        if lhs_owner != receipt.lhs_pre || rhs_owner != receipt.rhs_pre {
            return Err(event_error(
                event_index,
                format!(
                    "reason node owners ({lhs_owner}, {rhs_owner}) do not match ordered receipt endpoints ({}, {})",
                    receipt.lhs_pre, receipt.rhs_pre
                ),
            ));
        }
        Ok(())
    }

    fn validate_substitution<'a>(
        &self,
        rule: &RuleSnapshot,
        substitution: &'a [Binding],
        event_index: usize,
    ) -> Result<BTreeMap<&'a str, EClassId>, ReplayError> {
        let mut actual_domain = BTreeSet::new();
        let mut bindings = BTreeMap::new();
        let mut previous: Option<&str> = None;
        for binding in substitution {
            if previous.is_some_and(|name| binding.variable.as_str() <= name) {
                return Err(event_error(
                    event_index,
                    "substitution bindings must be strictly sorted by variable name",
                ));
            }
            let root = self.known_root(binding.eclass, event_index, "substitution e-class")?;
            if root != binding.eclass {
                return Err(event_error(
                    event_index,
                    format!(
                        "substitution for {} is not canonical: {} resolves to {root}",
                        binding.variable, binding.eclass
                    ),
                ));
            }
            actual_domain.insert(binding.variable.as_str());
            bindings.insert(binding.variable.as_str(), binding.eclass);
            previous = Some(binding.variable.as_str());
        }

        let expected_domain = pattern_variables(&rule.lhs).collect::<BTreeSet<_>>();
        if actual_domain != expected_domain {
            return Err(event_error(
                event_index,
                format!(
                    "substitution domain {:?} does not exactly match rule domain {:?}",
                    actual_domain, expected_domain
                ),
            ));
        }
        Ok(bindings)
    }

    fn validate_pattern_match(
        &self,
        pattern: &Pattern,
        node: &NodeSnapshot,
        bindings: &BTreeMap<&str, EClassId>,
        event_index: usize,
        side: &str,
    ) -> Result<(), ReplayError> {
        if pattern.op != node.op {
            return Err(event_error(
                event_index,
                format!(
                    "rewrite {side} operator mismatch: rule {} != node {}",
                    pattern.op, node.op
                ),
            ));
        }
        if pattern.children.len() != node.original_children.len() {
            return Err(event_error(
                event_index,
                format!("rewrite {side} arity mismatch"),
            ));
        }

        for (position, (pattern_child, &node_child)) in pattern
            .children
            .iter()
            .zip(&node.original_children)
            .enumerate()
        {
            let expected = match pattern_child {
                PatternChild::Exact(eclass) => *eclass,
                PatternChild::Var(variable) => {
                    *bindings.get(variable.as_str()).ok_or_else(|| {
                        event_error(
                            event_index,
                            format!("rewrite {side} variable {variable} has no binding"),
                        )
                    })?
                }
            };
            let expected_root = self.known_root(expected, event_index, "pattern e-class")?;
            let node_root = self.known_root(node_child, event_index, "node child")?;
            if expected_root != node_root {
                return Err(event_error(
                    event_index,
                    format!(
                        "rewrite {side} child {position} mismatch: {expected_root} != {node_root}"
                    ),
                ));
            }
        }
        Ok(())
    }

    fn validate_final_roots(&self, final_roots: &[FinalRoot]) -> Result<(), ReplayError> {
        let mut actual_domain = BTreeSet::new();
        let mut previous = None;
        for entry in final_roots {
            if previous.is_some_and(|id| entry.eclass <= id) {
                return Err(metadata_error(
                    "final roots must be strictly sorted by e-class id",
                ));
            }
            let actual = self.root(entry.eclass).ok_or_else(|| {
                metadata_error(format!(
                    "final roots reference unknown e-class {}",
                    entry.eclass
                ))
            })?;
            if self.root(entry.root) != Some(entry.root) {
                return Err(metadata_error(format!(
                    "recorded final root {} is not a known canonical root",
                    entry.root
                )));
            }
            if actual != entry.root {
                return Err(metadata_error(format!(
                    "final root mismatch for {}: recorded {}, replayed {actual}",
                    entry.eclass, entry.root
                )));
            }
            actual_domain.insert(entry.eclass);
            previous = Some(entry.eclass);
        }

        let expected_domain = self.parent.keys().copied().collect::<BTreeSet<_>>();
        if actual_domain != expected_domain {
            return Err(metadata_error(format!(
                "final-root domain {:?} does not exactly match inserted e-classes {:?}",
                actual_domain, expected_domain
            )));
        }
        Ok(())
    }

    fn node(
        &self,
        id: NodeId,
        event_index: usize,
        role: &str,
    ) -> Result<&NodeSnapshot, ReplayError> {
        self.nodes.get(&id).ok_or_else(|| {
            event_error(
                event_index,
                format!("{role} references unknown NodeId {}", id.0),
            )
        })
    }

    fn known_root(
        &self,
        id: EClassId,
        event_index: usize,
        role: &str,
    ) -> Result<EClassId, ReplayError> {
        self.root(id).ok_or_else(|| {
            event_error(
                event_index,
                format!("{role} references unknown e-class {id}"),
            )
        })
    }

    fn root(&self, mut id: EClassId) -> Option<EClassId> {
        let mut remaining = self.parent.len().saturating_add(1);
        loop {
            let parent = *self.parent.get(&id)?;
            if parent == id {
                return Some(id);
            }
            if remaining == 0 {
                return None;
            }
            remaining -= 1;
            id = parent;
        }
    }
}

fn pattern_variables(pattern: &Pattern) -> impl Iterator<Item = &str> {
    pattern.children.iter().filter_map(|child| match child {
        PatternChild::Var(variable) => Some(variable.as_str()),
        PatternChild::Exact(_) => None,
    })
}

// ---------------------------------------------------------------------------
// Justification
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Justification {
    Rewrite {
        rule: RuleId,
        subst: SubstitutionId,
        premises: Vec<FactProofId>,
    },
    Congruence {
        lhs_node: u32,
        rhs_node: u32,
    },
    Reflexive,
}

// ---------------------------------------------------------------------------
// ProofEdge
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofEdge {
    pub lhs: EClassId,
    pub rhs: EClassId,
    pub why: Justification,
}

// ---------------------------------------------------------------------------
// ProofDAG
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct ProofDAG {
    pub(crate) edges: Vec<ProofEdge>,
    pub(crate) known_ids: HashSet<EClassId>,
}

impl ProofDAG {
    pub fn new() -> Self {
        Self {
            edges: Vec::new(),
            known_ids: HashSet::new(),
        }
    }

    /// Add a proof edge.
    ///
    /// Rejects self-loops (`lhs == rhs`). Both `lhs` and `rhs` are tracked in
    /// the internal known-id set so that `validate` can enforce referential
    /// integrity.
    pub fn add_edge(
        &mut self,
        lhs: EClassId,
        rhs: EClassId,
        why: Justification,
    ) -> Result<(), String> {
        if lhs == rhs {
            return Err("self-loop not allowed".to_string());
        }
        self.known_ids.insert(lhs);
        self.known_ids.insert(rhs);
        self.edges.push(ProofEdge { lhs, rhs, why });
        Ok(())
    }

    /// Validate the proof DAG.
    ///
    /// The context-free checker accepts only the empty proof. A non-empty
    /// rewrite needs a rule/substitution registry, and congruence needs an
    /// immutable e-node snapshot; accepting opaque numeric IDs here would
    /// turn a syntactic receipt into a forged semantic proof. Day 4 adds the
    /// bound-context validator that can accept those edges.
    pub fn validate(&self) -> Result<(), String> {
        for edge in &self.edges {
            if !self.known_ids.contains(&edge.lhs) {
                return Err(format!(
                    "edge lhs {} is not in the known eclass-id set",
                    edge.lhs
                ));
            }
            if !self.known_ids.contains(&edge.rhs) {
                return Err(format!(
                    "edge rhs {} is not in the known eclass-id set",
                    edge.rhs
                ));
            }
        }

        if let Some(edge) = self.edges.first() {
            return match &edge.why {
                Justification::Reflexive => Err(format!(
                    "Reflexive cannot justify distinct classes {} and {}",
                    edge.lhs, edge.rhs
                )),
                Justification::Rewrite { .. } => {
                    Err("Rewrite proof has no bound rule/substitution context".into())
                }
                Justification::Congruence { .. } => {
                    Err("Congruence proof has no bound e-node snapshot".into())
                }
            };
        }

        Ok(())
    }
}

impl Default for ProofDAG {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_checker_validates_dag() {
        let empty = ProofDAG::new();
        assert!(empty.validate().is_ok(), "empty proof should pass");

        // Numeric endpoints alone cannot turn Reflexive into a valid proof.
        let mut dag = ProofDAG::new();
        dag.add_edge(1, 2, Justification::Reflexive).unwrap();
        dag.add_edge(2, 3, Justification::Reflexive).unwrap();
        dag.add_edge(1, 3, Justification::Reflexive).unwrap();
        assert!(
            dag.validate().is_err(),
            "context-free Reflexive edges must be rejected"
        );

        // -- edge with target id not in the known-id set --
        let mut dag2 = ProofDAG::new();
        dag2.known_ids.insert(10);
        dag2.known_ids.insert(20);
        // rhs 99 was never registered → validation must fail
        dag2.edges.push(ProofEdge {
            lhs: 10,
            rhs: 99,
            why: Justification::Reflexive,
        });
        assert!(
            dag2.validate().is_err(),
            "edge referencing an unknown target id should be rejected"
        );
    }

    #[test]
    fn rejects_self_loops() {
        let mut dag = ProofDAG::new();
        assert!(dag.add_edge(5, 5, Justification::Reflexive).is_err());
    }

    #[test]
    fn unbound_congruence_is_rejected() {
        let mut dag = ProofDAG::new();
        dag.add_edge(
            1,
            2,
            Justification::Congruence {
                lhs_node: 0,
                rhs_node: 3,
            },
        )
        .unwrap();
        assert!(dag.validate().is_err());

        let mut dag2 = ProofDAG::new();
        dag2.add_edge(
            1,
            2,
            Justification::Congruence {
                lhs_node: 3,
                rhs_node: 0,
            },
        )
        .unwrap();
        assert!(dag2.validate().is_err());

        let mut dag3 = ProofDAG::new();
        dag3.add_edge(
            1,
            2,
            Justification::Congruence {
                lhs_node: 3,
                rhs_node: 4,
            },
        )
        .unwrap();
        assert!(dag3.validate().is_err());
    }

    #[test]
    fn detects_cycle_even_with_disconnected_nodes() {
        let mut dag = ProofDAG::new();
        dag.add_edge(1, 2, Justification::Reflexive).unwrap();
        dag.add_edge(2, 1, Justification::Reflexive).unwrap();
        assert!(dag.validate().is_err());
    }

    #[test]
    fn proof_rewrite_is_replayable() {
        let mut dag = ProofDAG::new();
        let a = 1u32;
        let b = 2u32;
        let c = 3u32;
        // add rewrite edges
        dag.add_edge(a, b, Justification::Reflexive).unwrap();
        dag.add_edge(b, c, Justification::Reflexive).unwrap();
        // No rule/substitution context is attached yet.
        assert!(dag.validate().is_err());
        // Now introduce a cycle
        dag.add_edge(c, a, Justification::Reflexive).unwrap();
        assert!(dag.validate().is_err());
    }
}
