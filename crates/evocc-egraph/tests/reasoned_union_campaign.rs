use std::collections::{BTreeMap, BTreeSet, HashMap};

use evocc_egraph::{
    apply_rewrites, EClassId, Egraph, InsertOutcome, OpId, Pattern, PatternChild, Rewrite,
};
use evocc_proof::{
    CoverageStatus, FinalRoot, MergeReceipt, NodeId, ProofCertificate, ProofContext, ProofEvent,
    RuleId, RuleSnapshot, UnionReason,
};

const SEEDS: u32 = 100;
const BLOCKS_PER_SEED: u32 = 10;
const ACTIONS_PER_BLOCK: usize = 10;
const REWRITES_PER_SEED: usize = 10;
const CONGRUENCE_MERGES_PER_SEED: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NodeHandle {
    eclass: EClassId,
    node: NodeId,
}

#[derive(Clone, Debug)]
struct OracleNode {
    op: OpId,
    original_children: Vec<EClassId>,
    owner: EClassId,
    node: NodeId,
}

#[derive(Default)]
struct OracleTrace {
    nodes: Vec<OracleNode>,
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

struct OracleResult {
    dsu: Dsu,
    explicit_merges: usize,
    congruence_merges: usize,
}

fn least_congruence(trace: &OracleTrace) -> OracleResult {
    let mut dsu = Dsu::default();
    for node in &trace.nodes {
        dsu.add(node.owner);
        for &child in &node.original_children {
            dsu.add(child);
        }
    }

    let mut explicit_merges = 0;
    for &(left, right) in &trace.explicit_unions {
        explicit_merges += usize::from(dsu.union(left, right));
    }

    let mut congruence_merges = 0;
    loop {
        let mut changed = false;
        let mut owner_by_key: HashMap<(OpId, Vec<EClassId>), EClassId> = HashMap::new();
        for node in &trace.nodes {
            let key = (
                node.op,
                node.original_children
                    .iter()
                    .map(|&child| dsu.find(child))
                    .collect(),
            );
            if let Some(&other) = owner_by_key.get(&key) {
                if dsu.union(node.owner, other) {
                    congruence_merges += 1;
                    changed = true;
                }
            } else {
                owner_by_key.insert(key, node.owner);
            }
        }
        if !changed {
            break;
        }
    }

    OracleResult {
        dsu,
        explicit_merges,
        congruence_merges,
    }
}

fn pattern(op: OpId, left: &str, right: &str) -> Pattern {
    Pattern {
        op,
        children: vec![
            PatternChild::Var(left.to_owned()),
            PatternChild::Var(right.to_owned()),
        ],
    }
}

fn trusted_rules(seed: u32) -> Vec<RuleSnapshot> {
    let mut rules = (0..BLOCKS_PER_SEED)
        .map(|block| {
            let add_op = add_op(seed, block);
            RuleSnapshot {
                id: rule_id(seed, block),
                lhs: pattern(add_op, "x", "y"),
                rhs: pattern(add_op, "y", "x"),
            }
        })
        .collect::<Vec<_>>();
    rules.sort_by_key(|rule| rule.id);
    rules
}

fn rule_id(seed: u32, block: u32) -> RuleId {
    seed * 100 + block
}

fn add_op(seed: u32, block: u32) -> OpId {
    (1_000 + seed * 10 + block) as OpId
}

fn wrap_op(seed: u32, block: u32) -> OpId {
    (5_000 + seed * 10 + block) as OpId
}

fn leaf_ops(seed: u32, block: u32) -> (OpId, OpId) {
    let first = 10_000 + seed * 20 + block * 2;
    (first as OpId, (first + 1) as OpId)
}

fn union_event_count(certificate: &ProofCertificate) -> usize {
    certificate
        .events
        .iter()
        .filter(|event| matches!(event, ProofEvent::Union(_)))
        .count()
}

fn insert_new(
    graph: &mut Egraph,
    _rules: &[RuleSnapshot],
    trace: &mut OracleTrace,
    actions: &mut usize,
    op: OpId,
    children: Vec<EClassId>,
) -> NodeHandle {
    let before = graph.proof_certificate().events.len();
    *actions += 1;
    let InsertOutcome {
        eclass,
        node,
        inserted,
    } = graph.insert_with_node(op, children.clone());
    assert!(
        inserted,
        "campaign primary insertion unexpectedly hash-consed"
    );

    let certificate = graph.proof_certificate();
    assert_eq!(
        certificate.events.len(),
        before + 1,
        "one fresh insertion must append exactly one immutable event"
    );
    assert_eq!(
        certificate.events.last(),
        Some(&ProofEvent::Insert(evocc_proof::NodeSnapshot {
            id: node,
            owner: eclass,
            op,
            original_children: children.clone(),
        }))
    );
    trace.nodes.push(OracleNode {
        op,
        original_children: children,
        owner: eclass,
        node,
    });
    NodeHandle { eclass, node }
}

fn insert_pair_in_seed_order(
    graph: &mut Egraph,
    rules: &[RuleSnapshot],
    trace: &mut OracleTrace,
    actions: &mut usize,
    seed: u32,
    block: u32,
) -> (NodeHandle, NodeHandle) {
    let (x_op, y_op) = leaf_ops(seed, block);
    if (seed + block) & 1 == 0 {
        let x = insert_new(graph, rules, trace, actions, x_op, Vec::new());
        let y = insert_new(graph, rules, trace, actions, y_op, Vec::new());
        (x, y)
    } else {
        let y = insert_new(graph, rules, trace, actions, y_op, Vec::new());
        let x = insert_new(graph, rules, trace, actions, x_op, Vec::new());
        (x, y)
    }
}

fn assert_union_delta(
    before: &ProofCertificate,
    after: &ProofCertificate,
    expected_rewrite: usize,
    expected_congruence: usize,
) {
    let new_events = &after.events[before.events.len()..];
    let rewrite = new_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ProofEvent::Union(MergeReceipt {
                    reason: UnionReason::UnconditionalRewrite { .. },
                    ..
                })
            )
        })
        .count();
    let congruence = new_events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ProofEvent::Union(MergeReceipt {
                    reason: UnionReason::Congruence { .. },
                    ..
                })
            )
        })
        .count();
    assert_eq!(rewrite, expected_rewrite, "rewrite receipt delta drifted");
    assert_eq!(
        congruence, expected_congruence,
        "congruence receipt delta drifted"
    );
    assert_eq!(
        new_events.len(),
        expected_rewrite + expected_congruence,
        "rewrite/rebuild appended a non-union event"
    );
}

