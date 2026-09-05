//! What a reader does with a certificate version it does not implement.
//!
//! Section 11.3, "Versions": `evp1` carries no budget line, no epoch markers
//! and no premises, so a checker validates it without a scheduler claim;
//! `evp2` adds the three, and "a reader that does not implement a version
//! rejects it as malformed rather than guessing". Both versions are read here;
//! what the budget line then claims is tested in `certificate_budget.rs`.
//!
//! The two readings this file once recorded as SPEC_GAPs -- the unordered
//! `a/b` of a budget quantity and the single fuel integer with no bound -- were
//! ruled in revision 1.9.36: every quantity is consumed `/` bound, and `fuel`
//! carries both its bound and the `n` that bound derives from.

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
        extern_objects: Vec::new(),
        declares: Vec::new(),
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
fn evp2_is_produced_and_verified() {
    let case = certificate();
    assert!(
        case.proof.starts_with("evp2\n"),
        "the producer emits evp2: {}",
        case.proof.lines().next().unwrap_or_default()
    );
    assert!(
        case.proof.contains("\nbudget "),
        "evp2 carries a budget line"
    );
    assert!(
        !case.proof.contains(" premises "),
        "no Tier B rule exists yet, so no rewrite carries a premise"
    );
    checker()
        .check(&case.before, &case.after, &case.proof)
        .expect("an evp2 certificate verifies");
}

/// An evp1 certificate carries none of the three additions, and a reader that
/// finds one has been handed a certificate no conforming producer wrote.
#[test]
fn evp1_carries_none_of_the_three_additions() {
    let case = certificate();
    let downgraded = case
        .proof
        .lines()
        .filter(|line| !line.starts_with("budget ") && !line.starts_with("epoch "))
        .map(|line| {
            if line == "evp2" {
                "evp1\n".to_owned()
            } else {
                format!("{line}\n")
            }
        })
        .collect::<String>();
    checker()
        .check(&case.before, &case.after, &downgraded)
        .expect("an evp1 certificate still verifies");

    let smuggled = downgraded.replacen(
        "evp1\n",
        "evp1\nbudget nodes=0/100000 epochs=0/1024 fuel=0/256 n=0 stop=saturated\n",
        1,
    );
    let error = checker()
        .check(&case.before, &case.after, &smuggled)
        .expect_err("evp1 with a budget line is not a certificate this format defines");
    assert!(
        error.contains("malformed"),
        "an evp1 budget line is malformed input, not a refused claim: {error}"
    );
}

#[test]
fn an_unimplemented_version_is_malformed_rather_than_guessed() {
    let case = certificate();
    for version in ["evp3", "evp9", "EVP1", "evp"] {
        let relabelled = case.proof.replacen("evp2\n", &format!("{version}\n"), 1);
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

/// Which registry each version admits (Section 11.6 "Registries and revisions").
///
/// `p0.scalar.v2` includes every rule of v1 unchanged and in order, so a
/// certificate whose rewrites are all v1 rules is equally a v2 certificate;
/// what changes is which versions may name it. The digest travels with the
/// name, because a certificate naming a registry this checker knows while
/// carrying another's digest would claim rules neither side agreed on.
#[test]
fn evp2_admits_both_registries_and_evp1_admits_only_the_first() {
    const V1: &str =
        "p0.scalar.v1 ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";
    const V2: &str =
        "p0.scalar.v2 c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce";

    let case = certificate();
    assert!(case.proof.contains(V1), "the producer still names v1");

    let revised = case.proof.replacen(V1, V2, 1);
    checker()
        .check(&case.before, &case.after, &revised)
        .expect("evp2 admits the revision");

    let crossed = case
        .proof
        .replacen("evp2\n", "evp1\n", 1)
        .replacen(V1, V2, 1);
    let crossed = crossed
        .lines()
        .filter(|line| !line.starts_with("budget ") && !line.starts_with("epoch "))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    let error = checker()
        .check(&case.before, &case.after, &crossed)
        .expect_err("evp1 admits p0.scalar.v1 only");
    assert!(
        error.contains("refused"),
        "a registry the version does not admit is a refused claim, not malformed input: {error}"
    );

    let forged = case.proof.replacen(
        "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9",
        "c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce",
        1,
    );
    checker()
        .check(&case.before, &case.after, &forged)
        .expect_err("a name and a digest that do not belong together are refused");
}
