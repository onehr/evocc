//! What a reader does with a certificate version it does not implement.
//!
//! Section 11.3, "Versions": `evp1` carries no budget line, no epoch markers
//! and no premises, so a checker validates it without a scheduler claim;
//! `evp2` adds the three, and "a reader that does not implement a version
//! rejects it as malformed rather than guessing".
//!
//! `evp2` is not produced here. Its `budget` line is undecidable as the grammar
//! stands -- see the SPEC_GAP recorded against revision 1.9.35 -- so emitting
//! one would mean choosing a field order the specification does not fix, and a
//! second implementation that chose the other order would disagree with every
//! certificate this producer wrote. Refusing to guess is the whole value of a
//! standalone checker, so this file pins the refusal instead.

use std::path::PathBuf;

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::render_evp1;
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

fn checker() -> ProofChecker {
    ProofChecker::locate().expect("build the workspace so evo-proofcheck is present")
}

/// A module with one foldable region, so the certificate has real content.
fn module() -> Module {
    let two = Value {
        id: 0,
        ty: EvoIRType::Bits(8),
    };
    let three = Value {
        id: 1,
        ty: EvoIRType::Bits(8),
    };
    let sum = Value {
        id: 2,
        ty: EvoIRType::Bits(8),
    };
    let memory = Value {
        id: 3,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 4,
        ty: EvoIRType::World,
    };
    let argument = |value: &Value| BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    };
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
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&memory), argument(&world)],
            insts: vec![
                Inst::Const(two.clone(), 2),
                Inst::Const(three.clone(), 3),
                Inst::Add(sum.clone(), two, three),
            ],
            terminator: Inst::Ret(vec![sum, memory, world]),
        }],
    };
    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module
}

struct Certificate {
    before: String,
    after: String,
    proof: String,
}

fn certificate() -> Certificate {
    let before = module();
    validate_module(&before).expect("the module is valid EvoIR");
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
    assert!(
        !outcome.certificate.regions.is_empty(),
        "the module must be optimized, or the version is all that is tested"
    );
    Certificate {
        before: render_module(&before),
        after: render_module(&outcome.module),
        proof: render_evp1(&outcome.certificate),
    }
}

#[test]
fn evp1_is_produced_and_verified() {
    let case = certificate();
    assert!(
        case.proof.starts_with("evp1\n"),
        "the producer emits evp1: {}",
        case.proof.lines().next().unwrap_or_default()
    );
    assert!(
        !case.proof.contains("\nbudget "),
        "evp1 carries no budget line"
    );
    assert!(
        !case.proof.contains("\nepoch "),
        "evp1 carries no epoch markers"
    );
    assert!(
        !case.proof.contains(" premises "),
        "evp1 carries no premises"
    );
    checker()
        .check(&case.before, &case.after, &case.proof)
        .expect("an evp1 certificate verifies");
}

#[test]
fn an_unimplemented_version_is_malformed_rather_than_guessed() {
    let case = certificate();
    for version in ["evp2", "evp3", "EVP1", "evp"] {
        let relabelled = case.proof.replacen("evp1\n", &format!("{version}\n"), 1);
        let error = checker()
            .check(&case.before, &case.after, &relabelled)
            .expect_err("a version this checker does not implement must be refused");
        assert!(
            error.contains("malformed"),
            "{version} must be malformed input (status 2), not a refused claim: {error}"
        );
    }
}

/// The checker really is the separate process, not a library call.
#[test]
fn the_version_check_runs_in_the_standalone_checker() {
    let binary: PathBuf = checker().binary().clone();
    assert!(binary.is_file(), "{} is not a file", binary.display());
}
