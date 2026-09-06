//! The producer's extraction policy is a swappable, deterministic parameter.
//!
//! Section 7.5 at revision 1.9.50 makes the cost function a producer policy
//! the checker never verifies. These tests hold the interface to what that
//! sentence permits: a policy names itself, the default policy reproduces the
//! producer byte for byte, each policy is deterministic, and a cost of zero
//! for an operator declines every region rooted at it rather than claiming an
//! improvement it cannot show.

use evocc_eqopt::{
    optimize_scalar_module_with_evp1, optimize_scalar_module_with_policy, policy_by_name,
    shipped_policies, CertifiedScalarEqOutcome, CostFunction, CostModel, ExtractionPolicy,
    RegistryCeiling, ScalarEqBudget, ScalarOperator, SchedulePolicy,
};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_proof::{render_evp1, EvpEvent};
use evocc_types::{EvoIRType, Region};

const WIDTH: u16 = 32;

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(WIDTH),
    }
}

/// `@main(%x, %mem, %world)` holding one region of each shape the shipped
/// policies could disagree about: an `x + 0` rooted at an add, an `x * 1`
/// rooted at a multiply, and an `and` against an all-ones mask, whose rule
/// belongs to the second registry rather than the first.
fn sample_module() -> Module {
    let x = bits(0);
    let memory = Value {
        id: 1,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 2,
        ty: EvoIRType::World,
    };
    let insts = vec![
        Inst::Const(bits(3), 0),
        Inst::Const(bits(4), 1),
        Inst::Const(bits(5), u64::from(u32::MAX)),
        Inst::Add(bits(6), x.clone(), bits(3)),
        Inst::Mul(bits(7), x.clone(), bits(4)),
        Inst::And(bits(8), x.clone(), bits(5)),
        Inst::Add(bits(9), bits(6), bits(7)),
        Inst::Add(bits(10), bits(9), bits(8)),
    ];
    let argument = |value: &Value| BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    };
    let function = Function {
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(WIDTH),
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
            EvoIRType::Bits(WIDTH),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&x), argument(&memory), argument(&world)],
            insts,
            terminator: Inst::Ret(vec![bits(10), memory, world]),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the sample module is well formed");
    module
}

fn default_policy() -> ExtractionPolicy {
    policy_by_name("instruction-count").expect("the default policy ships")
}

/// Regions of `outcome` whose root instruction in the original block matches
/// `predicate`.
fn regions_rooted_at(
    module: &Module,
    outcome: &CertifiedScalarEqOutcome,
    predicate: fn(&Inst) -> bool,
) -> usize {
    outcome
        .certificate
        .regions
        .iter()
        .filter(|region| {
            module.functions[0].blocks[0]
                .insts
                .get(region.id.instruction as usize)
                .is_some_and(predicate)
        })
        .count()
}

#[test]
fn every_shipped_policy_answers_to_its_own_name() {
    let policies = shipped_policies();
    assert!(
        policies.len() >= 2,
        "the interface is proved by more than one policy"
    );
    for policy in &policies {
        let looked_up = policy_by_name(policy.name()).expect("a shipped policy is findable");
        assert_eq!(&looked_up, policy);
    }
    assert!(policy_by_name("no-such-policy").is_none());
}

#[test]
fn the_default_policy_is_section_7_5_instruction_count() {
    let policy = default_policy();
    assert_eq!(policy.cost(), CostFunction::InstructionCount);
    assert_eq!(policy.registry(), RegistryCeiling::V4);
    assert_eq!(ExtractionPolicy::default(), policy);

    let cost = CostFunction::InstructionCount;
    assert_eq!(cost.operator_cost(ScalarOperator::Const), 0);
    for operator in [
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
        ScalarOperator::ICmp(CmpOp::Eq),
    ] {
        assert_eq!(cost.operator_cost(operator), 1, "{operator:?}");
    }
}

#[test]
fn the_weighted_policy_charges_three_for_a_multiply() {
    let cost = CostFunction::WeightedOperators;
    assert_eq!(cost.operator_cost(ScalarOperator::Mul), 3);
    assert_eq!(cost.operator_cost(ScalarOperator::Const), 0);
    for operator in [
        ScalarOperator::Shl,
        ScalarOperator::LShr,
        ScalarOperator::AShr,
        ScalarOperator::ZExt,
        ScalarOperator::SExt,
        ScalarOperator::Trunc,
        ScalarOperator::Add,
    ] {
        assert_eq!(cost.operator_cost(operator), 1, "{operator:?}");
    }
}

