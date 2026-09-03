//! What the six transfer rows of revision 1.9.43 can reach.
//!
//! Section 8.10 now gives `shl`, `lshr`, `ashr`, `zext`, `sext` and `trunc` a
//! row each. A premise, though, is "recomputed by the checker from the region's
//! nodes" (8.10 Use, 11.3 Premises), and Section 8.5's operator allowlist keeps
//! every one of those six out of a region: they may not be a root, and the
//! closure follows an operand only into an allowlisted instruction, so such a
//! value is a seed. Section 11.3's `op_id` table has no spelling for them
//! either, so no certificate can carry one.
//!
//! The rows are therefore live in the producer's own environment, where the
//! sweep uses them to propose a rewrite, and unreachable in a region, where the
//! fact is recomputed and comes back top. This file measures both ends of that.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::EvpEvent;
use evocc_randgate::{generate_module, GeneratorConfig};
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
/// %k = const.bits<8> 2
/// %s = shl %x, %k      -- low two bits known zero, by the new row
/// %m = const.bits<8> 252
/// %r = and %s, %m      -- `covers(%m, %s)`, so %r is %s
/// ```
///
/// The fact is real and the rule is in the registry, and the rewrite is still
/// not stateable: `%s` is a seed, Section 8.10 gives a seed no fact, and the
/// side condition is decided from the recomputed fact.
#[test]
fn a_fact_carried_by_a_shift_cannot_reach_a_region() {
    let amount = bits(4);
    let shifted = bits(5);
    let mask = bits(6);
    let masked = bits(7);
    let module = probe(
        vec![
            Inst::Const(amount.clone(), 2),
            Inst::Shl(shifted.clone(), bits(0), amount),
            Inst::Const(mask.clone(), 0xfc),
            Inst::And(masked.clone(), shifted, mask),
        ],
        masked,
    );
    let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
    assert!(
        outcome.certificate.regions.is_empty(),
        "a shift is not on the Section 8.5 allowlist, so its fact reaches no region"
    );
    assert!(
        render_module(&outcome.module).contains("and.bits<8>"),
        "and so the conjunction stays"
    );
}

/// The same with a width change in place of the shift, for the same reason.
///
/// ```text
/// %n = trunc.bits<4> %x
/// %w = zext.bits<8> %n   -- upper four bits known zero, by the new row
/// %m = const.bits<8> 15
/// %r = and %w, %m        -- `covers(%m, %w)`, so %r is %w
/// ```
#[test]
fn a_fact_carried_by_a_width_change_cannot_reach_a_region() {
    let narrow = Value {
        id: 4,
        ty: EvoIRType::Bits(4),
    };
    let widened = bits(5);
    let mask = bits(6);
    let masked = bits(7);
    let module = probe(
        vec![
            Inst::Trunc(narrow.clone(), bits(0), EvoIRType::Bits(4)),
            Inst::ZExt(widened.clone(), narrow, EvoIRType::Bits(8)),
            Inst::Const(mask.clone(), 0x0f),
            Inst::And(masked.clone(), widened, mask),
        ],
        masked,
    );
    let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
    assert!(
        outcome.certificate.regions.is_empty(),
        "an extension is not on the Section 8.5 allowlist either"
    );
}

/// How many rewrites the campaign's own programs state a premise for.
///
/// The number is what the shift and width rows are measured against: they can
/// only add to it by way of a region, and a region cannot hold the operator
/// that carries the fact.
#[test]
fn the_campaign_counts_its_premised_rewrites() {
    let mut premised = 0_u64;
    let mut rewrites = 0_u64;
    for seed in 0..256_u64 {
        let module = generate_module(seed, GeneratorConfig::default());
        let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
        for region in &outcome.certificate.regions {
            for event in &region.events {
                if let EvpEvent::Rewrite(rewrite) = event {
                    rewrites += 1;
                    if !rewrite.premises.is_empty() {
                        premised += 1;
                    }
                }
            }
        }
    }
    println!("rewrites={rewrites}");
    println!("premisedRewrites={premised}");
    assert!(premised > 0, "the fact-driven rules must still fire");
}
