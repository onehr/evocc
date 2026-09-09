//! Congruence closure, the one way a partition changes without a rule firing.
//!
//! Section 7.4: equality is a congruence relation, so two nodes with the same
//! operation whose children are pairwise equal are themselves equal. Section
//! 11.3 gives that its own event, because "a format carrying only rewrites
//! cannot reproduce the partition": a reader replaying the event stream has to
//! reach the same classes the producer did, and a merge nobody recorded would
//! leave it somewhere else.
//!
//! Two shapes reach it. Two `const` nodes with the same payload are congruent
//! the moment they are inserted, which no merge is needed for; two operation
//! nodes become congruent only once a rewrite has put their children in the
//! same class. Both are here, because they exercise different halves of the
//! producer's closure and different halves of the checker's verification.

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::{render_evp1, EvpCongruence, EvpEvent, EvpRegion};
use evocc_randgate::ProofChecker;
use evocc_types::{EvoIRType, Region};

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

/// `@main(%x, %y, %m, %w)` returning one `bits<8>`.
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
                argument(&bits(0)),
                argument(&bits(1)),
                argument(&memory),
                argument(&world),
            ],
            insts,
            terminator: Inst::Ret(vec![root, memory, world]),
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

struct Case {
    before: String,
    after: String,
    proof: String,
    region: EvpRegion,
}

impl Case {
    fn new(insts: Vec<Inst>, root: Value) -> Self {
        let before = probe(insts, root);
        let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::new(20_000, 256));
        let proof = render_evp1(&outcome.certificate);
        let region = outcome
            .certificate
            .regions
            .last()
            .cloned()
            .unwrap_or_else(|| panic!("no region was admitted:\n{proof}"));
        Self {
            before: render_module(&before),
            after: render_module(&outcome.module),
            proof,
            region,
        }
    }

    fn congruences(&self) -> Vec<EvpCongruence> {
        self.region
            .events
            .iter()
            .filter_map(|event| match event {
                EvpEvent::Congruence(congruence) => Some(congruence.clone()),
                _ => None,
            })
            .collect()
    }

    /// Where a congruence sits relative to the epoch markers, counting markers
    /// seen before it. Zero means it was recorded before any epoch ran.
    fn markers_before_first_congruence(&self) -> Option<u64> {
        let mut seen = 0;
        for event in &self.region.events {
            match event {
                EvpEvent::Epoch(marker) => seen = *marker,
                EvpEvent::Congruence(_) => return Some(seen),
                _ => {}
            }
        }
        None
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

/// ```text
/// %z0 = const.bits<8> 0
/// %z1 = const.bits<8> 0
/// %a  = or %x, %z0      -- p0_007, aliased to %x
/// %r  = and %a, %z1     -- p0_005, replaced by zero
/// ```
///
/// `%z0` and `%z1` are congruent as inserted: same operator, same sort, same
/// attribute, no children. Nothing merges them but the closure rule, and until
/// it does they are two classes holding the same constant -- the shape the
/// first campaign's refusals were reduced to.
fn equal_constants() -> Case {
    let zero = bits(4);
    let other_zero = bits(5);
    let widened = bits(6);
    let result = bits(7);
    Case::new(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Const(other_zero.clone(), 0),
            Inst::Or(widened.clone(), bits(0), zero),
            Inst::And(result.clone(), widened, other_zero),
        ],
        result,
    )
}

#[test]
fn equal_constants_are_one_class_before_any_epoch() {
    let case = equal_constants();
    let congruences = case.congruences();
    assert_eq!(
        congruences.len(),
        1,
        "the two zeroes are congruent and nothing else is:\n{}",
        case.proof
    );
    let congruence = &congruences[0];
    assert_eq!(
        (congruence.lhs_node, congruence.rhs_node),
        (0, 1),
        "the congruence names the two constant nodes"
    );
    assert_eq!(
        congruence.result,
        congruence.lhs_pre.min(congruence.rhs_pre),
        "a union's result is the smaller of its two canonical roots"
    );
    // Section 11.3: an `epoch k` line precedes the events that epoch produced.
    // No epoch produced this one; the two nodes were congruent as inserted.
    assert_eq!(
        case.markers_before_first_congruence(),
        Some(0),
        "a congruence that holds at insertion precedes the first marker:\n{}",
        case.proof
    );
    case.accepts(&case.proof, "the producer's own congruence replays");
}

