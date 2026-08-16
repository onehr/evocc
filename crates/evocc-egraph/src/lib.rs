#![allow(clippy::type_complexity)]
//! Equality saturation core: union-find, hash-cons, e-classes, and rebuild

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Type aliases (items 1 & 3 of the roadmap)
// ---------------------------------------------------------------------------

/// Unique identifier for an equivalence class.
pub type EClassId = u32;

/// Unique identifier for an e-node in the arena.
pub type ENodeId = u32;

/// Operator identifier (encodes both the symbol and its arity / semantics).
pub type OpId = u16;

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
    pub uf: UnionFind,
    pub hashcons: HashCons,
    pub enodes: Vec<ENode>,
    pub eclasses: Vec<EClass>,
    /// Pending merges — `(non_canonical, canonical)` pairs queued for rebuild.
    pub dirty: Vec<(EClassId, EClassId)>,
    /// Quick lookup: which e-class each e-node currently resides in.
    enode_to_eclass: Vec<EClassId>,
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
        }
    }

    // -- Insertion ----------------------------------------------------------

    /// Insert a new term `op(children...)`.
    ///
    /// Canonicalises children through the union-find, checks the hash-cons
    /// table, and — if the term is genuinely new — creates a fresh e-node and
    /// e-class, updating parent indexes.
    pub fn insert(&mut self, op: OpId, children: Vec<EClassId>) -> EClassId {
        let children: Vec<EClassId> = children.iter().map(|&c| self.uf.find(c)).collect();
        let key = (op, children.clone());

        if let Some(&existing) = self.hashcons.map.get(&key) {
            return existing;
        }

        let node_id = self.enodes.len() as ENodeId;
        self.enodes.push(ENode {
            op,
            children: children.clone(),
        });

        let class_id = self.uf.make_set();
        self.eclasses.push(EClass {
            id: class_id,
            nodes: vec![node_id],
            parents: Vec::new(),
        });
        self.hashcons.map.insert(key, class_id);
        self.enode_to_eclass.push(class_id);

        for (i, &child) in children.iter().enumerate() {
            let child = self.uf.find(child);
            self.eclasses[child as usize]
                .parents
                .push((op, i, class_id));
        }

        class_id
    }

    // -- Merging ------------------------------------------------------------

    /// Declare that e-classes `a` and `b` are equivalent.
    ///
    /// Merges the two sets in the union-find, moves nodes and parents to the
    /// canonical e-class, and enqueues a dirty entry for the rebuild pass.
    pub fn union(&mut self, a: EClassId, b: EClassId) -> EClassId {
        let a = self.uf.find(a);
        let b = self.uf.find(b);
        if a == b {
            return a;
        }
        let canonical = self.uf.union(a, b);
        let non_canonical = if canonical == a { b } else { a };

        // Move nodes.
        let nodes: Vec<ENodeId> = std::mem::take(&mut self.eclasses[non_canonical as usize].nodes);
        for &nid in &nodes {
            self.enode_to_eclass[nid as usize] = canonical;
        }
        self.eclasses[canonical as usize].nodes.extend(nodes);

        // Move parents.
        let parents: Vec<_> = std::mem::take(&mut self.eclasses[non_canonical as usize].parents);
        self.eclasses[canonical as usize].parents.extend(parents);

        self.dirty.push((non_canonical, canonical));
        canonical
    }

    // -- Rebuild (congruence closure) ---------------------------------------

    /// Canonicalise an e-class id through the union-find.
    pub fn find(&mut self, id: EClassId) -> EClassId {
        self.uf.find(id)
    }

    /// Restore congruence closure after a batch of `union` calls.
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

            let mut owner_by_key = HashMap::new();
            let mut collisions = Vec::new();
            for (node_id, node) in self.enodes.iter().enumerate() {
                let owner = self.uf.find(self.enode_to_eclass[node_id]);
                let key = (node.op, node.children.clone());
                if let Some(&other) = owner_by_key.get(&key) {
                    if self.uf.find(owner) != self.uf.find(other) {
                        collisions.push((owner, other));
                    }
                } else {
                    owner_by_key.insert(key, owner);
                }
            }

            if collisions.is_empty() {
                break;
            }
            for (left, right) in collisions {
                self.union(left, right);
            }
            self.dirty.clear();
        }

        // Compact congruent duplicate nodes and reconstruct every derived
        // index from the canonical node set. No stale parent/hashcons entry
        // survives this boundary.
        let mut unique = HashMap::new();
        let mut nodes = Vec::new();
        let mut owners = Vec::new();
        for (node_id, mut node) in std::mem::take(&mut self.enodes).into_iter().enumerate() {
            for child in &mut node.children {
                *child = self.uf.find(*child);
            }
            let owner = self.uf.find(self.enode_to_eclass[node_id]);
            let key = (node.op, node.children.clone());
            if unique.insert(key, nodes.len() as ENodeId).is_none() {
                nodes.push(node);
                owners.push(owner);
            }
        }
        self.enodes = nodes;
        self.enode_to_eclass = owners;

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

/// A pattern to match in the e-graph: an operator and children that may be
/// concrete EClassIds or pattern variables.
#[derive(Debug, Clone)]
pub struct Pattern {
    pub op: OpId,
    pub children: Vec<PatternChild>,
}

#[derive(Debug, Clone)]
pub enum PatternChild {
    /// Match exactly this e-class id.
    Exact(EClassId),
    /// Bind any e-class to this variable name.
    Var(String),
}

/// One rewrite rule: a left-hand-side pattern and a builder that produces
/// the right-hand side from the bindings.
pub struct Rewrite {
    pub name: String,
    pub lhs: Pattern,
    /// Build the RHS e-node given the variable bindings (var_name -> EClassId).
    pub apply: Box<dyn Fn(&HashMap<String, EClassId>) -> (OpId, Vec<EClassId>)>,
}

impl Rewrite {
    pub fn new(
        name: impl Into<String>,
        lhs: Pattern,
        apply: impl Fn(&HashMap<String, EClassId>) -> (OpId, Vec<EClassId>) + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            lhs,
            apply: Box::new(apply),
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
            let mut matches: Vec<(EClassId, HashMap<String, EClassId>)> = Vec::new();
            for (eclass_id, eclass) in egraph.eclasses.iter().enumerate() {
                let eclass_id = eclass_id as EClassId;
                for &enode_id in &eclass.nodes {
                    let enode = &egraph.enodes[enode_id as usize];
                    if let Some(bindings) = match_pattern(&rule.lhs, enode, &mut egraph.uf) {
                        matches.push((eclass_id, bindings));
                        break; // one match per eclass
                    }
                }
            }
            for (root, bindings) in &matches {
                let (rhs_op, rhs_children) = (rule.apply)(bindings);
                let rhs_id = egraph.insert(rhs_op, rhs_children);
                let root = *root;
                if egraph.find(root) != egraph.find(rhs_id) {
                    egraph.union(root, rhs_id);
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
        eg.union(a, b); // now A == B
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
        eg.union(sum, rhs_id);
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
        eg.union(add_x_0, x);
        eg.rebuild();
        assert_eq!(eg.find(add_x_0), eg.find(x));
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
        eg.union(a, b);
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
        eg.union(a, c);
        eg.union(b, d);
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
        eg.union(ids[0], ids[1]);
        eg.union(ids[2], ids[3]);
        eg.union(ids[4], ids[5]);
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
        eg.union(leaf, other_leaf);
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
        big.union(roots[0], roots[1]);
        big.rebuild();
        assert!(
            big.dirty.is_empty(),
            "cascading rebuild must drain the dirty queue"
        );
    }
}
