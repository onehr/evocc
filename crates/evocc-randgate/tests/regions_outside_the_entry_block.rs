//! Which blocks a region may sit in, and where a seed may come from.
//!
//! Section 8.5 decides eligibility with two monotone fixed points -- taint and
//! exclusion -- and Section 8.7 records each seed's origin as `input` for a
//! block argument or `definition` for a value defined earlier in the function.
//! Neither section mentions the entry block. A producer that admits a region
//! only when every seed is an argument of the entry block is therefore
//! narrower than the profile it claims, and every region of every later block
//! is lost, including the congruence-only regions that revision 1.9.44
//! admitted.
//!
//! These tests say what the profile admits, count what the corpus contains,
//! and pin the reason a seed of origin `definition` never survives the
//! exclusion fixed point.

use std::collections::{BTreeMap, HashMap};

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_proof::{render_evp1, EvpEvent, EvpSeedOrigin};
use evocc_randgate::{generate_module, GeneratorConfig, ProofChecker};
use evocc_types::{EvoIRType, Region};

/// The corpus every measurement in this file is taken over.
const CORPUS: u64 = 256;

fn budget() -> ScalarEqBudget {
    ScalarEqBudget::new(20_000, 256)
}

fn checker() -> ProofChecker {
    ProofChecker::locate().expect("build the workspace so evo-proofcheck is present")
}

fn value(id: u32, width: u16) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(width),
    }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

fn memory(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Mem(Region::All),
    }
}

fn world(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::World,
    }
}

/// A two-block function whose entry block computes nothing and forwards its
/// arguments, so every instruction under test sits in the second block and
/// every seed it names is an argument of that block.
///
/// ```text
/// ^0(%0, %1, %2, %3):        -- the interface
///   br ^1(%0, %1, %2, %3)
/// ^1(%4, %5, %6, %7):        -- where the region is
///   <insts>
///   ret <root>, %6, %7
/// ```
fn later_block(insts: Vec<Inst>, root: Value) -> Module {
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
        blocks: vec![
            Block {
                id: 0,
                args: vec![
                    argument(&value(0, 8)),
                    argument(&value(1, 8)),
                    argument(&memory(2)),
                    argument(&world(3)),
                ],
                insts: Vec::new(),
                terminator: Inst::Br(1, vec![value(0, 8), value(1, 8), memory(2), world(3)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&value(4, 8)),
                    argument(&value(5, 8)),
                    argument(&memory(6)),
                    argument(&world(7)),
                ],
                insts,
                terminator: Inst::Ret(vec![root, memory(6), world(7)]),
            },
        ],
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the probe is valid EvoIR");
    module
}

struct Outcome {
    regions: usize,
    rules: Vec<String>,
    congruences: usize,
    proof: String,
    after: String,
}

/// Optimize, then hand the whole transaction to `evo-proofcheck`, which
/// recomputes eligibility, the witness closure and both manifests for itself.
fn optimize(module: &Module) -> Outcome {
    let outcome = optimize_scalar_module_with_evp1(module, budget());
    let before = render_module(module);
    let after = render_module(&outcome.module);
    let proof = render_evp1(&outcome.certificate);
    checker()
        .check(&before, &after, &proof)
        .unwrap_or_else(|error| panic!("{error}\n{proof}"));
    let events = outcome
        .certificate
        .regions
        .iter()
        .flat_map(|region| &region.events);
    Outcome {
        regions: outcome.certificate.regions.len(),
        rules: events
            .clone()
            .filter_map(|event| match event {
                EvpEvent::Rewrite(rewrite) => Some(rewrite.rule.clone()),
                _ => None,
            })
            .collect(),
        congruences: events
            .filter(|event| matches!(event, EvpEvent::Congruence(_)))
            .count(),
        proof,
        after,
    }
}

const fn origin_name(origin: EvpSeedOrigin) -> &'static str {
    match origin {
        EvpSeedOrigin::Input => "input",
        EvpSeedOrigin::Definition => "definition",
    }
}