fn roots_by_eclass(final_roots: &[FinalRoot]) -> BTreeMap<EClassId, EClassId> {
    let mut roots = BTreeMap::new();
    for entry in final_roots {
        assert!(
            roots.insert(entry.eclass, entry.root).is_none(),
            "certificate repeats final root for eclass {}",
            entry.eclass
        );
    }
    roots
}

#[derive(Default)]
struct CampaignMetrics {
    seeds: usize,
    actions: usize,
    insert_events: usize,
    rewrite_receipts: usize,
    congruence_receipts: usize,
    pair_checks: usize,
    replayed_unions: usize,
    seed_signatures: BTreeSet<Vec<(OpId, Vec<EClassId>)>>,
    forward_blocks: usize,
    reverse_blocks: usize,
}

struct SeedArtifact {
    certificate: ProofCertificate,
    trusted_rules: Vec<RuleSnapshot>,
    producer_commitment: ProducerCommitment,
}

/// Independent observation of what the producer actually did. A proof can
/// establish that every event it contains is valid, but without this boundary
/// it cannot prove that the producer did not omit an entire self-contained
/// component and its final roots.
struct ProducerCommitment {
    inserted: BTreeMap<NodeId, (EClassId, OpId, Vec<EClassId>)>,
    final_eclasses: BTreeSet<EClassId>,
    event_count: usize,
    union_count: usize,
}

