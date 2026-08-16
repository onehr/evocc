use evocc_proof::{Justification, ProofDAG};

#[test]
fn empty_proof_is_a_valid_positive_control() {
    let dag = ProofDAG::new();
    assert!(dag.validate().is_ok());
    println!("PROOF_SEMANTICS positive=1");
}

#[test]
fn reflexive_cannot_justify_distinct_classes() {
    let mut dag = ProofDAG::new();
    dag.add_edge(41, 42, Justification::Reflexive)
        .expect("syntax admission is separate from semantic validation");
    assert!(dag.validate().is_err(), "Reflexive must not prove 41 == 42");
    println!("PROOF_SEMANTICS reflexive_rejected=1");
}

#[test]
fn rewrite_without_a_rule_and_substitution_context_is_rejected() {
    let mut dag = ProofDAG::new();
    dag.add_edge(
        7,
        8,
        Justification::Rewrite {
            rule: 11,
            subst: 13,
            premises: Vec::new(),
        },
    )
    .expect("syntax admission is separate from semantic validation");
    assert!(
        dag.validate().is_err(),
        "opaque rule and substitution IDs are not a replayable proof"
    );
    println!("PROOF_SEMANTICS unbound_rewrite_rejected=1");
}

#[test]
fn congruence_without_an_enode_snapshot_is_rejected() {
    let mut dag = ProofDAG::new();
    dag.add_edge(
        17,
        18,
        Justification::Congruence {
            lhs_node: 1,
            rhs_node: 2,
        },
    )
    .expect("syntax admission is separate from semantic validation");
    assert!(
        dag.validate().is_err(),
        "node IDs without operator/children/owner context are not a proof"
    );
    println!("PROOF_SEMANTICS unbound_congruence_rejected=1");
}
