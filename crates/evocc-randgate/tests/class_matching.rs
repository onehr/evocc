//! Matching a rule over classes rather than over the names a block gave.
//!
//! Section 7.4 makes equality a congruence, so two instructions with the same
//! operator whose operands are already equal compute one value. A matcher that
//! compares operands by name cannot see that: `icmp.eq %p, %q` on two such
//! conjunctions is `icmp.eq %x, %x` and the reviewed registry answers it, but
//! only once `%p` and `%q` are known to be one class.
//!
//! Both shapes are here. In the first the two conjunctions are congruent as
//! written; in the second they become congruent only after a rewrite has put
//! their operands in one class, which is the case no syntactic matcher reaches.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_proof::{render_evp1, EvpEvent, EvpProofRef, EvpRegion};
use evocc_randgate::{generate_module, GeneratorConfig, ProofChecker};
use evocc_types::{EvoIRType, Region};

fn checker() -> ProofChecker {
    ProofChecker::locate().expect("build the workspace so evo-proofcheck is present")
}

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(8),
    }
}

fn flag(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(1),
    }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

fn probe(insts: Vec<Inst>, root: Value) -> Module {
    let memory = Value {
        id: 2,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 3,
        ty: EvoIRType::World,
    };
    let function = Function {
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(8),
            },
            FunctionParam {
                name: "y".to_owned(),
                ty: EvoIRType::Bits(8),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "arg3".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            root.ty.clone(),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![
                argument(&bits(0)),
                argument(&bits(1)),
                argument(&memory),
                argument(&world),
            ],
            insts,
            terminator: Inst::Ret(vec![root, memory, world]),
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
    validate_module(&module).expect("the probe is valid EvoIR");
    module
}

/// Every rule the optimization of `module` instantiates, once the checker has
/// accepted the certificate that claims them.
fn rules(module: &Module) -> Vec<String> {
    let outcome = optimize_scalar_module_with_evp1(module, ScalarEqBudget::new(20_000, 256));
    let before = render_module(module);
    let after = render_module(&outcome.module);
    let proof = render_evp1(&outcome.certificate);
    checker()
        .check(&before, &after, &proof)
        .unwrap_or_else(|error| panic!("{error}\n{proof}"));
    outcome
        .certificate
        .regions
        .iter()
        .flat_map(|region| &region.events)
        .filter_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite.rule.clone()),
            _ => None,
        })
        .collect()
}

/// ```text
/// %p = and %x, %y
/// %q = and %x, %y     -- the same node, written twice
/// %r = icmp.ne %p, %q -- false, because %p and %q are one class
/// ```
#[test]
fn a_comparison_of_two_copies_of_one_expression_is_decided() {
    let left = bits(4);
    let right = bits(5);
    let result = flag(6);
    let module = probe(
        vec![
            Inst::And(left.clone(), bits(0), bits(1)),
            Inst::And(right.clone(), bits(0), bits(1)),
            Inst::ICmp(result.clone(), CmpOp::Ne, left, right),
        ],
        result,
    );
    let fired = rules(&module);
    assert!(
        fired.contains(&"p0_011_icmp_ne_self".to_owned()),
        "a comparison of one expression with itself is decided however it is spelled: {fired:?}"
    );
}

/// The same comparison where the two conjunctions are congruent only after a
/// rewrite: `%a` becomes `%x`, and only then is `and %a, %y` the node that
/// `and %x, %y` already is.
///
/// ```text
/// %z = const.bits<8> 0
/// %a = add %x, %z       -- p0_001, so %a is %x
/// %p = and %a, %y
/// %q = and %x, %y
/// %r = icmp.eq %p, %q   -- true, on a class the merge created
/// ```
#[test]
fn a_comparison_across_a_merge_is_decided() {
    let zero = bits(4);
    let shifted = bits(5);
    let left = bits(6);
    let right = bits(7);
    let result = flag(8);
    let module = probe(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Add(shifted.clone(), bits(0), zero),
            Inst::And(left.clone(), shifted, bits(1)),
            Inst::And(right.clone(), bits(0), bits(1)),
            Inst::ICmp(result.clone(), CmpOp::Eq, left, right),
        ],
        result,
    );
    let fired = rules(&module);
    assert!(
        fired.contains(&"p0_001_add_zero_rhs".to_owned()),
        "the merge that creates the class must itself be recorded: {fired:?}"
    );
    assert!(
        fired.contains(&"p0_010_icmp_eq_self".to_owned()),
        "the comparison is decided by the class the merge created: {fired:?}"
    );
}