impl ProducerCommitment {
    fn from_trace(trace: &OracleTrace, certificate: &ProofCertificate) -> Self {
        Self {
            inserted: trace
                .nodes
                .iter()
                .map(|node| {
                    (
                        node.node,
                        (node.owner, node.op, node.original_children.clone()),
                    )
                })
                .collect(),
            final_eclasses: trace.nodes.iter().map(|node| node.owner).collect(),
            event_count: certificate.events.len(),
            union_count: union_event_count(certificate),
        }
    }

    fn matches(&self, certificate: &ProofCertificate) -> bool {
        let inserted = certificate
            .events
            .iter()
            .filter_map(|event| match event {
                ProofEvent::Insert(node) => Some((
                    node.id,
                    (node.owner, node.op, node.original_children.clone()),
                )),
                ProofEvent::Union(_) => None,
            })
            .collect::<BTreeMap<_, _>>();
        let final_eclasses = certificate
            .final_roots
            .iter()
            .map(|entry| entry.eclass)
            .collect::<BTreeSet<_>>();
        inserted == self.inserted
            && final_eclasses == self.final_eclasses
            && certificate.events.len() == self.event_count
            && union_event_count(certificate) == self.union_count
    }
}

fn run_seed(seed: u32, metrics: &mut CampaignMetrics) -> SeedArtifact {
    let trusted = trusted_rules(seed);
    let context = ProofContext::new(trusted.clone()).expect("trusted registry is well formed");
    let mut graph = Egraph::with_rules(&trusted).expect("graph accepts the trusted rule registry");
    let mut trace = OracleTrace::default();
    let mut seed_actions = 0;

    for block in 0..BLOCKS_PER_SEED {
        let (x, y) = insert_pair_in_seed_order(
            &mut graph,
            &trusted,
            &mut trace,
            &mut seed_actions,
            seed,
            block,
        );
        let add = add_op(seed, block);

        let (xy, yx) = if (seed ^ block) & 1 == 0 {
            metrics.forward_blocks += 1;
            let xy = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                add,
                vec![x.eclass, y.eclass],
            );
            let yx = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                add,
                vec![y.eclass, x.eclass],
            );
            (xy, yx)
        } else {
            metrics.reverse_blocks += 1;
            let yx = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                add,
                vec![y.eclass, x.eclass],
            );
            let xy = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                add,
                vec![x.eclass, y.eclass],
            );
            (xy, yx)
        };

        let wrap = wrap_op(seed, block);
        let (wrapped_xy, wrapped_yx) = if (seed.wrapping_mul(3) + block) & 1 == 0 {
            let wrapped_xy = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                wrap,
                vec![xy.eclass],
            );
            let wrapped_yx = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                wrap,
                vec![yx.eclass],
            );
            (wrapped_xy, wrapped_yx)
        } else {
            let wrapped_yx = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                wrap,
                vec![yx.eclass],
            );
            let wrapped_xy = insert_new(
                &mut graph,
                &trusted,
                &mut trace,
                &mut seed_actions,
                wrap,
                vec![xy.eclass],
            );
            (wrapped_xy, wrapped_yx)
        };

        let rule_snapshot = &trusted[block as usize];
        let rewrite = Rewrite::new(
            rule_snapshot.id,
            format!("commute-seed-{seed}-block-{block}"),
            rule_snapshot.lhs.clone(),
            rule_snapshot.rhs.clone(),
        );
        assert_eq!(
            rewrite.snapshot(),
            rule_snapshot.clone(),
            "production rewrite differs from the trusted declarative rule"
        );

        let before_apply = graph.proof_certificate();
        seed_actions += 1;
        assert_eq!(
            apply_rewrites(&mut graph, &[rewrite], 1),
            1,
            "each isolated commutativity rule must perform one real rewrite union"
        );
        trace.explicit_unions.push((xy.eclass, yx.eclass));
        let after_apply = graph.proof_certificate();
        assert_union_delta(&before_apply, &after_apply, 1, 1);
        let added = &after_apply.events[before_apply.events.len()..];
        assert_eq!(added.len(), 2);
        let ProofEvent::Union(MergeReceipt {
            reason:
                UnionReason::UnconditionalRewrite {
                    lhs_node, rhs_node, ..
                },
            ..
        }) = &added[0]
        else {
            panic!("apply_rewrites must log its rewrite before rebuild congruence");
        };
        assert_eq!(
            BTreeSet::from([*lhs_node, *rhs_node]),
            BTreeSet::from([xy.node, yx.node]),
            "rewrite receipt lost one of its stable insertion witnesses"
        );
        let ProofEvent::Union(MergeReceipt {
            reason: UnionReason::Congruence { lhs_node, rhs_node },
            ..
        }) = &added[1]
        else {
            panic!("rebuild must log dependent congruence after the rewrite");
        };
        assert_eq!(
            BTreeSet::from([*lhs_node, *rhs_node]),
            BTreeSet::from([wrapped_xy.node, wrapped_yx.node]),
            "congruence receipt lost a wrapper removed from compact live storage"
        );

        // Explicit production rebuild remains a no-op after apply_rewrites has
        // reached congruence closure for this block.
        seed_actions += 1;
        let before_rebuild = graph.proof_certificate();
        graph.rebuild();
        let after_rebuild = graph.proof_certificate();
        assert_union_delta(&before_rebuild, &after_rebuild, 0, 0);

        // A hash-cons hit returns the original stable witness and emits no
        // second insertion event, even after rebuild compacted live storage.
        seed_actions += 1;
        let before_duplicate = graph.proof_certificate();
        let duplicate = graph.insert_with_node(add, vec![x.eclass, y.eclass]);
        assert!(!duplicate.inserted, "hash-cons probe created a fresh node");
        assert_eq!(duplicate.node, xy.node, "stable node identity drifted");
        assert_eq!(graph.find(duplicate.eclass), graph.find(xy.eclass));
        let after_duplicate = graph.proof_certificate();
        assert_eq!(
            before_duplicate.events, after_duplicate.events,
            "hash-cons hit appended an audit event"
        );

        seed_actions += 1;
        let before_final_rebuild = graph.proof_certificate();
        graph.rebuild();
        let after_final_rebuild = graph.proof_certificate();
        assert_union_delta(&before_final_rebuild, &after_final_rebuild, 0, 0);
    }

    assert_eq!(
        seed_actions,
        BLOCKS_PER_SEED as usize * ACTIONS_PER_BLOCK,
        "per-seed action denominator drifted"
    );

    let certificate = graph.proof_certificate();
    assert_eq!(certificate.rules, trusted, "observed rule copy drifted");
    let insert_events = certificate
        .events
        .iter()
        .filter(|event| matches!(event, ProofEvent::Insert(_)))
        .count();
    let rewrite_receipts = certificate
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ProofEvent::Union(MergeReceipt {
                    reason: UnionReason::UnconditionalRewrite { .. },
                    ..
                })
            )
        })
        .count();
    let congruence_receipts = certificate
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ProofEvent::Union(MergeReceipt {
                    reason: UnionReason::Congruence { .. },
                    ..
                })
            )
        })
        .count();
    assert_eq!(insert_events, 60);
    assert_eq!(rewrite_receipts, REWRITES_PER_SEED);
    assert_eq!(congruence_receipts, CONGRUENCE_MERGES_PER_SEED);
    assert_eq!(
        union_event_count(&certificate),
        REWRITES_PER_SEED + CONGRUENCE_MERGES_PER_SEED
    );
    assert_eq!(certificate.events.len(), 80);

    let inserted_by_id = certificate
        .events
        .iter()
        .filter_map(|event| match event {
            ProofEvent::Insert(node) => Some((node.id, node)),
            ProofEvent::Union(_) => None,
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        inserted_by_id.len(),
        trace.nodes.len(),
        "stable NodeId was reused"
    );
    for node in &trace.nodes {
        let observed = inserted_by_id
            .get(&node.node)
            .expect("every returned stable NodeId has one insert event");
        assert_eq!(observed.owner, node.owner);
        assert_eq!(observed.op, node.op);
        assert_eq!(observed.original_children, node.original_children);
    }

    let report = context
        .replay(&certificate)
        .expect("independent trusted-context replay must accept the live certificate");
    assert_eq!(report.unions_replayed, 20);
    assert_eq!(report.rewrite_unions_replayed, 10);
    assert_eq!(report.congruence_unions_replayed, 10);
    assert_eq!(
        report.coverage.unconditional_rewrites,
        CoverageStatus::Covered
    );
    assert_eq!(report.coverage.congruence, CoverageStatus::Covered);
    assert_eq!(report.coverage.guarded_facts, CoverageStatus::NotCovered);

    let OracleResult {
        mut dsu,
        explicit_merges,
        congruence_merges,
    } = least_congruence(&trace);
    assert_eq!(explicit_merges, REWRITES_PER_SEED);
    assert_eq!(congruence_merges, CONGRUENCE_MERGES_PER_SEED);

    let roots = roots_by_eclass(&certificate.final_roots);
    let owners = trace
        .nodes
        .iter()
        .map(|node| node.owner)
        .collect::<Vec<_>>();
    assert_eq!(
        roots.len(),
        owners.len(),
        "final root table is not complete"
    );
    let mut pair_checks = 0;
    for (index, &left) in owners.iter().enumerate() {
        assert_eq!(roots[&left], graph.find(left), "live root copy drifted");
        for &right in &owners[index..] {
            let oracle_equal = dsu.find(left) == dsu.find(right);
            assert_eq!(
                graph.find(left) == graph.find(right),
                oracle_equal,
                "live graph differs from independent least congruence"
            );
            assert_eq!(
                roots[&left] == roots[&right],
                oracle_equal,
                "certificate partition is not iff the independent partition"
            );
            pair_checks += 1;
        }
    }
    assert_eq!(pair_checks, 1_830);

    metrics.seeds += 1;
    metrics.actions += seed_actions;
    metrics.insert_events += insert_events;
    metrics.rewrite_receipts += rewrite_receipts;
    metrics.congruence_receipts += congruence_receipts;
    metrics.pair_checks += pair_checks;
    metrics.replayed_unions += report.unions_replayed;
    metrics.seed_signatures.insert(
        trace
            .nodes
            .iter()
            .map(|node| (node.op, node.original_children.clone()))
            .collect(),
    );

    let producer_commitment = ProducerCommitment::from_trace(&trace, &certificate);
    assert!(producer_commitment.matches(&certificate));

    SeedArtifact {
        certificate,
        trusted_rules: trusted,
        producer_commitment,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FaultKind {
    DeletedUnion,
    DuplicatedUnion,
    ReorderedDependentUnions,
    WrongEndpoint,
    WrongRule,
    WrongSubstitution,
    WrongWitness,
    SilentExtraUnion,
}

const FAULTS: [FaultKind; 8] = [
    FaultKind::DeletedUnion,
    FaultKind::DuplicatedUnion,
    FaultKind::ReorderedDependentUnions,
    FaultKind::WrongEndpoint,
    FaultKind::WrongRule,
    FaultKind::WrongSubstitution,
    FaultKind::WrongWitness,
    FaultKind::SilentExtraUnion,
];

fn first_rewrite_index(certificate: &ProofCertificate) -> usize {
    certificate
        .events
        .iter()
        .position(|event| {
            matches!(
                event,
                ProofEvent::Union(MergeReceipt {
                    reason: UnionReason::UnconditionalRewrite { .. },
                    ..
                })
            )
        })
        .expect("campaign has a rewrite receipt")
}

fn first_congruence_after(certificate: &ProofCertificate, start: usize) -> usize {
    certificate
        .events
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(index, event)| {
            matches!(
                event,
                ProofEvent::Union(MergeReceipt {
                    reason: UnionReason::Congruence { .. },
                    ..
                })
            )
            .then_some(index)
        })
        .expect("campaign has a dependent congruence receipt")
}

fn first_leaf_node(certificate: &ProofCertificate) -> NodeId {
    certificate
        .events
        .iter()
        .find_map(|event| match event {
            ProofEvent::Insert(node) if node.original_children.is_empty() => Some(node.id),
            _ => None,
        })
        .expect("campaign has a leaf witness")
}

fn mutate(certificate: &mut ProofCertificate, kind: FaultKind) {
    let rewrite_index = first_rewrite_index(certificate);
    match kind {
        FaultKind::DeletedUnion => {
            certificate.events.remove(rewrite_index);
        }
        FaultKind::DuplicatedUnion => {
            let duplicate = certificate.events[rewrite_index].clone();
            certificate.events.insert(rewrite_index + 1, duplicate);
        }
        FaultKind::ReorderedDependentUnions => {
            let congruence_index = first_congruence_after(certificate, rewrite_index);
            certificate.events.swap(rewrite_index, congruence_index);
        }
        FaultKind::WrongEndpoint => {
            let (lhs_pre, rhs_pre) = match &certificate.events[rewrite_index] {
                ProofEvent::Union(receipt) => (receipt.lhs_pre, receipt.rhs_pre),
                ProofEvent::Insert(_) => unreachable!(),
            };
            let roots = roots_by_eclass(&certificate.final_roots);
            let outsider = certificate
                .final_roots
                .iter()
                .find(|entry| {
                    entry.eclass == entry.root
                        && entry.root != roots[&lhs_pre]
                        && entry.root != roots[&rhs_pre]
                })
                .expect("campaign has an unrelated canonical endpoint")
                .eclass;
            let ProofEvent::Union(receipt) = &mut certificate.events[rewrite_index] else {
                unreachable!();
            };
            receipt.rhs_pre = outsider;
        }
        FaultKind::WrongRule => {
            let alternate_rule = certificate.rules[1].id;
            let ProofEvent::Union(MergeReceipt {
                reason: UnionReason::UnconditionalRewrite { rule, .. },
                ..
            }) = &mut certificate.events[rewrite_index]
            else {
                unreachable!();
            };
            assert_ne!(*rule, alternate_rule);
            *rule = alternate_rule;
        }
        FaultKind::WrongSubstitution => {
            let ProofEvent::Union(MergeReceipt {
                reason: UnionReason::UnconditionalRewrite { substitution, .. },
                ..
            }) = &mut certificate.events[rewrite_index]
            else {
                unreachable!();
            };
            assert_eq!(substitution.len(), 2);
            substitution[0].eclass = substitution[1].eclass;
        }
        FaultKind::WrongWitness => {
            let leaf = first_leaf_node(certificate);
            let ProofEvent::Union(MergeReceipt {
                reason: UnionReason::UnconditionalRewrite { lhs_node, .. },
                ..
            }) = &mut certificate.events[rewrite_index]
            else {
                unreachable!();
            };
            assert_ne!(*lhs_node, leaf);
            *lhs_node = leaf;
        }
        FaultKind::SilentExtraUnion => {
            let distinct_roots = certificate
                .final_roots
                .iter()
                .map(|entry| entry.root)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(2)
                .collect::<Vec<_>>();
            assert_eq!(distinct_roots.len(), 2);
            let source = distinct_roots[0];
            let target = distinct_roots[1];
            for entry in &mut certificate.final_roots {
                if entry.root == target {
                    entry.root = source;
                }
            }
        }
    }
}

fn delete_last_complete_block(certificate: &mut ProofCertificate) {
    const EVENTS_PER_BLOCK: usize = 8;
    let keep = certificate
        .events
        .len()
        .checked_sub(EVENTS_PER_BLOCK)
        .expect("campaign contains one complete block");
    let removed = &certificate.events[keep..];
    assert_eq!(
        removed
            .iter()
            .filter(|event| matches!(event, ProofEvent::Insert(_)))
            .count(),
        6
    );
    assert_eq!(
        removed
            .iter()
            .filter(|event| matches!(event, ProofEvent::Union(_)))
            .count(),
        2
    );
    let removed_eclasses = removed
        .iter()
        .filter_map(|event| match event {
            ProofEvent::Insert(node) => Some(node.owner),
            ProofEvent::Union(_) => None,
        })
        .collect::<BTreeSet<_>>();
    certificate.events.truncate(keep);
    certificate
        .final_roots
        .retain(|entry| !removed_eclasses.contains(&entry.eclass));
}

#[test]
fn production_reasoned_unions_replay_and_every_fault_is_rejected() {
    let mut metrics = CampaignMetrics::default();
    let mut killed = BTreeMap::<FaultKind, usize>::new();
    let mut omitted_complete_blocks_detected = 0;

    for seed in 1..=SEEDS {
        let artifact = run_seed(seed, &mut metrics);
        let context = ProofContext::new(artifact.trusted_rules.clone())
            .expect("trusted registry is well formed");
        for fault in FAULTS {
            let mut mutant = artifact.certificate.clone();
            mutate(&mut mutant, fault);
            assert!(
                context.replay(&mutant).is_err(),
                "trusted replay accepted {fault:?} mutant for seed {seed}"
            );
            *killed.entry(fault).or_default() += 1;
        }

        // The smaller certificate is internally valid: no checker can infer
        // an omitted disconnected component from a self-contained proof. The
        // independent producer commitment is the boundary that detects it.
        let mut omitted_block = artifact.certificate.clone();
        delete_last_complete_block(&mut omitted_block);
        context
            .replay(&omitted_block)
            .expect("remaining nine blocks form a valid smaller certificate");
        assert!(
            !artifact.producer_commitment.matches(&omitted_block),
            "producer commitment accepted an omitted complete block"
        );
        omitted_complete_blocks_detected += 1;
    }

    assert_eq!(metrics.seeds, 100);
    assert_eq!(metrics.actions, 10_000);
    assert_eq!(metrics.insert_events, 6_000);
    assert_eq!(metrics.rewrite_receipts, 1_000);
    assert_eq!(metrics.congruence_receipts, 1_000);
    assert_eq!(metrics.replayed_unions, 2_000);
    assert_eq!(metrics.pair_checks, 183_000);
    assert_eq!(metrics.seed_signatures.len(), 100);
    assert_eq!(metrics.forward_blocks, 500);
    assert_eq!(metrics.reverse_blocks, 500);
    assert_eq!(killed.len(), FAULTS.len());
    for fault in FAULTS {
        assert_eq!(killed[&fault], 100, "{fault:?} denominator drifted");
    }
    assert_eq!(omitted_complete_blocks_detected, 100);

    println!(
        "REASONED_UNION_CAMPAIGN seeds={} actions={} inserts={} rewrites={} congruence={} replayed={} pairs={} faults=8 killed=800 omitted_blocks=100/100",
        metrics.seeds,
        metrics.actions,
        metrics.insert_events,
        metrics.rewrite_receipts,
        metrics.congruence_receipts,
        metrics.replayed_unions,
        metrics.pair_checks,
    );
}