/// ```text
/// %z = const.bits<8> 0
/// %a = add %x, %z    -- p0_001, aliased to %x, which merges %a into %x
/// %p = and %a, %y
/// %q = and %x, %y    -- congruent with %p only after that merge
/// %t = and %p, %q
/// %r = mul %t, %z    -- p1_205, known zero whatever %t is
/// ```
fn merged_operands() -> Case {
    let zero = bits(4);
    let shifted = bits(5);
    let left = bits(6);
    let right = bits(7);
    let joined = bits(8);
    let result = bits(9);
    Case::new(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Add(shifted.clone(), bits(0), zero.clone()),
            Inst::And(left.clone(), shifted, bits(1)),
            Inst::And(right.clone(), bits(0), bits(1)),
            Inst::And(joined.clone(), left, right),
            Inst::Mul(result.clone(), joined, zero),
        ],
        result,
    )
}

#[test]
fn two_operations_become_congruent_once_their_children_merge() {
    let case = merged_operands();
    let congruences = case.congruences();
    assert_eq!(
        congruences.len(),
        1,
        "the two conjunctions become congruent and nothing else does:\n{}",
        case.proof
    );
    let congruence = &congruences[0];
    assert_eq!(
        (congruence.lhs_node, congruence.rhs_node),
        (2, 3),
        "the congruence names the two conjunctions"
    );
    // This one was produced by the merge the first epoch's rewrite made, so it
    // follows that epoch's marker rather than preceding every marker.
    assert_eq!(
        case.markers_before_first_congruence(),
        Some(1),
        "a congruence a merge enabled belongs to the epoch that merged:\n{}",
        case.proof
    );
    case.accepts(&case.proof, "the producer's own congruence replays");
}

/// A congruence whose two nodes are not congruent is refused.
///
/// Section 11.3: the two nodes "must have identical operator, sort, effect
/// class, attributes and arity, and every pair of corresponding child classes
/// must already have the same canonical root before the event". Pointing the
/// event at the conjunction and its operand keeps every other field of the
/// certificate intact and violates exactly that.
#[test]
fn a_congruence_between_nodes_that_are_not_congruent_is_refused() {
    let case = merged_operands();
    let line = case
        .proof
        .lines()
        .find(|line| line.starts_with("congruence "))
        .expect("the probe carries a congruence")
        .to_owned();
    let edited = line.replace(" n2 n3", " n2 n4");
    assert_ne!(edited, line, "the edit must change the node pair: {line}");
    case.refuses(
        &case.proof.replacen(&line, &edited, 1),
        "a congruence between nodes of different arity must be refused",
    );
}

/// A premise may name any class of the merged set, because Section 11.6
/// resolves premise classes through the partition as it resolves bindings.
///
/// ```text
/// %z0 = const.bits<8> 0
/// %z1 = const.bits<8> 0
/// %a  = or %x, %z0      -- p0_007, aliased to %x
/// %r  = xor %a, %z1     -- p1_224, whose premise is the class of %z1
/// ```
///
/// The congruence has already made `%z0` and `%z1` one class, so the premise's
/// canonical class and the class the node's child names are different
/// identifiers for it. Both must be confirmed.
#[test]
fn a_premise_class_is_resolved_through_the_partition() {
    let zero = bits(4);
    let other_zero = bits(5);
    let widened = bits(6);
    let result = bits(7);
    let case = Case::new(
        vec![
            Inst::Const(zero.clone(), 0),
            Inst::Const(other_zero.clone(), 0),
            Inst::Or(widened.clone(), bits(0), zero),
            Inst::Xor(result.clone(), widened, other_zero),
        ],
        result,
    );
    let line = case
        .proof
        .lines()
        .find(|line| line.contains("p1_224_xor_known_zero_rhs"))
        .expect("the probe reaches the known-zero identity")
        .to_owned();
    let (head, premises) = line
        .split_once(" premises ")
        .expect("a fact-driven rewrite records its premise");
    let (canonical, fact) = premises
        .split_once(':')
        .expect("a premise is class:mask:value");
    assert_eq!(canonical, "c1", "the canonical class of the two zeroes");
    // `c2` is the second zero's own class, merged into `c1` by the congruence.
    let edited = format!("{head} premises c2:{fact}");
    case.accepts(
        &case.proof.replacen(&line, &edited, 1),
        "a premise naming a class of the merged set is the same claim",
    );
}
