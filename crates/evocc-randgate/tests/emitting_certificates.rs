//! What an independent checker does with an emitting right-hand side, with a
//! replacement that is not cheaper, and with an extraction that changes
//! nothing.
//!
//! Three claims of Section 11.3 and 11.6 are pinned here.
//!
//! * An emitting right-hand side is reconstructed by the checker from the
//!   pattern and the recorded bindings, so a certificate the producer wrote
//!   from the same rule replays against an implementation that shares no code
//!   with it (11.6 "Emitting right-hand sides").
//! * The checker does not judge cost (11.3, revision 1.9.51). A replacement
//!   that ties the original under the default cost function is accepted;
//!   whether it was an improvement is the producer's policy and is not
//!   observable here.
//! * A region must change something. An extraction that hands back the
//!   original root node is refused with status 3, which is what keeps the
//!   removal of the cost rule from admitting a certificate that proves
//!   nothing.

use evocc_eqopt::{
    optimize_scalar_module_with_evp1, optimize_scalar_module_with_policy, policy_by_name,
    ScalarEqBudget,
};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_proof::render_evp1;
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

const REGISTRY_V3: &str =
    "registry p0.scalar.v3 aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15";
const REGISTRY_V4: &str =
    "registry p0.scalar.v4 1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74";

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

fn module_of(insts: Vec<Inst>, result: Value) -> Module {
    let x = bits(0);
    let memory = Value {
        id: 1,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 2,
        ty: EvoIRType::World,
    };
    let result_ty = result.ty.clone();
    let function = Function {
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
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
        ret_tys: vec![result_ty, EvoIRType::Mem(Region::All), EvoIRType::World],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&x), argument(&memory), argument(&world)],
            insts,
            terminator: Inst::Ret(vec![result, memory, world]),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        aliases: Vec::new(),
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

fn mul_two_rhs() -> Module {
    let two = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(two.clone(), 2),
            Inst::Mul(result.clone(), bits(0), two),
        ],
        result,
    )
}

fn mul_two_lhs() -> Module {
    let two = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(two.clone(), 2),
            Inst::Mul(result.clone(), two, bits(0)),
        ],
        result,
    )
}

fn add_self() -> Module {
    let result = bits(3);
    module_of(vec![Inst::Add(result.clone(), bits(0), bits(0))], result)
}

fn shl_one() -> Module {
    let one = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(one.clone(), 1),
            Inst::Shl(result.clone(), bits(0), one),
        ],
        result,
    )
}

/// `%r = icmp.eq %x, %x`, the smallest region whose replacement is a builder.
fn identity_comparison() -> Module {
    let result = Value {
        id: 3,
        ty: EvoIRType::Bits(1),
    };
    module_of(
        vec![Inst::ICmp(result.clone(), CmpOp::Eq, bits(0), bits(0))],
        result,
    )
}

struct Case {
    before: String,
    after: String,
    proof: String,
}

fn under(module: &Module, policy: &str) -> Case {
    let policy = policy_by_name(policy).expect("this producer ships the named policy");
    let outcome = optimize_scalar_module_with_policy(module, &policy);
    Case {
        before: render_module(module),
        after: render_module(&outcome.module),
        proof: render_evp1(&outcome.certificate),
    }
}

impl Case {
    fn accepts(&self, proof: &str, why: &str) {
        checker()
            .check(&self.before, &self.after, proof)
            .unwrap_or_else(|error| panic!("{why}: {error}\n{proof}"));
    }

    fn refuses(&self, after: &str, proof: &str, why: &str) {
        let error = checker()
            .check(&self.before, after, proof)
            .expect_err(&format!("{why}\n{proof}"));
        assert!(
            error.contains("refused"),
            "{why}: expected a refused claim (status 3), got {error}"
        );
    }
}

/// Every emitting rule replays against the independent checker.
#[test]
fn the_checker_reconstructs_every_emitted_right_hand_side() {
    for (rule, module, policy) in [
        ("p2_300_mul_two_rhs", mul_two_rhs(), "weighted-operators"),
        ("p2_301_mul_two_lhs", mul_two_lhs(), "weighted-operators"),
        ("p2_302_add_self_shl", add_self(), "prefer-shift"),
        ("p2_303_shl_one_add", shl_one(), "prefer-add"),
    ] {
        let case = under(&module, policy);
        assert!(
            case.proof.contains(rule),
            "{rule} never fired under {policy}:\n{}",
            case.proof
        );
        assert!(
            case.proof.contains(REGISTRY_V4),
            "an emitting certificate names p0.scalar.v4:\n{}",
            case.proof
        );
        case.accepts(&case.proof, &format!("{rule} replays"));
    }
}

