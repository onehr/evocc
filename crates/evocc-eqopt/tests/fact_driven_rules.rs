//! The eighteen fact-driven rules of `p0.scalar.v2`, one module per rule.
//!
//! Each case is the smallest program whose optimization must fire exactly one
//! named rule, and the assertion names the rule, its bindings and the premises
//! Section 11.6 requires it to record. A rule that fired with the right answer
//! but the wrong premise order would still be unverifiable by a checker that
//! recomputes facts, so the order is asserted rather than the set.
//!
//! The facts a case relies on are stated in its comment as bit patterns, not as
//! prose: the transfer table of Section 8.10 is what decides these, and a case
//! whose comment disagreed with the table would be testing the implementation
//! against itself.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_proof::{render_evp1, EvpEvent, EvpPremise, EvpRewrite};
use evocc_types::{EvoIRType, Region};

const WIDTH: u16 = 8;

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(WIDTH),
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

/// `@main(%x, %y, %m, %w)` with two opaque `bits<8>` inputs.
///
/// Two are needed rather than one because the covering rules want a masked
/// value and a covering mask that are not the same expression; deriving both
/// from one input would make every case depend on whether the analysis looks
/// through a shared operand, which the Section 8.10 table does not do.
fn probe(insts: Vec<Inst>, root: Value) -> Module {
    let x = bits(0);
    let y = bits(1);
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
                ty: EvoIRType::Bits(WIDTH),
            },
            FunctionParam {
                name: "y".to_owned(),
                ty: EvoIRType::Bits(WIDTH),
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
                argument(&x),
                argument(&y),
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

/// The single rewrite the optimization of `module` performs on `root`.
fn only_rewrite(module: &Module) -> EvpRewrite {
    let outcome = optimize_scalar_module_with_evp1(module, ScalarEqBudget::new(20_000, 256));
    let text = render_evp1(&outcome.certificate);
    let region = outcome
        .certificate
        .regions
        .last()
        .unwrap_or_else(|| panic!("no region was admitted:\n{text}"));
    let rewrites = region
        .events
        .iter()
        .filter_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    rewrites
        .last()
        .cloned()
        .unwrap_or_else(|| panic!("the last region carries no rewrite:\n{text}"))
}

/// `class:mask:value` for one premise, so a failure prints what was claimed.
fn premise(premise: &EvpPremise) -> String {
    format!(
        "c{}:{:0width$x}:{:0width$x}",
        premise.class,
        premise.mask,
        premise.value,
        width = premise.digits
    )
}

fn premises(rewrite: &EvpRewrite) -> Vec<String> {
    rewrite.premises.iter().map(premise).collect()
}

fn bindings(rewrite: &EvpRewrite) -> Vec<String> {
    rewrite
        .bindings
        .iter()
        .map(|(name, _)| name.clone())
        .collect()
}

/// The fact a `fully_known` premise must carry: every bit of the matched
/// node's own class known, and nothing else read.
fn assert_fully_known(rewrite: &EvpRewrite, rule: &str, width: u16, value: u64) {
    assert_eq!(rewrite.rule, rule, "rule");
    assert_eq!(bindings(rewrite), ["a", "b"], "{rule} binds ?a and ?b");
    assert_eq!(
        rewrite.premises.len(),
        1,
        "{rule} reads one class: the matched node's"
    );
    let recorded = rewrite.premises[0];
    assert_eq!(
        recorded.class, rewrite.lhs_pre,
        "{rule} names the matched node's class"
    );
    let full = if width >= 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let digits = usize::from(width.div_ceil(4));
    assert_eq!(
        (recorded.mask, recorded.value, recorded.digits),
        (full, value, digits),
        "{rule} premise {}",
        premise(&recorded)
    );
}

// -- fully_known over the binary operators ---------------------------------

/// `mul(?x, 0)`: the trailing known-zero run of `0` is the whole width, so the
/// product is known zero without either operand being a constant class.
#[test]
fn a_known_zero_product_folds_through_its_fact() {
    let zero = bits(4);
    let product = bits(5);
    let module = probe(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Mul(product.clone(), bits(0), zero),
        ],
        product,
    );
    assert_fully_known(&only_rewrite(&module), "p1_205_known_mul", WIDTH, 0);
}

/// Two values with complementary known-zero nibbles.
///
/// `and(%x, 0x0f)` knows bits four to seven zero and `and(%y, 0xf0)` knows bits
/// zero to three zero, so their conjunction is known zero on every bit while no
/// class in sight holds a constant of its own. It is the shape in which the
/// analysis derives a constant the before witness closure does not contain, and
/// every case below that needs such a constant starts from it.
fn complementary_masks() -> (Vec<Inst>, Value) {
    let low_mask = bits(4);
    let high_mask = bits(5);
    let low = bits(6);
    let high = bits(7);
    let product = bits(8);
    (
        vec![
            Inst::Const(low_mask.clone(), 0x0f),
            Inst::Const(high_mask.clone(), 0xf0),
            Inst::And(low.clone(), bits(0), low_mask),
            Inst::And(high.clone(), bits(1), high_mask),
            Inst::And(product.clone(), low, high),
        ],
        product,
    )
}

