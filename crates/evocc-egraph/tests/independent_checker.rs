use std::collections::{BTreeMap, BTreeSet, HashMap};

use evocc_egraph::{
    EClassId, Egraph, EgraphSnapshot, MergeOutcome, NodeId, OpId, Pattern, PatternChild,
    RuleSnapshot,
};

#[derive(Clone)]
struct NodeRecord {
    id: NodeId,
    op: OpId,
    children: Vec<EClassId>,
    owner: EClassId,
}

#[derive(Default)]
struct Trace {
    nodes: Vec<NodeRecord>,
    explicit_unions: Vec<(EClassId, EClassId)>,
}

#[derive(Clone, Default)]
struct Dsu {
    parent: HashMap<EClassId, EClassId>,
}

impl Dsu {
    fn add(&mut self, id: EClassId) {
        self.parent.entry(id).or_insert(id);
    }

    fn find(&mut self, id: EClassId) -> EClassId {
        let parent = *self.parent.get(&id).expect("oracle class is registered");
        if parent == id {
            id
        } else {
            let root = self.find(parent);
            self.parent.insert(id, root);
            root
        }
    }

    fn union(&mut self, left: EClassId, right: EClassId) -> bool {
        let left = self.find(left);
        let right = self.find(right);
        if left == right {
            return false;
        }
        let (root, child) = if left < right {
            (left, right)
        } else {
            (right, left)
        };
        self.parent.insert(child, root);
        true
    }
}

#[derive(Default)]
struct OracleStats {
    congruence_merges: usize,
    collisions: usize,
}

fn least_congruence(trace: &Trace) -> (Dsu, OracleStats) {
    let mut dsu = Dsu::default();
    for node in &trace.nodes {
        dsu.add(node.owner);
    }
    for &(left, right) in &trace.explicit_unions {
        dsu.union(left, right);
    }

    let mut stats = OracleStats::default();
    loop {
        let mut changed = false;
        let mut keys: HashMap<(OpId, Vec<EClassId>), EClassId> = HashMap::new();
        for node in &trace.nodes {
            let key = (
                node.op,
                node.children.iter().map(|&child| dsu.find(child)).collect(),
            );
            if let Some(&other) = keys.get(&key) {
                stats.collisions += 1;
                if dsu.union(node.owner, other) {
                    stats.congruence_merges += 1;
                    changed = true;
                }
            } else {
                keys.insert(key, node.owner);
            }
        }
        if !changed {
            break;
        }
    }
    (dsu, stats)
}

fn insert_traced(
    graph: &mut Egraph,
    trace: &mut Trace,
    op: OpId,
    children: Vec<EClassId>,
) -> EClassId {
    let before = graph.audit_snapshot();
    let outcome = graph.insert_with_node(op, children.clone());
    let after = graph.audit_snapshot();
    match (
        outcome.inserted,
        after.nodes.len().checked_sub(before.nodes.len()),
    ) {
        (true, Some(1)) => trace.nodes.push(NodeRecord {
            id: outcome.node,
            op,
            children,
            owner: outcome.eclass,
        }),
        (false, Some(0)) => {}
        other => panic!("one insertion reported an inconsistent node-arena change: {other:?}"),
    }
    assert!(
        after.nodes.iter().any(|node| node.id == outcome.node),
        "insert outcome references a missing stable node"
    );
    outcome.eclass
}

