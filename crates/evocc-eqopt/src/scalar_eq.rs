//! Day-7 pure-scalar EvoIR to e-graph bridge.

use std::collections::{BTreeMap, HashMap, HashSet};

use evocc_egraph::{
    EClassId, Egraph, MergeOutcome, OpId, Pattern, PatternChild, ProofCertificate, ProofContext,
    RuleSnapshot,
};
use evocc_types::EvoIRType;

use evocc_evoir::{CmpOp, Inst, Module, Value};

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
        Inst::Alloca { .. }
        | Inst::LifetimeEnd { .. }
        | Inst::PointerValid { .. }
        | Inst::AccessInBounds { .. }
        | Inst::Load { .. }
        | Inst::Store { .. }
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
    Negation { operand: Value },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Replacement {
    Alias(Value),
    Constant(u64),
}

#[derive(Debug, Clone)]
struct RewriteOpportunity {
    rule: ScalarRewrite,
    replacement: Replacement,
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
    pub(crate) rule: ScalarRewrite,
    pub(crate) replacement: Replacement,
}

impl AppliedRewrite {
    pub(crate) fn is_reviewed_instance(&self) -> bool {
        rewrite_opportunity(&self.instruction, &self.facts).is_some_and(|opportunity| {
            opportunity.rule == self.rule && opportunity.replacement == self.replacement
        })
    }
}

#[derive(Debug, Clone)]
enum GraphTerm {
    Input(Value),
    Constant { raw: u64, ty: EvoIRType },
    Operation,
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
        let raw = mask_raw(raw, ty);
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

