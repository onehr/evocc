//! What a block argument knows, and what a rule may do with it.
//!
//! Section 8.10 "Facts of seeds", added at revision 1.9.46: a seed of origin
//! `input` carries a fact computed once per function by a forward fixed point,
//! the fact of a block argument being the combination over every incoming edge
//! of the fact of the value passed there. Before the revision every seed was
//! top and no fact crossed a block edge, so a value masked in one block arrived
//! in the next knowing nothing.
//!
//! The tests below say what the fixed point must derive, what it must not, and
//! that a premise naming a seed is checked against a fact the checker
//! recomputes for itself rather than one the producer asserts.

use std::collections::BTreeMap;

use evocc_eqopt::{optimize_scalar_module_with_evp1, seed_fact_agreement, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::{render_evp1, EvpEvent};
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

/// A counted loop whose carried value is masked on the way in and, when
/// `remask` is set, on the way round again.
///
/// ```text
/// ^entry(%0, %1, %2):
///   %3  = const.bits<8> 15
///   %4  = and %0, %3            -- high nibble known zero
///   br ^body(%4, %1, %2)
/// ^body(%5, %6, %7):
///   %8  = const.bits<8> 15
///   %9  = and %5, %8            -- covers(%8, %5): the rule under test
///   %10 = const.bits<8> 1
///   %11 = add %9, %10
///   %12 = and %11, %8           -- present only when `remask`
///   %13 = const.bits<8> 0
///   %14 = icmp.eq %9, %13
///   cond_br %14, ^done(%9, %6, %7), ^body(%12 or %11, %6, %7)
/// ^done(%15, %16, %17):
///   ret %15, %16, %17
/// ```
///
/// With the back edge re-masked, every edge into `^body` passes a value whose
/// high nibble is known zero, so `%5` knows it too and `and(%5, 0x0f)` is `%5`.
/// Without it the back edge passes an addition that knows nothing, and the
/// combination over the two edges must know nothing either.
fn counted_loop(remask: bool) -> Module {
    let carried = bits(5);
    let mask = bits(8);
    let masked = bits(9);
    let one = bits(10);
    let stepped = bits(11);
    let remasked = bits(12);
    let zero = bits(13);
    let done = Value {
        id: 14,
        ty: EvoIRType::Bits(1),
    };
    let entry_mask = bits(3);
    let entry_masked = bits(4);

    let mut body = vec![
        Inst::Const(mask.clone(), 0x0f),
        Inst::And(masked.clone(), carried.clone(), mask.clone()),
        Inst::Const(one.clone(), 1),
        Inst::Add(stepped.clone(), masked.clone(), one),
    ];
    if remask {
        body.push(Inst::And(remasked.clone(), stepped.clone(), mask));
    }
    body.push(Inst::Const(zero.clone(), 0));
    body.push(Inst::ICmp(
        done.clone(),
        evocc_evoir::CmpOp::Eq,
        masked.clone(),
        zero,
    ));
    let carried_on = if remask { remasked } else { stepped };

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
                    argument(&bits(0)),
                    argument(&memory(1)),
                    argument(&world(2)),
                ],
                insts: vec![
                    Inst::Const(entry_mask.clone(), 0x0f),
                    Inst::And(entry_masked.clone(), bits(0), entry_mask),
                ],
                terminator: Inst::Br(1, vec![entry_masked, memory(1), world(2)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&carried),
                    argument(&memory(6)),
                    argument(&world(7)),
                ],
                insts: body,
                terminator: Inst::CondBr(
                    done,
                    2,
                    vec![masked, memory(6), world(7)],
                    1,
                    vec![carried_on, memory(6), world(7)],
                ),
            },
            Block {
                id: 2,
                args: vec![
                    argument(&bits(15)),
                    argument(&memory(16)),
                    argument(&world(17)),
                ],
                insts: Vec::new(),
                terminator: Inst::Ret(vec![bits(15), memory(16), world(17)]),
            },
        ],
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the loop is valid EvoIR");
    module
}

struct Outcome {
    rules: Vec<String>,
    proof: String,
    before: String,
    after: String,
    seed_classes: usize,
}

