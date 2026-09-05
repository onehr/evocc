//! What a checker does with a premise it did not compute.
//!
//! Section 11.3 "Premises": the checker recomputes the fact from the region's
//! nodes with its own implementation of the Section 8.10 transfers and refuses
//! the rewrite unless the recomputed fact is at or below the recorded premise,
//! that is, unless every bit the premise claims is known to the checker with
//! the same value. Revision 1.9.40 corrected the direction, which had been
//! stated so that the strongest premise always passed.
//!
//! Every case here is one field of a real certificate, edited. A hand-written
//! certificate differing in several places would be refused for whichever
//! reason the checker reached first, which would say nothing about premises.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::render_evp1;
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

const REGISTRY_V1: &str =
    "registry p0.scalar.v1 ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";
const REGISTRY_V2: &str =
    "registry p0.scalar.v2 c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce";

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

/// `%low = and(%x, 0x0f)`, `%high = or(%y, 0x0f)`, `%r = and(%low, %high)`,
/// and a fold of two constants beside them.
///
/// The third is `p1_220_and_cover_rhs`: every bit not known zero in `%low` is
/// known one in `%high`. Two premises are recorded, `?m` then `?x`, and neither
/// class is fully known, so both fields of both premises carry real content.
/// The fold reads no fact and records none, which is what the last case here
/// needs.
fn covering_module() -> Module {
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
    let mask = bits(4);
    let low = bits(5);
    let high = bits(6);
    let result = bits(7);
    // A fold beside the covering rewrite, so the certificate carries a
    // premise-free rule as well as a premised one.
    let three = bits(8);
    let four = bits(9);
    let seven = bits(10);

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
            EvoIRType::Bits(8),
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
            insts: vec![
                Inst::Const(mask.clone(), 0x0f),
                Inst::And(low.clone(), x, mask.clone()),
                Inst::Or(high.clone(), y, mask),
                Inst::And(result.clone(), low, high),
                Inst::Const(three.clone(), 3),
                Inst::Const(four.clone(), 4),
                Inst::Add(seven.clone(), three, four),
            ],
            terminator: Inst::Ret(vec![result, memory, world]),
        }],
    };
    let mut module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the covering probe is valid EvoIR");
    module
}

struct Case {
    before: String,
    after: String,
    proof: String,
}

impl Case {
    fn new() -> Self {
        let before = covering_module();
        let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
        let proof = render_evp1(&outcome.certificate);
        assert!(
            proof.contains(" premises "),
            "the covering probe must produce a premised rewrite:\n{proof}"
        );
        Self {
            before: render_module(&before),
            after: render_module(&outcome.module),
            proof,
        }
    }

    /// The one rewrite line that carries premises.
    fn premised_line(&self) -> String {
        let mut lines = self
            .proof
            .lines()
            .filter(|line| line.starts_with("rewrite ") && line.contains(" premises "));
        let line = lines
            .next()
            .unwrap_or_else(|| panic!("no premised rewrite:\n{}", self.proof))
            .to_owned();
        assert!(
            lines.next().is_none(),
            "the probe must carry exactly one premised rewrite:\n{}",
            self.proof
        );
        line
    }

    fn premises(&self) -> Vec<String> {
        let line = self.premised_line();
        let (_, tail) = line
            .split_once(" premises ")
            .expect("the line was selected for its premises");
        tail.split(',').map(str::to_owned).collect()
    }

    /// Replace the premise list with `replacement`, changing nothing else.
    fn with_premises(&self, replacement: &str) -> String {
        let line = self.premised_line();
        let (head, tail) = line
            .split_once(" premises ")
            .expect("the line was selected for its premises");
        let _ = tail;
        self.proof
            .replacen(&line, &format!("{head} premises {replacement}"), 1)
    }

    fn accepts(&self, proof: &str, why: &str) {
        checker()
            .check(&self.before, &self.after, proof)
            .unwrap_or_else(|error| panic!("{why}: {error}\n{proof}"));
    }

    fn refuses(&self, proof: &str, why: &str) {
        let error = checker()
            .check(&self.before, &self.after, proof)
            .expect_err(&format!("{why}\n{proof}"));
        assert!(
            error.contains("refused"),
            "{why}: expected a refused claim (status 3), got {error}"
        );
    }
}

/// The premises the producer records are the facts the checker recomputes, so
/// its own certificate replays. Without this the two cases below could both
/// pass on a checker that refused everything.
#[test]
fn the_producers_own_premises_are_confirmed() {
    let case = Case::new();
    let premises = case.premises();
    println!("premises={}", premises.join(","));
    assert_eq!(premises.len(), 2, "covers reads ?m then ?x");
    // `or(%y, 0x0f)` knows its low nibble one; `and(%x, 0x0f)` knows its high
    // nibble zero. Both are rows of the Section 8.10 table.
    assert!(premises[0].ends_with(":0f:0f"), "?m: {}", premises[0]);
    assert!(premises[1].ends_with(":f0:00"), "?x: {}", premises[1]);
    case.accepts(&case.proof, "the producer's own certificate verifies");
}

