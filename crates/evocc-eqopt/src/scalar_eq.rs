//! Day-7 pure-scalar EvoIR to e-graph bridge.

use std::collections::{BTreeMap, HashMap};

use evocc_egraph::{
    EClassId, Egraph, MergeOutcome, OpId, Pattern, PatternChild, ProofCertificate, ProofContext,
    RuleSnapshot,
};
use evocc_types::EvoIRType;

use evocc_evoir::{inst_results, CmpOp, Inst, Module, Value};

use crate::eligibility::Admission;
use crate::known_bits::{Comparison, KnownBits};
use crate::seed_facts::SeedFacts;

pub(crate) use evocc_evoir::{remap_inst_uses, resolve_alias};

const OP_ADD: OpId = 1;
const OP_SUB: OpId = 2;
const OP_MUL: OpId = 3;
const OP_AND: OpId = 4;
const OP_OR: OpId = 5;
const OP_XOR: OpId = 6;
const OP_ICMP_EQ: OpId = 10;
const OP_ICMP_NE: OpId = 11;
const OP_ICMP_SLT: OpId = 12;
const OP_ICMP_SLE: OpId = 13;
const OP_ICMP_ULT: OpId = 14;
const OP_ICMP_ULE: OpId = 15;
const OP_SHL: OpId = 20;
const OP_LSHR: OpId = 21;
const OP_ASHR: OpId = 22;
const OP_ZEXT: OpId = 23;
const OP_SEXT: OpId = 24;
const OP_TRUNC: OpId = 25;
const DYNAMIC_OP_BASE: OpId = 1_000;

/// Public Inst-to-operator authority for the Day-7 P0 profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarOperator {
    Const,
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    ICmp(CmpOp),
    /// Admitted to the Section 8.5 allowlist at revision 1.9.44. None of them
    /// may trap; a shift's definedness guard is a separate predicate and stays
    /// excluded, and a width change consumes one sort and produces another.
    Shl,
    LShr,
    AShr,
    ZExt,
    SExt,
    Trunc,
}

/// Why an instruction cannot enter the Day-7 pure scalar e-graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarBridgeError {
    code: &'static str,
    message: &'static str,
}

impl ScalarBridgeError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn message(&self) -> &'static str {
        self.message
    }
}

/// Map an EvoIR instruction to the P0 operator table, rejecting effects at the
/// bridge boundary rather than silently dropping them.
pub fn ingest_scalar_inst(inst: &Inst) -> Result<ScalarOperator, ScalarBridgeError> {
    let operator = match inst {
        Inst::Const(..) => ScalarOperator::Const,
        Inst::Add(..) => ScalarOperator::Add,
        Inst::Sub(..) => ScalarOperator::Sub,
        Inst::Mul(..) => ScalarOperator::Mul,
        Inst::And(..) => ScalarOperator::And,
        Inst::Or(..) => ScalarOperator::Or,
        Inst::Xor(..) => ScalarOperator::Xor,
        Inst::ICmp(_, comparison, ..) => ScalarOperator::ICmp(*comparison),
        Inst::Shl(..) => ScalarOperator::Shl,
        Inst::LShr(..) => ScalarOperator::LShr,
        Inst::AShr(..) => ScalarOperator::AShr,
        Inst::ZExt(..) => ScalarOperator::ZExt,
        Inst::SExt(..) => ScalarOperator::SExt,
        Inst::Trunc(..) => ScalarOperator::Trunc,
        Inst::Alloca { .. }
        | Inst::LifetimeEnd { .. }
        | Inst::PointerValid { .. }
        | Inst::AccessInBounds { .. }
        | Inst::Load { .. }
        | Inst::Store { .. }
        | Inst::Copy { .. }
        | Inst::Call { .. }
        | Inst::IndirectCall { .. } => {
            return Err(ScalarBridgeError {
                code: "E4001",
                message: "effect-bearing-instruction-is-not-scalar-egraph-eligible",
            });
        }
        _ => {
            return Err(ScalarBridgeError {
                code: "E4002",
                message: "instruction-is-not-total-day7-p0-scalar",
            });
        }
    };
    Ok(operator)
}

/// Hard resource ceilings. Exhaustion is a successful fallback to the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalarEqBudget {
    pub max_nodes: usize,
    pub max_epochs: usize,
}

impl ScalarEqBudget {
    #[must_use]
    pub const fn new(max_nodes: usize, max_epochs: usize) -> Self {
        Self {
            max_nodes,
            max_epochs,
        }
    }
}

impl Default for ScalarEqBudget {
    fn default() -> Self {
        Self::new(100_000, 1_024)
    }
}

/// P0 rule identity recorded alongside every independently replayable proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarRewrite {
    AddZero,
    MulOne,
    AndZero,
    OrZero,
    DoubleNegation,
    IdentityComparison,
    ConstantFold,
    /// A class whose every bit the analysis knows, replaced by that constant.
    KnownConst,
    /// `and`/`or` against a mask that covers the other operand's live bits.
    MaskCover,
    /// `xor` against an operand every bit of which is known zero.
    XorKnownZero,
    /// `mul(?x, 2)` in either operand order, rewritten to `add(?x, ?x)`.
    /// The first rule whose right-hand side emits an instruction.
    DoubleByAddition,
    /// `add(?x, ?x)`, rewritten to `shl(?x, 1)`.
    DoubleByShift,
    /// `shl(?x, 1)`, rewritten to `add(?x, ?x)`.
    ShiftedDoubleByAddition,
    /// Not a rule at all: the instruction is an earlier one's structural
    /// duplicate, and Section 7.4 makes the two one value. What justifies the
    /// replacement is the congruence the region records, not a rewrite.
    Congruent,
}

impl ScalarRewrite {
    const fn rule_id(self) -> u32 {
        match self {
            Self::AddZero => 1,
            Self::MulOne => 2,
            Self::AndZero => 3,
            Self::OrZero => 4,
            Self::DoubleNegation => 5,
            Self::IdentityComparison => 6,
            Self::ConstantFold => 7,
            Self::KnownConst => 8,
            Self::MaskCover => 9,
            Self::XorKnownZero => 10,
            Self::Congruent => 11,
            Self::DoubleByAddition => 12,
            Self::DoubleByShift => 13,
            Self::ShiftedDoubleByAddition => 14,
        }
    }
}