fn optimize(module: &Module) -> Outcome {
    let outcome = optimize_scalar_module_with_evp1(module, budget());
    let before = render_module(module);
    let after = render_module(&outcome.module);
    let proof = render_evp1(&outcome.certificate);
    checker()
        .check(&before, &after, &proof)
        .unwrap_or_else(|error| panic!("{error}\n{proof}"));
    Outcome {
        rules: outcome
            .certificate
            .regions
            .iter()
            .flat_map(|region| &region.events)
            .filter_map(|event| match event {
                EvpEvent::Rewrite(rewrite) => Some(rewrite.rule.clone()),
                _ => None,
            })
            .collect(),
        seed_classes: premises_asserting_a_seed(&outcome.certificate),
        proof,
        before,
        after,
    }
}

/// How many premises the certificate records against a class a seed owns
/// that assert at least one bit.
///
/// Seed classes are the first `seeds.len()` class identifiers of a region
/// (Section 11.3), so a premise below that bound is a claim about a value the
/// region treats as opaque. A premise over a top fact has an empty mask and
/// claims nothing; only one with a bit in its mask says something the seed
/// fixed point had to derive, so only those are counted.
fn premises_asserting_a_seed(certificate: &evocc_proof::EvpCertificate) -> usize {
    let mut count = 0;
    for region in &certificate.regions {
        let seeds = u32::try_from(region.seeds.len()).expect("a region's seed count fits u32");
        for event in &region.events {
            if let EvpEvent::Rewrite(rewrite) = event {
                count += rewrite
                    .premises
                    .iter()
                    .filter(|premise| premise.class < seeds && premise.mask != 0)
                    .count();
            }
        }
    }
    count
}

/// The mask survives the back edge, and the rule reads it off the argument.
#[test]
fn a_loop_carried_mask_reaches_a_rule_through_the_argument() {
    let module = counted_loop(true);
    let outcome = optimize(&module);
    assert!(
        outcome.rules.contains(&"p1_220_and_cover_rhs".to_owned()),
        "the carried argument's fact must decide the covering identity: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        outcome.seed_classes > 0,
        "the covering identity must state a premise about the seed:\n{}",
        outcome.proof
    );
}

/// The same loop without the re-mask knows nothing about its argument, because
/// one incoming edge knows nothing and every edge counts.
///
/// This is the soundness half. A combination that kept a bit known on only one
/// edge would let the rule fire here, and the value it aliased away could have
/// any high nibble at all.
#[test]
fn an_unmasked_back_edge_leaves_the_argument_unknown() {
    let module = counted_loop(false);
    let outcome = optimize(&module);
    assert!(
        !outcome.rules.contains(&"p1_220_and_cover_rhs".to_owned()),
        "no edge-crossing fact may survive an unmasked back edge: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert_eq!(
        outcome.seed_classes, 0,
        "no premise may name the argument's class:\n{}",
        outcome.proof
    );
}

/// A premise about a seed that claims more than the checker's own fixed point
/// derives is refused with status 3.
///
/// The certificate is the producer's own, edited in one field: the mask of the
/// seed premise is widened to claim the low nibble known as well, which the
/// recomputed fact leaves unknown.
#[test]
fn a_seed_premise_above_the_recomputed_fact_is_refused() {
    let module = counted_loop(true);
    let outcome = optimize(&module);
    let line = outcome
        .proof
        .lines()
        .find(|line| line.contains(" premises "))
        .unwrap_or_else(|| panic!("no premised rewrite to edit:\n{}", outcome.proof))
        .to_owned();
    let (head, premises) = line
        .split_once(" premises ")
        .expect("the line was selected for its premises");
    let overstated = premises
        .split(',')
        .map(|premise| {
            let mut fields = premise.split(':').collect::<Vec<_>>();
            let last = fields.len() - 1;
            // Claim every bit known, keeping the values already claimed.
            fields[last - 1] = "ff";
            fields.join(":")
        })
        .collect::<Vec<_>>()
        .join(",");
    let edited = outcome
        .proof
        .replacen(&line, &format!("{head} premises {overstated}"), 1);
    assert_ne!(edited, outcome.proof, "the edit changed nothing");
    let error = checker()
        .check(&outcome.before, &outcome.after, &edited)
        .expect_err(&format!(
            "an overstated seed premise must be refused\n{edited}"
        ));
    assert!(
        error.contains("refused"),
        "expected a refused claim (status 3), got {error}"
    );
}