/// `and(?a, ?b)` where each operand knows the bits the other leaves unknown to
/// be zero. Nothing here is a constant, and the conjunction is still known.
#[test]
fn an_and_of_complementary_masks_folds_through_its_fact() {
    let (insts, product) = complementary_masks();
    let module = probe(insts, product);
    assert_fully_known(&only_rewrite(&module), "p1_200_known_and", WIDTH, 0);
}

/// `or(?x, 255)`: every bit is known one because one input knows it one.
#[test]
fn an_or_with_all_ones_folds_through_its_fact() {
    let ones = bits(4);
    let result = bits(5);
    let module = probe(
        vec![
            Inst::Const(ones.clone(), 0xff),
            Inst::Or(result.clone(), bits(0), ones),
        ],
        result,
    );
    assert_fully_known(&only_rewrite(&module), "p1_201_known_or", WIDTH, 0xff);
}

/// `xor`, `add` and `sub` are fully known only when both operands are, which
/// under the Section 8.10 transfers is exactly when `all_const` holds. Section
/// 11.6 makes the producer apply the first applicable rule in registry order,
/// so the premise-free fold always precedes the fact rule on such a node and
/// `p1_202`, `p1_203` and `p1_204` are dominated: printed so their ids keep
/// their meaning, and fired by no conforming producer.
fn against_a_derived_constant(build: fn(Value, Value, Value) -> Inst) -> EvpRewrite {
    let (mut insts, product) = complementary_masks();
    let five = bits(9);
    let result = bits(10);
    insts.push(Inst::Const(five.clone(), 5));
    insts.push(build(result.clone(), product, five));
    only_rewrite(&probe(insts, result))
}

#[test]
fn the_fold_dominates_the_fact_rule_on_xor_add_and_sub() {
    for (build, rule) in [
        (
            Inst::Xor as fn(Value, Value, Value) -> Inst,
            "p0_105_fold_xor",
        ),
        (Inst::Add, "p0_100_fold_add"),
        (
            (|result, product, five| Inst::Sub(result, five, product))
                as fn(Value, Value, Value) -> Inst,
            "p0_101_fold_sub",
        ),
    ] {
        let rewrite = against_a_derived_constant(build);
        assert_eq!(rewrite.rule, rule, "registry order puts the fold first");
        assert!(
            rewrite.premises.is_empty(),
            "a fold reads its constants from the nodes and carries no premise"
        );
    }
}

// -- fully_known over the comparisons ---------------------------------------

/// `and(?x, 0x0f)` knows its high nibble zero and nothing else, so it ranges
/// over `0..=15`. Compared against `16` every ordering is decided and equality
/// is decided by bit four, which the two disagree on.
fn compared_against_sixteen(operation: CmpOp) -> EvpRewrite {
    let mask = bits(4);
    let low = bits(5);
    let sixteen = bits(6);
    let result = flag(7);
    let module = probe(
        vec![
            Inst::Const(mask.clone(), 0x0f),
            Inst::And(low.clone(), bits(0), mask),
            Inst::Const(sixteen.clone(), 0x10),
            Inst::ICmp(result.clone(), operation, low, sixteen),
        ],
        result,
    );
    only_rewrite(&module)
}

#[test]
fn an_equality_decided_by_a_disagreeing_bit_folds_through_its_fact() {
    assert_fully_known(
        &compared_against_sixteen(CmpOp::Eq),
        "p1_210_known_icmp_eq",
        1,
        0,
    );
    assert_fully_known(
        &compared_against_sixteen(CmpOp::Ne),
        "p1_211_known_icmp_ne",
        1,
        1,
    );
}

#[test]
fn an_ordering_decided_by_intervals_folds_through_its_fact() {
    assert_fully_known(
        &compared_against_sixteen(CmpOp::SLt),
        "p1_212_known_icmp_slt",
        1,
        1,
    );
    assert_fully_known(
        &compared_against_sixteen(CmpOp::SLe),
        "p1_213_known_icmp_sle",
        1,
        1,
    );
    assert_fully_known(
        &compared_against_sixteen(CmpOp::ULt),
        "p1_214_known_icmp_ult",
        1,
        1,
    );
    assert_fully_known(
        &compared_against_sixteen(CmpOp::ULe),
        "p1_215_known_icmp_ule",
        1,
        1,
    );
}

// -- covers and known_zero --------------------------------------------------

