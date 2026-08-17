use evocc_proof::{
    replay_certificate, Binding, CoverageStatus, FinalRoot, MergeReceipt, NodeId, NodeSnapshot,
    Pattern, PatternChild, ProofCertificate, ProofEvent, RuleSnapshot, UnionReason,
};

fn valid_rewrite_then_congruence_certificate() -> ProofCertificate {
    let nodes = [
        NodeSnapshot {
            id: NodeId(0),
            owner: 0,
            op: 1,
            original_children: vec![],
        },
        NodeSnapshot {
            id: NodeId(1),
            owner: 1,
            op: 2,
            original_children: vec![],
        },
        NodeSnapshot {
            id: NodeId(2),
            owner: 2,
            op: 10,
            original_children: vec![0, 1],
        },
        NodeSnapshot {
            id: NodeId(3),
            owner: 3,
            op: 10,
            original_children: vec![1, 0],
        },
        NodeSnapshot {
            id: NodeId(4),
            owner: 4,
            op: 20,
            original_children: vec![2],
        },
        NodeSnapshot {
            id: NodeId(5),
            owner: 5,
            op: 20,
            original_children: vec![3],
        },
    ];

    ProofCertificate {
        rules: vec![RuleSnapshot {
            id: 7,
            lhs: Pattern {
                op: 10,
                children: vec![PatternChild::Var("x".into()), PatternChild::Var("y".into())],
            },
            rhs: Pattern {
                op: 10,
                children: vec![PatternChild::Var("y".into()), PatternChild::Var("x".into())],
            },
        }],
        events: nodes
            .into_iter()
            .map(ProofEvent::Insert)
            .chain([
                ProofEvent::Union(MergeReceipt {
                    lhs_pre: 2,
                    rhs_pre: 3,
                    result: 2,
                    reason: UnionReason::UnconditionalRewrite {
                        rule: 7,
                        lhs_node: NodeId(2),
                        rhs_node: NodeId(3),
                        substitution: vec![
                            Binding {
                                variable: "x".into(),
                                eclass: 0,
                            },
                            Binding {
                                variable: "y".into(),
                                eclass: 1,
                            },
                        ],
                    },
                }),
                ProofEvent::Union(MergeReceipt {
                    lhs_pre: 4,
                    rhs_pre: 5,
                    result: 4,
                    reason: UnionReason::Congruence {
                        lhs_node: NodeId(4),
                        rhs_node: NodeId(5),
                    },
                }),
            ])
            .collect(),
        final_roots: vec![
            FinalRoot { eclass: 0, root: 0 },
            FinalRoot { eclass: 1, root: 1 },
            FinalRoot { eclass: 2, root: 2 },
            FinalRoot { eclass: 3, root: 2 },
            FinalRoot { eclass: 4, root: 4 },
            FinalRoot { eclass: 5, root: 4 },
        ],
    }
}

fn trusted_rules() -> Vec<RuleSnapshot> {
    valid_rewrite_then_congruence_certificate().rules
}

fn replay(
    certificate: &ProofCertificate,
) -> Result<evocc_proof::ReplayReport, evocc_proof::ReplayError> {
    replay_certificate(certificate, &trusted_rules())
}

#[test]
fn replays_valid_non_empty_rewrite_and_congruence() {
    let report = replay(&valid_rewrite_then_congruence_certificate())
        .expect("a fully bound rewrite followed by congruence must replay");
    assert_eq!(report.unions_replayed, 2);
    assert_eq!(report.coverage.guarded_facts, CoverageStatus::NotCovered);
}

fn inserted_node_mut(certificate: &mut ProofCertificate, index: usize) -> &mut NodeSnapshot {
    match &mut certificate.events[index] {
        ProofEvent::Insert(node) => node,
        ProofEvent::Union(_) => panic!("event {index} is not an insertion"),
    }
}

