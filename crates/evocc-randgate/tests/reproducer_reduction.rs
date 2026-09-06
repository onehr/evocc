//! Reductions that reach the shapes the fact-driven rules and congruence need,
//! and what they do to the two reproducers those windows reduced by hand.
//!
//! Deleting an instruction cannot shorten a chain every step of which is used
//! by the next, and neither can collapsing a branch or merging a block. Two
//! reductions reach these: replacing an instruction with a constant the module
//! already names, which is what turns a covering mask into the mask itself, and
//! replacing one of two structurally identical instructions by the other, which
//! is the congruence of Section 7.4 applied to the source rather than to the
//! e-graph.
//!
//! The two hand-reduced reproducers are then handed to the shrinker under the
//! property each was built to hold, and how far it gets is measured rather than
//! predicted.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam, Inst, Module,
    Value,
};
use evocc_proof::{EvpEvent, EvpNode, EvpProofRef, EvpRegion};
use evocc_randgate::shrink_module;
use evocc_types::{EvoIRType, Region};

const ROUNDS: u32 = 8;

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

/// `@main(%x, %y, %m, %w)` over `bits<8>`, returning one value.
fn probe(insts: Vec<Inst>, root: Value) -> Module {
    let memory = value(2, EvoIRType::Mem(Region::All));
    let world = value(3, EvoIRType::World);
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
                argument(&value(0, EvoIRType::Bits(8))),
                argument(&value(1, EvoIRType::Bits(8))),
                argument(&memory),
                argument(&world),
            ],
            insts,
            terminator: Inst::Ret(vec![root, memory, world]),
        }],
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

fn instructions(module: &Module) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .map(|block| block.insts.len())
        .sum()
}

fn holds(module: &Module, mut predicate: impl FnMut(&Inst) -> bool) -> bool {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .any(&mut predicate)
}

/// ```text
/// %lo = const.bits<8> 15
/// %m  = or %y, %lo     -- the covering mask
/// %r  = and %x, %m
/// ```
///
/// Nothing here can be deleted: every instruction is used by the next. The mask
/// is what the shrinker has to be able to collapse.
#[test]
fn a_covering_mask_collapses_to_a_constant() {
    let low = value(4, EvoIRType::Bits(8));
    let covering = value(5, EvoIRType::Bits(8));
    let masked = value(6, EvoIRType::Bits(8));
    let before = probe(
        vec![
            Inst::Const(low.clone(), 0x0f),
            Inst::Or(covering.clone(), value(1, EvoIRType::Bits(8)), low),
            Inst::And(masked.clone(), value(0, EvoIRType::Bits(8)), covering),
        ],
        masked,
    );
    let mut still = |module: &Module| holds(module, |inst| matches!(inst, Inst::And(..)));
    let after = shrink_module(&before, &mut still, ROUNDS);
    println!(
        "coveringMask before={} after={}",
        instructions(&before),
        instructions(&after)
    );
    assert!(
        !holds(&after, |inst| matches!(inst, Inst::Or(..))),
        "the covering mask must collapse to a constant"
    );
    assert!(
        instructions(&after) < instructions(&before),
        "collapsing it must shorten the module"
    );
}

/// ```text
/// %p = and %x, %y
/// %q = and %x, %y     -- the same node, written twice
/// %r = icmp.ne %p, %q
/// ```
///
/// Deleting either conjunction leaves the comparison with a dangling operand,
/// so the validator rejects it and the module does not shrink. Replacing one by
/// the other is the reduction that reaches this.
#[test]
fn a_congruent_pair_becomes_one_node() {
    let left = value(4, EvoIRType::Bits(8));
    let right = value(5, EvoIRType::Bits(8));
    let compared = value(6, EvoIRType::Bits(1));
    let before = probe(
        vec![
            Inst::And(
                left.clone(),
                value(0, EvoIRType::Bits(8)),
                value(1, EvoIRType::Bits(8)),
            ),
            Inst::And(
                right.clone(),
                value(0, EvoIRType::Bits(8)),
                value(1, EvoIRType::Bits(8)),
            ),
            Inst::ICmp(compared.clone(), CmpOp::Ne, left, right),
        ],
        compared,
    );
    let mut still = |module: &Module| holds(module, |inst| matches!(inst, Inst::ICmp(..)));
    let after = shrink_module(&before, &mut still, ROUNDS);
    println!(
        "congruentPair before={} after={}",
        instructions(&before),
        instructions(&after)
    );
    assert!(
        instructions(&after) < instructions(&before),
        "one of two identical conjunctions must go"
    );
}