/// The control: with the two operands genuinely different, nothing in the
/// registry decides the comparison and no rule fires at all.
#[test]
fn a_comparison_of_two_different_expressions_is_not_decided() {
    let left = bits(4);
    let right = bits(5);
    let result = flag(6);
    let module = probe(
        vec![
            Inst::And(left.clone(), bits(0), bits(1)),
            Inst::Or(right.clone(), bits(0), bits(1)),
            Inst::ICmp(result.clone(), CmpOp::Ne, left, right),
        ],
        result,
    );
    let fired = rules(&module);
    assert!(
        fired.is_empty(),
        "nothing in the registry answers a comparison of two different values: {fired:?}"
    );
}

/// How many rewrites in one region instantiate a rule with a repeated variable
/// on a node whose two children are different class identifiers.
///
/// This is the measurement of what matching over classes buys, computed from
/// the certificate alone. The identity comparisons are the only rules of the
/// reviewed registry whose left pattern names one variable twice, so they are
/// the only ones a matcher comparing operands by name can miss; it misses them
/// exactly when the two operands are one class under different names. Rules
/// with distinct variables are not counted, because a merge between their
/// operands is incidental to the match rather than what allowed it.
fn matches_through_a_merge(region: &EvpRegion) -> usize {
    let mut parent =
        (0..u32::try_from(region.seeds.len()).expect("seed count fits")).collect::<Vec<u32>>();
    let mut nodes: Vec<Vec<u32>> = Vec::new();
    let mut found = 0;

    fn find(parent: &[u32], mut class: u32) -> u32 {
        while parent[class as usize] != class {
            class = parent[class as usize];
        }
        class
    }

    for event in &region.events {
        match event {
            EvpEvent::Node(node) => {
                let owner = u32::try_from(parent.len()).expect("class count fits");
                parent.push(owner);
                nodes.push(node.children.clone());
            }
            EvpEvent::Congruence(congruence) => {
                let (left, right) = (congruence.lhs_pre, congruence.rhs_pre);
                parent[left.max(right) as usize] = left.min(right);
            }
            EvpEvent::Rewrite(rewrite) => {
                let repeated_variable = rewrite.bindings.len() == 1
                    && rewrite.rule.starts_with("p0_01")
                    && rewrite.rule.ends_with("_self");
                if let (EvpProofRef::Node(id), true) = (rewrite.lhs, repeated_variable) {
                    let children = &nodes[id as usize];
                    let merged = children.iter().enumerate().any(|(index, one)| {
                        children.iter().skip(index + 1).any(|other| {
                            one != other && find(&parent, *one) == find(&parent, *other)
                        })
                    });
                    if merged {
                        found += 1;
                    }
                }
                let (left, right) = (rewrite.lhs_pre, rewrite.rhs_pre);
                parent[left.max(right) as usize] = left.min(right);
            }
            EvpEvent::Epoch(_) => {}
        }
    }
    found
}

/// The campaign's own measure of the same thing, over the seeds the coverage
/// assertions use.
#[test]
fn the_campaign_matches_rules_through_merges() {
    let mut regions = 0_u64;
    let mut through_a_merge = 0_u64;
    let mut rewrites = 0_u64;
    for seed in 0..256_u64 {
        let module = generate_module(seed, GeneratorConfig::default());
        let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
        for region in &outcome.certificate.regions {
            regions += 1;
            let found = matches_through_a_merge(region) as u64;
            rewrites += found;
            if found > 0 {
                through_a_merge += 1;
            }
        }
    }
    println!("regions={regions}");
    println!("regionsMatchedThroughAMerge={through_a_merge}");
    println!("rewritesMatchedThroughAMerge={rewrites}");
    assert!(
        through_a_merge > 0,
        "no rule matched a node whose children one merge had joined"
    );
}