fn union_mut(certificate: &mut ProofCertificate, index: usize) -> &mut MergeReceipt {
    match &mut certificate.events[index] {
        ProofEvent::Union(receipt) => receipt,
        ProofEvent::Insert(_) => panic!("event {index} is not a union"),
    }
}

#[test]
fn rejects_mutated_rewrite_witnesses_and_receipts() {
    let mut mutants: Vec<(&str, ProofCertificate)> = Vec::new();

    let mut duplicate_node = valid_rewrite_then_congruence_certificate();
    inserted_node_mut(&mut duplicate_node, 1).id = NodeId(0);
    mutants.push(("duplicate/non-monotonic NodeId", duplicate_node));

    let mut non_min_result = valid_rewrite_then_congruence_certificate();
    union_mut(&mut non_min_result, 6).result = 3;
    mutants.push(("non-minimum result", non_min_result));

    let mut reversed_endpoints = valid_rewrite_then_congruence_certificate();
    let receipt = union_mut(&mut reversed_endpoints, 6);
    (receipt.lhs_pre, receipt.rhs_pre) = (receipt.rhs_pre, receipt.lhs_pre);
    mutants.push(("reversed ordered endpoints", reversed_endpoints));

    let mut unknown_rule = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { rule, .. } =
        &mut union_mut(&mut unknown_rule, 6).reason
    {
        *rule = 99;
    }
    mutants.push(("unknown RuleId", unknown_rule));

    let mut unsorted_substitution = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { substitution, .. } =
        &mut union_mut(&mut unsorted_substitution, 6).reason
    {
        substitution.swap(0, 1);
    }
    mutants.push(("unsorted substitution", unsorted_substitution));

    let mut incomplete_substitution = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { substitution, .. } =
        &mut union_mut(&mut incomplete_substitution, 6).reason
    {
        substitution.pop();
    }
    mutants.push(("incomplete substitution domain", incomplete_substitution));

    let mut mutated_rhs_children = valid_rewrite_then_congruence_certificate();
    inserted_node_mut(&mut mutated_rhs_children, 3)
        .original_children
        .swap(0, 1);
    mutants.push(("RHS original child order", mutated_rhs_children));

    for (name, mutant) in mutants {
        assert!(replay(&mutant).is_err(), "mutant survived: {name}");
    }
}

#[test]
fn rejects_mutated_congruence_and_final_partition() {
    let mut wrong_operator = valid_rewrite_then_congruence_certificate();
    inserted_node_mut(&mut wrong_operator, 5).op = 21;
    assert!(replay(&wrong_operator).is_err());

    let mut unequal_children = valid_rewrite_then_congruence_certificate();
    inserted_node_mut(&mut unequal_children, 5).original_children[0] = 1;
    assert!(replay(&unequal_children).is_err());

    let mut missing_final_class = valid_rewrite_then_congruence_certificate();
    missing_final_class.final_roots.pop();
    assert!(replay(&missing_final_class).is_err());

    let mut false_final_root = valid_rewrite_then_congruence_certificate();
    false_final_root.final_roots[3].root = 3;
    assert!(replay(&false_final_root).is_err());
}

#[test]
fn trusted_rule_registry_rejects_a_self_authorized_forgery() {
    let trusted = trusted_rules();
    let forged_rule = RuleSnapshot {
        id: 7,
        lhs: Pattern {
            op: 1,
            children: vec![],
        },
        rhs: Pattern {
            op: 2,
            children: vec![],
        },
    };
    let mut certificate = valid_rewrite_then_congruence_certificate();
    certificate.rules = vec![forged_rule];
    certificate.events.truncate(2);
    certificate.events.push(ProofEvent::Union(MergeReceipt {
        lhs_pre: 0,
        rhs_pre: 1,
        result: 0,
        reason: UnionReason::UnconditionalRewrite {
            rule: 7,
            lhs_node: NodeId(0),
            rhs_node: NodeId(1),
            substitution: vec![],
        },
    }));
    certificate.final_roots = vec![
        FinalRoot { eclass: 0, root: 0 },
        FinalRoot { eclass: 1, root: 0 },
    ];

    assert!(
        replay_certificate(&certificate, &trusted).is_err(),
        "a certificate must not be able to authorize its own forged rule"
    );
}

