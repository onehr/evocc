//! A defect the randomized gate found, reduced to one hand-written module.
//!
//! Seed 1592591013 of the standing campaign produced a module whose optimized
//! form is semantically identical to its input -- return values, volatile trace
//! and surviving static memory all agree -- and whose EVP1 certificate
//! `evo-proofcheck` nonetheless refuses with status 3.
//!
//! The refusal is one recorded binding. In the campaign's certificate the
//! rewrite
//!
//! ```text
//! rewrite 10 p0_010_icmp_eq_self c5 c7 c5 node:n5 node:n7 bind ?x:c3
//! ```
//!
//! names class `c3`, while the certificate's own node for that comparison is
//! `node 5 n5 c5 ... op=icmp.eq ... children=c4,c4` and the recorded merges
//! make `find(c4) = c1`. The checker recomputes the substitution from the
//! node's children (`crates/evocc-proofcheck/src/replay.rs:616`) and refuses.
//! Rewriting that one token to `bind ?x:c1` turns the same three files from
//! status 3 into status 0, which is what identifies the binding rather than the
//! merge, the extraction or the reassembly as the refused claim.
//!
//! `c1` and `c3` both hold `const.bits<2> 1`: two e-classes with equal
//! constants that are never unified, because a constant is a dynamic operator
//! identity carrying neither width nor payload
//! (`crates/evocc-eqopt/src/scalar_eq.rs`, `ProofAuthority::ProducerSupplied`).
//! The producer computes the binding from the *remapped* instruction operand
//! (`crates/evocc-eqopt/src/evp1.rs:291`) while the node stream records the
//! pre-remap children, so the two disagree exactly when an earlier replacement
//! in the same witness closure redirected an operand to a different, equal
//! constant's class.
//!
//! Whether the producer should bind from the node's children or equal constants
//! should share an e-class is a design decision this crate does not get to
//! make. The test states the property and fails until that decision is taken.

use std::path::PathBuf;

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    parse_section9_module, render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary,
    Function, FunctionParam, Inst, Module, Value,
};
use evocc_proof::render_evp1;
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

/// ```text
/// %v2 = and.bits<2> 2, 1   -- folds to 0
/// %v4 = or.bits<2> %v2, 1  -- folds to 1, aliased to one of two equal constants
/// %v5 = icmp.eq %v4, %v4   -- the identity comparison whose binding disagrees
/// ```
fn module() -> Module {
    let two = value(0, EvoIRType::Bits(2));
    let one = value(1, EvoIRType::Bits(2));
    let masked = value(2, EvoIRType::Bits(2));
    let other_one = value(3, EvoIRType::Bits(2));
    let merged = value(4, EvoIRType::Bits(2));
    let identity = value(5, EvoIRType::Bits(1));
    let memory = value(6, EvoIRType::Mem(Region::All));
    let world = value(7, EvoIRType::World);

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
            EvoIRType::Bits(1),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&memory), argument(&world)],
            insts: vec![
                Inst::Const(two.clone(), 2),
                Inst::Const(one.clone(), 1),
                Inst::And(masked.clone(), two, one),
                Inst::Const(other_one.clone(), 1),
                Inst::Or(merged.clone(), masked, other_one),
                Inst::ICmp(identity.clone(), CmpOp::Eq, merged.clone(), merged.clone()),
            ],
            terminator: Inst::Ret(vec![identity, memory.clone(), world.clone()]),
        }],
    };

    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module
}

fn checker() -> ProofChecker {
    ProofChecker::locate().expect("build the workspace so evo-proofcheck is present")
}

#[test]
fn a_binding_must_name_the_class_the_certificates_own_node_children_name() {
    let before = module();
    validate_module(&before).expect("the reduced module is valid EvoIR");
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
    validate_module(&outcome.module).expect("the optimized module is valid EvoIR");

    let before_text = render_module(&before);
    let after_text = render_module(&outcome.module);
    let proof_text = render_evp1(&outcome.certificate);

    assert!(
        !outcome.certificate.regions.is_empty(),
        "the reduced module must still be optimized, or it pins nothing:\n{before_text}"
    );

    if let Err(error) = checker().check(&before_text, &after_text, &proof_text) {
        panic!(
            "the standalone checker refused a semantically correct optimization: {error}\n\
             --- before.evo ---\n{before_text}\n--- after.evo ---\n{after_text}\n\
             --- proof.evp ---\n{proof_text}"
        );
    }
}

/// A module with nothing to optimize whose SSA value identifiers are not dense.
///
/// Value identities are function-unique but need not be consecutive: nothing in
/// Section 2.4 requires it, and a module that has had an instruction removed --
/// by this crate's shrinker, or by any earlier pass -- has gaps.
fn sparsely_numbered_module() -> Module {
    let input = value(4, EvoIRType::Bits(32));
    let memory = value(9, EvoIRType::Mem(Region::All));
    let world = value(11, EvoIRType::World);
    let result = value(6, EvoIRType::Bits(32));

    let function = Function {
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "arg0".to_owned(),
                ty: EvoIRType::Bits(32),
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
            EvoIRType::Bits(32),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&input), argument(&memory), argument(&world)],
            insts: vec![Inst::Xor(result.clone(), input.clone(), input)],
            terminator: Inst::Ret(vec![result, memory, world]),
        }],
    };

    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module
}