fn merge_validated(graph: &mut Egraph, left: EClassId, right: EClassId) {
    let lhs_node = graph
        .node_witness(left)
        .expect("left structural-test class has a node witness");
    let rhs_node = graph
        .node_witness(right)
        .expect("right structural-test class has a node witness");
    let lhs = graph.node_snapshot(lhs_node).expect("lhs test snapshot");
    let rhs = graph.node_snapshot(rhs_node).expect("rhs test snapshot");
    let rule = RuleSnapshot {
        id: graph
            .proof_certificate()
            .rules
            .last()
            .map_or(1, |entry| entry.id + 1),
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
        .expect("structural-test rule is valid");
    let outcome = graph
        .merge_rewrite(rule.id, lhs_node, rhs_node, Vec::new())
        .expect("structural-test rewrite validates before merging");
    assert!(matches!(outcome, MergeOutcome::Merged(_)));
}

fn assert_partition(graph: &mut Egraph, trace: &Trace, oracle: &mut Dsu) -> usize {
    let owners = trace
        .nodes
        .iter()
        .map(|node| node.owner)
        .collect::<Vec<_>>();
    let mut checks = 0;
    for (index, &left) in owners.iter().enumerate() {
        for &right in &owners[index..] {
            assert_eq!(
                graph.find(left) == graph.find(right),
                oracle.find(left) == oracle.find(right),
                "partition mismatch for insertion handles {left} and {right}"
            );
            checks += 1;
        }
    }
    checks
}

type NodeKey = (OpId, Vec<EClassId>);
type ParentEntry = (OpId, usize, EClassId);

fn structural_snapshot(
    graph: &mut Egraph,
    snapshot: &EgraphSnapshot,
    trace: &Trace,
    oracle: &mut Dsu,
) -> (
    Vec<EClassId>,
    BTreeSet<NodeKey>,
    BTreeMap<EClassId, Vec<ParentEntry>>,
) {
    assert!(snapshot.dirty.is_empty(), "rebuild left pending dirty work");

    let owners = trace
        .nodes
        .iter()
        .map(|node| node.owner)
        .collect::<Vec<_>>();
    let partition = owners
        .iter()
        .map(|&owner| graph.find(owner))
        .collect::<Vec<_>>();

    let class_roots = snapshot
        .classes
        .iter()
        .enumerate()
        .map(|(index, class)| {
            assert_eq!(
                class.id, index as EClassId,
                "snapshot class order/id drifted"
            );
            graph.find(class.id)
        })
        .collect::<Vec<_>>();
    let nodes_by_id = snapshot
        .nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        nodes_by_id.len(),
        snapshot.nodes.len(),
        "snapshot repeats a stable node id"
    );
    let mut node_owners = BTreeMap::new();
    for (index, class) in snapshot.classes.iter().enumerate() {
        let root = class_roots[index];
        if root != class.id {
            assert!(
                class.nodes.is_empty(),
                "retired class {} retains nodes",
                class.id
            );
            assert!(
                class.parents.is_empty(),
                "retired class {} retains parents",
                class.id
            );
            continue;
        }
        for &node_id in &class.nodes {
            let node = nodes_by_id
                .get(&node_id)
                .expect("e-class references a present stable node");
            assert!(
                node_owners.insert(node_id, root).is_none(),
                "node {node_id:?} has multiple owners"
            );
            assert_eq!(
                node.owner, root,
                "node {node_id:?} owner disagrees with class membership"
            );
        }
    }
    assert_eq!(
        node_owners.len(),
        snapshot.nodes.len(),
        "every live arena node has exactly one canonical owner"
    );

    let mut actual_keys = BTreeSet::new();
    let mut expected_parents: BTreeMap<EClassId, Vec<ParentEntry>> = BTreeMap::new();
    for node in &snapshot.nodes {
        let node_id = node.id;
        let owner = node_owners[&node_id];
        let owner_root = *class_roots
            .get(node.owner as usize)
            .expect("node owner is an in-range class");
        assert_eq!(
            node.owner, owner_root,
            "node {node_id:?} has a non-canonical owner"
        );
        let mut children = Vec::new();
        for (position, &child) in node.children.iter().enumerate() {
            let canonical = *class_roots
                .get(child as usize)
                .expect("node child is an in-range class");
            assert_eq!(child, canonical, "node {node_id:?} retains a stale child");
            children.push(canonical);
            expected_parents
                .entry(canonical)
                .or_default()
                .push((node.op, position, owner));
        }
        actual_keys.insert((node.op, children));
    }

    let mut expected_keys = BTreeSet::new();
    let mut expected_owner_by_key = BTreeMap::new();
    for node in &trace.nodes {
        let key = (
            node.op,
            node.children
                .iter()
                .map(|&child| oracle.find(child))
                .collect::<Vec<_>>(),
        );
        expected_keys.insert(key.clone());
        expected_owner_by_key.insert(key, oracle.find(node.owner));
    }
    assert_eq!(
        actual_keys, expected_keys,
        "live canonical node-key set drifted"
    );

    let live_nodes = snapshot
        .nodes
        .iter()
        .map(|node| {
            (
                node.id,
                node.op,
                node.children.clone(),
                node_owners[&node.id],
            )
        })
        .collect::<Vec<_>>();
    for (node_id, op, children, owner) in live_nodes {
        let key = (op, children);
        let expected_owner = expected_owner_by_key[&key];
        let witness = trace
            .nodes
            .iter()
            .find(|record| {
                record.op == key.0
                    && record
                        .children
                        .iter()
                        .map(|&child| oracle.find(child))
                        .collect::<Vec<_>>()
                        == key.1
            })
            .expect("key has an oracle witness");
        assert_eq!(
            oracle.find(witness.owner),
            expected_owner,
            "oracle key owner is inconsistent"
        );
        assert_eq!(
            graph.find(owner),
            graph.find(witness.owner),
            "node {node_id:?} key belongs to the wrong production class"
        );
        assert_eq!(
            node_id, witness.id,
            "canonical key is attached to the wrong stable node"
        );
    }

    let mut actual_parents = BTreeMap::new();
    for (index, class) in snapshot.classes.iter().enumerate() {
        if class_roots[index] == class.id {
            let mut parents = class.parents.clone();
            parents.sort_unstable();
            actual_parents.insert(class.id, parents);
        }
    }
    for parents in expected_parents.values_mut() {
        parents.sort_unstable();
    }
    for root in class_roots.iter().copied().collect::<BTreeSet<_>>() {
        expected_parents.entry(root).or_default();
        actual_parents.entry(root).or_default();
    }
    assert_eq!(
        actual_parents, expected_parents,
        "reverse-parent index drifted"
    );

    (partition, actual_keys, actual_parents)
}