/// Section 8.5 names no block. A region whose seeds are the arguments of the
/// block it sits in is eligible wherever that block is.
///
/// ```text
/// ^1(%4, %5, ...):
///   %8 = const.bits<8> 0
///   %9 = add %4, %8      -- p0_001_add_zero_rhs
/// ```
#[test]
fn a_region_in_a_later_block_is_admitted_and_replays() {
    let zero = value(8, 8);
    let sum = value(9, 8);
    let module = later_block(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Add(sum.clone(), value(4, 8), zero),
        ],
        sum,
    );
    let outcome = optimize(&module);
    assert!(
        outcome.rules.contains(&"p0_001_add_zero_rhs".to_owned()),
        "the identity must fire outside the entry block: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        !outcome.after.contains("add.bits<8>"),
        "the addition must leave the output:\n{}",
        outcome.after
    );
}

/// The congruence-only region of revision 1.9.44, in a block that is not the
/// entry block.
///
/// ```text
/// ^1(%4, %5, ...):
///   %8 = and %4, %5
///   %9 = and %4, %5     -- the root, and its twin is the whole proof
/// ```
#[test]
fn a_duplicate_in_a_later_block_is_a_congruence_only_region() {
    let first = value(8, 8);
    let second = value(9, 8);
    let module = later_block(
        vec![
            Inst::And(first, value(4, 8), value(5, 8)),
            Inst::And(second.clone(), value(4, 8), value(5, 8)),
        ],
        second,
    );
    let outcome = optimize(&module);
    assert!(
        outcome.rules.is_empty(),
        "no rule fires: the twin is the whole proof: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        outcome.congruences >= 1,
        "the two conjunctions are congruent as inserted:\n{}",
        outcome.proof
    );
    assert_eq!(
        outcome.after.matches("and.bits<8>").count(),
        1,
        "the duplicate leaves the output:\n{}",
        outcome.after
    );
}

/// A region deeper than one edge, so admission cannot be a special case for
/// the block the entry block branches to.
#[test]
fn a_region_two_edges_from_the_entry_block_is_admitted() {
    let zero = value(12, 8);
    let sum = value(13, 8);
    let function = Function {
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
        blocks: vec![
            Block {
                id: 0,
                args: vec![
                    argument(&value(0, 8)),
                    argument(&memory(1)),
                    argument(&world(2)),
                ],
                insts: Vec::new(),
                terminator: Inst::Br(1, vec![value(0, 8), memory(1), world(2)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&value(3, 8)),
                    argument(&memory(4)),
                    argument(&world(5)),
                ],
                insts: Vec::new(),
                terminator: Inst::Br(2, vec![value(3, 8), memory(4), world(5)]),
            },
            Block {
                id: 2,
                args: vec![
                    argument(&value(6, 8)),
                    argument(&memory(7)),
                    argument(&world(8)),
                ],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::Add(sum.clone(), value(6, 8), zero),
                ],
                terminator: Inst::Ret(vec![sum, memory(7), world(8)]),
            },
        ],
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the probe is valid EvoIR");
    let outcome = optimize(&module);
    assert!(
        outcome.rules.contains(&"p0_001_add_zero_rhs".to_owned()),
        "two edges from the entry block is still a region: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
}

/// Poison is not laundered by an edge (8.5), so a block argument fed a tainted
/// value on any incoming edge makes every region built on it ineligible --
/// which is the clause that stops "any block" from meaning "any value".
///
/// ```text
/// ^0(...):
///   %3 = uninitialized.bits<8>
///   br ^1(%3, ...)
/// ^1(%6, ...):
///   %9 = const.bits<8> 0
///   %10 = add %6, %9      -- built on poison: not a region
/// ```
#[test]
fn a_tainted_edge_argument_keeps_a_later_block_ineligible() {
    let poison = value(3, 8);
    let zero = value(9, 8);
    let sum = value(10, 8);
    let function = Function {
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "arg0".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            EvoIRType::Bits(8),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![argument(&memory(1)), argument(&world(2))],
                insts: vec![Inst::Uninitialized(poison.clone())],
                terminator: Inst::Br(1, vec![poison, memory(1), world(2)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&value(6, 8)),
                    argument(&memory(7)),
                    argument(&world(8)),
                ],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::Add(sum.clone(), value(6, 8), zero),
                ],
                terminator: Inst::Ret(vec![sum, memory(7), world(8)]),
            },
        ],
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the probe is valid EvoIR");
    let outcome = optimize(&module);
    assert_eq!(
        outcome.regions, 0,
        "an expression built on poison is not a region:\n{}",
        outcome.proof
    );
}

