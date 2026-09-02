//! The six properties a checker verifies about an `evp2` budget line.
//!
//! Section 11.3 "Budget", as fixed by revision 1.9.36: every `quantity` is
//! written consumed `/` bound; the `nodes` and `epochs` bounds are the profile
//! constants of Section 8.11; `fuel` carries the `n` its bound derives from;
//! and a stop reason of `epochs`, `nodes` or `fuel` claims that quantity
//! reached its bound. `saturated` and `no-new-best` are telemetry a checker
//! cannot verify without re-running the scheduler, so it does not try.
//!
//! Each property gets a failing form the checker must refuse with status 3 and
//! a holding form it must accept, both built by editing one field of a real
//! certificate. Editing one field is what makes a test about that field: a
//! hand-written certificate differing in many places would be refused for
//! whichever reason the checker happened to reach first.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    Inst, Module, Value,
};
use evocc_proof::render_evp1;
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

/// Section 8.11: the P0 profile's ceilings, which a budget line must record.
const MAX_NODES: u64 = 100_000;
const MAX_EPOCHS: u64 = 1_024;

fn checker() -> ProofChecker {
    ProofChecker::locate().expect("build the workspace so evo-proofcheck is present")
}

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

/// A module with several foldable regions, so the certificate carries real
/// counters and more than one region block to place epoch markers in.
fn module() -> Module {
    let two = value(0, EvoIRType::Bits(8));
    let three = value(1, EvoIRType::Bits(8));
    let sum = value(2, EvoIRType::Bits(8));
    let product = value(3, EvoIRType::Bits(8));
    let same = value(4, EvoIRType::Bits(1));
    let memory = value(5, EvoIRType::Mem(Region::All));
    let world = value(6, EvoIRType::World);

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
                Inst::Const(three.clone(), 3),
                Inst::Add(sum.clone(), two, three.clone()),
                Inst::Mul(product.clone(), sum, three),
                Inst::ICmp(same.clone(), CmpOp::Eq, product.clone(), product),
            ],
            terminator: Inst::Ret(vec![same, memory, world]),
        }],
    };

    let mut module = Module {
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module
}

struct Case {
    before: String,
    after: String,
    proof: String,
}

impl Case {
    fn new() -> Self {
        let before = module();
        validate_module(&before).expect("the module is valid EvoIR");
        let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
        assert!(
            outcome.certificate.regions.len() > 1,
            "the module must produce several regions, or the markers are untested"
        );
        Self {
            before: render_module(&before),
            after: render_module(&outcome.module),
            proof: render_evp1(&outcome.certificate),
        }
    }

    fn budget_line(&self) -> String {
        self.proof
            .lines()
            .find(|line| line.starts_with("budget "))
            .expect("an evp2 certificate carries a budget line")
            .to_owned()
    }

    /// One `key=value` field of the budget line.
    fn field(&self, key: &str) -> String {
        self.budget_line()
            .split(' ')
            .find_map(|field| field.strip_prefix(&format!("{key}=")).map(str::to_owned))
            .unwrap_or_else(|| panic!("the budget line has no {key} field"))
    }

    fn consumed(&self, key: &str) -> u64 {
        self.field(key)
            .split('/')
            .next()
            .expect("a quantity is consumed/bound")
            .parse()
            .expect("consumed is an integer")
    }

    fn with_budget(&self, replacement: &str) -> String {
        self.proof.replacen(&self.budget_line(), replacement, 1)
    }

