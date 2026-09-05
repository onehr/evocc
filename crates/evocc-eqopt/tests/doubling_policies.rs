//! Which spelling of a doubling a policy extracts.
//!
//! Section 7.5 "Cost is a policy": a producer may extract under any cost
//! function that is deterministic, applies the same strict-improvement rule
//! relative to its own costs, and is named in the producer's report. With
//! `p0.scalar.v4` the three spellings of a doubling are one class, so the cost
//! function is what decides which survives. Before the emitting rules every
//! replacement was an alias or a constant and two all-positive cost functions
//! optimized the same programs; this measures that they no longer do.

use evocc_eqopt::{
    optimize_scalar_module_with_policy, policy_by_name, shipped_policies, CostFunction, CostModel,
    ScalarOperator,
};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_types::{EvoIRType, Region};

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(8),
    }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

fn module_of(insts: Vec<Inst>, result: Value) -> Module {
    let x = bits(0);
    let memory = Value {
        id: 1,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 2,
        ty: EvoIRType::World,
    };
    let function = Function {
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(8),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            EvoIRType::Bits(8),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&x), argument(&memory), argument(&world)],
            insts,
            terminator: Inst::Ret(vec![result, memory, world]),
        }],
    };
    let mut module = Module {
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the doubling probe is valid EvoIR");
    module
}

fn multiplied() -> Module {
    let two = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(two.clone(), 2),
            Inst::Mul(result.clone(), bits(0), two),
        ],
        result,
    )
}

fn added() -> Module {
    let result = bits(3);
    module_of(vec![Inst::Add(result.clone(), bits(0), bits(0))], result)
}

fn shifted() -> Module {
    let one = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(one.clone(), 1),
            Inst::Shl(result.clone(), bits(0), one),
        ],
        result,
    )
}

fn extracted(module: &Module, policy: &str) -> String {
    let policy = policy_by_name(policy).expect("this producer ships the named policy");
    render_module(&optimize_scalar_module_with_policy(module, &policy).module)
}

/// The producer ships the two weightings that make the choice visible, and
/// every one of them is deterministic and named.
#[test]
fn the_shipped_policies_include_both_doubling_weightings() {
    let names = shipped_policies()
        .iter()
        .map(|policy| policy.name().to_owned())
        .collect::<Vec<_>>();
    println!("policies={}", names.join(","));
    for name in [
        "instruction-count",
        "weighted-operators",
        "zero-multiply",
        "prefer-add",
        "prefer-shift",
    ] {
        assert!(names.contains(&name.to_owned()), "{name} is not shipped");
    }
}

/// The weighted cost functions prefer `add` or `shl` to `mul` by their own
/// weights, which is what makes a doubling a strict improvement under them.
#[test]
fn the_weighted_functions_charge_more_for_a_multiply() {
    for function in [
        CostFunction::WeightedOperators,
        CostFunction::PreferAdd,
        CostFunction::PreferShift,
    ] {
        let multiply = function.operator_cost(ScalarOperator::Mul);
        let add = function.operator_cost(ScalarOperator::Add);
        let shift = function.operator_cost(ScalarOperator::Shl);
        println!("{function:?}: mul={multiply} add={add} shl={shift}");
        assert!(
            multiply > add || multiply > shift,
            "{function:?} must prefer an addition or a shift to a multiply"
        );
        assert!(
            function.operator_cost(ScalarOperator::Const) == 0,
            "a constant emits no instruction"
        );
    }
    // All positive except the control, which is what makes "two all-positive
    // functions differ" a claim about weights rather than about a zero.
    for function in [
        CostFunction::InstructionCount,
        CostFunction::WeightedOperators,
        CostFunction::PreferAdd,
        CostFunction::PreferShift,
    ] {
        for operator in [
            ScalarOperator::Add,
            ScalarOperator::Sub,
            ScalarOperator::Mul,
            ScalarOperator::And,
            ScalarOperator::Or,
            ScalarOperator::Xor,
            ScalarOperator::Shl,
        ] {
            assert!(
                function.operator_cost(operator) > 0,
                "{function:?} charges nothing for {operator:?}"
            );
        }
    }
}

/// The two all-positive shipped functions now differ on a program containing a
/// doubling, which they could not before the emitting rules.
#[test]
fn the_two_all_positive_functions_differ_on_a_doubling() {
    let module = multiplied();
    let counted = extracted(&module, "instruction-count");
    let weighted = extracted(&module, "weighted-operators");
    println!("instruction-count:\n{counted}");
    println!("weighted-operators:\n{weighted}");
    assert_eq!(
        counted,
        render_module(&module),
        "a multiply and an addition both cost one, so the region ties and is declined"
    );
    assert_ne!(
        counted, weighted,
        "a weighted multiply makes the doubling a strict improvement"
    );
    assert!(
        weighted.contains("add"),
        "the weighted policy extracts the addition:\n{weighted}"
    );
}

/// The two weightings that separate `add` from `shl` extract different
/// spellings of the same doubling.
#[test]
fn the_two_weightings_extract_opposite_spellings() {
    let added = added();
    let shifted = shifted();

    let add_under_shift = extracted(&added, "prefer-shift");
    let add_under_add = extracted(&added, "prefer-add");
    println!("add under prefer-shift:\n{add_under_shift}");
    assert!(
        add_under_shift.contains("shl"),
        "prefer-shift rewrites a self-addition to a shift:\n{add_under_shift}"
    );
    assert_eq!(
        add_under_add,
        render_module(&added),
        "prefer-add keeps the addition it already has"
    );

    let shl_under_add = extracted(&shifted, "prefer-add");
    let shl_under_shift = extracted(&shifted, "prefer-shift");
    println!("shl under prefer-add:\n{shl_under_add}");
    assert!(
        shl_under_add.contains("add"),
        "prefer-add rewrites a one-bit shift to a self-addition:\n{shl_under_add}"
    );
    assert_eq!(
        shl_under_shift,
        render_module(&shifted),
        "prefer-shift keeps the shift it already has"
    );
}