/// Why a seed of origin `definition` never reaches a certificate.
///
/// Section 2.4 lets a value be used only where it is defined by an earlier
/// instruction of the same block or bound as an argument of that block, so a
/// closure leaf is either a block argument -- origin `input` -- or a value the
/// same block defines with an instruction the closure would not follow, which
/// is exactly an instruction absent from Section 8.5's allowlist. Such a value
/// starts excluded in the exclusion fixed point, so the region carrying it is
/// ineligible. Origin `definition` is spelled by Section 8.7 and reachable by
/// nothing the two sections together admit.
///
/// ```text
/// ^1(%4, %5, ...):
///   %8 = srem %4, %5     -- not allowlisted, so %8 starts excluded
///   %9 = const.bits<8> 0
///   %10 = add %8, %9     -- would be `add(?x, 0)` over a definition seed
/// ```
#[test]
fn a_scalar_from_an_unlisted_instruction_is_an_excluded_definition_seed() {
    let quotient = value(8, 8);
    let zero = value(9, 8);
    let sum = value(10, 8);
    let module = later_block(
        vec![
            Inst::SRem(quotient.clone(), value(4, 8), value(5, 8)),
            Inst::Const(zero.clone(), 0),
            Inst::Add(sum.clone(), quotient, zero),
        ],
        sum,
    );
    let outcome = optimize(&module);
    assert_eq!(
        outcome.regions, 0,
        "an unlisted instruction's result is an excluded seed, not an opaque one:\n{}",
        outcome.proof
    );
}

/// Every seed of every region the corpus produces, by origin.
///
/// The certificate records the origin and the checker recomputes it, so this
/// is the campaign's own answer to whether Section 8.7's second origin is
/// reachable at all.
#[test]
fn the_corpus_reports_its_seeds_by_origin() {
    let mut origins: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut regions_with_definition_seed = 0_usize;
    let mut regions_with_only_input_seeds = 0_usize;
    let mut seedless_regions = 0_usize;
    for seed in 0..CORPUS {
        let module = generate_module(seed, GeneratorConfig::default());
        let outcome = optimize_scalar_module_with_evp1(&module, budget());
        for region in &outcome.certificate.regions {
            for entry in &region.seeds {
                *origins.entry(origin_name(entry.origin)).or_insert(0) += 1;
            }
            if region.seeds.is_empty() {
                seedless_regions += 1;
            } else if region
                .seeds
                .iter()
                .any(|entry| entry.origin == EvpSeedOrigin::Definition)
            {
                regions_with_definition_seed += 1;
            } else {
                regions_with_only_input_seeds += 1;
            }
        }
    }
    println!("seedsByOrigin={origins:?}");
    println!("regionsWithOnlyInputSeeds={regions_with_only_input_seeds}");
    println!("regionsWithADefinitionSeed={regions_with_definition_seed}");
    println!("seedlessRegions={seedless_regions}");
    assert!(
        regions_with_only_input_seeds > 0,
        "the corpus must still produce regions"
    );
}