    fn with_field(&self, key: &str, replacement: &str) -> String {
        let old = format!("{key}={}", self.field(key));
        let line = self
            .budget_line()
            .replacen(&old, &format!("{key}={replacement}"), 1);
        self.with_budget(&line)
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

#[test]
fn the_producer_emits_evp2_with_a_budget_line_and_epoch_markers() {
    let case = Case::new();
    assert!(
        case.proof.starts_with("evp2\n"),
        "the producer emits evp2: {}",
        case.proof.lines().next().unwrap_or_default()
    );
    assert!(
        case.proof.lines().any(|line| line.starts_with("epoch ")),
        "an evp2 certificate carries epoch markers:\n{}",
        case.proof
    );
    assert!(
        !case.proof.contains(" premises "),
        "no Tier B rule exists yet, so no rewrite carries a premise"
    );

    // Counters, not placeholders. A record of all zeroes would satisfy every
    // check below while describing no transaction at all.
    println!("budgetLine={}", case.budget_line());
    assert!(case.consumed("nodes") > 0, "{}", case.budget_line());
    assert!(case.consumed("epochs") > 0, "{}", case.budget_line());
    assert!(case.consumed("fuel") > 0, "{}", case.budget_line());
    assert!(
        case.field("n").parse::<u64>().expect("n is an integer") > 0,
        "{}",
        case.budget_line()
    );

    case.accepts(&case.proof, "the producer's own certificate verifies");
}

#[test]
fn the_profile_node_bound_is_verified() {
    let case = Case::new();
    let consumed = case.consumed("nodes");
    case.accepts(
        &case.with_field("nodes", &format!("{consumed}/{MAX_NODES}")),
        "the profile node ceiling holds",
    );
    case.refuses(
        &case.with_field("nodes", &format!("{consumed}/{}", MAX_NODES - 1)),
        "a node bound that is not the profile constant must be refused",
    );
}

#[test]
fn the_profile_epoch_bound_is_verified() {
    let case = Case::new();
    let consumed = case.consumed("epochs");
    case.accepts(
        &case.with_field("epochs", &format!("{consumed}/{MAX_EPOCHS}")),
        "the profile epoch ceiling holds",
    );
    case.refuses(
        &case.with_field("epochs", &format!("{consumed}/{}", MAX_EPOCHS + 1)),
        "an epoch bound that is not the profile constant must be refused",
    );
}

#[test]
fn a_consumed_quantity_may_not_exceed_its_bound() {
    let case = Case::new();
    case.accepts(&case.proof, "the produced consumption is within its bounds");
    case.refuses(
        &case.with_field("nodes", &format!("{}/{MAX_NODES}", MAX_NODES + 1)),
        "consuming more nodes than the bound must be refused",
    );
    case.refuses(
        &case.with_field("epochs", &format!("{}/{MAX_EPOCHS}", MAX_EPOCHS + 1)),
        "consuming more epochs than the bound must be refused",
    );
}

#[test]
fn the_fuel_bound_is_derived_from_the_recorded_node_count() {
    let case = Case::new();
    let consumed = case.consumed("fuel");
    let declared = format!(" n={}", case.field("n"));

    // max(256, 8 * n): at n = 100 the derived bound is 800, and 256 is the
    // floor that applies for every n below thirty-two.
    let proof = case
        .with_field("fuel", &format!("{consumed}/800"))
        .replacen(&declared, " n=100", 1);
    case.accepts(
        &proof,
        "a fuel bound of max(256, 8 * 100) holds for n = 100",
    );

    let proof = case
        .with_field("fuel", &format!("{consumed}/256"))
        .replacen(&declared, " n=100", 1);
    case.refuses(
        &proof,
        "a fuel bound that is not max(256, 8 * n) must be refused",
    );
}

#[test]
fn an_epoch_marker_may_not_exceed_the_consumed_epoch_count() {
    let case = Case::new();
    let consumed = case.consumed("epochs");
    case.accepts(&case.proof, "every marker is within the consumed count");

    let marker = case
        .proof
        .lines()
        .find(|line| line.starts_with("epoch "))
        .expect("an evp2 certificate carries epoch markers")
        .to_owned();
    let beyond = format!("epoch {}", consumed + 1);
    case.refuses(
        &case.proof.replacen(&marker, &beyond, 1),
        "a marker for an epoch that never ran must be refused",
    );
}

/// Each of the three exhaustion reasons, in a form that holds and one that does
/// not.
///
/// The certificates are hand-edited rather than produced. No input this
/// producer can be given reaches a profile ceiling: the node and epoch bounds
/// are fixed at a hundred thousand and 1024, and the fuel bound is
/// `max(256, 8n)`, so one rule would have to fire 256 times inside a single
/// region's single epoch. What is testable is therefore the checking half, and
/// that is what these are.
#[test]
fn an_exhaustion_stop_must_coincide_with_its_bound() {
    let case = Case::new();
    let epochs = case.consumed("epochs");
    let fuel = case.consumed("fuel");
    let nodes = case.consumed("nodes");

    for (holds, refused, why) in [
        (
            format!("budget nodes={MAX_NODES}/{MAX_NODES} epochs={epochs}/{MAX_EPOCHS} fuel={fuel}/256 n=0 stop=nodes"),
            format!("budget nodes=1/{MAX_NODES} epochs={epochs}/{MAX_EPOCHS} fuel={fuel}/256 n=0 stop=nodes"),
            "nodes",
        ),
        (
            format!("budget nodes={nodes}/{MAX_NODES} epochs={MAX_EPOCHS}/{MAX_EPOCHS} fuel={fuel}/256 n=0 stop=epochs"),
            format!("budget nodes={nodes}/{MAX_NODES} epochs={epochs}/{MAX_EPOCHS} fuel={fuel}/256 n=0 stop=epochs"),
            "epochs",
        ),
        (
            format!("budget nodes={nodes}/{MAX_NODES} epochs={epochs}/{MAX_EPOCHS} fuel=256/256 n=0 stop=fuel"),
            format!("budget nodes={nodes}/{MAX_NODES} epochs={epochs}/{MAX_EPOCHS} fuel={fuel}/256 n=0 stop=fuel"),
            "fuel",
        ),
    ] {
        case.accepts(
            &case.with_budget(&holds),
            &format!("stop={why} holds when that quantity reached its bound"),
        );
        case.refuses(
            &case.with_budget(&refused),
            &format!("stop={why} must be refused when that quantity is below its bound"),
        );
    }
}

#[test]
fn telemetry_stop_reasons_are_not_verified() {
    let case = Case::new();
    let epochs = case.consumed("epochs");
    let fuel = case.consumed("fuel");
    for reason in ["saturated", "no-new-best"] {
        let line = format!(
            "budget nodes=1/{MAX_NODES} epochs={epochs}/{MAX_EPOCHS} fuel={fuel}/256 n=0 stop={reason}"
        );
        case.accepts(
            &case.with_budget(&line),
            "a telemetry stop reason is recorded and not checked",
        );
    }
}

/// The previous version still reads exactly as it did.
///
/// Dropping the budget line and the epoch markers, and relabelling the header,
/// is precisely the difference Section 11.3 "Versions" describes, so the result
/// is the evp1 certificate this producer would have written before.
#[test]
fn an_evp1_certificate_is_still_accepted() {
    let case = Case::new();
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
    assert!(
        downgraded.starts_with("evp1\n"),
        "the downgrade produced: {downgraded}"
    );
    case.accepts(&downgraded, "evp1 is still read and verified");
}

/// A rewrite whose match needs an earlier merge belongs to a later epoch.
///
/// `or(and(2, 1), 1)` cannot fold until `and(2, 1)` has folded to zero, so the
/// two rewrites cannot share an epoch: Section 8.11's tiers exist because the
/// facts one epoch sharpens are what the next one matches on. The markers are
/// therefore expected to increase *inside* one region block, not merely between
/// blocks, and the certificate must still verify.
#[test]
fn a_region_whose_rewrites_depend_on_each_other_spans_epochs() {
    let case = Case::new();
    let mut deepest = 0_u64;
    for block in case.proof.split("\nregion ") {
        let markers = block
            .lines()
            .filter_map(|line| line.strip_prefix("epoch "))
            .map(|epoch| epoch.parse::<u64>().expect("an epoch marker is an integer"))
            .collect::<Vec<_>>();
        let mut sorted = markers.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(markers, sorted, "markers must strictly increase in a block");
        deepest = deepest.max(markers.len() as u64);
    }
    assert!(
        deepest > 1,
        "no region block carried more than one epoch marker:\n{}",
        case.proof
    );
    assert!(
        case.consumed("epochs") > 1,
        "a transaction that ran several epochs must record them: {}",
        case.budget_line()
    );
    case.accepts(&case.proof, "a multi-epoch certificate verifies");
}

/// Fuel is per rule per epoch, so the recorded consumption is the largest
/// number of firings one rule made in one epoch and never the total.
#[test]
fn recorded_fuel_is_the_busiest_rule_in_one_epoch() {
    let case = Case::new();
    let mut rewrites = 0_u64;
    for line in case.proof.lines() {
        if line.starts_with("rewrite ") {
            rewrites += 1;
        }
    }
    assert!(
        case.consumed("fuel") <= rewrites,
        "fuel {} exceeds the {rewrites} rewrites the certificate records: {}",
        case.consumed("fuel"),
        case.budget_line()
    );
    assert!(case.consumed("fuel") > 0, "{}", case.budget_line());
}

/// A producer that stops at its own ceiling keeps the work it had done.
///
/// Section 11.3, revision 1.9.38: "Hitting any ceiling keeps every region
/// admitted before the stop (8.11): a producer that answers its own budget
/// exhaustion with a zero-region certificate is non-conforming, because it
/// discards work the checker would have verified." The box the certificate
/// records stays the profile's; only the reason changes, because a producer's
/// own budget is not a profile ceiling.
#[test]
fn a_budget_stop_keeps_the_regions_admitted_before_it() {
    let before = module();
    validate_module(&before).expect("the module is valid EvoIR");

    let generous = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
    let admitted = generous.certificate.regions.len();
    assert!(
        admitted > 1,
        "the input must admit several regions, or a partial stop proves nothing"
    );

    // One epoch of the producer's own allowance: enough to admit something,
    // not enough to admit everything.
    let stopped = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 1));
    let kept = stopped.certificate.regions.len();
    println!("regionsAdmitted={admitted}");
    println!("regionsKeptAfterBudgetStop={kept}");
    assert!(
        kept > 0,
        "a budget stop discarded every region it had already admitted"
    );
    assert!(
        kept < admitted,
        "the tight budget did not actually stop the producer early"
    );

    let budget = stopped
        .certificate
        .budget
        .expect("an evp2 certificate carries a budget line");
    assert_eq!(
        budget.stop,
        evocc_proof::EvpStop::Policy,
        "a producer's own budget is not a profile ceiling"
    );
    assert_eq!(budget.nodes.bound, MAX_NODES, "the box stays the profile's");
    assert_eq!(
        budget.epochs.bound, MAX_EPOCHS,
        "the box stays the profile's"
    );

    let before_text = render_module(&before);
    let after_text = render_module(&stopped.module);
    let proof_text = render_evp1(&stopped.certificate);
    validate_module(&stopped.module).expect("a partial optimization is still valid EvoIR");
    checker()
        .check(&before_text, &after_text, &proof_text)
        .unwrap_or_else(|error| panic!("a partial certificate must verify: {error}\n{proof_text}"));
}