/// One real merge and the complete proof stream required to replay it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarMergeReceipt {
    pub rule: ScalarRewrite,
    pub merge: evocc_egraph::MergeReceipt,
    pub certificate: ProofCertificate,
}

/// Measured optimization facts. No predicted denominator appears here.
/// Which authority a certificate was replayed against.
///
/// A replay establishes soundness only when the rules come from an authority the
/// producer could not choose. Recording which of the two actually happened keeps
/// a frozen receipt bound to a measured property rather than to an adjective, so
/// the day the authority changes, the number it reports changes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProofAuthority {
    /// The certificate supplied the rules it was then checked against.
    ///
    /// The replay establishes that the recorded merges follow from the recorded
    /// rules, and nothing about whether those rules are themselves sound. This
    /// still catches an e-graph whose certificate is internally incoherent, which
    /// is why it is worth running, but it is not a proof of the rewrite.
    ///
    /// The scalar bridge is in this state for a concrete reason rather than an
    /// oversight in the check: it manufactures one exact rule per candidate whose
    /// patterns name concrete e-classes instead of variables. Checking such a
    /// rewrite against a general reviewed rule such as `add(?x, 0) -> ?x` needs a
    /// certificate in which a constant carries its width and payload; constants
    /// are currently dynamic operator identities carrying neither, so the general
    /// rule cannot be stated, let alone matched.
    #[default]
    ProducerSupplied,
    /// The rules came from a reviewed registry that a producer cannot extend.
    Reviewed { registry: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScalarEqReport {
    /// The authority every receipt in this report was replayed against.
    pub proof_authority: ProofAuthority,
    pub regions_admitted: usize,
    pub graph_nodes: usize,
    pub epochs: usize,
    pub merges: usize,
    pub optimized_values: usize,
    /// Congruence events the transaction recorded (Section 7.4), and the
    /// regions whose root is replaced on the strength of one with no rewrite of
    /// its own -- the regions closing congruence is what buys.
    pub congruences: usize,
    pub congruent_roots: usize,
    /// Block evaluations the Section 8.10 seed-fact fixed point took, summed
    /// over every function of the module. It buys the facts a block argument
    /// carries, and it is the only cost this analysis has.
    pub seed_fact_work_items: usize,
    pub budget_exhausted: bool,
    pub proof_replay_failed: bool,
    pub fell_back: bool,
    pub receipts: Vec<ScalarMergeReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarEqOutcome {
    pub module: Module,
    pub report: ScalarEqReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScalarFact {
    Constant { raw: u64, ty: EvoIRType },
}

/// Which value each `sub(0, %v)` negates.
///
/// Kept beside the constant environment rather than inside it because a
/// negation of a constant is both facts at once: `-(-3)` is `3`, and the
/// double-negation rule of the reviewed registry matches on the shape while
/// the fold matches on the value. Recording only the value would make the rule
/// unreachable wherever the inner negation happens to fold, which is exactly
/// where a C front end writes it.
pub(crate) type NegationEnv = HashMap<u32, Value>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Replacement {
    Alias(Value),
    Constant(u64),
    /// Section 11.6 "Emitting right-hand sides": an operator applied to bound
    /// variables and `const_like` terms. The producer emits it as a derived
    /// node the checker reconstructs from the pattern and the bindings, so what
    /// is carried here is the pattern's shape and not a node identity.
    Emit(Emitted),
}

/// The right-hand side of an emitting rule, instantiated under a substitution.
///
/// One operator over operands that are either a bound variable, named by the
/// value it is bound to in this block, or a `const_like` term whose payload the
/// checker recomputes from the pattern. The P0 emitting rules need no deeper
/// nesting than this and the shape says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Emitted {
    pub(crate) operator: ScalarOperator,
    pub(crate) result_ty: EvoIRType,
    pub(crate) operands: Vec<EmittedOperand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmittedOperand {
    /// A variable of the left pattern, bound to this block-local value.
    Bound(Value),
    /// `const_like(?x, K)`: a constant of the bound variable's own sort.
    Constant(u64),
}

#[derive(Debug, Clone)]
pub(crate) struct RewriteOpportunity {
    pub(crate) rule: ScalarRewrite,
    pub(crate) replacement: Replacement,
}

/// One semantically checked rewrite selected by the transient optimizer.
///
/// The persistent layer deliberately receives the original instruction and
/// its complete scalar-fact environment rather than trusting the producer's
/// rule label. It re-derives the opportunity before mutating EqualityState.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedRewrite {
    pub(crate) function: String,
    pub(crate) block: u32,
    pub(crate) result: Value,
    pub(crate) instruction: Inst,
    pub(crate) facts: HashMap<u32, ScalarFact>,
    /// The earlier instruction this one duplicates, for a `Congruent` event.
    /// Carried so the re-derivation can check Section 7.4 for itself instead of
    /// taking the sweep's word that two instructions were the same node.
    pub(crate) twin: Option<Inst>,
    pub(crate) negations: NegationEnv,
    /// What Section 8.10 knew about each value's bits where the rewrite was
    /// selected. Recorded beside `facts` rather than folded into it because the
    /// two answer different questions: one names constants the block wrote,
    /// the other names bits the lattice derived.
    pub(crate) known: KnownEnv,
    pub(crate) rule: ScalarRewrite,
    pub(crate) replacement: Replacement,
}

impl AppliedRewrite {
    pub(crate) fn is_reviewed_instance(&self) -> bool {
        if self.rule == ScalarRewrite::Congruent {
            // Section 7.4: the same operation, the same attributes, and
            // operands that are pairwise the same values.
            let Some(twin) = &self.twin else {
                return false;
            };
            let same_shape = ingest_scalar_inst(&self.instruction).ok()
                == ingest_scalar_inst(twin).ok()
                && scalar_result(&self.instruction).map(|value| &value.ty)
                    == scalar_result(twin).map(|value| &value.ty)
                && scalar_operands(&self.instruction)
                    .into_iter()
                    .map(|operand| operand.id)
                    .eq(scalar_operands(twin).into_iter().map(|operand| operand.id))
                && constant_payload(&self.instruction) == constant_payload(twin);
            let replaces_with_the_twin = scalar_result(twin)
                .is_some_and(|value| self.replacement == Replacement::Alias(value.clone()));
            return same_shape && replaces_with_the_twin;
        }
        let instantiates = |candidate: Option<RewriteOpportunity>| {
            candidate.is_some_and(|opportunity| {
                opportunity.rule == self.rule && opportunity.replacement == self.replacement
            })
        };
        instantiates(rewrite_opportunity(
            &self.instruction,
            &self.facts,
            &self.negations,
        )) || instantiates(constant_fold(&self.instruction, &self.facts))
            || instantiates(fact_driven_opportunity(&self.instruction, &self.known))
            || instantiates(emitting_opportunity(&self.instruction, &self.facts))
    }
}

#[derive(Debug, Clone)]
enum GraphTerm {
    Input(Value),
    Constant {
        raw: u64,
        ty: EvoIRType,
    },
    Operation,
    /// A node an emitting right-hand side built, and the pattern it was built
    /// from. Extraction selects it only when the class holds nothing cheaper,
    /// which is what makes an emitting rule the last resort it is.
    Emitted(Emitted),
}

struct CandidateGraph {
    graph: Egraph,
    terms: BTreeMap<OpId, GraphTerm>,
    values: HashMap<Value, (EClassId, evocc_egraph::NodeId)>,
    constants: HashMap<(EvoIRType, u64), (EClassId, evocc_egraph::NodeId)>,
    next_dynamic_op: OpId,
}

impl CandidateGraph {
    fn new() -> Self {
        Self {
            graph: Egraph::new(),
            terms: BTreeMap::new(),
            values: HashMap::new(),
            constants: HashMap::new(),
            next_dynamic_op: DYNAMIC_OP_BASE,
        }
    }

    fn fresh_dynamic_op(&mut self) -> OpId {
        let op = self.next_dynamic_op;
        self.next_dynamic_op = self
            .next_dynamic_op
            .checked_add(1)
            .expect("one local scalar graph exhausted OpId space");
        op
    }

    fn input(&mut self, value: &Value) -> (EClassId, evocc_egraph::NodeId) {
        if let Some(existing) = self.values.get(value) {
            return *existing;
        }
        let op = self.fresh_dynamic_op();
        self.terms.insert(op, GraphTerm::Input(value.clone()));
        let inserted = self.graph.insert_with_node(op, Vec::new());
        let result = (inserted.eclass, inserted.node);
        self.values.insert(value.clone(), result);
        result
    }

    fn constant(&mut self, raw: u64, ty: &EvoIRType) -> (EClassId, evocc_egraph::NodeId) {
        let raw = mask_raw(raw, ty).unwrap_or(raw);
        let key = (ty.clone(), raw);
        if let Some(existing) = self.constants.get(&key) {
            return *existing;
        }
        let op = self.fresh_dynamic_op();
        self.terms.insert(
            op,
            GraphTerm::Constant {
                raw,
                ty: ty.clone(),
            },
        );
        let inserted = self.graph.insert_with_node(op, Vec::new());
        let result = (inserted.eclass, inserted.node);
        self.constants.insert(key, result);
        result
    }

    fn operand(
        &mut self,
        value: &Value,
        facts: &HashMap<u32, ScalarFact>,
    ) -> (EClassId, evocc_egraph::NodeId) {
        match facts.get(&value.id) {
            Some(ScalarFact::Constant { raw, ty }) => self.constant(*raw, ty),
            _ => self.input(value),
        }
    }
}

/// Optimize all eligible block-local total scalar expressions. Any resource or
/// proof failure returns the original module byte-for-byte.
#[must_use]
pub fn optimize_scalar_module(module: &Module, budget: ScalarEqBudget) -> ScalarEqOutcome {
    optimize_scalar_module_with_events(module, budget).0
}

pub(crate) fn optimize_scalar_module_with_events(
    module: &Module,
    budget: ScalarEqBudget,
) -> (ScalarEqOutcome, Vec<AppliedRewrite>) {
    let original = module.clone();
    let mut optimized = module.clone();
    let mut report = ScalarEqReport::default();
    let mut applied = Vec::new();
    // Section 8.11: hitting a ceiling is a successful stop. Once it is reached
    // the sweep stops admitting and copies the rest of the module through
    // unchanged, so everything already admitted survives; discarding it would
    // throw away work a checker would have verified.
    let mut stopped = false;

    for (index, function) in optimized.functions.iter_mut().enumerate() {
        // Section 8.10: the facts of this function's block arguments, computed
        // once, before any region is formed and before any rewrite is
        // proposed.
        let admission = Admission::of(&original.functions[index]);
        let seed_facts = SeedFacts::of(&original.functions[index], &admission);
        report.seed_fact_work_items += seed_facts.work_items();
        for (block_index, block) in function.blocks.iter_mut().enumerate() {
            let mut aliases = HashMap::new();
            // Section 7.4: two instructions with the same operator whose
            // operands are already equal compute one value, so the second is
            // the first. Recording that here is what lets a rule match over
            // classes rather than over the names the block happened to give
            // them: `icmp.eq %p, %q` on two such conjunctions is
            // `icmp.eq %p, %p`, which the reviewed registry answers, and no
            // amount of comparing operand names reaches it.
            //
            // Constants are left out. Equal payloads are already equal to the
            // constant environment, and aliasing one to another would point an
            // operand at a value a region's witness closure need not contain,
            // while the region's own congruence closure joins their classes
            // anyway.
            let mut congruent: HashMap<(ScalarOperator, EvoIRType, Vec<u32>), (Value, Inst)> =
                HashMap::new();
            let mut facts = HashMap::new();
            let mut negations = NegationEnv::new();
            // A block argument is no longer opaque: it carries what every
            // edge into this block agrees on.
            let mut known = seed_facts.arguments_of(&original.functions[index].blocks[block_index]);
            let mut rewritten = Vec::with_capacity(block.insts.len());
            for original_inst in &block.insts {
                let mut inst = original_inst.clone();
                remap_inst_uses(&mut inst, &aliases);
                if stopped {
                    rewritten.push(inst);
                    continue;
                }
                if matches!(ingest_scalar_inst(&inst), Ok(ScalarOperator::Const)) {
                    record_fact(&inst, &mut facts);
                    record_known(&inst, &mut known);
                    rewritten.push(inst);
                    continue;
                }
                let Ok(operator) = ingest_scalar_inst(&inst) else {
                    // Not a region root, and still a value the Section 8.10
                    // table has a row for since revision 1.9.43.
                    record_known(&inst, &mut known);
                    rewritten.push(inst);
                    continue;
                };
                report.regions_admitted += 1;
                // Section 8.11 runs the tiers in order, so a canonical
                // reduction or a fold is preferred and the fact-driven rules
                // answer only what neither could.
                record_negation(&inst, &facts, &mut negations);
                let opportunity = rewrite_opportunity(&inst, &facts, &negations)
                    .or_else(|| fact_driven_opportunity(&inst, &known))
                    .or_else(|| emitting_opportunity(&inst, &facts));
                let shape = scalar_result(&inst).cloned().map(|result| {
                    (
                        result.clone(),
                        (
                            operator,
                            // Two width changes from one source differ only in
                            // what they produce, so the result's sort is part
                            // of what makes them the same node.
                            result.ty,
                            scalar_operands(&inst)
                                .into_iter()
                                .map(|operand| operand.id)
                                .collect::<Vec<_>>(),
                        ),
                    )
                });
                let mut duplicate_of = None;
                if let Some((result, shape)) = shape {
                    let first = congruent
                        .entry(shape)
                        .or_insert((result.clone(), inst.clone()))
                        .clone();
                    // Only where no rule answers the instruction: a rule that
                    // does states the equality itself, and it is the narrower
                    // claim.
                    if opportunity.is_none() && first.0 != result {
                        duplicate_of = Some(first);
                    }
                }
                if let Some((twin, twin_instruction)) = duplicate_of {
                    let Some(result) = scalar_result(&inst).cloned() else {
                        rewritten.push(inst);
                        continue;
                    };
                    aliases.insert(result.id, twin.clone());
                    record_known(&inst, &mut known);
                    applied.push(AppliedRewrite {
                        function: function.name.clone(),
                        block: block.id,
                        result,
                        instruction: inst.clone(),
                        facts: facts.clone(),
                        twin: Some(twin_instruction),
                        negations: negations.clone(),
                        known: known.clone(),
                        rule: ScalarRewrite::Congruent,
                        replacement: Replacement::Alias(twin),
                    });
                    report.merges += 1;
                    report.optimized_values += 1;
                    rewritten.push(inst);
                    continue;
                }
                let bridge = bridge_candidate(&inst, &facts, opportunity.as_ref());
                report.graph_nodes = report.graph_nodes.saturating_add(bridge.node_count);
                if report.graph_nodes > budget.max_nodes
                    || (opportunity.is_some() && report.epochs >= budget.max_epochs)
                {
                    report.budget_exhausted = true;
                    stopped = true;
                    rewritten.push(inst);
                    continue;
                }
                let Some(receipt) = bridge.receipt else {
                    record_fact(&inst, &mut facts);
                    record_known(&inst, &mut known);
                    rewritten.push(inst);
                    continue;
                };
                // The authority is recorded next to the line that chooses it, so
                // the field cannot drift away from what was actually checked.
                // Passing the certificate's own rules is what makes this a
                // consistency check rather than a proof; see `ProofAuthority`.
                report.proof_authority = ProofAuthority::ProducerSupplied;
                let context = match ProofContext::new(receipt.certificate.rules.clone()) {
                    Ok(context) => context,
                    Err(_) => {
                        report.proof_replay_failed = true;
                        report.fell_back = true;
                        report.receipts.clear();
                        return (
                            ScalarEqOutcome {
                                module: original,
                                report,
                            },
                            Vec::new(),
                        );
                    }
                };
                if context.replay(&receipt.certificate).is_err() {
                    report.proof_replay_failed = true;
                    report.fell_back = true;
                    report.receipts.clear();
                    return (
                        ScalarEqOutcome {
                            module: original,
                            report,
                        },
                        Vec::new(),
                    );
                }
                let Some(result) = scalar_result(&inst).cloned() else {
                    rewritten.push(inst);
                    continue;
                };
                let mut emitting = false;
                let actual_replacement = match bridge.extracted {
                    Some(Replacement::Alias(value)) => {
                        let replacement = resolve_alias(&value, &aliases);
                        aliases.insert(result.id, replacement.clone());
                        Replacement::Alias(replacement)
                    }
                    Some(Replacement::Constant(raw)) => {
                        let replacement = Inst::Const(result.clone(), raw);
                        record_fact(&replacement, &mut facts);
                        record_known(&replacement, &mut known);
                        rewritten.push(replacement);
                        Replacement::Constant(raw)
                    }
                    // An emitting rewrite hands out new value identities, and
                    // Section 8.8 is where they are handed out: the transient
                    // module keeps the instruction it started with, so nothing
                    // downstream in this sweep reads a name reassembly has not
                    // yet assigned.
                    Some(Replacement::Emit(term)) => {
                        record_fact(&inst, &mut facts);
                        record_known(&inst, &mut known);
                        rewritten.push(inst.clone());
                        emitting = true;
                        Replacement::Emit(term)
                    }
                    None => {
                        record_fact(&inst, &mut facts);
                        record_known(&inst, &mut known);
                        rewritten.push(inst);
                        continue;
                    }
                };
                applied.push(AppliedRewrite {
                    function: function.name.clone(),
                    block: block.id,
                    result: result.clone(),
                    instruction: inst,
                    facts: facts.clone(),
                    twin: None,
                    negations: negations.clone(),
                    known: known.clone(),
                    rule: opportunity
                        .expect("receipt requires a rewrite opportunity")
                        .rule,
                    replacement: actual_replacement,
                });
                report.epochs += 1;
                report.merges += 1;
                // An emitting rewrite is a real merge and not a replacement in
                // this module: the transient sweep keeps the instruction it
                // started with, and Section 8.8 is where the emitted
                // instructions and their value identities appear. Counting it
                // as an optimized value would report work this module does not
                // carry.
                if !emitting {
                    report.optimized_values += 1;
                }
                report.receipts.push(receipt);
            }
            block.insts = rewritten;
            remap_inst_uses(&mut block.terminator, &aliases);
        }
    }

    (
        ScalarEqOutcome {
            module: optimized,
            report,
        },
        applied,
    )
}

struct BridgeOutcome {
    node_count: usize,
    extracted: Option<Replacement>,
    receipt: Option<ScalarMergeReceipt>,
}

fn bridge_candidate(
    inst: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    opportunity: Option<&RewriteOpportunity>,
) -> BridgeOutcome {
    let mut bridge = CandidateGraph::new();
    let operator = ingest_scalar_inst(inst).expect("candidate was classified before bridging");
    let op = operator_id(operator);
    bridge.terms.insert(op, GraphTerm::Operation);
    let children = scalar_operands(inst)
        .into_iter()
        .map(|value| bridge.operand(value, facts).0)
        .collect::<Vec<_>>();
    let root = bridge.graph.insert_with_node(op, children.clone());

    let mut receipt = None;
    if let Some(opportunity) = opportunity {
        let (_, rhs_node) = match &opportunity.replacement {
            Replacement::Alias(value) => bridge.input(value),
            Replacement::Constant(raw) => {
                let ty = scalar_result(inst)
                    .map(|value| &value.ty)
                    .expect("rewrite candidate has a scalar result");
                bridge.constant(*raw, ty)
            }
            Replacement::Emit(term) => {
                let children = term
                    .operands
                    .iter()
                    .map(|operand| match operand {
                        EmittedOperand::Bound(value) => bridge.input(value).0,
                        EmittedOperand::Constant(raw) => bridge.constant(*raw, &term.result_ty).0,
                    })
                    .collect::<Vec<_>>();
                let emitted = operator_id(term.operator);
                bridge
                    .terms
                    .insert(emitted, GraphTerm::Emitted(term.clone()));
                let inserted = bridge.graph.insert_with_node(emitted, children);
                (inserted.eclass, inserted.node)
            }
        };
        let rhs_snapshot = bridge
            .graph
            .node_snapshot(rhs_node)
            .expect("inserted RHS has a stable node snapshot");
        let rule = RuleSnapshot {
            id: opportunity.rule.rule_id(),
            lhs: Pattern {
                op,
                children: children.iter().copied().map(PatternChild::Exact).collect(),
            },
            rhs: Pattern {
                op: rhs_snapshot.op,
                children: rhs_snapshot
                    .original_children
                    .iter()
                    .copied()
                    .map(PatternChild::Exact)
                    .collect(),
            },
        };
        bridge
            .graph
            .register_rule(rule)
            .expect("one candidate rule is structurally valid");
        let merge = bridge
            .graph
            .merge_rewrite(opportunity.rule.rule_id(), root.node, rhs_node, Vec::new())
            .expect("candidate-specific exact rewrite validates");
        let MergeOutcome::Merged(merge) = merge else {
            unreachable!("candidate root and cheaper RHS start in distinct classes")
        };
        bridge.graph.rebuild();
        let certificate = bridge.graph.proof_certificate();
        receipt = Some(ScalarMergeReceipt {
            rule: opportunity.rule,
            merge,
            certificate,
        });
    }

    let root = bridge.graph.find(root.eclass);
    let snapshot = bridge.graph.audit_snapshot();
    let extracted = extract_cheapest(root, &snapshot.nodes, &bridge.terms);
    BridgeOutcome {
        node_count: snapshot.nodes.len(),
        extracted,
        receipt,
    }
}

fn extract_cheapest(
    root: EClassId,
    nodes: &[evocc_egraph::AuditNode],
    terms: &BTreeMap<OpId, GraphTerm>,
) -> Option<Replacement> {
    nodes
        .iter()
        .filter(|node| node.owner == root)
        .filter_map(|node| match terms.get(&node.op)? {
            GraphTerm::Input(value) => Some((0_usize, node.op, Replacement::Alias(value.clone()))),
            GraphTerm::Constant { raw, ty } => {
                let _type_is_part_of_the_operator_identity = ty;
                Some((1, node.op, Replacement::Constant(*raw)))
            }
            // Section 7.5: an emitted instruction is the only choice that adds
            // one, so it is selected only where the class holds no seed and no
            // constant. Whether it is worth emitting is then the policy's
            // question, asked once the region is built.
            GraphTerm::Emitted(term) => Some((2, node.op, Replacement::Emit(term.clone()))),
            GraphTerm::Operation => None,
        })
        .min_by_key(|(cost, op, _)| (*cost, *op))
        .map(|(_, _, replacement)| replacement)
}

/// What Section 8.10 knows about each block-local value's bits.
///
/// A value absent from the environment is top: nothing known. Nothing outside
/// the block ever enters it, because a region's seeds are block arguments and
/// Section 8.10 gives those no fact.
pub(crate) type KnownEnv = HashMap<u32, KnownBits>;

fn comparison_of(operation: CmpOp) -> Comparison {
    match operation {
        CmpOp::Eq => Comparison::Eq,
        CmpOp::Ne => Comparison::Ne,
        CmpOp::SLt => Comparison::SLt,
        CmpOp::SLe => Comparison::SLe,
        CmpOp::ULt => Comparison::ULt,
        CmpOp::ULe => Comparison::ULe,
    }
}

/// `None` for an operand Section 8.10 has no lattice for. `icmp` admits two
/// pointers as readily as two bit vectors, and a pointer has no known bits.
pub(crate) fn known_of(value: &Value, known: &KnownEnv) -> Option<KnownBits> {
    let width = match value.ty {
        EvoIRType::Bits(width @ 1..=64) => width,
        _ => return None,
    };
    Some(
        known
            .get(&value.id)
            .copied()
            .unwrap_or_else(|| KnownBits::top(width)),
    )
}

/// The Section 8.10 transfer for one instruction's result. `None` for every
/// operator the table does not list, which transfers to top.
///
/// The table reaches wider than the region does. Section 8.5's allowlist keeps
/// a shift or a width change out of a witness closure, so a fact one of them
/// carries can never become a premise; it is still worth deriving here, because
/// this environment is what the sweep proposes rewrites from and a proposal the
/// region declines costs nothing but the proposing.
pub(crate) fn node_fact(inst: &Inst, known: &KnownEnv) -> Option<KnownBits> {
    let result = inst_results(inst).first().copied()?;
    let fact = match inst {
        Inst::Const(_, raw) => {
            KnownBits::constant(scalar_width(&result.ty)?, mask_raw(*raw, &result.ty)?)
        }
        Inst::And(_, left, right) => known_of(left, known)?.transfer_and(known_of(right, known)?),
        Inst::Or(_, left, right) => known_of(left, known)?.transfer_or(known_of(right, known)?),
        Inst::Xor(_, left, right) => known_of(left, known)?.transfer_xor(known_of(right, known)?),
        Inst::Add(_, left, right) => known_of(left, known)?.transfer_add(known_of(right, known)?),
        Inst::Sub(_, left, right) => known_of(left, known)?.transfer_sub(known_of(right, known)?),
        Inst::Mul(_, left, right) => known_of(left, known)?.transfer_mul(known_of(right, known)?),
        Inst::ICmp(_, operation, left, right) => {
            known_of(left, known)?.transfer_icmp(known_of(right, known)?, comparison_of(*operation))
        }
        Inst::Shl(_, shifted, amount) => {
            known_of(shifted, known)?.transfer_shl(known_of(amount, known)?)
        }
        Inst::LShr(_, shifted, amount) => {
            known_of(shifted, known)?.transfer_lshr(known_of(amount, known)?)
        }
        Inst::AShr(_, shifted, amount) => {
            known_of(shifted, known)?.transfer_ashr(known_of(amount, known)?)
        }
        Inst::ZExt(_, source, _) => {
            known_of(source, known)?.transfer_zext(scalar_width(&result.ty)?)
        }
        Inst::SExt(_, source, _) => {
            known_of(source, known)?.transfer_sext(scalar_width(&result.ty)?)
        }
        Inst::Trunc(_, source, _) => {
            known_of(source, known)?.transfer_trunc(scalar_width(&result.ty)?)
        }
        _ => return None,
    };
    Some(fact)
}

pub(crate) fn record_known(inst: &Inst, known: &mut KnownEnv) {
    if let (Some(result), Some(fact)) = (inst_results(inst).first(), node_fact(inst, known)) {
        known.insert(result.id, fact);
    }
}

/// The fact-driven rules the revision added, in the order the producer fires
/// them: a class whose every bit is known becomes that constant, and otherwise
/// a covering mask or a known-zero operand makes the operation an identity.
///
/// The order inside the family is a producer choice, not a rule: a class that
/// is fully known and also covered would be replaced by its constant, which is
/// never worse than an alias and is what a reader expects to see.
pub(crate) fn fact_driven_opportunity(inst: &Inst, known: &KnownEnv) -> Option<RewriteOpportunity> {
    if matches!(inst, Inst::Const(..)) {
        return None;
    }
    let fact = node_fact(inst, known)?;
    if fact.is_constant() {
        return constant(ScalarRewrite::KnownConst, fact.value());
    }
    match inst {
        Inst::And(_, left, right) => {
            let (left_fact, right_fact) = (known_of(left, known)?, known_of(right, known)?);
            if right_fact.covers(left_fact) {
                alias(ScalarRewrite::MaskCover, left)
            } else if left_fact.covers(right_fact) {
                alias(ScalarRewrite::MaskCover, right)
            } else {
                None
            }
        }
        Inst::Or(_, left, right) => {
            let (left_fact, right_fact) = (known_of(left, known)?, known_of(right, known)?);
            if right_fact.covers(left_fact) {
                alias(ScalarRewrite::MaskCover, right)
            } else if left_fact.covers(right_fact) {
                alias(ScalarRewrite::MaskCover, left)
            } else {
                None
            }
        }
        Inst::Xor(_, left, right) => {
            if known_of(right, known)?.is_known_zero() {
                alias(ScalarRewrite::XorKnownZero, left)
            } else if known_of(left, known)?.is_known_zero() {
                alias(ScalarRewrite::XorKnownZero, right)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn rewrite_opportunity(
    inst: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    negations: &NegationEnv,
) -> Option<RewriteOpportunity> {
    match inst {
        Inst::Add(_, left, right) => {
            if is_constant(right, facts, 0) {
                alias(ScalarRewrite::AddZero, left)
            } else if is_constant(left, facts, 0) {
                alias(ScalarRewrite::AddZero, right)
            } else {
                constant_fold(inst, facts)
            }
        }
        Inst::Mul(_, left, right) => {
            if is_constant(right, facts, 1) {
                alias(ScalarRewrite::MulOne, left)
            } else if is_constant(left, facts, 1) {
                alias(ScalarRewrite::MulOne, right)
            } else {
                constant_fold(inst, facts)
            }
        }
        Inst::And(_, left, right) => {
            if is_constant(left, facts, 0) || is_constant(right, facts, 0) {
                constant(ScalarRewrite::AndZero, 0)
            } else {
                constant_fold(inst, facts)
            }
        }
        Inst::Or(_, left, right) => {
            if is_constant(right, facts, 0) {
                alias(ScalarRewrite::OrZero, left)
            } else if is_constant(left, facts, 0) {
                alias(ScalarRewrite::OrZero, right)
            } else {
                constant_fold(inst, facts)
            }
        }
        Inst::Sub(_, left, right) if is_constant(left, facts, 0) => {
            if let Some(operand) = negations.get(&right.id) {
                alias(ScalarRewrite::DoubleNegation, operand)
            } else {
                constant_fold(inst, facts)
            }
        }
        Inst::ICmp(_, comparison, left, right) if left == right => {
            let raw = u64::from(matches!(comparison, CmpOp::Eq | CmpOp::SLe | CmpOp::ULe));
            constant(ScalarRewrite::IdentityComparison, raw)
        }
        _ => constant_fold(inst, facts),
    }
}

/// The emitting rules of `p0.scalar.v4`, applied after every earlier tier.
///
/// Section 11.6 fixes the order: within a tier the producer applies the first
/// applicable rule in registry order, and these four are printed after every
/// canonical reduction, every `all_const` fold and every fact-driven rule. A
/// `mul(?x, 2)` whose `?x` is itself constant is therefore folded, not doubled.
///
/// `width_min(?x, 2)` holds when the sort of `?x` is `bits<N>` with `N >= 2`.
/// It reads no class and records no premise, which is why these rules sit in a
/// registry revision without being Tier B rules.
pub(crate) fn emitting_opportunity(
    inst: &Inst,
    facts: &HashMap<u32, ScalarFact>,
) -> Option<RewriteOpportunity> {
    match inst {
        Inst::Mul(_, left, right) => {
            let doubled = if is_constant(right, facts, 2) {
                left
            } else if is_constant(left, facts, 2) {
                right
            } else {
                return None;
            };
            emit(
                ScalarRewrite::DoubleByAddition,
                ScalarOperator::Add,
                doubled,
                vec![
                    EmittedOperand::Bound(doubled.clone()),
                    EmittedOperand::Bound(doubled.clone()),
                ],
            )
        }
        Inst::Add(_, left, right) if left == right => emit(
            ScalarRewrite::DoubleByShift,
            ScalarOperator::Shl,
            left,
            vec![
                EmittedOperand::Bound(left.clone()),
                EmittedOperand::Constant(1),
            ],
        ),
        // `shl(?x, const_like(?x, 1))` fixes the amount's sort to the shifted
        // value's own sort. A shift by a one of some other width is a different
        // left pattern and this rule does not state it.
        Inst::Shl(_, shifted, amount)
            if amount.ty == shifted.ty && is_constant(amount, facts, 1) =>
        {
            emit(
                ScalarRewrite::ShiftedDoubleByAddition,
                ScalarOperator::Add,
                shifted,
                vec![
                    EmittedOperand::Bound(shifted.clone()),
                    EmittedOperand::Bound(shifted.clone()),
                ],
            )
        }
        _ => None,
    }
}

/// `width_min(?x, 2)` and the emitted term, or nothing.
fn emit(
    rule: ScalarRewrite,
    operator: ScalarOperator,
    bound: &Value,
    operands: Vec<EmittedOperand>,
) -> Option<RewriteOpportunity> {
    // `width_min(?x, 2)` reads the sort of `?x`, and a value outside this
    // profile's `bits<N>` bound has no sort the rule can read.
    if scalar_width(&bound.ty).is_none_or(|width| width < 2) {
        return None;
    }
    Some(RewriteOpportunity {
        rule,
        replacement: Replacement::Emit(Emitted {
            operator,
            result_ty: bound.ty.clone(),
            operands,
        }),
    })
}

fn alias(rule: ScalarRewrite, value: &Value) -> Option<RewriteOpportunity> {
    Some(RewriteOpportunity {
        rule,
        replacement: Replacement::Alias(value.clone()),
    })
}

fn constant(rule: ScalarRewrite, raw: u64) -> Option<RewriteOpportunity> {
    Some(RewriteOpportunity {
        rule,
        replacement: Replacement::Constant(raw),
    })
}

pub(crate) fn constant_fold(
    inst: &Inst,
    facts: &HashMap<u32, ScalarFact>,
) -> Option<RewriteOpportunity> {
    let operands = scalar_operands(inst);
    let raw = operands
        .iter()
        .map(|value| constant_raw(value, facts))
        .collect::<Option<Vec<_>>>()?;
    let result = scalar_result(inst)?;
    let [left, right] = raw.as_slice() else {
        return None;
    };
    let operator = ingest_scalar_inst(inst).ok()?;
    let folded = fold_raw(operator, *left, *right, &operands[0].ty, &result.ty)?;
    constant(ScalarRewrite::ConstantFold, folded)
}

/// The `all_const` result of one P0 operator on two raw operands.
///
/// This is the only implementation of the fold table in the producer: the
/// region builder computes constants from the emitted node stream rather than
/// from the sweep's environment, and both reach this function.
pub(crate) fn fold_raw(
    operator: ScalarOperator,
    left: u64,
    right: u64,
    operand_ty: &EvoIRType,
    result_ty: &EvoIRType,
) -> Option<u64> {
    let folded = match operator {
        ScalarOperator::Add => left.wrapping_add(right),
        ScalarOperator::Sub => left.wrapping_sub(right),
        ScalarOperator::Mul => left.wrapping_mul(right),
        ScalarOperator::And => left & right,
        ScalarOperator::Or => left | right,
        ScalarOperator::Xor => left ^ right,
        ScalarOperator::ICmp(comparison) => {
            u64::from(fold_comparison(comparison, left, right, operand_ty)?)
        }
        // `p0.scalar.v3` gives the shifts and width changes a fact-driven fold
        // and no `all_const` one, so there is no fold to compute here; their
        // exact value comes from the Section 8.10 row instead.
        ScalarOperator::Const
        | ScalarOperator::Shl
        | ScalarOperator::LShr
        | ScalarOperator::AShr
        | ScalarOperator::ZExt
        | ScalarOperator::SExt
        | ScalarOperator::Trunc => return None,
    };
    mask_raw(folded, result_ty)
}

fn fold_comparison(comparison: CmpOp, left: u64, right: u64, ty: &EvoIRType) -> Option<bool> {
    let width = scalar_width(ty)?;
    let left = mask_to_width(left, width);
    let right = mask_to_width(right, width);
    Some(match comparison {
        CmpOp::Eq => left == right,
        CmpOp::Ne => left != right,
        CmpOp::ULt => left < right,
        CmpOp::ULe => left <= right,
        CmpOp::SLt => signed_value(left, width) < signed_value(right, width),
        CmpOp::SLe => signed_value(left, width) <= signed_value(right, width),
    })
}

fn signed_value(raw: u64, width: u16) -> i128 {
    let sign = 1_u64 << (width - 1);
    if raw & sign == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    }
}

/// A constant's payload, which is the one attribute a P0 node carries.
fn constant_payload(instruction: &Inst) -> Option<u64> {
    match instruction {
        Inst::Const(value, raw) => mask_raw(*raw, &value.ty),
        _ => None,
    }
}

pub(crate) fn record_fact(inst: &Inst, facts: &mut HashMap<u32, ScalarFact>) {
    if let Inst::Const(result, raw) = inst {
        // A width outside the profile's bound records no fact, which is what
        // leaves the value excluded rather than folded.
        let Some(raw) = mask_raw(*raw, &result.ty) else {
            return;
        };
        facts.insert(
            result.id,
            ScalarFact::Constant {
                raw,
                ty: result.ty.clone(),
            },
        );
    }
}

/// Record the shape of one instruction before any replacement takes its place.
pub(crate) fn record_negation(
    inst: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    negations: &mut NegationEnv,
) {
    if let Inst::Sub(result, left, right) = inst {
        if is_constant(left, facts, 0) {
            negations.insert(result.id, right.clone());
        }
    }
}

fn is_constant(value: &Value, facts: &HashMap<u32, ScalarFact>, expected: u64) -> bool {
    mask_raw(expected, &value.ty)
        .is_some_and(|expected| constant_raw(value, facts) == Some(expected))
}

fn constant_raw(value: &Value, facts: &HashMap<u32, ScalarFact>) -> Option<u64> {
    match facts.get(&value.id) {
        Some(ScalarFact::Constant { raw, ty }) if ty == &value.ty => Some(*raw),
        _ => None,
    }
}

pub(crate) fn scalar_result(inst: &Inst) -> Option<&Value> {
    match inst {
        Inst::Const(result, ..)
        | Inst::Add(result, ..)
        | Inst::Sub(result, ..)
        | Inst::Mul(result, ..)
        | Inst::And(result, ..)
        | Inst::Or(result, ..)
        | Inst::Xor(result, ..)
        | Inst::ICmp(result, ..)
        | Inst::Shl(result, ..)
        | Inst::LShr(result, ..)
        | Inst::AShr(result, ..)
        | Inst::ZExt(result, ..)
        | Inst::SExt(result, ..)
        | Inst::Trunc(result, ..) => Some(result),
        _ => None,
    }
}

pub(crate) fn scalar_operands(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::Const(..) => Vec::new(),
        Inst::Add(_, left, right)
        | Inst::Sub(_, left, right)
        | Inst::Mul(_, left, right)
        | Inst::And(_, left, right)
        | Inst::Or(_, left, right)
        | Inst::Xor(_, left, right)
        | Inst::ICmp(_, _, left, right)
        | Inst::Shl(_, left, right)
        | Inst::LShr(_, left, right)
        | Inst::AShr(_, left, right) => vec![left, right],
        Inst::ZExt(_, source, _) | Inst::SExt(_, source, _) | Inst::Trunc(_, source, _) => {
            vec![source]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn operator_id(operator: ScalarOperator) -> OpId {
    match operator {
        ScalarOperator::Const => unreachable!("constants are operand leaves"),
        ScalarOperator::Add => OP_ADD,
        ScalarOperator::Sub => OP_SUB,
        ScalarOperator::Mul => OP_MUL,
        ScalarOperator::And => OP_AND,
        ScalarOperator::Or => OP_OR,
        ScalarOperator::Xor => OP_XOR,
        ScalarOperator::ICmp(CmpOp::Eq) => OP_ICMP_EQ,
        ScalarOperator::ICmp(CmpOp::Ne) => OP_ICMP_NE,
        ScalarOperator::ICmp(CmpOp::SLt) => OP_ICMP_SLT,
        ScalarOperator::ICmp(CmpOp::SLe) => OP_ICMP_SLE,
        ScalarOperator::ICmp(CmpOp::ULt) => OP_ICMP_ULT,
        ScalarOperator::ICmp(CmpOp::ULe) => OP_ICMP_ULE,
        ScalarOperator::Shl => OP_SHL,
        ScalarOperator::LShr => OP_LSHR,
        ScalarOperator::AShr => OP_ASHR,
        ScalarOperator::ZExt => OP_ZEXT,
        ScalarOperator::SExt => OP_SEXT,
        ScalarOperator::Trunc => OP_TRUNC,
    }
}

/// The width Section 8.10 gives a value, or `None` for a type it has no
/// lattice for.
pub(crate) fn scalar_width(ty: &EvoIRType) -> Option<u16> {
    match ty {
        EvoIRType::Bits(width @ 1..=64) => Some(*width),
        _ => None,
    }
}

/// The bit pattern a constant of `ty` carries, or `None` for a width this
/// profile has no representation for.
///
/// Section 11 bounds every root and witness to "`bits<N>` with `1 <= N <= 64`"
/// and adds that "silently truncating a wider constant to the host's integer
/// width is invalid". `bits<128>` is an executed type since EvoIR 1.9.61, so a
/// module reaches here holding one; the eligibility fixed point already
/// excludes it, but a constant's width is read before that runs. Declining is
/// the answer the bound already gives; truncating and aborting are both
/// wrong.
pub(crate) fn mask_raw(raw: u64, ty: &EvoIRType) -> Option<u64> {
    Some(mask_to_width(raw, scalar_width(ty)?))
}

fn mask_to_width(raw: u64, width: u16) -> u64 {
    if width == 64 {
        raw
    } else {
        raw & ((1_u64 << width) - 1)
    }
}