/// What facts crossing an edge are worth.
#[test]
fn the_corpus_measures_premises_that_assert_a_seed() {
    let mut regions = 0_usize;
    let mut premised = 0_usize;
    let mut naming_a_seed = 0_usize;
    for seed in 0..CORPUS {
        let module = generate_module(seed, GeneratorConfig::default());
        let outcome = optimize_scalar_module_with_evp1(&module, budget());
        regions += outcome.certificate.regions.len();
        naming_a_seed += premises_asserting_a_seed(&outcome.certificate);
        premised += outcome
            .certificate
            .regions
            .iter()
            .flat_map(|region| &region.events)
            .filter(
                |event| matches!(event, EvpEvent::Rewrite(rewrite) if !rewrite.premises.is_empty()),
            )
            .count();
    }
    println!("regions={regions}");
    println!("premisedRewrites={premised}");
    println!("premisesAssertingASeed={naming_a_seed}");
    assert!(
        naming_a_seed > 0,
        "a fact crossing a block edge must reach a premise somewhere in the corpus"
    );
}

/// What the fixed point costs, in block evaluations over the transaction.
///
/// A work item is one block taken off the queue and evaluated: its arguments'
/// facts read, its instructions walked, and what it passes carried across its
/// edges. That is the only cost this analysis has, and a transaction pays it
/// twice, once for the sweep that proposes rewrites and once for the region
/// building that states them.
#[test]
fn the_corpus_measures_the_fixed_points_work_items() {
    let mut items: BTreeMap<usize, usize> = BTreeMap::new();
    let mut most = 0_usize;
    for seed in 0..CORPUS {
        let module = generate_module(seed, GeneratorConfig::default());
        let outcome = optimize_scalar_module_with_evp1(&module, budget());
        *items
            .entry(outcome.report.seed_fact_work_items)
            .or_insert(0) += 1;
        most = most.max(outcome.report.seed_fact_work_items);
    }
    println!("seedFactWorkItems={items:?}");
    println!("maxSeedFactWorkItems={most}");
    assert!(
        most >= 2,
        "a corpus with branches must evaluate more than one block somewhere"
    );
    assert!(
        !items.contains_key(&0),
        "every function is walked at least once"
    );
}

/// Section 8.10: "the order of visits does not change the result".
///
/// That sentence is the whole licence for re-evaluating only what an edge
/// change reaches, so it is measured rather than believed: over the corpus, the
/// iteration the producer runs and the sweep of the whole function it replaced
/// must derive the same fact for every block argument of every function, bit
/// for bit, and the iteration must cost strictly less.
#[test]
fn the_iteration_and_the_whole_function_sweep_agree_on_the_corpus() {
    let mut arguments = 0_usize;
    let mut work_items = 0_usize;
    let mut whole_pass_items = 0_usize;
    let mut deepest = 0_usize;
    let mut distribution: BTreeMap<usize, usize> = BTreeMap::new();
    for seed in 0..CORPUS {
        let module = generate_module(seed, GeneratorConfig::default());
        for function in &module.functions {
            let measured = seed_fact_agreement(function).unwrap_or_else(|error| panic!("{error}"));
            arguments += measured.arguments;
            work_items += measured.work_items;
            whole_pass_items += measured.whole_pass_items;
            deepest = deepest.max(measured.whole_passes);
            *distribution.entry(measured.work_items).or_insert(0) += 1;
        }
    }
    println!("arguments={arguments}");
    println!("workItems={work_items}");
    println!("wholePassItems={whole_pass_items}");
    println!("deepestWholePassCount={deepest}");
    println!("workItemsPerFunction={distribution:?}");
    assert!(
        arguments > 0,
        "the corpus must give some block argument a fact"
    );
    assert!(
        deepest >= 2,
        "a corpus with branches must need more than one sweep somewhere"
    );
    assert!(
        work_items < whole_pass_items,
        "the iteration spent {work_items} block evaluations and the sweep {whole_pass_items}: \
         re-evaluating only what an edge change reaches has to cost less"
    );
}