/// A certificate that names a registry not printing the rule it fired is
/// refused, whatever else is right about it.
///
/// Section 11.6: a certificate commits to a registry and cannot extend it, and
/// `p0.scalar.v3` is the last registry that does not print the emitting rules.
#[test]
fn an_emitting_rule_is_refused_under_the_registry_before_it() {
    let case = under(&add_self(), "prefer-shift");
    let downgraded = case.proof.replacen(REGISTRY_V4, REGISTRY_V3, 1);
    assert_ne!(downgraded, case.proof, "the registry line must be present");
    case.refuses(
        &case.after,
        &downgraded,
        "p2_302 is not a rule of p0.scalar.v3",
    );
}

/// A replacement that is not an improvement under the default cost function is
/// accepted.
///
/// Section 11.3 (revision 1.9.51): the checker verifies equivalence,
/// non-vacuity and structure, never improvement. Under the default policy a
/// `shl` and the `add` it replaces both cost one, and the derived constant
/// costs zero, so this certificate ties -- and before the revision the checker
/// refused exactly this.
#[test]
fn a_replacement_that_is_not_cheaper_is_accepted() {
    let case = under(&add_self(), "prefer-shift");
    assert!(
        case.proof.contains("p2_302_add_self_shl"),
        "the probe must emit a shift:\n{}",
        case.proof
    );
    // The default cost of the replacement, recomputed here rather than
    // asserted: one emitted `shl`, a seed and a constant that both cost zero.
    let emitted = case
        .proof
        .lines()
        .filter(|line| line.starts_with("node ") && line.contains("source=derived"))
        .filter(|line| !line.contains("op=const"))
        .count();
    assert_eq!(
        emitted, 1,
        "one non-constant node replaces one:\n{}",
        case.proof
    );
    case.accepts(
        &case.proof,
        "a tie under the default cost is producer policy, not a checker verdict",
    );
}

/// An extraction that hands back the original root node is refused.
///
/// Section 11.3: a region must change something. With the cost rule gone this
/// is the whole of what stops a certificate that proves an equality and then
/// extracts the value it started from, which would let a producer claim a
/// region it never optimized.
#[test]
fn an_extraction_that_returns_the_original_root_is_refused() {
    let module = identity_comparison();
    let outcome = optimize_scalar_module_with_evp1(&module, ScalarEqBudget::new(20_000, 256));
    let case = Case {
        before: render_module(&module),
        after: render_module(&outcome.module),
        proof: render_evp1(&outcome.certificate),
    };
    case.accepts(&case.proof, "the producer's own certificate verifies");

    // The identity comparison's own before node, and the class the region ends
    // in. Selecting that node instead of the constant the rule built is the
    // vacuous extraction.
    let root_node = case
        .proof
        .lines()
        .find(|line| line.starts_with("node ") && line.contains("op=icmp.eq"))
        .expect("the probe's root is an equality comparison")
        .split_whitespace()
        .nth(2)
        .expect("a node record names its identity")
        .to_owned();
    let root_class = case
        .proof
        .lines()
        .find(|line| line.starts_with("root "))
        .expect("a region block ends with its root class")
        .split_whitespace()
        .nth(1)
        .expect("the root line names a class")
        .to_owned();
    let seed_class = case
        .proof
        .lines()
        .find(|line| line.starts_with("seed 0 "))
        .expect("the probe has one seed")
        .split_whitespace()
        .nth(5)
        .expect("a seed record names its class")
        .to_owned();
    let extraction = case
        .proof
        .lines()
        .find(|line| line.starts_with("extract "))
        .expect("a region block extracts something")
        .to_owned();
    let vacuous = case.proof.replacen(
        &extraction,
        &format!("extract 0 {seed_class} seed:0\nextract 1 {root_class} node:{root_node}"),
        1,
    );
    assert_ne!(vacuous, case.proof, "the extraction line must be present");
    // Reassembling that extraction emits the instruction the region started
    // with, so the after-module is the canonical rendering of the input.
    let unchanged = render_module(&module);
    case.refuses(
        &unchanged,
        &vacuous,
        "extracting the original root node proves no optimization",
    );
}