#[test]
fn rejects_rhs_only_variables_and_non_exact_substitutions() {
    let mut rhs_only_rules = trusted_rules();
    rhs_only_rules[0].rhs.children[0] = PatternChild::Var("z".into());
    let mut rhs_only_certificate = valid_rewrite_then_congruence_certificate();
    rhs_only_certificate.rules = rhs_only_rules.clone();
    assert!(replay_certificate(&rhs_only_certificate, &rhs_only_rules).is_err());

    let mut extra = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { substitution, .. } =
        &mut union_mut(&mut extra, 6).reason
    {
        substitution.push(Binding {
            variable: "z".into(),
            eclass: 0,
        });
    }
    assert!(replay(&extra).is_err());

    let mut duplicate = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { substitution, .. } =
        &mut union_mut(&mut duplicate, 6).reason
    {
        substitution.insert(1, substitution[0].clone());
    }
    assert!(replay(&duplicate).is_err());
}

#[test]
fn rejects_temporal_owner_node_and_rule_mutants() {
    let mut future_node = valid_rewrite_then_congruence_certificate();
    let rewrite = future_node.events.remove(6);
    future_node.events.insert(3, rewrite);
    assert!(replay(&future_node).is_err());

    let mut premature_congruence = valid_rewrite_then_congruence_certificate();
    premature_congruence.events.swap(6, 7);
    assert!(
        replay(&premature_congruence).is_err(),
        "congruence cannot consume a child equality established by a future event"
    );

    let mut unknown_node = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { rhs_node, .. } =
        &mut union_mut(&mut unknown_node, 6).reason
    {
        *rhs_node = NodeId(999);
    }
    assert!(replay(&unknown_node).is_err());

    let mut wrong_owner = valid_rewrite_then_congruence_certificate();
    if let UnionReason::UnconditionalRewrite { lhs_node, .. } =
        &mut union_mut(&mut wrong_owner, 6).reason
    {
        *lhs_node = NodeId(4);
    }
    assert!(replay(&wrong_owner).is_err());

    let mut wrong_rewrite_op = valid_rewrite_then_congruence_certificate();
    inserted_node_mut(&mut wrong_rewrite_op, 3).op = 11;
    assert!(replay(&wrong_rewrite_op).is_err());

    let mut duplicate_rules = trusted_rules();
    duplicate_rules.push(duplicate_rules[0].clone());
    let mut duplicate_rule_certificate = valid_rewrite_then_congruence_certificate();
    duplicate_rule_certificate.rules = duplicate_rules.clone();
    assert!(replay_certificate(&duplicate_rule_certificate, &duplicate_rules).is_err());
}

#[test]
fn rejects_extra_final_root_and_duplicate_union() {
    let mut extra_final = valid_rewrite_then_congruence_certificate();
    extra_final
        .final_roots
        .push(FinalRoot { eclass: 6, root: 6 });
    assert!(replay(&extra_final).is_err());

    let mut duplicate_union = valid_rewrite_then_congruence_certificate();
    duplicate_union
        .events
        .push(duplicate_union.events[6].clone());
    assert!(replay(&duplicate_union).is_err());
}

#[test]
fn empty_certificate_does_not_claim_observed_reason_coverage() {
    let rules = trusted_rules();
    let certificate = ProofCertificate {
        rules: rules.clone(),
        events: vec![],
        final_roots: vec![],
    };
    let report = replay_certificate(&certificate, &rules).unwrap();
    assert_eq!(report.unions_replayed, 0);
    assert_eq!(
        report.coverage.unconditional_rewrites,
        CoverageStatus::NotCovered
    );
    assert_eq!(report.coverage.congruence, CoverageStatus::NotCovered);
    assert_eq!(report.coverage.guarded_facts, CoverageStatus::NotCovered);
}