/// A `xor` both of whose operands a block argument knows completely folds to
/// its constant, on the fact rule rather than on the fold.
///
/// ```text
/// ^entry(%0, %1, %2):
///   %3 = const.bits<8> 5
///   %4 = const.bits<8> 3
///   br ^b(%3, %4, %1, %2)
/// ^b(%5, %6, %7, %8):
///   %9 = xor %5, %6        -- fully known (6); no const node in the region
///   ret %9, %7, %8
/// ```
///
/// This is the input that separates two side conditions Section 11.6 used to
/// treat as one. `%5` and `%6` are fully known by the seed fixed point while
/// the region holds no `const` node for either, so `all_const` fails and
/// `fully_known` holds: registry order still puts `p0_105` first, and it does
/// not apply, so `p1_202` answers the node. Before revision 1.9.46 no input
/// could do this, which is why the three fact folds were called dominated;
/// revision 1.9.47 lifted the sentence that forbade them.
#[test]
fn a_xor_of_two_fully_known_arguments_folds_on_its_fact() {
    let five = bits(3);
    let three = bits(4);
    let carried = bits(5);
    let other = bits(6);
    let result = bits(9);
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
                    argument(&bits(0)),
                    argument(&memory(1)),
                    argument(&world(2)),
                ],
                insts: vec![Inst::Const(five.clone(), 5), Inst::Const(three.clone(), 3)],
                terminator: Inst::Br(1, vec![five, three, memory(1), world(2)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&carried),
                    argument(&other),
                    argument(&memory(7)),
                    argument(&world(8)),
                ],
                insts: vec![Inst::Xor(result.clone(), carried, other)],
                terminator: Inst::Ret(vec![result, memory(7), world(8)]),
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
        outcome.rules.contains(&"p1_202_known_xor".to_owned()),
        "the fact rule answers what the fold cannot: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        !outcome.rules.contains(&"p0_105_fold_xor".to_owned()),
        "all_const does not hold here, so the fold must not claim it: {:?}",
        outcome.rules
    );
    assert!(
        !outcome.after.contains("xor.bits<8>"),
        "the xor leaves the output:\n{}",
        outcome.after
    );
}

/// A comparison the sweep saw as an identity is not one to the region.
///
/// Reduced from seed 1592634837 of the long campaign, which the checker
/// refused.
///
/// ```text
/// ^entry(%0, %1, %2):
///   %3 = const.bits<64> 0
///   br ^b(%3, %1, %2)
/// ^b(%4, %5, %6):
///   %7 = mul %4, %4        -- the seed is known zero, so this folds
///   %8 = or %4, %7         -- the sweep sees or(%4, 0) and aliases it to %4
///   %9 = icmp.sle %4, %8   -- so the sweep reads icmp.sle(%4, %4)
/// ```
///
/// The sweep works on its own rewritten stream, where `%7` has become a
/// constant node and `%8` an alias of `%4`, so `icmp.sle(?x, ?x)` matches. The
/// region is built from `before.evo`, where the comparison's operands are the
/// seed's class and the disjunction's class; the disjunction joins the
/// multiplication and the derived zero, and nothing ever joins the seed to
/// them, because a seed carries a fact rather than a node and no rule turns a
/// fully known fact into a constant class. `p0_013_icmp_sle_self` is therefore
/// a claim the region cannot support, and the fact-driven comparison is the
/// rule that can.
#[test]
fn a_comparison_the_sweep_folded_is_stated_as_the_rule_the_region_can_prove() {
    let wide = |id: u32| Value {
        id,
        ty: EvoIRType::Bits(64),
    };
    let zero = wide(3);
    let carried = wide(4);
    let squared = wide(7);
    let joined = wide(8);
    let compared = Value {
        id: 9,
        ty: EvoIRType::Bits(1),
    };
    let function = Function {
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(64),
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
            EvoIRType::Bits(1),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![
                    argument(&wide(0)),
                    argument(&memory(1)),
                    argument(&world(2)),
                ],
                insts: vec![Inst::Const(zero.clone(), 0)],
                terminator: Inst::Br(1, vec![zero, memory(1), world(2)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&carried),
                    argument(&memory(5)),
                    argument(&world(6)),
                ],
                insts: vec![
                    Inst::Mul(squared.clone(), carried.clone(), carried.clone()),
                    Inst::Or(joined.clone(), carried.clone(), squared),
                    Inst::ICmp(compared.clone(), evocc_evoir::CmpOp::SLe, carried, joined),
                ],
                terminator: Inst::Ret(vec![compared, memory(5), world(6)]),
            },
        ],
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the probe is valid EvoIR");
    // `optimize` sends the whole transaction through evo-proofcheck, so a
    // certificate claiming the identity is refused here rather than counted.
    let outcome = optimize(&module);
    assert!(
        !outcome.rules.contains(&"p0_013_icmp_sle_self".to_owned()),
        "the region cannot join the seed's class to the disjunction's: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        outcome.rules.contains(&"p1_213_known_icmp_sle".to_owned()),
        "the facts decide the comparison, and that is the rule to state: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
}

