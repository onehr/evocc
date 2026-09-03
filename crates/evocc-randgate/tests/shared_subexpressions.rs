//! What a duplicate expression can and cannot become.
//!
//! The sweep knows that two instructions with the same operator and equal
//! operands are one value (Section 7.4), and a later instruction sees one
//! operand where the block wrote two names. The duplicate itself, though, stays
//! in the output, and this file is about why.
//!
//! Section 8.5 builds a region's witness closure by following operands: "the
//! closure contains the root and every instruction reached that way". The
//! earlier twin of a duplicate is not one of its operands, so it is not in the
//! closure, and Section 8.7 lets the certificate neither add it as a witness nor
//! reinterpret it as a seed. Without it in the node stream there is no
//! congruence to record between the two and no node for the extraction to name,
//! so the duplicate's class holds one node of cost one and nothing is cheaper.
//!
//! Everything here is computed from the module, independently of the producer:
//! the duplicate detection and the closure are recomputed from Section 8.5's
//! own words rather than asked for.

use std::collections::{HashMap, HashSet};

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    inst_results, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_randgate::{generate_module, GeneratorConfig};
use evocc_types::{EvoIRType, Region};

/// The operator allowlist of Section 8.5, and the operand order of Section 9.3.
fn allowlisted(instruction: &Inst) -> Option<(&'static str, Vec<u32>)> {
    let (name, operands) = match instruction {
        Inst::Const(_, raw) => {
            // A constant's payload is part of what makes two of them the same
            // node, and it is not an operand, so it rides in the name.
            return Some((
                Box::leak(format!("const/{raw}").into_boxed_str()),
                Vec::new(),
            ));
        }
        Inst::Add(_, left, right) => ("add", vec![left.id, right.id]),
        Inst::Sub(_, left, right) => ("sub", vec![left.id, right.id]),
        Inst::Mul(_, left, right) => ("mul", vec![left.id, right.id]),
        Inst::And(_, left, right) => ("and", vec![left.id, right.id]),
        Inst::Or(_, left, right) => ("or", vec![left.id, right.id]),
        Inst::Xor(_, left, right) => ("xor", vec![left.id, right.id]),
        Inst::ICmp(_, comparison, left, right) => (
            match comparison {
                CmpOp::Eq => "icmp.eq",
                CmpOp::Ne => "icmp.ne",
                CmpOp::SLt => "icmp.slt",
                CmpOp::SLe => "icmp.sle",
                CmpOp::ULt => "icmp.ult",
                CmpOp::ULe => "icmp.ule",
            },
            vec![left.id, right.id],
        ),
        _ => return None,
    };
    Some((name, operands))
}

/// Section 8.5: follow an operand only into an earlier allowlisted instruction
/// of the same block.
fn closure(block: &Block, root: usize) -> HashSet<usize> {
    let definitions = block
        .insts
        .iter()
        .enumerate()
        .flat_map(|(index, instruction)| {
            inst_results(instruction)
                .into_iter()
                .map(move |value| (value.id, index))
        })
        .collect::<HashMap<_, _>>();
    let mut reached = HashSet::new();
    let mut pending = vec![root];
    while let Some(index) = pending.pop() {
        if !reached.insert(index) {
            continue;
        }
        let Some((_, operands)) = allowlisted(&block.insts[index]) else {
            continue;
        };
        for operand in operands {
            if let Some(definition) = definitions.get(&operand) {
                if *definition < index && allowlisted(&block.insts[*definition]).is_some() {
                    pending.push(*definition);
                }
            }
        }
    }
    reached
}

/// Every duplicate in one block, paired with the earlier instruction it
/// duplicates, and whether that earlier one is in the duplicate's closure.
fn duplicates(block: &Block) -> Vec<(usize, usize, bool)> {
    let mut first: HashMap<(&'static str, Vec<u32>), usize> = HashMap::new();
    let mut found = Vec::new();
    for (index, instruction) in block.insts.iter().enumerate() {
        let Some(shape) = allowlisted(instruction) else {
            continue;
        };
        if matches!(instruction, Inst::Const(..)) {
            continue;
        }
        if let Some(twin) = first.get(&shape) {
            let reachable = closure(block, index).contains(twin);
            found.push((index, *twin, reachable));
        } else {
            first.insert(shape, index);
        }
    }
    found
}

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
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the probe is valid EvoIR");
    module
}

