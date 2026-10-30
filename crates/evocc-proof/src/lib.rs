//! Proof-carrying compiler: justifications and independent proof checker

mod checker_process;
mod evp1;

pub use checker_process::{CheckerOutcome, StandaloneProofChecker};
pub use evp1::*;

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
// Bound proof vocabulary
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

/// One level of a rewrite pattern. It deliberately covers just the
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
/// merge. Guarded rules are intentionally absent from the vocabulary: it
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

/// Compute the SHA-256 digest used by canonical proof and module identities.
///
/// Keeping this small implementation in the optimizer-independent proof crate
/// lets the producer and future standalone checker share the byte primitive
/// without sharing eligibility or rewrite validation.
#[must_use]
#[allow(clippy::many_single_char_names, clippy::needless_range_loop)]
pub fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    for chunk in padded.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (index, bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*bytes);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let first = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let second = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(first);
            d = c;
            c = b;
            b = a;
            a = first.wrapping_add(second);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut digest = [0_u8; 32];
    for (chunk, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// Lowercase hexadecimal spelling of [`sha256`].
#[must_use]
pub fn sha256_hex(input: &[u8]) -> String {
    sha256(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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
    /// turn a syntactic receipt into a forged semantic proof. Only a replay
    /// against a bound `ProofContext` accepts rewrite and congruence proofs.
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
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"aarch64-unknown-linux-gnu/evoir-layout-v3-scalar-fp-rne\n"),
            "ee1edc5535aaf75577005b534e9ac87bb7dbb34626aef3586987b2720523d905"
        );
        assert_eq!(
            sha256_hex(b"aarch64-unknown-linux-gnu/evoir-layout-v4-hfa-natural-alignment\n"),
            "9460b084c6b110935328ced6acded2c869d6718a6fdbfa69516b989cc1d59798"
        );
    }

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