/// A premise the checker cannot confirm is refused, whichever field carries it.
///
/// Claiming a bit is known when the recomputed fact leaves it unknown, and
/// claiming a known bit has the other value, are the two ways a premise can
/// exceed the fact. Both must be refused: an optimizer that could assert a fact
/// the checker does not derive would be trusted, which is the whole thing the
/// premise record exists to prevent.
#[test]
fn a_premise_the_checker_cannot_confirm_is_refused() {
    let case = Case::new();
    let premises = case.premises();
    let mask_class = premises[0]
        .split(':')
        .next()
        .expect("a premise is class:mask:value")
        .to_owned();
    let value_class = premises[1]
        .split(':')
        .next()
        .expect("a premise is class:mask:value")
        .to_owned();

    case.refuses(
        &case.with_premises(&format!("{mask_class}:ff:0f,{}", premises[1])),
        "claiming the covering mask's high nibble known must be refused",
    );
    case.refuses(
        &case.with_premises(&format!("{},{value_class}:f0:f0", premises[0])),
        "claiming the masked value's high nibble known one must be refused",
    );
}

/// A premise weaker than the fact is confirmed, and the side condition is still
/// decided by the recomputed fact.
///
/// The second case claims nothing at all about `?m`. Judged from the premise,
/// `covers(?m, ?x)` would fail; judged from the recomputed fact it holds, and
/// Section 11.3 makes the recomputation the authority. A checker that read the
/// side condition off the premise would refuse this.
#[test]
fn a_premise_weaker_than_the_fact_is_confirmed() {
    let case = Case::new();
    let premises = case.premises();
    let mask_class = premises[0]
        .split(':')
        .next()
        .expect("a premise is class:mask:value")
        .to_owned();
    let value_class = premises[1]
        .split(':')
        .next()
        .expect("a premise is class:mask:value")
        .to_owned();

    case.accepts(
        &case.with_premises(&format!("{},{value_class}:e0:00", premises[0])),
        "a premise claiming fewer known bits than the fact holds",
    );
    case.accepts(
        &case.with_premises(&format!("{mask_class}:00:00,{}", premises[1])),
        "the side condition is decided by the recomputed fact, not the premise",
    );
}

/// A certificate that names `p0.scalar.v1` cannot use a rule the revision added.
///
/// Section 11.6: a certificate commits to the registry and cannot extend it. The
/// rule text, the premises and every other field are the producer's own; only
/// the registry line changes.
#[test]
fn a_v1_certificate_may_not_use_a_revised_rule() {
    let case = Case::new();
    assert!(
        case.proof.contains(REGISTRY_V2),
        "the premised certificate names the revision:\n{}",
        case.proof
    );
    case.refuses(
        &case.proof.replacen(REGISTRY_V2, REGISTRY_V1, 1),
        "a v1 certificate carrying a v2 rule must be refused",
    );
}

/// A premise on a rule whose side condition reads no class is invalid.
///
/// Section 11.6 ratified this at revision 1.9.41: a rule records the facts its
/// side condition read, and `all_const` reads its constants from the nodes. The
/// premise added here is one the checker can confirm -- it is the covering
/// mask's own fact -- so the only thing wrong with the certificate is that the
/// rule never makes that claim.
#[test]
fn a_premise_on_a_rule_that_reads_no_fact_is_invalid() {
    let case = Case::new();
    let confirmable = case.premises()[0].clone();
    let line = case
        .proof
        .lines()
        .find(|line| line.starts_with("rewrite ") && !line.contains(" premises "))
        .expect("the probe carries a premise-free rewrite too")
        .to_owned();
    case.refuses(
        &case
            .proof
            .replacen(&line, &format!("{line} premises {confirmable}"), 1),
        "a premise-free rule may not carry a premise",
    );
}

/// A premise whose digit count disagrees with its class's width is invalid.
///
/// The format cannot catch this: Section 11.3 makes only a length disagreement
/// between the two fields and a value bit outside the mask malformed, and a
/// reader of the grammar alone does not know the class's sort. It is therefore
/// the checker that holds the width, and it refuses rather than reinterpreting.
#[test]
fn a_premise_counted_for_another_width_is_invalid() {
    let case = Case::new();
    let premises = case.premises();
    let (class, fact) = premises[1]
        .split_once(':')
        .expect("a premise is class:mask:value");
    assert_eq!(fact, "f0:00", "the masked value knows its high nibble zero");
    // The same fact about the same class, written for a sixteen-bit one.
    case.refuses(
        &case.with_premises(&format!("{},{class}:00f0:0000", premises[0])),
        "a premise counted for another width must be refused",
    );
}
