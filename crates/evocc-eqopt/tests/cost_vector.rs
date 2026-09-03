//! A cost function is a vector of per-operator weights.
//!
//! Section 7.5 ("Cost is a policy") lets a producer extract under any
//! deterministic cost function. The shipped functions are five named points;
//! this file pins that they are points *in one space* -- a weight per operator
//! the P0 profile can emit or extract, with the constant fixed at zero by the
//! profile's shape rather than by any policy. A search over cost is a search
//! over that vector, so the vector form has to reproduce each shipped function
//! exactly, on every operator, before anything is searched.

use evocc_eqopt::{shipped_policies, CostAxis, CostFunction, CostModel, OperatorWeights};
use evocc_evoir::CmpOp;
use evocc_eqopt::ScalarOperator;

/// Every operator the P0 e-graph admits, the constant included.
fn every_operator() -> Vec<ScalarOperator> {
    let mut operators = vec![
        ScalarOperator::Const,
        ScalarOperator::Add,
        ScalarOperator::Sub,
        ScalarOperator::Mul,
        ScalarOperator::And,
        ScalarOperator::Or,
        ScalarOperator::Xor,
        ScalarOperator::Shl,
        ScalarOperator::LShr,
        ScalarOperator::AShr,
        ScalarOperator::ZExt,
        ScalarOperator::SExt,
        ScalarOperator::Trunc,
    ];
    for comparison in [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::SLt,
        CmpOp::SLe,
        CmpOp::ULt,
        CmpOp::ULe,
    ] {
        operators.push(ScalarOperator::ICmp(comparison));
    }
    operators
}

#[test]
fn the_axis_table_names_every_operator_but_the_constant() {
    assert_eq!(CostAxis::ALL.len(), 13);
    let mut names = CostAxis::ALL
        .iter()
        .map(|axis| axis.name())
        .collect::<Vec<_>>();
    names.sort_unstable();
    let distinct = {
        let mut copy = names.clone();
        copy.dedup();
        copy
    };
    assert_eq!(names, distinct, "two axes share a name");
    assert_eq!(
        names,
        vec![
            "add", "and", "ashr", "icmp", "lshr", "mul", "or", "sext", "shl", "sub", "trunc",
            "xor", "zext",
        ]
    );

    assert_eq!(CostAxis::of(ScalarOperator::Const), None);
    for operator in every_operator() {
        if operator == ScalarOperator::Const {
            continue;
        }
        assert!(
            CostAxis::of(operator).is_some(),
            "{operator:?} has no weight axis, so a vector cannot price it"
        );
    }
    // Every comparison shares one axis: the vector prices `icmp`, not each
    // predicate, which is what makes the projection of a shipped function on
    // to the vector lossless.
    for comparison in [CmpOp::Eq, CmpOp::Ne, CmpOp::SLt, CmpOp::SLe, CmpOp::ULt] {
        assert_eq!(
            CostAxis::of(ScalarOperator::ICmp(comparison)),
            Some(CostAxis::ICmp)
        );
    }
}

#[test]
fn each_shipped_cost_function_equals_its_vector_form_on_every_operator() {
    for policy in shipped_policies() {
        let named = policy.cost();
        let vector = CostFunction::Vector(named.weights());
        for operator in every_operator() {
            assert_eq!(
                named.operator_cost(operator),
                vector.operator_cost(operator),
                "{} disagrees with its vector form on {operator:?}",
                policy.name()
            );
        }
    }
}

#[test]
fn the_shipped_weight_vectors_are_the_recorded_numbers() {
    // add, sub, mul, and, or, xor, shl, lshr, ashr, zext, sext, trunc, icmp
    let expected: [(&str, [u64; 13]); 5] = [
        (
            "instruction-count",
            [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        ),
        (
            "weighted-operators",
            [1, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        ),
        ("zero-multiply", [1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]),
        ("prefer-add", [1, 1, 3, 1, 1, 1, 2, 2, 2, 1, 1, 1, 1]),
        ("prefer-shift", [2, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]),
    ];
    for (name, values) in expected {
        let policy = evocc_eqopt::policy_by_name(name).expect("a shipped policy");
        assert_eq!(
            policy.cost().weights().values(),
            values,
            "{name} moved off its recorded weight vector"
        );
    }
}

#[test]
fn a_constant_costs_nothing_under_every_weighting() {
    let heavy = OperatorWeights::uniform(7);
    assert_eq!(
        CostFunction::Vector(heavy).operator_cost(ScalarOperator::Const),
        0,
        "the constant's zero belongs to the profile, not to a policy"
    );
    for policy in shipped_policies() {
        assert_eq!(policy.cost().operator_cost(ScalarOperator::Const), 0);
    }
}

#[test]
fn a_weight_vector_renders_and_parses_back() {
    let weights = OperatorWeights::uniform(1)
        .with(CostAxis::Mul, 5)
        .with(CostAxis::Shl, 0);
    let text = weights.render();
    assert_eq!(
        text,
        "add=1,sub=1,mul=5,and=1,or=1,xor=1,shl=0,lshr=1,ashr=1,zext=1,sext=1,trunc=1,icmp=1"
    );
    assert_eq!(OperatorWeights::parse(&text).expect("a round trip"), weights);
    assert!(OperatorWeights::parse("add=1").is_err());
    assert!(OperatorWeights::parse(&text.replace("mul", "mull")).is_err());
}

#[test]
fn a_vector_policy_carries_its_weights_into_its_own_description() {
    let weights = OperatorWeights::uniform(2).with(CostAxis::Add, 4);
    let rendered = format!("{:?}", CostFunction::Vector(weights));
    assert!(
        rendered.contains("add=4"),
        "a searched policy must name its weights in the report: {rendered}"
    );
}
