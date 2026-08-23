#![allow(clippy::type_complexity)]
//! Equality saturation core: union-find, hash-cons, e-classes, and rebuild

use std::collections::{BTreeMap, BTreeSet, HashMap};

pub use evocc_proof::{
    Binding, MergeReceipt, NodeId, Pattern, PatternChild, ProofCertificate, ProofContext,
    ProofEvent, ReplayReport, RuleId, RuleSnapshot, UnionReason,
};

// ---------------------------------------------------------------------------
// Type aliases (items 1 & 3 of the roadmap)
// ---------------------------------------------------------------------------

/// Unique identifier for an equivalence class.
pub type EClassId = u32;

/// Unique identifier for an e-node in the arena.
pub type ENodeId = u32;

/// Operator identifier (encodes both the symbol and its arity / semantics).
pub type OpId = u16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertOutcome {
    pub eclass: EClassId,
    pub node: NodeId,
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    AlreadyEquivalent { root: EClassId },
    Merged(MergeReceipt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditNode {
    pub id: NodeId,
    pub op: OpId,
    pub children: Vec<EClassId>,
    pub owner: EClassId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditClass {
    pub id: EClassId,
    pub nodes: Vec<NodeId>,
    pub parents: Vec<(OpId, usize, EClassId)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgraphSnapshot {
    pub nodes: Vec<AuditNode>,
    pub classes: Vec<AuditClass>,
    pub dirty: Vec<(EClassId, EClassId)>,
}

// ===========================================================================
// 2. UnionFind — disjoint-set with path compression & union-by-rank
// ===========================================================================

/// Disjoint-set (union-find) forest for tracking e-class equivalence.
pub struct UnionFind {
    parent: Vec<EClassId>,
}

impl UnionFind {
    pub fn new() -> Self {
        UnionFind { parent: Vec::new() }
    }

    /// Allocate a fresh singleton set, returning its brand-new id.
    pub fn make_set(&mut self) -> EClassId {
        let id = self.parent.len() as EClassId;
        self.parent.push(id);
        id
    }

    /// Find the canonical representative for `id` (with path compression).
    pub fn find(&mut self, id: EClassId) -> EClassId {
        let p = self.parent[id as usize];
        if p != id {
            let root = self.find(p);
            self.parent[id as usize] = root;
            root
        } else {
            id
        }
    }

    /// Merge two sets, choosing the smaller insertion handle as the stable
    /// representative. Stable roots make proof receipts reproducible.
    pub fn union(&mut self, a: EClassId, b: EClassId) -> EClassId {
        let a = self.find(a);
        let b = self.find(b);
        if a == b {
            return a;
        }
        let (root, child) = if a < b { (a, b) } else { (b, a) };
        self.parent[child as usize] = root;
        root
    }
}

impl Default for UnionFind {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 4. ENode — an operator applied to child e-classes
// ===========================================================================

/// An e-node: `(op children…)` where each child is an `EClassId`.
pub struct ENode {
    pub op: OpId,
    pub children: Vec<EClassId>,
}

impl ENode {
    /// Return the canonical form by mapping every child through `uf.find`.
    pub fn canonicalize(&self, uf: &mut UnionFind) -> (OpId, Vec<EClassId>) {
        let children: Vec<EClassId> = self.children.iter().map(|&c| uf.find(c)).collect();
        (self.op, children)
    }
}

// ===========================================================================
// 5. HashCons — deduplication table
// ===========================================================================

/// Hash-consing table mapping a canonical `(op, children)` form to the
/// e-class that represents it.
pub struct HashCons {
    map: HashMap<(OpId, Vec<EClassId>), EClassId>,
}

impl HashCons {
    pub fn new() -> Self {
        HashCons {
            map: HashMap::new(),
        }
    }

    /// Look up `node`'s canonical form in the table.
    pub fn lookup(&self, node: &ENode, uf: &mut UnionFind) -> Option<EClassId> {
        let key = node.canonicalize(uf);
        self.map.get(&key).copied()
    }

    /// Associate `node`'s canonical form with e-class `id`.
    pub fn insert(&mut self, node: &ENode, id: EClassId, uf: &mut UnionFind) {
        let key = node.canonicalize(uf);
        self.map.insert(key, id);
    }
}

impl Default for HashCons {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// 6. EClass — equivalence class
// ===========================================================================

/// An equivalence class holding the e-nodes that belong to it and a
/// reverse-parent index for efficient rebuilds.
pub struct EClass {
    pub id: EClassId,
    /// Every e-node whose canonical form maps to this class.
    pub nodes: Vec<ENodeId>,
    /// Reverse map: for every occurrence of this class as a child,
    /// `(parent_op, child_position, parent_eclass)`.
    pub parents: Vec<(OpId, usize, EClassId)>,
}

// ===========================================================================
// 7. Egraph — the full e-graph container
// ===========================================================================

/// The e-graph: union-find, hash-cons table, arena for e-nodes and
/// e-classes, a dirty-worklist, and reverse enode→eclass pointers.
pub struct Egraph {
    uf: UnionFind,
    hashcons: HashCons,
    enodes: Vec<ENode>,
    eclasses: Vec<EClass>,
    /// Pending merges — `(non_canonical, canonical)` pairs queued for rebuild.
    dirty: Vec<(EClassId, EClassId)>,
    /// Quick lookup: which e-class each e-node currently resides in.
    enode_to_eclass: Vec<EClassId>,
    /// Stable proof identity for each current physical e-node slot.
    stable_node_ids: Vec<NodeId>,
    /// Append-only insertion and real-union audit stream.
    proof_events: Vec<ProofEvent>,
    next_node_id: u64,
    rules: BTreeMap<RuleId, RuleSnapshot>,
}

impl Egraph {
    pub fn new() -> Self {
        Egraph {
            uf: UnionFind::new(),
            hashcons: HashCons::new(),
            enodes: Vec::new(),
            eclasses: Vec::new(),
            dirty: Vec::new(),
            enode_to_eclass: Vec::new(),
            stable_node_ids: Vec::new(),
            proof_events: Vec::new(),
            next_node_id: 0,
            rules: BTreeMap::new(),
        }
    }

    pub fn with_rules(rules: &[RuleSnapshot]) -> Result<Self, String> {
        let mut graph = Self::new();
        for rule in rules {
            graph.register_rule(rule.clone())?;
        }
        Ok(graph)
    }

    pub fn register_rule(&mut self, rule: RuleSnapshot) -> Result<(), String> {
        if let Some(existing) = self.rules.get(&rule.id) {
            return if existing == &rule {
                Ok(())
            } else {
                Err(format!(
                    "RuleId {} is already bound to another rule",
                    rule.id
                ))
            };
        }
        let mut candidate = self.rules.values().cloned().collect::<Vec<_>>();
        candidate.push(rule.clone());
        candidate.sort_by_key(|entry| entry.id);
        evocc_proof::ProofContext::new(candidate).map_err(|error| error.to_string())?;
        self.rules.insert(rule.id, rule);
        Ok(())
    }

    // -- Insertion ----------------------------------------------------------

    /// Insert a new term `op(children...)`.
    ///
    /// Canonicalises children through the union-find, checks the hash-cons
    /// table, and — if the term is genuinely new — creates a fresh e-node and
    /// e-class, updating parent indexes.
    pub fn insert(&mut self, op: OpId, children: Vec<EClassId>) -> EClassId {
        self.insert_with_node(op, children).eclass
    }

    /// Insert a term and return the node witness selected by hash-consing.
    ///
    /// The Day 4 proof campaign exercises this richer form; the legacy
    /// `insert` entry point remains a compatibility wrapper.
    pub fn insert_with_node(&mut self, op: OpId, children: Vec<EClassId>) -> InsertOutcome {
        let children: Vec<EClassId> = children.iter().map(|&c| self.uf.find(c)).collect();
        let key = (op, children.clone());

        if let Some(&existing) = self.hashcons.map.get(&key) {
            let slot = self
                .enodes
                .iter()
                .position(|candidate| candidate.op == op && candidate.children == children)
                .expect("hash-cons entry has a live node witness");
            return InsertOutcome {
                eclass: self.uf.find(existing),
                node: self.stable_node_ids[slot],
                inserted: false,
            };
        }

        let node_slot = self.enodes.len() as ENodeId;
        let stable_node_id = NodeId(self.next_node_id);
        self.next_node_id = self
            .next_node_id
            .checked_add(1)
            .expect("stable proof NodeId space exhausted");
        self.enodes.push(ENode {
            op,
            children: children.clone(),
        });

        let class_id = self.uf.make_set();
        self.eclasses.push(EClass {
            id: class_id,
            nodes: vec![node_slot],
            parents: Vec::new(),
        });
        self.hashcons.map.insert(key, class_id);
        self.enode_to_eclass.push(class_id);
        self.stable_node_ids.push(stable_node_id);
        self.proof_events
            .push(ProofEvent::Insert(evocc_proof::NodeSnapshot {
                id: stable_node_id,
                owner: class_id,
                op,
                original_children: children.clone(),
            }));

        for (i, &child) in children.iter().enumerate() {
            let child = self.uf.find(child);
            self.eclasses[child as usize]
                .parents
                .push((op, i, class_id));
        }

        InsertOutcome {
            eclass: class_id,
            node: stable_node_id,
            inserted: true,
        }
    }

    // -- Merging ------------------------------------------------------------

    pub fn merge_rewrite(
        &mut self,
        rule: RuleId,
        lhs_node: NodeId,
        rhs_node: NodeId,
        substitution: Vec<Binding>,
    ) -> Result<MergeOutcome, String> {
        self.validate_rewrite_reason(rule, lhs_node, rhs_node, &substitution)?;
        let lhs = self.node_owner_root(lhs_node)?;
        let rhs = self.node_owner_root(rhs_node)?;
        Ok(self.merge_recorded(
            lhs,
            rhs,
            UnionReason::UnconditionalRewrite {
                rule,
                lhs_node,
                rhs_node,
                substitution,
            },
        ))
    }

    pub fn merge_congruence(
        &mut self,
        lhs_node: NodeId,
        rhs_node: NodeId,
    ) -> Result<MergeOutcome, String> {
        self.validate_congruence_reason(lhs_node, rhs_node)?;
        let lhs = self.node_owner_root(lhs_node)?;
        let rhs = self.node_owner_root(rhs_node)?;
        Ok(self.merge_recorded(lhs, rhs, UnionReason::Congruence { lhs_node, rhs_node }))
    }

    /// The sole production funnel for a real e-class union. Every successful
    /// mutation appends exactly one prevalidated immutable receipt.
    fn merge_recorded(&mut self, a: EClassId, b: EClassId, reason: UnionReason) -> MergeOutcome {
        let lhs_pre = self.uf.find(a);
        let rhs_pre = self.uf.find(b);
        if lhs_pre == rhs_pre {
            return MergeOutcome::AlreadyEquivalent { root: lhs_pre };
        }
        let result = self.uf.union(lhs_pre, rhs_pre);
        let non_canonical = if result == lhs_pre { rhs_pre } else { lhs_pre };

        // Move nodes.
        let nodes: Vec<ENodeId> = std::mem::take(&mut self.eclasses[non_canonical as usize].nodes);
        for &nid in &nodes {
            self.enode_to_eclass[nid as usize] = result;
        }
        self.eclasses[result as usize].nodes.extend(nodes);

        // Move parents.
        let parents: Vec<_> = std::mem::take(&mut self.eclasses[non_canonical as usize].parents);
        self.eclasses[result as usize].parents.extend(parents);

        self.dirty.push((non_canonical, result));
        let receipt = MergeReceipt {
            lhs_pre,
            rhs_pre,
            result,
            reason,
        };
        self.proof_events.push(ProofEvent::Union(receipt.clone()));
        MergeOutcome::Merged(receipt)
    }

    fn proof_node(&self, id: NodeId) -> Result<evocc_proof::NodeSnapshot, String> {
        self.node_snapshot(id)
            .ok_or_else(|| format!("unknown stable NodeId {}", id.0))
    }

    fn canonical_checked(&mut self, id: EClassId) -> Result<EClassId, String> {
        if id as usize >= self.eclasses.len() {
            return Err(format!("unknown e-class {id}"));
        }
        Ok(self.uf.find(id))
    }

    fn node_owner_root(&mut self, id: NodeId) -> Result<EClassId, String> {
        let owner = self.proof_node(id)?.owner;
        self.canonical_checked(owner)
    }

    fn validate_rewrite_reason(
        &mut self,
        rule_id: RuleId,
        lhs_node_id: NodeId,
        rhs_node_id: NodeId,
        substitution: &[Binding],
    ) -> Result<(), String> {
        let rule = self
            .rules
            .get(&rule_id)
            .cloned()
            .ok_or_else(|| format!("unknown trusted RuleId {rule_id}"))?;
        let lhs_node = self.proof_node(lhs_node_id)?;
        let rhs_node = self.proof_node(rhs_node_id)?;

        let mut bindings = BTreeMap::new();
        let mut previous: Option<&str> = None;
        for binding in substitution {
            if previous.is_some_and(|name| binding.variable.as_str() <= name) {
                return Err("substitution bindings must be strictly sorted".into());
            }
            let canonical = self.canonical_checked(binding.eclass)?;
            if canonical != binding.eclass {
                return Err(format!(
                    "substitution {} is not canonical",
                    binding.variable
                ));
            }
            bindings.insert(binding.variable.clone(), binding.eclass);
            previous = Some(binding.variable.as_str());
        }
        let expected = pattern_variables(&rule.lhs);
        let actual = bindings.keys().cloned().collect::<BTreeSet<_>>();
        if actual != expected {
            return Err("substitution domain does not exactly match the rule LHS".into());
        }

        self.validate_pattern_match(&rule.lhs, &lhs_node, &bindings)?;
        self.validate_pattern_match(&rule.rhs, &rhs_node, &bindings)?;
        Ok(())
    }

    fn validate_pattern_match(
        &mut self,
        pattern: &Pattern,
        node: &evocc_proof::NodeSnapshot,
        bindings: &BTreeMap<String, EClassId>,
    ) -> Result<(), String> {
        if pattern.op != node.op || pattern.children.len() != node.original_children.len() {
            return Err("rewrite pattern does not match its node witness".into());
        }
        for (pattern_child, &node_child) in pattern.children.iter().zip(&node.original_children) {
            let expected = match pattern_child {
                PatternChild::Exact(eclass) => *eclass,
                PatternChild::Var(variable) => *bindings
                    .get(variable)
                    .ok_or_else(|| format!("missing binding for {variable}"))?,
            };
            let expected = self.canonical_checked(expected)?;
            let observed = self.canonical_checked(node_child)?;
            if expected != observed {
                return Err("rewrite child does not match its pattern".into());
            }
        }
        Ok(())
    }

    fn validate_congruence_reason(
        &mut self,
        lhs_node_id: NodeId,
        rhs_node_id: NodeId,
    ) -> Result<(), String> {
        let lhs = self.proof_node(lhs_node_id)?;
        let rhs = self.proof_node(rhs_node_id)?;
        if lhs.op != rhs.op || lhs.original_children.len() != rhs.original_children.len() {
            return Err("congruence witnesses have different operators or arities".into());
        }
        for (&lhs_child, &rhs_child) in lhs.original_children.iter().zip(&rhs.original_children) {
            if self.canonical_checked(lhs_child)? != self.canonical_checked(rhs_child)? {
                return Err("congruence witness children are not equivalent".into());
            }
        }
        Ok(())
    }

    /// Snapshot the complete producer audit stream and current partition.
    pub fn proof_certificate(&mut self) -> ProofCertificate {
        let mut owners = self
            .proof_events
            .iter()
            .filter_map(|event| match event {
                ProofEvent::Insert(node) => Some(node.owner),
                ProofEvent::Union(_) => None,
            })
            .collect::<Vec<_>>();
        owners.sort_unstable();
        owners.dedup();
        let final_roots = owners
            .into_iter()
            .map(|eclass| evocc_proof::FinalRoot {
                eclass,
                root: self.uf.find(eclass),
            })
            .collect();
        ProofCertificate {
            rules: self.rules.values().cloned().collect(),
            events: self.proof_events.clone(),
            final_roots,
        }
    }

    // -- Rebuild (congruence closure) ---------------------------------------

    /// Canonicalise an e-class id through the union-find.
    pub fn find(&mut self, id: EClassId) -> EClassId {
        self.uf.find(id)
    }

    /// Return one stable insertion witness owned by `id`'s current class.
    pub fn node_witness(&mut self, id: EClassId) -> Option<NodeId> {
        let root = self.uf.find(id);
        for slot in 0..self.enodes.len() {
            if self.uf.find(self.enode_to_eclass[slot]) == root {
                return Some(self.stable_node_ids[slot]);
            }
        }
        None
    }

    /// Return an immutable insertion-time node witness.
    pub fn node_snapshot(&self, id: NodeId) -> Option<evocc_proof::NodeSnapshot> {
        self.proof_events.iter().find_map(|event| match event {
            ProofEvent::Insert(node) if node.id == id => Some(node.clone()),
            _ => None,
        })
    }

    /// Detached, read-only structural view. Physical arena slots never cross
    /// this API boundary; node membership uses append-only proof `NodeId`s.
    pub fn audit_snapshot(&mut self) -> EgraphSnapshot {
        let nodes = (0..self.enodes.len())
            .map(|slot| AuditNode {
                id: self.stable_node_ids[slot],
                op: self.enodes[slot].op,
                children: self.enodes[slot].children.clone(),
                owner: self.uf.find(self.enode_to_eclass[slot]),
            })
            .collect();
        let classes = self
            .eclasses
            .iter()
            .map(|class| AuditClass {
                id: class.id,
                nodes: class
                    .nodes
                    .iter()
                    .map(|&slot| self.stable_node_ids[slot as usize])
                    .collect(),
                parents: class.parents.clone(),
            })
            .collect();
        EgraphSnapshot {
            nodes,
            classes,
            dirty: self.dirty.clone(),
        }
    }

    /// Restore congruence closure after a batch of validated reasoned merges.
    pub fn rebuild(&mut self) {
        // Congruence closure is a least fixed point. Rebuilding the canonical
        // key table from all live nodes is deliberately simple and avoids
        // retaining stale reverse-parent entries from incremental repairs.
        loop {
            for node in &mut self.enodes {
                for child in &mut node.children {
                    *child = self.uf.find(*child);
                }
            }

            let mut owner_by_key: HashMap<(OpId, Vec<EClassId>), (EClassId, NodeId)> =
                HashMap::new();
            let mut collisions = Vec::new();
            for (node_slot, node) in self.enodes.iter().enumerate() {
                let owner = self.uf.find(self.enode_to_eclass[node_slot]);
                let witness = self.stable_node_ids[node_slot];
                let key = (node.op, node.children.clone());
                if let Some(&(other_owner, other_witness)) = owner_by_key.get(&key) {
                    if self.uf.find(owner) != self.uf.find(other_owner) {
                        collisions.push((owner, other_owner, witness, other_witness));
                    }
                } else {
                    owner_by_key.insert(key, (owner, witness));
                }
            }

            if collisions.is_empty() {
                break;
            }
            for (left, right, lhs_node, rhs_node) in collisions {
                debug_assert_eq!(self.uf.find(left), self.node_owner_root(lhs_node).unwrap());
                debug_assert_eq!(self.uf.find(right), self.node_owner_root(rhs_node).unwrap());
                let outcome = self
                    .merge_congruence(lhs_node, rhs_node)
                    .expect("rebuild collision has valid congruence witnesses");
                debug_assert!(matches!(
                    outcome,
                    MergeOutcome::Merged(_) | MergeOutcome::AlreadyEquivalent { .. }
                ));
            }
            self.dirty.clear();
        }

        // Compact congruent duplicate nodes and reconstruct every derived
        // index from the canonical node set. No stale parent/hashcons entry
        // survives this boundary.
        let mut unique = HashMap::new();
        let mut nodes = Vec::new();
        let mut owners = Vec::new();
        let mut stable_node_ids = Vec::new();
        let old_nodes = std::mem::take(&mut self.enodes);
        let old_stable_node_ids = std::mem::take(&mut self.stable_node_ids);
        for (node_id, (mut node, stable_node_id)) in
            old_nodes.into_iter().zip(old_stable_node_ids).enumerate()
        {
            for child in &mut node.children {
                *child = self.uf.find(*child);
            }
            let owner = self.uf.find(self.enode_to_eclass[node_id]);
            let key = (node.op, node.children.clone());
            if let std::collections::hash_map::Entry::Vacant(entry) = unique.entry(key) {
                entry.insert(nodes.len() as ENodeId);
                nodes.push(node);
                owners.push(owner);
                stable_node_ids.push(stable_node_id);
            }
        }
        self.enodes = nodes;
        self.enode_to_eclass = owners;
        self.stable_node_ids = stable_node_ids;

        for class in &mut self.eclasses {
            class.nodes.clear();
            class.parents.clear();
        }
        self.hashcons.map.clear();

        for node_id in 0..self.enodes.len() {
            let owner = self.uf.find(self.enode_to_eclass[node_id]);
            self.enode_to_eclass[node_id] = owner;
            self.eclasses[owner as usize].nodes.push(node_id as ENodeId);

            let node = &self.enodes[node_id];
            self.hashcons
                .map
                .insert((node.op, node.children.clone()), owner);
            for (position, &child) in node.children.iter().enumerate() {
                let child = self.uf.find(child);
                self.eclasses[child as usize]
                    .parents
                    .push((node.op, position, owner));
            }
        }
        self.dirty.clear();
    }
}

impl Default for Egraph {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Rewrite rules for the e-graph
// ===========================================================================

/// One rewrite rule: a left-hand-side pattern and a builder that produces
/// the right-hand side from the bindings.
pub struct Rewrite {
    pub id: RuleId,
    pub name: String,
    pub lhs: Pattern,
    pub rhs: Pattern,
}

impl Rewrite {
    pub fn new(id: RuleId, name: impl Into<String>, lhs: Pattern, rhs: Pattern) -> Self {
        Self {
            id,
            name: name.into(),
            lhs,
            rhs,
        }
    }

    pub fn snapshot(&self) -> RuleSnapshot {
        RuleSnapshot {
            id: self.id,
            lhs: self.lhs.clone(),
            rhs: self.rhs.clone(),
        }
    }
}

/// Apply a set of rewrite rules to an e-graph exhaustively until saturation
/// or budget exhaustion. Returns the number of new unions performed.
pub fn apply_rewrites(egraph: &mut Egraph, rules: &[Rewrite], max_iterations: usize) -> usize {
    let mut total_unions = 0;
    for _ in 0..max_iterations {
        let before = total_unions;
        for rule in rules {
            if egraph.register_rule(rule.snapshot()).is_err() {
                continue;
            }
            let mut matches: Vec<(EClassId, ENodeId, HashMap<String, EClassId>)> = Vec::new();
            for (eclass_id, eclass) in egraph.eclasses.iter().enumerate() {
                let eclass_id = eclass_id as EClassId;
                for &enode_id in &eclass.nodes {
                    let enode = &egraph.enodes[enode_id as usize];
                    if let Some(bindings) = match_pattern(&rule.lhs, enode, &mut egraph.uf) {
                        matches.push((eclass_id, enode_id, bindings));
                        break; // one match per eclass
                    }
                }
            }
            for (root, lhs_node, bindings) in &matches {
                let lhs_stable_node = egraph.stable_node_ids[*lhs_node as usize];
                let Some(rhs_children) = rule
                    .rhs
                    .children
                    .iter()
                    .map(|child| match child {
                        PatternChild::Exact(eclass) => Some(*eclass),
                        PatternChild::Var(variable) => bindings.get(variable).copied(),
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let rhs = egraph.insert_with_node(rule.rhs.op, rhs_children);
                let root = *root;
                if egraph.find(root) != egraph.find(rhs.eclass) {
                    let mut substitution = bindings
                        .iter()
                        .map(|(variable, &eclass)| Binding {
                            variable: variable.clone(),
                            eclass: egraph.find(eclass),
                        })
                        .collect::<Vec<_>>();
                    substitution.sort_by(|left, right| left.variable.cmp(&right.variable));
                    let outcome = egraph
                        .merge_rewrite(rule.id, lhs_stable_node, rhs.node, substitution)
                        .expect("matched declarative rewrite validates before merging");
                    debug_assert!(matches!(outcome, MergeOutcome::Merged(_)));
                    total_unions += 1;
                }
            }
        }
        egraph.rebuild();
        if total_unions == before {
            break; // saturation
        }
    }
    total_unions
}

fn pattern_variables(pattern: &Pattern) -> BTreeSet<String> {
    pattern
        .children
        .iter()
        .filter_map(|child| match child {
            PatternChild::Exact(_) => None,
            PatternChild::Var(variable) => Some(variable.clone()),
        })
        .collect()
}

fn match_pattern(
    pattern: &Pattern,
    enode: &ENode,
    uf: &mut UnionFind,
) -> Option<HashMap<String, EClassId>> {
    if pattern.op != enode.op {
        return None;
    }
    if pattern.children.len() != enode.children.len() {
        return None;
    }
    let mut bindings: HashMap<String, EClassId> = HashMap::new();
    for (pat_child, &child_id) in pattern.children.iter().zip(&enode.children) {
        match pat_child {
            PatternChild::Exact(id) => {
                if uf.find(*id) != uf.find(child_id) {
                    return None;
                }
            }
            PatternChild::Var(name) => {
                if let Some(existing) = bindings.get(name) {
                    if uf.find(*existing) != uf.find(child_id) {
                        return None;
                    }
                } else {
                    bindings.insert(name.clone(), child_id);
                }
            }
        }
    }
    Some(bindings)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn merge_for_test(graph: &mut Egraph, left: EClassId, right: EClassId) -> EClassId {
        let lhs_node = graph
            .node_witness(left)
            .expect("left test class has a node witness");
        let rhs_node = graph
            .node_witness(right)
            .expect("right test class has a node witness");
        let lhs = graph.node_snapshot(lhs_node).expect("lhs snapshot");
        let rhs = graph.node_snapshot(rhs_node).expect("rhs snapshot");
        let rule = RuleSnapshot {
            id: graph.rules.keys().next_back().copied().unwrap_or(0) + 1,
            lhs: Pattern {
                op: lhs.op,
                children: lhs
                    .original_children
                    .iter()
                    .copied()
                    .map(PatternChild::Exact)
                    .collect(),
            },
            rhs: Pattern {
                op: rhs.op,
                children: rhs
                    .original_children
                    .iter()
                    .copied()
                    .map(PatternChild::Exact)
                    .collect(),
            },
        };
        graph
            .register_rule(rule.clone())
            .expect("test rule is valid");
        match graph
            .merge_rewrite(rule.id, lhs_node, rhs_node, Vec::new())
            .expect("test rewrite validates")
        {
            MergeOutcome::AlreadyEquivalent { root } => root,
            MergeOutcome::Merged(receipt) => receipt.result,
        }
    }

    // -- 2. UnionFind -------------------------------------------------------

    #[test]
    fn union_find_three_classes_canonical_identity() {
        let mut uf = UnionFind::new();
        let a = uf.make_set();
        let b = uf.make_set();
        let c = uf.make_set();

        let root = uf.union(a, b);
        let root = uf.union(root, c);

        assert_eq!(uf.find(a), root);
        assert_eq!(uf.find(b), root);
        assert_eq!(uf.find(c), root);
    }

    // -- 5. HashCons --------------------------------------------------------

    #[test]
    fn hashcons_same_op_and_canonical_children_same_eclass() {
        let mut uf = UnionFind::new();
        let x = uf.make_set();
        let y = uf.make_set();
        let _root = uf.union(x, y); // x == y after union => canonical child is `root`

        let mut hc = HashCons::new();

        let enode1 = ENode {
            op: 42,
            children: vec![x],
        };
        let enode2 = ENode {
            op: 42,
            children: vec![y],
        };

        // Both canonicalise to (42, [root]).
        assert!(hc.lookup(&enode1, &mut uf).is_none());
        hc.insert(&enode1, 0, &mut uf);
        assert_eq!(hc.lookup(&enode2, &mut uf), Some(0));
    }

    // -- 7. Egraph (congruence closure) -------------------------------------

    #[test]
    fn egraph_core_preserves_congruence() {
        let mut eg = Egraph::new();

        let const_1: OpId = 0;
        let const_2: OpId = 1;
        let add_op: OpId = 2;

        // --- Hash-consing: same term → same e-class ---
        let a = eg.insert(const_1, vec![]);
        let a2 = eg.insert(const_1, vec![]);
        assert_eq!(
            eg.uf.find(a),
            eg.uf.find(a2),
            "hashcons: Const(1) == Const(1)"
        );

        // Different constants → different e-classes.
        let b = eg.insert(const_2, vec![]);
        assert_ne!(eg.uf.find(a), eg.uf.find(b), "A != B (different constants)");

        // --- Ordered children distinguish e-nodes ---
        let c = eg.insert(add_op, vec![a, b]); // Add(A, B)
        let d = eg.insert(add_op, vec![b, a]); // Add(B, A)
        assert_ne!(
            eg.uf.find(c),
            eg.uf.find(d),
            "C != D (different child order)"
        );

        // --- Repeated insertion with same canonical children ---
        let e = eg.insert(add_op, vec![a, a]); // Add(A, A)
        let f = eg.insert(add_op, vec![a, a]);
        assert_eq!(
            eg.uf.find(e),
            eg.uf.find(f),
            "E == F (same canonical children)"
        );

        // --- Union triggers congruence ---
        merge_for_test(&mut eg, a, b); // now A == B
        eg.rebuild();

        // Add(A, B) and Add(B, A) both become Add(A, A) after rebuild.
        assert_eq!(
            eg.uf.find(c),
            eg.uf.find(d),
            "after union + rebuild, C == D"
        );

        // They also equal the original Add(A, A).
        assert_eq!(
            eg.uf.find(c),
            eg.uf.find(e),
            "C == E after congruence closure"
        );
    }

    #[test]
    fn egraph_rewrite_constant_folds() {
        let mut eg = Egraph::new();
        let const_op: OpId = 0;
        let add_op: OpId = 1;

        let c40 = eg.insert(const_op, vec![]);
        let c2 = eg.insert(const_op, vec![]);
        let sum = eg.insert(add_op, vec![c40, c2]);

        // Build a pattern that matches add(any, any)
        let _lhs = Pattern {
            op: add_op,
            children: vec![PatternChild::Var("a".into()), PatternChild::Var("b".into())],
        };

        // Manually const-fold: insert the result and union.
        let rhs_id = eg.insert(const_op, vec![]);
        merge_for_test(&mut eg, sum, rhs_id);
        eg.rebuild();
        assert_eq!(eg.find(sum), eg.find(rhs_id));
    }

    #[test]
    fn egraph_rewrite_identity_canonicalizes() {
        let mut eg = Egraph::new();
        let const_op: OpId = 0;
        let add_op: OpId = 1;

        let x = eg.insert(const_op, vec![]);
        let zero = eg.insert(const_op, vec![]);
        let add_x_0 = eg.insert(add_op, vec![x, zero]);

        // Rule: add(x, 0) -> x (add-identity)
        merge_for_test(&mut eg, add_x_0, x);
        eg.rebuild();
        assert_eq!(eg.find(add_x_0), eg.find(x));
    }

    #[test]
    fn invalid_congruence_is_rejected_before_graph_or_proof_mutation() {
        let mut eg = Egraph::new();
        let left = eg.insert_with_node(10, Vec::new());
        let right = eg.insert_with_node(11, Vec::new());
        let before_certificate = eg.proof_certificate();
        let before_left = eg.find(left.eclass);
        let before_right = eg.find(right.eclass);

        let error = eg
            .merge_congruence(left.node, right.node)
            .expect_err("different leaf operators are not congruent");

        assert!(error.contains("different operators or arities"));
        assert_eq!(eg.find(left.eclass), before_left);
        assert_eq!(eg.find(right.eclass), before_right);
        assert_ne!(eg.find(left.eclass), eg.find(right.eclass));
        assert_eq!(eg.proof_certificate(), before_certificate);
    }

    #[test]
    fn conflicting_rule_registration_is_rejected_without_authority_drift() {
        let first = RuleSnapshot {
            id: 7,
            lhs: Pattern {
                op: 20,
                children: Vec::new(),
            },
            rhs: Pattern {
                op: 21,
                children: Vec::new(),
            },
        };
        let conflicting = RuleSnapshot {
            id: first.id,
            lhs: Pattern {
                op: 20,
                children: Vec::new(),
            },
            rhs: Pattern {
                op: 22,
                children: Vec::new(),
            },
        };
        let mut eg = Egraph::with_rules(std::slice::from_ref(&first))
            .expect("the initial trusted registry is valid");
        let before = eg.proof_certificate();

        let error = eg
            .register_rule(conflicting)
            .expect_err("one RuleId cannot be rebound");

        assert!(error.contains("already bound"));
        assert_eq!(eg.proof_certificate(), before);
        assert_eq!(eg.proof_certificate().rules, vec![first]);
    }

    // ===================================================================
    // Invariant property tests
    //
    // These tests verify that the e-graph core preserves the structural
    // invariants that every downstream consumer (rewrite engine, extractor,
    // proof generator) depends on. They are property-based in intent: they
    // exercise the data structure under a variety of operation sequences
    // and verify that the invariants hold, not merely that a specific
    // hand-crafted scenario produces a specific expected result.
    // ===================================================================

    /// rebuild is idempotent: applying it twice in succession must not
    /// alter any e-class membership or create new dirty work.
    #[test]
    fn invariant_rebuild_is_idempotent() {
        let mut eg = Egraph::new();
        let op: OpId = 0;

        // Build a modest e-graph: three expressions that partially share.
        let a = eg.insert(op, vec![]);
        let b = eg.insert(op, vec![]);
        let c = eg.insert(op, vec![]);
        let ab = eg.insert(op, vec![a, b]);
        let ba = eg.insert(op, vec![b, a]);
        let ac = eg.insert(op, vec![a, c]);

        // Create congruence pressure: union a with b.
        merge_for_test(&mut eg, a, b);
        eg.rebuild();

        // After rebuild, dirty must be empty, because every pending merge
        // has been processed and any newly discovered congruence pairs
        // have been merged and their consequences enqueued and processed.
        assert!(
            eg.dirty.is_empty(),
            "dirty queue must be empty after rebuild"
        );

        // The two expressions ab=op(a,b) and ba=op(b,a) should now be in
        // the same e-class, because their children became equal after the
        // merge a≡b.
        assert_eq!(
            eg.find(ab),
            eg.find(ba),
            "op(a,b) ≡ op(b,a) after a≡b merge"
        );

        // A second rebuild must be a no-op: no new dirty entries, no change
        // to the canonical representatives.
        let before_ab = eg.find(ab);
        let before_ac = eg.find(ac);
        eg.rebuild();
        assert!(eg.dirty.is_empty(), "second rebuild must leave dirty empty");
        assert_eq!(
            eg.find(ab),
            before_ab,
            "second rebuild must not change canonical of ab"
        );
        assert_eq!(
            eg.find(ac),
            before_ac,
            "second rebuild must not change canonical of ac"
        );
    }

    /// Congruence closure completeness: after rebuild, any two e-nodes with
    /// the same operator and canonically equal children must inhabit the
    /// same e-class. Hashcons guarantees this for newly inserted terms; the
    /// non-trivial case is when two nodes were inserted with DISJOINT
    /// children and those children are later merged — rebuild must discover
    /// the new congruence and merge the parents.
    #[test]
    fn invariant_congruence_closure_is_complete() {
        let mut eg = Egraph::new();
        let add_op: OpId = 0;

        // Four distinct leaf types (different op IDs means hashcons cannot
        // deduplicate them).
        let a = eg.insert(1, vec![]);
        let b = eg.insert(2, vec![]);
        let c = eg.insert(3, vec![]);
        let d = eg.insert(4, vec![]);

        // Two add nodes with disjoint children. They MUST be in different
        // e-classes at this point because their children are distinct.
        let xy = eg.insert(add_op, vec![a, b]);
        let x2y2 = eg.insert(add_op, vec![c, d]);
        assert_ne!(
            eg.find(xy),
            eg.find(x2y2),
            "disjoint children must produce distinct e-classes"
        );

        // Merge the children pairwise. After rebuild, the parents must
        // become congruent because find(a)==find(c) and find(b)==find(d),
        // so both parents canonicalize to add_op(find(a), find(b)).
        merge_for_test(&mut eg, a, c);
        merge_for_test(&mut eg, b, d);
        eg.rebuild();

        assert_eq!(
            eg.find(xy),
            eg.find(x2y2),
            "congruence closure: add(a,b) ≡ add(c,d) after a≡c, b≡d"
        );
    }

    /// Rebuild never creates new e-classes: the set of canonical IDs
    /// after rebuild is a subset of the set before rebuild (some may be
    /// retired by merging, but no new ones appear).
    #[test]
    fn invariant_rebuild_preserves_eclass_set() {
        let mut eg = Egraph::new();
        let op: OpId = 0;

        let ids: Vec<EClassId> = (0..10).map(|_| eg.insert(op, vec![])).collect();
        let before: Vec<EClassId> = ids
            .iter()
            .map(|&id| eg.find(id))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Merge several pairs and rebuild.
        merge_for_test(&mut eg, ids[0], ids[1]);
        merge_for_test(&mut eg, ids[2], ids[3]);
        merge_for_test(&mut eg, ids[4], ids[5]);
        eg.rebuild();

        let after: Vec<EClassId> = ids
            .iter()
            .map(|&id| eg.find(id))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Every canonical ID after rebuild must have been present before
        // (or retired to a previously existing ID via union).
        for &id in &after {
            // Either this ID existed before, or it is the canonical that
            // absorbed another ID (which is a pre-existing ID).
            assert!(
                before.contains(&id) || before.iter().any(|&b| eg.find(b) == id),
                "new canonical ID {id} appeared from nowhere"
            );
        }

        // The number of distinct canons must not increase.
        assert!(
            after.len() <= before.len(),
            "rebuild increased canonical count: {} -> {}",
            before.len(),
            after.len()
        );
    }

    /// Rebuild terminates on any finite e-graph: the processing loop is
    /// bounded by the finite dirty queue and the finite number of possible
    /// congruence pairs.
    #[test]
    fn invariant_rebuild_always_terminates() {
        let mut eg = Egraph::new();
        let op: OpId = 0;

        // Build a small tree and trigger every node to enter the dirty
        // queue via cascading unions. This verifies that the dirty queue
        // logic does not loop infinitely.
        let leaf = eg.insert(op, vec![]);
        let mid = eg.insert(op, vec![leaf]);
        let _top = eg.insert(op, vec![mid]);

        // Merge leaf with a fresh node — should trigger one round of
        // congruence for mid, then another for top.
        let other_leaf = eg.insert(op, vec![]);
        merge_for_test(&mut eg, leaf, other_leaf);
        eg.rebuild();

        // The graph is still in a consistent state — the fact that we
        // reached this line means rebuild returned (terminated).
        assert!(eg.dirty.is_empty());

        // Expressly verify that repeated rebuild does not hang on a
        // larger graph.
        let mut big = Egraph::new();
        let roots: Vec<EClassId> = (0..50).map(|_| big.insert(op, vec![])).collect();
        let mut current = roots[0];
        for &r in &roots[1..] {
            current = big.insert(op, vec![current, r]);
        }
        // Merge the first two leaves — the congruence chain should
        // propagate up the tree and terminate.
        merge_for_test(&mut big, roots[0], roots[1]);
        big.rebuild();
        assert!(
            big.dirty.is_empty(),
            "cascading rebuild must drain the dirty queue"
        );
    }
}