fn assert_hashcons_probe(graph: &mut Egraph, keys: &BTreeSet<NodeKey>) {
    for (op, children) in keys {
        let before = graph.audit_snapshot();
        let outcome = graph.insert_with_node(*op, children.clone());
        let after = graph.audit_snapshot();
        assert!(!outcome.inserted, "hashcons inserted an existing key");
        assert_eq!(
            after.nodes.len(),
            before.nodes.len(),
            "hashcons missed existing key ({op}, {children:?})"
        );
        assert_eq!(
            before, after,
            "hashcons probe changed the detached structural snapshot"
        );
        let expected_owner = graph.find(outcome.eclass);
        let matching = after
            .nodes
            .iter()
            .find(|node| node.op == *op && node.children == *children)
            .expect("existing key has a live node");
        assert_eq!(
            outcome.node, matching.id,
            "hashcons returned the wrong node"
        );
        assert_eq!(expected_owner, graph.find(matching.owner));
    }
}

fn assert_graph_snapshot(
    graph: &mut Egraph,
    trace: &Trace,
    snapshot: EgraphSnapshot,
) -> (usize, OracleStats) {
    let (mut oracle, stats) = least_congruence(trace);
    let checks = assert_partition(graph, trace, &mut oracle);
    let before = structural_snapshot(graph, &snapshot, trace, &mut oracle);
    assert_hashcons_probe(graph, &before.1);
    graph.rebuild();
    let rebuilt = graph.audit_snapshot();
    let after = structural_snapshot(graph, &rebuilt, trace, &mut oracle);
    assert_eq!(before, after, "a second rebuild changed observable state");
    (checks, stats)
}

fn assert_graph(graph: &mut Egraph, trace: &Trace) -> (usize, OracleStats) {
    let snapshot = graph.audit_snapshot();
    assert_graph_snapshot(graph, trace, snapshot)
}

fn simple_graph() -> (Egraph, Trace, EClassId, EClassId) {
    let mut graph = Egraph::new();
    let mut trace = Trace::default();
    let left = insert_traced(&mut graph, &mut trace, 1, vec![]);
    let right = insert_traced(&mut graph, &mut trace, 2, vec![]);
    insert_traced(&mut graph, &mut trace, 3, vec![left, right]);
    graph.rebuild();
    (graph, trace, left, right)
}