    for function in &mut optimized.functions {
        for block in &mut function.blocks {
            let mut aliases = HashMap::new();
            let mut facts = HashMap::new();
            let mut rewritten = Vec::with_capacity(block.insts.len());
            for original_inst in &block.insts {
                let mut inst = original_inst.clone();
                remap_inst_uses(&mut inst, &aliases);
                if matches!(ingest_scalar_inst(&inst), Ok(ScalarOperator::Const)) {
                    record_fact(&inst, &mut facts);
                    rewritten.push(inst);
                    continue;
                }
                let Ok(_) = ingest_scalar_inst(&inst) else {
                    rewritten.push(inst);
                    continue;
                };
                report.regions_admitted += 1;
                let opportunity = rewrite_opportunity(&inst, &facts);
                let bridge = bridge_candidate(&inst, &facts, opportunity.as_ref());
                report.graph_nodes = report.graph_nodes.saturating_add(bridge.node_count);
                if report.graph_nodes > budget.max_nodes
                    || (opportunity.is_some() && report.epochs >= budget.max_epochs)
                {
                    report.budget_exhausted = true;
                    report.fell_back = true;
                    report.receipts.clear();
                    report.merges = 0;
                    report.optimized_values = 0;
                    return (
                        ScalarEqOutcome {
                            module: original,
                            report,
                        },
                        Vec::new(),
                    );
                }
                let Some(receipt) = bridge.receipt else {
                    record_fact(&inst, &mut facts);
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
                let actual_replacement = match bridge.extracted {
                    Some(Replacement::Alias(value)) => {
                        let replacement = resolve_alias(&value, &aliases);
                        aliases.insert(result.id, replacement.clone());
                        Replacement::Alias(replacement)
                    }
                    Some(Replacement::Constant(raw)) => {
                        let replacement = Inst::Const(result.clone(), raw);
                        record_fact(&replacement, &mut facts);
                        rewritten.push(replacement);
                        Replacement::Constant(raw)
                    }
                    None => {
                        record_fact(&inst, &mut facts);
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
                    rule: opportunity
                        .expect("receipt requires a rewrite opportunity")
                        .rule,
                    replacement: actual_replacement,
                });
                report.epochs += 1;
                report.merges += 1;
                report.optimized_values += 1;
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
            GraphTerm::Operation => None,
        })
        .min_by_key(|(cost, op, _)| (*cost, *op))
        .map(|(_, _, replacement)| replacement)
}

fn rewrite_opportunity(
    inst: &Inst,
    facts: &HashMap<u32, ScalarFact>,
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
            if let Some(ScalarFact::Negation { operand }) = facts.get(&right.id) {
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

fn constant_fold(inst: &Inst, facts: &HashMap<u32, ScalarFact>) -> Option<RewriteOpportunity> {
    let operands = scalar_operands(inst);
    let raw = operands
        .iter()
        .map(|value| constant_raw(value, facts))
        .collect::<Option<Vec<_>>>()?;
    let result = scalar_result(inst)?;
    let folded = match (inst, raw.as_slice()) {
        (Inst::Add(..), [left, right]) => left.wrapping_add(*right),
        (Inst::Sub(..), [left, right]) => left.wrapping_sub(*right),
        (Inst::Mul(..), [left, right]) => left.wrapping_mul(*right),
        (Inst::And(..), [left, right]) => left & right,
        (Inst::Or(..), [left, right]) => left | right,
        (Inst::Xor(..), [left, right]) => left ^ right,
        (Inst::ICmp(_, comparison, ..), [left, right]) => {
            u64::from(fold_comparison(*comparison, *left, *right, &operands[0].ty))
        }
        _ => return None,
    };
    constant(ScalarRewrite::ConstantFold, mask_raw(folded, &result.ty))
}

fn fold_comparison(comparison: CmpOp, left: u64, right: u64, ty: &EvoIRType) -> bool {
    let width = bit_width(ty);
    let left = mask_to_width(left, width);
    let right = mask_to_width(right, width);
    match comparison {
        CmpOp::Eq => left == right,
        CmpOp::Ne => left != right,
        CmpOp::ULt => left < right,
        CmpOp::ULe => left <= right,
        CmpOp::SLt => signed_value(left, width) < signed_value(right, width),
        CmpOp::SLe => signed_value(left, width) <= signed_value(right, width),
    }
}

fn signed_value(raw: u64, width: u16) -> i128 {
    let sign = 1_u64 << (width - 1);
    if raw & sign == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    }
}

fn record_fact(inst: &Inst, facts: &mut HashMap<u32, ScalarFact>) {
    match inst {
        Inst::Const(result, raw) => {
            facts.insert(
                result.id,
                ScalarFact::Constant {
                    raw: mask_raw(*raw, &result.ty),
                    ty: result.ty.clone(),
                },
            );
        }
        Inst::Sub(result, left, right) if is_constant(left, facts, 0) => {
            facts.insert(
                result.id,
                ScalarFact::Negation {
                    operand: right.clone(),
                },
            );
        }
        _ => {}
    }
}

fn is_constant(value: &Value, facts: &HashMap<u32, ScalarFact>, expected: u64) -> bool {
    constant_raw(value, facts).is_some_and(|raw| raw == mask_raw(expected, &value.ty))
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
        | Inst::ICmp(result, ..) => Some(result),
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
        | Inst::ICmp(_, _, left, right) => vec![left, right],
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
    }
}

fn resolve_alias(value: &Value, aliases: &HashMap<u32, Value>) -> Value {
    let mut value = value.clone();
    let mut visited = HashSet::new();
    while visited.insert(value.id) {
        let Some(next) = aliases.get(&value.id) else {
            break;
        };
        value = next.clone();
    }
    value
}

fn remap(value: &mut Value, aliases: &HashMap<u32, Value>) {
    *value = resolve_alias(value, aliases);
}

#[allow(clippy::too_many_lines)]
fn remap_inst_uses(inst: &mut Inst, aliases: &HashMap<u32, Value>) {
    match inst {
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Trap(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. } => {}
        Inst::Add(_, left, right)
        | Inst::Sub(_, left, right)
        | Inst::Mul(_, left, right)
        | Inst::SRem(_, left, right)
        | Inst::URem(_, left, right)
        | Inst::Shl(_, left, right)
        | Inst::AShr(_, left, right)
        | Inst::LShr(_, left, right)
        | Inst::And(_, left, right)
        | Inst::Or(_, left, right)
        | Inst::Xor(_, left, right)
        | Inst::SignedAddDefined(_, left, right)
        | Inst::SignedSubDefined(_, left, right)
        | Inst::SignedMulDefined(_, left, right)
        | Inst::SignedDivRemNoOverflow(_, left, right)
        | Inst::SDiv(_, left, right)
        | Inst::UDiv(_, left, right)
        | Inst::ICmp(_, _, left, right) => {
            remap(left, aliases);
            remap(right, aliases);
        }
        Inst::DivisorNonZero(_, divisor) | Inst::UbAssume(divisor, _) => remap(divisor, aliases),
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => {
            remap(shifted, aliases);
            remap(amount, aliases);
        }
        Inst::Br(_, arguments) | Inst::Ret(arguments) => {
            for argument in arguments {
                remap(argument, aliases);
            }
        }
        Inst::CondBr(condition, _, true_arguments, _, false_arguments) => {
            remap(condition, aliases);
            for argument in true_arguments.iter_mut().chain(false_arguments) {
                remap(argument, aliases);
            }
        }
        Inst::Load {
            address,
            mem_in,
            world_in,
            ..
        } => {
            remap(address, aliases);
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            ..
        } => {
            remap(address, aliases);
            remap(value, aliases);
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::Alloca {
            mem_in, world_in, ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
            remap(pointer, aliases);
        }
        Inst::PointerValid {
            mem_in, pointer, ..
        }
        | Inst::AccessInBounds {
            mem_in, pointer, ..
        } => {
            remap(mem_in, aliases);
            remap(pointer, aliases);
        }
        Inst::PtrOffset { base, index, .. } => {
            remap(base, aliases);
            remap(index, aliases);
        }
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            for argument in args {
                remap(argument, aliases);
            }
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::IndirectCall {
            callee,
            args,
            mem_in,
            world_in,
            ..
        } => {
            remap(callee, aliases);
            for argument in args {
                remap(argument, aliases);
            }
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::SExt(_, source, _) | Inst::ZExt(_, source, _) | Inst::Trunc(_, source, _) => {
            remap(source, aliases);
        }
    }
}

fn bit_width(ty: &EvoIRType) -> u16 {
    match ty {
        EvoIRType::Bits(width @ 1..=64) => *width,
        _ => unreachable!("P0 scalar operator carries a non-bit-vector type"),
    }
}

pub(crate) fn mask_raw(raw: u64, ty: &EvoIRType) -> u64 {
    mask_to_width(raw, bit_width(ty))
}

fn mask_to_width(raw: u64, width: u16) -> u64 {
    if width == 64 {
        raw
    } else {
        raw & ((1_u64 << width) - 1)
    }
}
