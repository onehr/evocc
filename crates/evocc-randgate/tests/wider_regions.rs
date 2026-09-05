//! What revision 1.9.44 admitted: six more operators, and a closure that
//! reaches a structural duplicate.
//!
//! Section 8.5 now lists `shl`, `lshr`, `ashr`, `zext`, `sext` and `trunc` as
//! roots and witnesses, so the transfer rows 1.9.43 added can finally reach a
//! premise, and `p0.scalar.v3` gives each of them a fact-driven fold. The same
//! revision extends the witness closure to every earlier allowlisted
//! instruction that is a structural duplicate of one already in it, which is
//! what lets a region say that its root is an earlier instruction's twin: the
//! two nodes are congruent as inserted, and the extraction names the earlier
//! one.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::{render_evp1, EvpEvent};
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

const REGISTRY_V3: &str =
    "registry p0.scalar.v3 aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15";

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
                argument(&value(0, 8)),
                argument(&value(1, 8)),
                argument(&memory),
                argument(&world),
            ],
            insts,
            terminator: Inst::Ret(vec![root, memory, world]),
        }],
    };
    let mut module = Module {
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the probe is valid EvoIR");
    module
}

struct Outcome {
    rules: Vec<String>,
    congruences: usize,
    proof: String,
    after: String,
}

fn optimize(module: &Module) -> Outcome {
    let outcome = optimize_scalar_module_with_evp1(module, ScalarEqBudget::new(20_000, 256));
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

/// Each of the six operators, folded through the fact its own row derives.
#[test]
fn the_six_new_operators_fold_through_their_facts() {
    let amount = value(4, 8);
    let source = value(5, 8);
    let shifted = value(6, 8);
    for (build, rule) in [
        (
            (|result, source, amount| Inst::Shl(result, source, amount))
                as fn(Value, Value, Value) -> Inst,
            "p1_206_known_shl",
        ),
        (
            |result, source, amount| Inst::LShr(result, source, amount),
            "p1_207_known_lshr",
        ),
        (
            |result, source, amount| Inst::AShr(result, source, amount),
            "p1_208_known_ashr",
        ),
    ] {
        let module = probe(
            vec![
                Inst::Const(amount.clone(), 2),
                Inst::Const(source.clone(), 0xa5),
                build(shifted.clone(), source.clone(), amount.clone()),
            ],
            shifted.clone(),
        );
        let outcome = optimize(&module);
        assert!(
            outcome.rules.contains(&rule.to_owned()),
            "{rule} must fire: {:?}\n{}",
            outcome.rules,
            outcome.proof
        );
    }
}

#[test]
fn the_width_changes_fold_through_their_facts() {
    let narrow = value(4, 4);
    let wide = value(5, 8);
    for (build, rule) in [
        (
            (|result: Value, source: Value| Inst::ZExt(result, source, EvoIRType::Bits(8)))
                as fn(Value, Value) -> Inst,
            "p1_230_known_zext",
        ),
        (
            |result: Value, source: Value| Inst::SExt(result, source, EvoIRType::Bits(8)),
            "p1_231_known_sext",
        ),
    ] {
        let module = probe(
            vec![
                Inst::Const(narrow.clone(), 0xd),
                build(wide.clone(), narrow.clone()),
            ],
            wide.clone(),
        );
        let outcome = optimize(&module);
        assert!(
            outcome.rules.contains(&rule.to_owned()),
            "{rule} must fire: {:?}\n{}",
            outcome.rules,
            outcome.proof
        );
    }

    let source = value(4, 8);
    let truncated = value(5, 4);
    let module = probe(
        vec![
            Inst::Const(source.clone(), 0x1f),
            Inst::Trunc(truncated.clone(), source, EvoIRType::Bits(4)),
        ],
        truncated,
    );
    let outcome = optimize(&module);
    assert!(
        outcome.rules.contains(&"p1_232_known_trunc".to_owned()),
        "p1_232_known_trunc must fire: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
}

/// A shift whose amount the analysis knows and whose value it does not still
/// carries a fact, and the fact reaches a rule that reads it.
///
/// ```text
/// %k = const.bits<8> 4
/// %s = shl %x, %k       -- low four bits known zero
/// %m = const.bits<8> 240
/// %r = and %s, %m       -- covers(%m, %s), so %r is %s
/// ```
#[test]
fn a_fact_carried_by_a_shift_now_reaches_a_region() {
    let amount = value(4, 8);
    let shifted = value(5, 8);
    let mask = value(6, 8);
    let masked = value(7, 8);
    let module = probe(
        vec![
            Inst::Const(amount.clone(), 4),
            Inst::Shl(shifted.clone(), value(0, 8), amount),
            Inst::Const(mask.clone(), 0xf0),
            Inst::And(masked.clone(), shifted, mask),
        ],
        masked,
    );
    let outcome = optimize(&module);
    assert!(
        outcome.rules.contains(&"p1_220_and_cover_rhs".to_owned()),
        "the shift's fact must decide the covering identity: {:?}\n{}",
        outcome.rules,
        outcome.proof
    );
}

/// A duplicate is a region, proved by congruence and nothing else.
///
/// ```text
/// %p = and %x, %y
/// %q = and %x, %y   -- the root, and the twin the closure now reaches
/// ```
#[test]
fn a_duplicate_is_a_region_proved_by_congruence_alone() {
    let first = value(4, 8);
    let second = value(5, 8);
    let module = probe(
        vec![
            Inst::And(first.clone(), value(0, 8), value(1, 8)),
            Inst::And(second.clone(), value(0, 8), value(1, 8)),
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

/// A certificate that fires one of the six new rules names the registry that
/// prints it, and no earlier one.
#[test]
fn a_transaction_using_the_new_rules_names_the_third_registry() {
    let amount = value(4, 8);
    let source = value(5, 8);
    let shifted = value(6, 8);
    let module = probe(
        vec![
            Inst::Const(amount.clone(), 2),
            Inst::Const(source.clone(), 0xa5),
            Inst::Shl(shifted.clone(), source, amount),
        ],
        shifted,
    );
    let outcome = optimize(&module);
    assert!(
        outcome.proof.contains(REGISTRY_V3),
        "a v3 rule needs the v3 registry:\n{}",
        outcome.proof
    );
}