/// What a duplicate is worth: how many the corpus writes, how many become a
/// region, and how each of those is proved.
///
/// A duplicate whose instruction a rule answers is recorded as that rewrite
/// rather than as a congruence, because the rule states the equality itself
/// and is the narrower claim, so both kinds count as a duplicate the
/// transaction removed. A duplicate that becomes no region at all is one the
/// exclusion fixed point refuses, and those are what the count below is
/// against.
#[test]
fn the_corpus_measures_its_duplicates_and_congruence_only_regions() {
    let mut duplicates = 0_usize;
    let mut computed_duplicates = 0_usize;
    let mut by_congruence = 0_usize;
    let mut by_rewrite = 0_usize;
    let mut unregioned = 0_usize;
    let mut congruence_only = 0_usize;
    let mut congruence_only_outside_entry = 0_usize;
    let mut regions = 0_usize;
    for seed in 0..CORPUS {
        let module = generate_module(seed, GeneratorConfig::default());
        let (all, computed) = structural_duplicates(&module);
        duplicates += all;
        computed_duplicates += computed;
        let outcome = optimize_scalar_module_with_evp1(&module, budget());
        regions += outcome.certificate.regions.len();
        let mut kinds: HashMap<(String, String, u64), RegionKind> = HashMap::new();
        for region in &outcome.certificate.regions {
            let kind = region_kind(region);
            if kind == RegionKind::Congruence {
                congruence_only += 1;
                if region.id.block != "entry" {
                    congruence_only_outside_entry += 1;
                }
            }
            kinds.insert(
                (
                    region.id.function.clone(),
                    region.id.block.clone(),
                    region.id.instruction,
                ),
                kind,
            );
        }
        for (function, block, index) in duplicate_sites(&module) {
            match kinds.get(&(function, block, index)) {
                Some(RegionKind::Congruence) => by_congruence += 1,
                Some(RegionKind::Rewrite) => by_rewrite += 1,
                _ => unregioned += 1,
            }
        }
    }
    println!("structuralDuplicates={duplicates}");
    println!("computedDuplicates={computed_duplicates}");
    println!("duplicatesProvedByCongruence={by_congruence}");
    println!("duplicatesProvedByARule={by_rewrite}");
    println!("duplicatesWithNoRegion={unregioned}");
    println!("congruenceOnlyRegions={congruence_only}");
    println!("congruenceOnlyRegionsOutsideTheEntryBlock={congruence_only_outside_entry}");
    println!("regions={regions}");
    assert!(
        congruence_only_outside_entry > 0,
        "a duplicate outside the entry block must be able to be a region"
    );
    assert!(
        unregioned < by_congruence + by_rewrite,
        "{unregioned} of {computed_duplicates} duplicates still leave no region"
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKind {
    Congruence,
    Rewrite,
    Neither,
}

/// A region whose root a congruence alone replaced, against one a rule
/// answered.
fn region_kind(region: &evocc_proof::EvpRegion) -> RegionKind {
    let congruent = region
        .events
        .iter()
        .any(|event| matches!(event, EvpEvent::Congruence(_)));
    let rewritten = region
        .events
        .iter()
        .any(|event| matches!(event, EvpEvent::Rewrite(_)));
    match (congruent, rewritten) {
        (_, true) => RegionKind::Rewrite,
        (true, false) => RegionKind::Congruence,
        (false, false) => RegionKind::Neither,
    }
}

/// Where the corpus writes a duplicate, in the region identity spelling of
/// Section 8.7 so the sites can be looked up among the admitted regions.
fn duplicate_sites(module: &Module) -> Vec<(String, String, u64)> {
    let mut sites = Vec::new();
    for function in &module.functions {
        for (block_index, block) in function.blocks.iter().enumerate() {
            let label = if block_index == 0 {
                "entry".to_owned()
            } else {
                format!("b{}", block.id)
            };
            let mut seen: HashMap<Shape, ()> = HashMap::new();
            for (index, instruction) in block.insts.iter().enumerate() {
                let Some(shape) = shape(instruction) else {
                    continue;
                };
                if shape.0 == "const" {
                    continue;
                }
                if seen.insert(shape, ()).is_some() {
                    sites.push((
                        function.name.clone(),
                        label.clone(),
                        u64::try_from(index).expect("an instruction index fits u64"),
                    ));
                }
            }
        }
    }
    sites
}

/// An instruction of a block that repeats the operation, sort, attributes and
/// operand values of an earlier one, counted the way Section 8.5 identifies a
/// structural duplicate.
///
/// The second count leaves out `const`, whose repetitions the congruence
/// closure of a region joins in any case and which no extraction has to
/// replace, so it is the number of duplicates that a region could remove from
/// the output.
fn structural_duplicates(module: &Module) -> (usize, usize) {
    let mut all = 0;
    let mut computed = 0;
    for function in &module.functions {
        for block in &function.blocks {
            let mut seen: HashMap<Shape, ()> = HashMap::new();
            for instruction in &block.insts {
                let Some(shape) = shape(instruction) else {
                    continue;
                };
                let repeated = shape.0 != "const";
                if seen.insert(shape, ()).is_some() {
                    all += 1;
                    if repeated {
                        computed += 1;
                    }
                }
            }
        }
    }
    (all, computed)
}

type Shape = (&'static str, EvoIRType, u64, Vec<u32>);

fn shape(instruction: &Inst) -> Option<Shape> {
    let (name, payload) = match instruction {
        Inst::Const(_, raw) => ("const", *raw),
        Inst::Add(..) => ("add", 0),
        Inst::Sub(..) => ("sub", 0),
        Inst::Mul(..) => ("mul", 0),
        Inst::And(..) => ("and", 0),
        Inst::Or(..) => ("or", 0),
        Inst::Xor(..) => ("xor", 0),
        Inst::ICmp(_, CmpOp::Eq, ..) => ("icmp.eq", 0),
        Inst::ICmp(_, CmpOp::Ne, ..) => ("icmp.ne", 0),
        Inst::ICmp(_, CmpOp::SLt, ..) => ("icmp.slt", 0),
        Inst::ICmp(_, CmpOp::SLe, ..) => ("icmp.sle", 0),
        Inst::ICmp(_, CmpOp::ULt, ..) => ("icmp.ult", 0),
        Inst::ICmp(_, CmpOp::ULe, ..) => ("icmp.ule", 0),
        Inst::Shl(..) => ("shl", 0),
        Inst::LShr(..) => ("lshr", 0),
        Inst::AShr(..) => ("ashr", 0),
        Inst::ZExt(..) => ("zext", 0),
        Inst::SExt(..) => ("sext", 0),
        Inst::Trunc(..) => ("trunc", 0),
        _ => return None,
    };
    let result = evocc_evoir::inst_results(instruction).first().copied()?;
    if !matches!(result.ty, EvoIRType::Bits(1..=64)) {
        return None;
    }
    Some((
        name,
        result.ty.clone(),
        payload,
        operands(instruction)
            .into_iter()
            .map(|value| value.id)
            .collect(),
    ))
}

fn operands(instruction: &Inst) -> Vec<&Value> {
    match instruction {
        Inst::Add(_, left, right)
        | Inst::Sub(_, left, right)
        | Inst::Mul(_, left, right)
        | Inst::And(_, left, right)
        | Inst::Or(_, left, right)
        | Inst::Xor(_, left, right)
        | Inst::Shl(_, left, right)
        | Inst::LShr(_, left, right)
        | Inst::AShr(_, left, right) => vec![left, right],
        Inst::ICmp(_, _, left, right) => vec![left, right],
        Inst::ZExt(_, source, _) | Inst::SExt(_, source, _) | Inst::Trunc(_, source, _) => {
            vec![source]
        }
        _ => Vec::new(),
    }
}