/// Whether some region resolves the nested rule's variable in a class that
/// owns more than one subtraction.
///
/// This is the property the reproducer exists for, and stating it exactly is
/// what makes handing the module back to the shrinker meaningful. A weaker
/// property -- that the nested rule fires at all -- is held by modules with one
/// subtraction in the class, where the variable can only resolve one way and
/// nothing is being pinned. The partition is replayed from the certificate's
/// own events, so the test does not need the producer to tell it anything.
fn resolves_among_several_subtractions(region: &EvpRegion) -> bool {
    let mut parent =
        (0..u32::try_from(region.seeds.len()).expect("seed count fits")).collect::<Vec<u32>>();
    let mut nodes: Vec<EvpNode> = Vec::new();

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
                nodes.push(node.clone());
            }
            EvpEvent::Congruence(congruence) => {
                let (left, right) = (congruence.lhs_pre, congruence.rhs_pre);
                parent[left.max(right) as usize] = left.min(right);
            }
            EvpEvent::Rewrite(rewrite) => {
                if rewrite.rule == "p0_009_double_neg" {
                    if let EvpProofRef::Node(id) = rewrite.lhs {
                        let inner = find(&parent, nodes[id as usize].children[1]);
                        let subtractions = nodes
                            .iter()
                            .filter(|node| {
                                node.operation == "sub" && find(&parent, node.owner) == inner
                            })
                            .count();
                        if subtractions > 1 {
                            return true;
                        }
                    }
                }
                let (left, right) = (rewrite.lhs_pre, rewrite.rhs_pre);
                parent[left.max(right) as usize] = left.min(right);
            }
            EvpEvent::Epoch(_) => {}
        }
    }
    false
}

/// The nested-pattern reproducer, handed back to the shrinker under the
/// property above. Anything shorter that still holds it is a better reproducer
/// than the one written by hand.
#[test]
fn the_nested_pattern_reproducer_is_re_reduced() {
    let zero = value(4, EvoIRType::Bits(8));
    let three = value(5, EvoIRType::Bits(8));
    let two = value(6, EvoIRType::Bits(8));
    let five = value(7, EvoIRType::Bits(8));
    let decoy = value(8, EvoIRType::Bits(8));
    let negated = value(9, EvoIRType::Bits(8));
    let restored = value(10, EvoIRType::Bits(8));
    let total = value(11, EvoIRType::Bits(8));
    let before = probe(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Const(three.clone(), 3),
            Inst::Const(two.clone(), 2),
            Inst::Const(five.clone(), 5),
            Inst::Sub(decoy.clone(), two, five),
            Inst::Sub(negated.clone(), zero.clone(), three),
            Inst::Sub(restored.clone(), zero, negated),
            Inst::Add(total.clone(), restored, decoy),
        ],
        total,
    );
    let mut still = |module: &Module| {
        let outcome = optimize_scalar_module_with_evp1(module, ScalarEqBudget::new(20_000, 256));
        outcome
            .certificate
            .regions
            .iter()
            .any(resolves_among_several_subtractions)
    };
    assert!(still(&before), "the reproducer must hold its own property");
    let after = shrink_module(&before, &mut still, ROUNDS);
    println!(
        "nestedPattern before={} after={}",
        instructions(&before),
        instructions(&after)
    );
    assert!(
        still(&after),
        "the shrinker may only return a module that still holds the property"
    );
    assert!(instructions(&after) <= instructions(&before));
}

/// The congruence reproducer, handed back to the shrinker.
///
/// Its property is that a congruence a merge enabled is recorded, which is what
/// separates it from the pair that is congruent as written.
#[test]
fn the_congruence_reproducer_is_re_reduced() {
    let zero = value(4, EvoIRType::Bits(8));
    let shifted = value(5, EvoIRType::Bits(8));
    let left = value(6, EvoIRType::Bits(8));
    let right = value(7, EvoIRType::Bits(8));
    let joined = value(8, EvoIRType::Bits(8));
    let result = value(9, EvoIRType::Bits(8));
    let before = probe(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Add(shifted.clone(), value(0, EvoIRType::Bits(8)), zero.clone()),
            Inst::And(left.clone(), shifted, value(1, EvoIRType::Bits(8))),
            Inst::And(
                right.clone(),
                value(0, EvoIRType::Bits(8)),
                value(1, EvoIRType::Bits(8)),
            ),
            Inst::And(joined.clone(), left, right),
            Inst::Mul(result.clone(), joined, zero),
        ],
        result,
    );
    let mut still = |module: &Module| {
        let outcome = optimize_scalar_module_with_evp1(module, ScalarEqBudget::new(20_000, 256));
        outcome.certificate.regions.iter().any(|region| {
            let mut seen_epoch = false;
            for event in &region.events {
                match event {
                    EvpEvent::Epoch(_) => seen_epoch = true,
                    // A congruence after a marker is one a merge enabled.
                    EvpEvent::Congruence(_) if seen_epoch => return true,
                    _ => {}
                }
            }
            false
        })
    };
    assert!(still(&before), "the reproducer must hold its own property");
    let after = shrink_module(&before, &mut still, ROUNDS);
    println!(
        "congruenceReproducer before={} after={}",
        instructions(&before),
        instructions(&after)
    );
    assert!(
        still(&after),
        "the shrinker may only return a module that still holds the property"
    );
    assert!(instructions(&after) <= instructions(&before));
}