/// A block whose double negation the sweep reads off two zeros of its own.
///
/// ```text
/// ^entry(%0, %1, %2):
///   %3 = const.bits<8> 0
///   br ^b(%3, %0, %1, %2)
/// ^b(%4, %5, %6, %7):        -- %4 is known zero by the seed fixed point
///   <lead>                   -- the zero the two subtractions share
///   %9  = sub <zero>, %5     -- the inner negation
///   %10 = mul %4, %4         -- fully known: a zero the transaction derived
///   %11 = sub <outer>, %9    -- the double negation
/// ```
///
/// `derived` decides where the shared zero comes from. With it set, the only
/// zero in the block is the one `mul(%4, %4)` folds to, and the region holds no
/// `const` node the before witness put there. Without it, a written
/// `const.bits<8> 0` leads the block and both subtractions read it.
fn double_negation(derived: bool) -> Module {
    let written = bits(8);
    let squared = bits(10);
    let negated = bits(9);
    let restored = bits(11);
    let zero = if derived {
        squared.clone()
    } else {
        written.clone()
    };
    let mut body = Vec::new();
    if derived {
        body.push(Inst::Mul(squared, bits(4), bits(4)));
    } else {
        body.push(Inst::Const(written, 0));
    }
    body.push(Inst::Sub(negated.clone(), zero.clone(), bits(5)));
    body.push(Inst::Sub(restored.clone(), zero, negated));
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
                    argument(&bits(0)),
                    argument(&memory(1)),
                    argument(&world(2)),
                ],
                insts: vec![Inst::Const(bits(3), 0)],
                terminator: Inst::Br(1, vec![bits(3), bits(0), memory(1), world(2)]),
            },
            Block {
                id: 1,
                args: vec![
                    argument(&bits(4)),
                    argument(&bits(5)),
                    argument(&memory(6)),
                    argument(&world(7)),
                ],
                insts: body,
                terminator: Inst::Ret(vec![restored, memory(6), world(7)]),
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

/// A double negation over a zero no before witness wrote states no rule.
///
/// `p0_009_double_neg` is written over `const_like` zeros, and Section 11.6
/// lets a left-hand `const_like` match only a node already in the before
/// witness closure. The sweep does not read the region's partition: it reads
/// its own constant environment, where the fold of `mul(%4, %4)` on a seed
/// known zero has put a zero that the region has only as a node this
/// transaction derived. The rule is therefore not the one the region can state,
/// and the region says nothing rather than saying it.
#[test]
fn a_double_negation_over_a_derived_zero_states_no_rule() {
    let module = double_negation(true);
    let outcome = optimize(&module);
    assert!(
        !outcome.rules.contains(&"p0_009_double_neg".to_owned()),
        "no before witness wrote the zero the two subtractions share: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        outcome.after.contains("sub.bits<8>"),
        "a rule the region cannot state must leave the subtraction alone:\n{}",
        outcome.after
    );
}

/// The same double negation over a zero a before witness wrote is stated.
///
/// This is the other half: the reduction is a real one whenever the partition
/// puts a before-witness zero in both subtractions' zero operand, and it must
/// keep firing there. The fold of `mul(%4, %4)` still happens, and its result
/// joins the class the written zero already owns, which is exactly why the
/// outer subtraction's zero operand is a class the before witness spoke for.
#[test]
fn a_double_negation_over_a_written_zero_is_stated_and_replayed() {
    let module = double_negation(false);
    let outcome = optimize(&module);
    assert!(
        outcome.rules.contains(&"p0_009_double_neg".to_owned()),
        "a written zero is what the rule is about: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
    assert!(
        outcome.after.contains("ret %v5"),
        "the double negation returns what it negated twice:\n{}",
        outcome.after
    );
}