/// A masked value and a mask that covers it.
///
/// `%low = and(%x, 0x0f)` knows bits four to seven zero; `%high = or(%y, 0x0f)`
/// knows bits zero to three one. Every bit not known zero in `%low` is known
/// one in `%high`, which is exactly `covers(%high, %low)`. Neither class is
/// fully known, so no fold rule can state either case.
fn covering_pair(build: fn(Value, Value, Value) -> Inst, root: Value) -> EvpRewrite {
    let mask = bits(4);
    let low = bits(5);
    let high = bits(6);
    let module = probe(
        vec![
            Inst::Const(mask.clone(), 0x0f),
            Inst::And(low.clone(), bits(0), mask.clone()),
            Inst::Or(high.clone(), bits(1), mask),
            build(root.clone(), low, high),
        ],
        root,
    );
    only_rewrite(&module)
}

/// `?m` then `?x`, the order Section 11.6 fixes for `covers`.
fn assert_covers(rewrite: &EvpRewrite, rule: &str) {
    assert_eq!(rewrite.rule, rule, "rule");
    assert_eq!(bindings(rewrite), ["m", "x"], "{rule} binds ?m and ?x");
    assert_eq!(
        premises(rewrite).len(),
        2,
        "{rule} reads two classes: {:?}",
        premises(rewrite)
    );
    let mask_class = rewrite
        .bindings
        .iter()
        .find(|(name, _)| name == "m")
        .map(|(_, class)| *class)
        .expect("?m is bound");
    let value_class = rewrite
        .bindings
        .iter()
        .find(|(name, _)| name == "x")
        .map(|(_, class)| *class)
        .expect("?x is bound");
    assert_eq!(
        (rewrite.premises[0].class, rewrite.premises[1].class),
        (mask_class, value_class),
        "{rule} records ?m before ?x"
    );
    // `or(%y, 0x0f)` knows its low nibble one; `and(%x, 0x0f)` knows its high
    // nibble zero. Both are facts of Section 8.10's table, not of this test.
    assert_eq!(
        (rewrite.premises[0].mask, rewrite.premises[0].value),
        (0x0f, 0x0f),
        "?m is the covering mask"
    );
    assert_eq!(
        (rewrite.premises[1].mask, rewrite.premises[1].value),
        (0xf0, 0x00),
        "?x is the masked value"
    );
}

#[test]
fn an_and_with_a_covering_mask_is_its_masked_operand() {
    assert_covers(&covering_pair(Inst::And, bits(7)), "p1_220_and_cover_rhs");
    assert_covers(
        &covering_pair(|result, low, high| Inst::And(result, high, low), bits(7)),
        "p1_221_and_cover_lhs",
    );
}

#[test]
fn an_or_with_a_covering_mask_is_the_mask() {
    assert_covers(&covering_pair(Inst::Or, bits(7)), "p1_222_or_cover_rhs");
    assert_covers(
        &covering_pair(|result, low, high| Inst::Or(result, high, low), bits(7)),
        "p1_223_or_cover_lhs",
    );
}

/// `xor(?x, 0)` is `?x` for any `?x`, and the premise is the zero operand's
/// fact alone.
#[test]
fn a_xor_with_a_known_zero_operand_is_the_other_operand() {
    for (rule, swapped) in [
        ("p1_224_xor_known_zero_rhs", false),
        ("p1_225_xor_known_zero_lhs", true),
    ] {
        let zero = bits(4);
        let result = bits(5);
        let operands = if swapped {
            (zero.clone(), bits(0))
        } else {
            (bits(0), zero.clone())
        };
        let module = probe(
            vec![
                Inst::Const(zero, 0),
                Inst::Xor(result.clone(), operands.0, operands.1),
            ],
            result,
        );
        let rewrite = only_rewrite(&module);
        assert_eq!(rewrite.rule, rule, "rule");
        assert_eq!(bindings(&rewrite), ["x", "z"], "{rule} binds ?x and ?z");
        assert_eq!(premises(&rewrite).len(), 1, "{rule} reads ?z alone");
        let zero_class = rewrite
            .bindings
            .iter()
            .find(|(name, _)| name == "z")
            .map(|(_, class)| *class)
            .expect("?z is bound");
        assert_eq!(rewrite.premises[0].class, zero_class, "{rule} records ?z");
        assert_eq!(
            (rewrite.premises[0].mask, rewrite.premises[0].value),
            (0xff, 0x00),
            "?z is known zero"
        );
    }
}

/// A transaction that fires a fact-driven rule commits to the registry that
/// contains it; one that fires none may still name the registry it used.
#[test]
fn a_fact_driven_transaction_names_the_revised_registry() {
    let zero = bits(4);
    let product = bits(5);
    let module = probe(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Mul(product.clone(), bits(0), zero),
        ],
        product,
    );
    let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
    assert_eq!(outcome.certificate.registry_name, "p0.scalar.v2");
    assert_eq!(
        outcome.certificate.registry_digest,
        "c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce"
    );
    // The optimized module still means what the input meant: the product of
    // anything with zero is zero.
    let text = render_module(&outcome.module);
    assert!(text.contains("const.bits<8> 0"), "{text}");
}