/// The second refusal the gate found, and a different defect from the first.
///
/// `xor(x, x)` on an opaque operand matches no rule in `p0.scalar.v1`, so the
/// producer admits no region and emits a certificate with no `region` block.
/// Section 11.3, "A certificate with no region", says what that claims:
/// `after.evo` is the canonical rendering of the module parsed from
/// `before.evo`, and nothing else. The module may therefore be renumbered; it
/// may not be otherwise changed, and the checker verifies the claim by
/// rendering the parsed input itself rather than comparing raw bytes, which
/// need not be canonical.
///
/// A densely numbered input hides the disagreement entirely, which is why a
/// randomized generator alone would not have reached it: the gate's own
/// shrinker produced the gapped module that did.
#[test]
fn a_certificate_with_no_region_claims_the_canonical_rendering_of_its_input() {
    let before = sparsely_numbered_module();
    validate_module(&before).expect("a gapped but valid module");
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));

    let before_text = render_module(&before);
    let after_text = render_module(&outcome.module);
    let proof_text = render_evp1(&outcome.certificate);

    assert!(
        outcome.certificate.regions.is_empty(),
        "this module must admit no region, or it pins nothing:\n{before_text}"
    );
    let canonical =
        render_module(&parse_section9_module(before_text.as_bytes()).expect("the input parses"));
    assert_eq!(
        canonical, after_text,
        "a no-region claim must be the canonical rendering of its own input"
    );
    checker()
        .check(&before_text, &after_text, &proof_text)
        .expect("a no-op claim must replay");
}

/// The same disagreement one field further along: the right-hand side.
///
/// Section 11.3 says a `class:cI` proof reference "is permitted only when that
/// side of the instantiated rule is a bare variable bound to `cI`". An alias
/// rule such as `mul(?x, 1) -> ?x` has exactly that shape, so its right-hand
/// side must name the class `?x` is bound to. The producer took it from the
/// instruction's operand instead, and an earlier replacement in the same
/// witness closure had pointed that operand at a different class holding an
/// equal constant -- so the binding said `c1` and the right-hand side said
/// `c3`, and the checker, which requires them to agree, refused.
///
/// Found by the twenty-thousand-program campaign at seed 1592636988, on a
/// module the extended generator produces and the earlier one did not.
///
/// ```text
/// %v0 = const.bits<2> 2
/// %v1 = const.bits<2> 1
/// %v2 = and.bits<2> %v0, %v1
/// %v3 = const.bits<2> 1
/// %v4 = or.bits<2> %v2, %v3
/// %v5 = mul.bits<2> %v4, %v1
/// ```
fn aliasing_module() -> Module {
    let two = value(0, EvoIRType::Bits(2));
    let one = value(1, EvoIRType::Bits(2));
    let masked = value(2, EvoIRType::Bits(2));
    let other_one = value(3, EvoIRType::Bits(2));
    let merged = value(4, EvoIRType::Bits(2));
    let scaled = value(5, EvoIRType::Bits(2));
    let memory = value(6, EvoIRType::Mem(Region::All));
    let world = value(7, EvoIRType::World);

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
            EvoIRType::Bits(2),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&memory), argument(&world)],
            insts: vec![
                Inst::Const(two.clone(), 2),
                Inst::Const(one.clone(), 1),
                Inst::And(masked.clone(), two, one.clone()),
                Inst::Const(other_one.clone(), 1),
                Inst::Or(merged.clone(), masked, other_one),
                Inst::Mul(scaled.clone(), merged, one),
            ],
            terminator: Inst::Ret(vec![scaled, memory.clone(), world.clone()]),
        }],
    };

    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module
}

#[test]
fn an_alias_right_hand_side_must_name_the_class_its_variable_is_bound_to() {
    let before = aliasing_module();
    validate_module(&before).expect("the reduced module is valid EvoIR");
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
    validate_module(&outcome.module).expect("the optimized module is valid EvoIR");

    let before_text = render_module(&before);
    let after_text = render_module(&outcome.module);
    let proof_text = render_evp1(&outcome.certificate);

    assert!(
        !outcome.certificate.regions.is_empty(),
        "the reduced module must still be optimized, or it pins nothing:\n{before_text}"
    );

    if let Err(error) = checker().check(&before_text, &after_text, &proof_text) {
        panic!(
            "the standalone checker refused a semantically correct optimization: {error}\n\
             --- before.evo ---\n{before_text}\n--- after.evo ---\n{after_text}\n\
             --- proof.evp ---\n{proof_text}"
        );
    }
}

/// The located checker is a real separate process, not a library call into the
/// optimizer's own code. Without this the tests above could pass by construction.
#[test]
fn the_checker_is_the_standalone_binary() {
    let binary: PathBuf = checker().binary().clone();
    assert!(binary.is_file(), "{} is not a file", binary.display());
    assert!(
        binary
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("evo-proofcheck")),
        "{} is not evo-proofcheck",
        binary.display()
    );
}