/// ```text
/// %p = and %x, %y
/// %q = and %x, %y   -- the duplicate, and the region that cannot exist
/// ```
///
/// Nothing can replace `%q` with `%p`: `%p` is not among `%q`'s operands, so
/// Section 8.5 leaves it out of the closure and Section 8.7 forbids adding it.
#[test]
fn a_duplicates_twin_is_not_in_its_closure() {
    let left = bits(4);
    let right = bits(5);
    let module = probe(
        vec![
            Inst::And(left.clone(), bits(0), bits(1)),
            Inst::And(right.clone(), bits(0), bits(1)),
        ],
        right,
    );
    let block = &module.functions[0].blocks[0];
    let found = duplicates(block);
    assert_eq!(found.len(), 1, "the second conjunction is the duplicate");
    let (duplicate, twin, reachable) = found[0];
    assert_eq!((duplicate, twin), (1, 0));
    assert!(
        !reachable,
        "the twin is not an operand ancestor, so no closure containing the \
         duplicate contains it"
    );
    assert_eq!(
        closure(block, duplicate).len(),
        1,
        "the duplicate's closure is the duplicate alone"
    );

    // And so the duplicate survives: its class holds one node of cost one and
    // the region has nothing cheaper to extract.
    let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
    assert!(
        outcome.certificate.regions.is_empty(),
        "no region can be admitted for a duplicate whose twin it may not name"
    );
    assert_eq!(
        evocc_evoir::render_module(&outcome.module)
            .matches("and.bits<8>")
            .count(),
        2,
        "both conjunctions are still in the output"
    );
}

/// The one shape in which the twin *is* in the closure: an operand ancestor
/// that a merge has made congruent with what it feeds.
///
/// ```text
/// %ones = const.bits<8> 255
/// %p    = and %x, %ones   -- p1_220, so %p is %x
/// %q    = and %p, %ones   -- now the same node as %p, and %p is its operand
/// ```
#[test]
fn a_twin_that_is_an_operand_ancestor_is_in_the_closure() {
    let ones = bits(4);
    let first = bits(5);
    let second = bits(6);
    let module = probe(
        vec![
            Inst::Const(ones.clone(), 0xff),
            Inst::And(first.clone(), bits(0), ones.clone()),
            Inst::And(second.clone(), first, ones),
        ],
        second,
    );
    let block = &module.functions[0].blocks[0];
    assert!(
        closure(block, 2).contains(&1),
        "an operand ancestor is in the closure by Section 8.5's own rule"
    );
    // The duplicate detection does not see these two as one shape, because
    // they are congruent only after a merge; the rule that fires on the second
    // is the covering identity, not a bare congruence.
    assert!(duplicates(block).is_empty());
    let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
    assert_eq!(
        outcome.certificate.regions.len(),
        2,
        "both conjunctions are replaced, each by a rule of its own"
    );
}

/// How many duplicates the campaign's own programs hold, and how many of them
/// a region could name its twin for.
#[test]
fn the_campaign_measures_its_duplicates() {
    let mut total = 0_u64;
    let mut in_closure = 0_u64;
    for seed in 0..256_u64 {
        let module = generate_module(seed, GeneratorConfig::default());
        for function in &module.functions {
            for block in &function.blocks {
                for (_, _, reachable) in duplicates(block) {
                    total += 1;
                    if reachable {
                        in_closure += 1;
                    }
                }
            }
        }
    }
    println!("duplicateInstructions={total}");
    println!("duplicatesWhoseTwinIsInTheirClosure={in_closure}");
    assert!(
        total > 0,
        "the generator must produce duplicates, or this measures nothing"
    );
}