fn must_reject_semantic_fault(
    label: &str,
    exercise: impl FnOnce(&mut Egraph, &mut Trace, EClassId, EClassId),
) {
    let (mut graph, mut trace, left, right) = simple_graph();
    exercise(&mut graph, &mut trace, left, right);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_graph(&mut graph, &trace)
    }));
    assert!(result.is_err(), "checker accepted injected fault {label}");
}

fn must_reject_snapshot_fault(
    label: &str,
    exercise: impl FnOnce(&mut EgraphSnapshot, EClassId, EClassId),
) {
    let (mut graph, trace, left, right) = simple_graph();
    let mut snapshot = graph.audit_snapshot();
    exercise(&mut snapshot, left, right);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_graph_snapshot(&mut graph, &trace, snapshot)
    }));
    assert!(result.is_err(), "checker accepted injected fault {label}");
}

#[test]
fn checker_fault_controls_are_detected() {
    let (mut baseline, baseline_trace, _, _) = simple_graph();
    assert_graph(&mut baseline, &baseline_trace);

    must_reject_semantic_fault("overmerge", |graph, _, left, right| {
        merge_validated(graph, left, right);
        graph.rebuild();
    });
    must_reject_semantic_fault("undermerge", |_, trace, left, right| {
        trace.explicit_unions.push((left, right));
    });
    must_reject_snapshot_fault("missing_parent", |snapshot, left, _| {
        snapshot
            .classes
            .iter_mut()
            .find(|class| class.id == left)
            .expect("left class is present in snapshot")
            .parents
            .clear();
    });
    must_reject_snapshot_fault("pending_dirty", |snapshot, left, right| {
        snapshot.dirty.push((left, right));
    });
    must_reject_snapshot_fault("wrong_node_op", |snapshot, _, _| {
        snapshot.nodes.last_mut().expect("parent node").op = 99;
    });

    println!("EGRAPH_CHECKER_CONTROLS baseline=1 faults=5");
}

#[derive(Default)]
struct CampaignMetrics {
    actions: usize,
    inserted_classes: usize,
    real_unions: usize,
    repaired_parent_candidates: usize,
    congruence_merges: usize,
    collisions: usize,
    pair_checks: usize,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn index(&mut self, len: usize) -> usize {
        self.next() as usize % len
    }
}

fn run_campaign(seed: u64, metrics: &mut CampaignMetrics) {
    let mut graph = Egraph::new();
    let mut trace = Trace::default();
    let mut handles = Vec::new();
    let mut rng = Rng::new(seed ^ 0x9e37_79b9_7f4a_7c15);
    let op_base = 1000 + (seed as OpId) * 32;

    for action in 0..100 {
        metrics.actions += 1;
        if action < 8 {
            let id = insert_traced(&mut graph, &mut trace, op_base + action as OpId, vec![]);
            handles.push(id);
        } else if action < 16 {
            let child = handles[action - 8];
            let id = insert_traced(&mut graph, &mut trace, 100, vec![child]);
            handles.push(id);
        } else if action < 24 {
            let left = handles[action - 16];
            let right = handles[(action - 15) % 8];
            let id = insert_traced(&mut graph, &mut trace, 200, vec![left, right]);
            handles.push(id);
        } else if action < 28 {
            let left = handles[action - 24];
            let right = handles[action - 20];
            let (mut before, _) = least_congruence(&trace);
            if before.find(left) != before.find(right) {
                metrics.repaired_parent_candidates += trace
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.children.iter().any(|&child| {
                            before.find(child) == before.find(left)
                                || before.find(child) == before.find(right)
                        })
                    })
                    .count();
                merge_validated(&mut graph, left, right);
                trace.explicit_unions.push((left, right));
                metrics.real_unions += 1;
            }
            graph.rebuild();
        } else {
            match rng.next() % 5 {
                0 => {
                    let op = op_base + (rng.next() % 12) as OpId;
                    let id = insert_traced(&mut graph, &mut trace, op, vec![]);
                    handles.push(id);
                }
                1 => {
                    let child = handles[rng.index(handles.len())];
                    let id = insert_traced(
                        &mut graph,
                        &mut trace,
                        100 + (rng.next() % 4) as OpId,
                        vec![child],
                    );
                    handles.push(id);
                }
                2 => {
                    let left = handles[rng.index(handles.len())];
                    let right = handles[rng.index(handles.len())];
                    let id = insert_traced(
                        &mut graph,
                        &mut trace,
                        200 + (rng.next() % 4) as OpId,
                        vec![left, right],
                    );
                    handles.push(id);
                }
                3 => {
                    let left = handles[rng.index(handles.len())];
                    let right = handles[rng.index(handles.len())];
                    let (mut before, _) = least_congruence(&trace);
                    if before.find(left) != before.find(right) {
                        merge_validated(&mut graph, left, right);
                        trace.explicit_unions.push((left, right));
                        metrics.real_unions += 1;
                    }
                    graph.rebuild();
                }
                _ => graph.rebuild(),
            }
        }

        graph.rebuild();
        let (checks, stats) = assert_graph(&mut graph, &trace);
        metrics.pair_checks += checks;
        metrics.congruence_merges += stats.congruence_merges;
        metrics.collisions += stats.collisions;
    }
    metrics.inserted_classes += trace.nodes.len();
}