#[test]
fn the_default_policy_reproduces_the_producer_byte_for_byte() {
    let module = sample_module();
    let budget = ScalarEqBudget::default();
    let schedule = SchedulePolicy {
        max_nodes: budget.max_nodes,
        max_epochs: budget.max_epochs,
        max_transactions: 1,
    };
    let baseline = optimize_scalar_module_with_evp1(&module, budget);
    let under_policy =
        optimize_scalar_module_with_policy(&module, &default_policy().with_schedule(schedule));
    assert_eq!(
        render_module(&baseline.module),
        render_module(&under_policy.module)
    );
    assert_eq!(
        render_evp1(&baseline.certificate),
        render_evp1(&under_policy.certificate)
    );
}

#[test]
fn each_policy_is_deterministic_over_one_module() {
    let module = sample_module();
    for policy in shipped_policies() {
        let first = optimize_scalar_module_with_policy(&module, &policy);
        let second = optimize_scalar_module_with_policy(&module, &policy);
        assert_eq!(
            render_module(&first.module),
            render_module(&second.module),
            "{}",
            policy.name()
        );
        assert_eq!(
            render_evp1(&first.certificate),
            render_evp1(&second.certificate),
            "{}",
            policy.name()
        );
    }
}

#[test]
fn a_zero_cost_operator_declines_every_region_rooted_at_it() {
    let module = sample_module();
    let control = policy_by_name("zero-multiply").expect("the control policy ships");
    assert_eq!(control.cost().operator_cost(ScalarOperator::Mul), 0);

    let under_default = optimize_scalar_module_with_policy(&module, &default_policy());
    let under_control = optimize_scalar_module_with_policy(&module, &control);

    let is_multiply = |inst: &Inst| matches!(inst, Inst::Mul(..));
    assert!(
        regions_rooted_at(&module, &under_default, is_multiply) > 0,
        "the sample module has a multiply the default policy rewrites"
    );
    assert_eq!(
        regions_rooted_at(&module, &under_control, is_multiply),
        0,
        "a multiply costing nothing is never a strict improvement"
    );
    assert_ne!(
        render_module(&under_default.module),
        render_module(&under_control.module),
        "a differing cost function must be able to change the output"
    );
}

#[test]
fn a_registry_ceiling_declines_rules_it_does_not_name() {
    let module = sample_module();
    let wide = default_policy();
    let narrow = ExtractionPolicy::new(
        "instruction-count-v1",
        RegistryCeiling::V1,
        CostFunction::InstructionCount,
        SchedulePolicy::default(),
    );
    assert_eq!(RegistryCeiling::V1.name(), "p0.scalar.v1");
    assert_eq!(RegistryCeiling::V2.name(), "p0.scalar.v2");
    assert_eq!(RegistryCeiling::V3.name(), "p0.scalar.v3");

    let under_wide = optimize_scalar_module_with_policy(&module, &wide);
    let under_narrow = optimize_scalar_module_with_policy(&module, &narrow);
    assert_eq!(under_narrow.certificate.registry_name, "p0.scalar.v1");
    for region in &under_narrow.certificate.regions {
        for event in &region.events {
            if let EvpEvent::Rewrite(rewrite) = event {
                assert!(
                    rewrite.rule.starts_with("p0_"),
                    "a first-registry certificate fired {}",
                    rewrite.rule
                );
            }
        }
    }
    assert!(
        under_narrow.certificate.regions.len() < under_wide.certificate.regions.len(),
        "the sample module has a region only the wider registry admits"
    );
}

#[test]
fn the_policy_carries_the_schedule_bounds() {
    let policy = policy_by_name("weighted-operators").expect("the weighted policy ships");
    let schedule = policy.schedule();
    assert_eq!(schedule.budget().max_nodes, schedule.max_nodes);
    assert_eq!(schedule.budget().max_epochs, schedule.max_epochs);
    assert!(schedule.max_transactions >= 1);
}
