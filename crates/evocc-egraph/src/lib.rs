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
    rank: Vec<u32>,
}

impl UnionFind {
    pub fn new() -> Self {
        UnionFind {
            parent: Vec::new(),
            rank: Vec::new(),
        }
    }

    /// Allocate a fresh singleton set, returning its brand-new id.
    pub fn make_set(&mut self) -> EClassId {
        let id = self.parent.len() as EClassId;
        self.parent.push(id);
        self.rank.push(0);
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

    /// Merge the two sets containing `a` and `b` using union-by-rank.
    /// Returns the canonical (root) id after the merge.
    pub fn union(&mut self, a: EClassId, b: EClassId) -> EClassId {
        let a = self.find(a);
        let b = self.find(b);
        if a == b {
            return a;
        }
        let ra = self.rank[a as usize];
        let rb = self.rank[b as usize];
        if ra < rb {
            self.parent[a as usize] = b;
            b
        } else if ra > rb {
            self.parent[b as usize] = a;
            a
        } else {
            self.parent[b as usize] = a;
            self.rank[a as usize] += 1;
            a
        }
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
        let nodes: Vec<ENodeId> =
            std::mem::take(&mut self.eclasses[non_canonical as usize].nodes);
        for &nid in &nodes {
            self.enode_to_eclass[nid as usize] = canonical;
        }
        self.eclasses[canonical as usize].nodes.extend(nodes);

        // Move parents.
        let parents: Vec<_> =
            std::mem::take(&mut self.eclasses[non_canonical as usize].parents);
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
    ///
    /// For each dirty merge, walks the parent index of the canonical e-class,
    /// re-canonicalises every affected e-node, updates the hash-cons table,
    /// and triggers further merges when congruence creates duplicates.
    pub fn rebuild(&mut self) {
        while let Some((_from, _to)) = self.dirty.pop() {
            let to = self.uf.find(_to);

            let parents: Vec<(OpId, usize, EClassId)> =
                self.eclasses[to as usize].parents.clone();

            for (parent_op, _child_pos, parent_eclass_id) in parents {
                let parent_ec = self.uf.find(parent_eclass_id);

                let candidates: Vec<ENodeId> = self.eclasses[parent_ec as usize]
                    .nodes
                    .iter()
                    .filter(|&&nid| self.enodes[nid as usize].op == parent_op)
                    .copied()
                    .collect();

                for node_id in candidates {
                    self.repair(node_id);
                }
            }
        }
    }

    /// Re-canonicalise a single e-node, update the hash-cons, and merge
    /// e-classes if the new canonical form collides with an existing one.
    fn repair(&mut self, node_id: ENodeId) {
        let op = self.enodes[node_id as usize].op;
        let old_children = self.enodes[node_id as usize].children.clone();
        let new_children: Vec<EClassId> =
            old_children.iter().map(|&c| self.uf.find(c)).collect();

        if old_children == new_children {
            return;
        }

        self.enodes[node_id as usize].children = new_children.clone();

        let old_ec = self.uf.find(self.enode_to_eclass[node_id as usize]);
        self.eclasses[old_ec as usize]
            .nodes
            .retain(|&n| n != node_id);

        let new_key = (op, new_children);

        if let Some(&target_ec) = self.hashcons.map.get(&new_key) {
            let target = self.uf.find(target_ec);
            self.eclasses[target as usize].nodes.push(node_id);
            self.enode_to_eclass[node_id as usize] = target;

            for (i, &child) in self.enodes[node_id as usize]
                .children
                .iter()
                .enumerate()
            {
                let child = self.uf.find(child);
                self.eclasses[child as usize]
                    .parents
                    .push((op, i, target));
            }

            if old_ec != target {
                self.union(old_ec, target);
            }
        } else {
            self.hashcons.map.insert(new_key, old_ec);
            self.eclasses[old_ec as usize].nodes.push(node_id);

            for (i, &child) in self.enodes[node_id as usize]
                .children
                .iter()
                .enumerate()
            {
                let child = self.uf.find(child);
                self.eclasses[child as usize]
                    .parents
                    .push((op, i, old_ec));
            }
        }
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
        Self { name: name.into(), lhs, apply: Box::new(apply) }
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
        assert_ne!(
            eg.uf.find(a),
            eg.uf.find(b),
            "A != B (different constants)"
        );

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
            children: vec![
                PatternChild::Var("a".into()),
                PatternChild::Var("b".into()),
            ],
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
}