#[test]
fn replayable_campaign_matches_least_congruence() {
    let mut metrics = CampaignMetrics::default();
    for seed in 1..=100 {
        run_campaign(seed, &mut metrics);
    }

    assert_eq!(metrics.actions, 10_000);
    assert!(metrics.inserted_classes > 100);
    assert!(metrics.real_unions > 0);
    assert!(metrics.repaired_parent_candidates > 0);
    assert!(metrics.congruence_merges > 0);
    assert!(metrics.collisions > 0);
    assert!(metrics.pair_checks > 0);
    println!(
        "EGRAPH_CAMPAIGN seeds=100 actions={} classes={} unions={} repaired={} congruence={} collisions={} pairs={}",
        metrics.actions,
        metrics.inserted_classes,
        metrics.real_unions,
        metrics.repaired_parent_candidates,
        metrics.congruence_merges,
        metrics.collisions,
        metrics.pair_checks
    );
}

fn execute_small_sequence(sequence: &[u8], pair_checks: &mut usize) {
    let mut graph = Egraph::new();
    let mut trace = Trace::default();
    let left = insert_traced(&mut graph, &mut trace, 1, vec![]);
    let right = insert_traced(&mut graph, &mut trace, 2, vec![]);

    for &action in sequence {
        match action {
            0 => {
                insert_traced(&mut graph, &mut trace, 10, vec![left]);
            }
            1 => {
                insert_traced(&mut graph, &mut trace, 10, vec![right]);
            }
            2 => {
                let (mut oracle, _) = least_congruence(&trace);
                if oracle.find(left) != oracle.find(right) {
                    merge_validated(&mut graph, left, right);
                    trace.explicit_unions.push((left, right));
                }
                graph.rebuild();
            }
            3 => {
                insert_traced(&mut graph, &mut trace, 20, vec![left, right]);
            }
            4 => graph.rebuild(),
            _ => unreachable!(),
        }
        graph.rebuild();
        let (checks, _) = assert_graph(&mut graph, &trace);
        *pair_checks += checks;
    }
}

fn enumerate_sequences(
    prefix: &mut Vec<u8>,
    remaining: usize,
    count: &mut usize,
    pairs: &mut usize,
) {
    execute_small_sequence(prefix, pairs);
    *count += 1;
    if remaining == 0 {
        return;
    }
    for action in 0..5 {
        prefix.push(action);
        enumerate_sequences(prefix, remaining - 1, count, pairs);
        prefix.pop();
    }
}

#[test]
fn exhaustive_small_action_domain_matches_oracle() {
    let mut count = 0;
    let mut pairs = 0;
    enumerate_sequences(&mut Vec::new(), 6, &mut count, &mut pairs);
    assert_eq!(count, 19_531, "5^0 + ... + 5^6 denominator changed");
    assert!(pairs > 0);
    println!("EGRAPH_EXHAUSTIVE alphabet=5 max_length=6 sequences={count} pairs={pairs}");
}
