//! Reviewed EVP1 production for the independently provable P0 subset.

use std::collections::{HashMap, HashSet};

use evocc_evoir::{inst_results, render_module, validate_module, CmpOp, Inst, Module, Value};
use evocc_proof::{
    sha256_hex, EvpCertificate, EvpEffectClass, EvpEvent, EvpExtraction, EvpExtractionChoice,
    EvpNode, EvpNodeSource, EvpProofRef, EvpRegion, EvpRegionId, EvpRewrite, EvpSeed,
    EvpSeedOrigin,
};
use evocc_types::EvoIRType;

use crate::scalar_eq::{
    constant_fold, ingest_scalar_inst, mask_raw, optimize_scalar_module_with_events,
    remap_inst_uses, resolve_alias, scalar_operands, AppliedRewrite, Replacement,
    RewriteOpportunity, ScalarFact,
};
use crate::{ProofAuthority, ScalarEqBudget, ScalarEqReport, ScalarRewrite};

const REGISTRY_NAME: &str = "p0.scalar.v1";
const REGISTRY_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";
pub(crate) type ValueRemap = HashMap<(String, u32), u32>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedScalarEqOutcome {
    pub module: Module,
    pub certificate: EvpCertificate,
    pub report: ScalarEqReport,
}

pub(crate) struct CertifiedScalarEqTransaction {
    pub(crate) outcome: CertifiedScalarEqOutcome,
    pub(crate) events: Vec<AppliedRewrite>,
    pub(crate) value_remap: ValueRemap,
}

/// Optimize only regions that can carry a self-contained current-transaction
/// EVP1 proof. More speculative or history-dependent opportunities are simply
/// omitted; optimization is never required for correctness.
#[must_use]
pub fn optimize_scalar_module_with_evp1(
    module: &Module,
    budget: ScalarEqBudget,
) -> CertifiedScalarEqOutcome {
    optimize_scalar_module_with_evp1_transaction(module, budget).outcome
}

pub(crate) fn optimize_scalar_module_with_evp1_transaction(
    module: &Module,
    budget: ScalarEqBudget,
) -> CertifiedScalarEqTransaction {
    let input = render_module(module);
    let (transient, applied) = optimize_scalar_module_with_events(module, budget);
    let mut report = transient.report;
    report.receipts.clear();
    report.merges = 0;
    report.optimized_values = 0;
    report.proof_authority = ProofAuthority::Reviewed {
        registry: REGISTRY_NAME,
    };
    if report.fell_back {
        return zero_transaction(module, input, report);
    }

    let mut regions = Vec::new();
    let mut accepted = Vec::new();
    let mut events = Vec::new();
    let mut proof_nodes = 0_usize;
    for (function_index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (instruction_index, instruction) in block.insts.iter().enumerate() {
                let Some(root_event) = applied.iter().find(|event| {
                    event.function == function.name
                        && event.block == block.id
                        && inst_results(instruction)
                            .first()
                            .is_some_and(|result| result.id == event.result.id)
                }) else {
                    continue;
                };
                let Some((region, normalized_event, output_replacement)) = build_region(
                    module,
                    function_index,
                    block_index,
                    instruction_index,
                    &applied,
                    root_event,
                ) else {
                    continue;
                };
                let new_nodes = region
                    .events
                    .iter()
                    .filter(|event| matches!(event, EvpEvent::Node(_)))
                    .count();
                if regions.len() >= budget.max_epochs
                    || proof_nodes.saturating_add(new_nodes) > budget.max_nodes
                {
                    report.budget_exhausted = true;
                    report.fell_back = true;
                    return zero_transaction(module, input, report);
                }
                proof_nodes += new_nodes;
                accepted.push(Accepted {
                    function: function_index,
                    block: block_index,
                    instruction: instruction_index,
                    root: normalized_event.result.id,
                    replacement: output_replacement,
                });
                events.push(normalized_event);
                regions.push(region);
            }
        }
    }
    let (output, value_remap) = match apply(module, &accepted) {
        Ok(output) => output,
        Err(()) => {
            report.proof_replay_failed = true;
            report.fell_back = true;
            return zero_transaction(module, input, report);
        }
    };
    let output_text = render_module(&output);
    report.regions_admitted = regions.len();
    report.graph_nodes = proof_nodes;
    report.epochs = regions.len();
    report.merges = regions.len();
    report.optimized_values = regions.len();
    CertifiedScalarEqTransaction {
        outcome: CertifiedScalarEqOutcome {
            module: output,
            certificate: EvpCertificate {
                registry_name: REGISTRY_NAME.to_owned(),
                registry_digest: REGISTRY_SHA256.to_owned(),
                input_digest: sha256_hex(input.as_bytes()),
                regions,
                output_digest: sha256_hex(output_text.as_bytes()),
            },
            report,
        },
        events,
        value_remap,
    }
}

fn zero_transaction(
    module: &Module,
    input: String,
    report: ScalarEqReport,
) -> CertifiedScalarEqTransaction {
    CertifiedScalarEqTransaction {
        outcome: zero(module, input, report),
        events: Vec::new(),
        value_remap: HashMap::new(),
    }
}

fn zero(module: &Module, input: String, mut report: ScalarEqReport) -> CertifiedScalarEqOutcome {
    report.merges = 0;
    report.optimized_values = 0;
    CertifiedScalarEqOutcome {
        module: module.clone(),
        certificate: EvpCertificate {
            registry_name: REGISTRY_NAME.to_owned(),
            registry_digest: REGISTRY_SHA256.to_owned(),
            input_digest: sha256_hex(input.as_bytes()),
            regions: Vec::new(),
            output_digest: sha256_hex(input.as_bytes()),
        },
        report,
    }
}

#[derive(Clone)]
struct Accepted {
    function: usize,
    block: usize,
    instruction: usize,
    root: u32,
    replacement: Replacement,
}

fn build_region(
    module: &Module,
    function_index: usize,
    block_index: usize,
    instruction_index: usize,
    applied: &[AppliedRewrite],
    root_event: &AppliedRewrite,
) -> Option<(EvpRegion, AppliedRewrite, Replacement)> {
    let function = &module.functions[function_index];
    let block = &function.blocks[block_index];
    let instruction = &block.insts[instruction_index];
    ingest_scalar_inst(instruction).ok()?;
    let definitions = block
        .insts
        .iter()
        .enumerate()
        .flat_map(|(index, instruction)| {
            inst_results(instruction)
                .into_iter()
                .map(move |value| (value.id, index))
        })
        .collect::<HashMap<_, _>>();
    let mut closure = HashSet::new();
    collect_closure(block, &definitions, instruction_index, &mut closure)?;
    let witness_indices = (0..block.insts.len())
        .filter(|index| closure.contains(index))
        .collect::<Vec<_>>();
    let mut seed_values = Vec::new();
    let mut seen = HashSet::new();
    for index in &witness_indices {
        for operand in scalar_operands(&block.insts[*index]) {
            let inside = definitions
                .get(&operand.id)
                .is_some_and(|definition| closure.contains(definition));
            if !inside && seen.insert(operand.id) {
                seed_values.push(operand.clone());
            }
        }
    }
    if !seed_values.iter().all(|seed| {
        block_index == 0
            && block.args.iter().any(|argument| argument.value == *seed)
            && matches!(seed.ty, EvoIRType::Bits(1..=64))
    }) {
        return None;
    }
    let seeds = seed_values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            Some(EvpSeed {
                value: value.id,
                sort: packed_type(&value.ty)?,
                origin: EvpSeedOrigin::Input,
                class: u32::try_from(index).ok()?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let mut value_classes = seeds
        .iter()
        .map(|seed| (seed.value, seed.class))
        .collect::<HashMap<_, _>>();
    let mut value_nodes = HashMap::new();
    let mut node_values = HashMap::new();
    let mut nodes = Vec::new();
    for index in &witness_indices {
        let instruction = &block.insts[*index];
        let node_id = u32::try_from(nodes.len()).ok()?;
        let owner = u32::try_from(seeds.len() + nodes.len()).ok()?;
        let node = before_node(instruction, node_id, owner, &value_classes)?;
        let result = inst_results(instruction)[0];
        value_classes.insert(result.id, owner);
        value_nodes.insert(result.id, node_id);
        node_values.insert(node_id, result.clone());
        nodes.push(node);
    }
    let root_node = *value_nodes.get(&root_event.result.id)?;
    let replacement_root = nodes.get(root_node as usize)?.owner;
    let mut classes = LocalClasses::new(seeds.len() + nodes.len())?;
    let mut events = nodes
        .iter()
        .cloned()
        .map(EvpEvent::Node)
        .collect::<Vec<_>>();
    let mut selected = None;
    let mut normalized_root = None;
    let mut output_replacement = None;
    for event in applied.iter().filter(|event| {
        event.function == function.name
            && event.block == block.id
            && definitions
                .get(&event.result.id)
                .is_some_and(|index| closure.contains(index))
    }) {
        let node_id = *value_nodes.get(&event.result.id)?;
        let lhs = classes.find(nodes.get(node_id as usize)?.owner)?;
        let opportunity = normalize_opportunity(event, &nodes, &value_classes, &classes)?;
        let normalized_event = AppliedRewrite {
            function: event.function.clone(),
            block: event.block,
            result: event.result.clone(),
            instruction: event.instruction.clone(),
            facts: event.facts.clone(),
            rule: opportunity.rule,
            replacement: opportunity.replacement.clone(),
        };
        let (rule, binding_paths) = exact_rule(&event.instruction, &event.facts, &opportunity)?;
        let bindings = binding_paths
            .iter()
            .map(|(name, path)| {
                Some((
                    (*name).to_owned(),
                    binding_class(&nodes, &classes, node_id, *path)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let (rhs, rhs_ref, event_selected, event_output) = match &opportunity.replacement {
            Replacement::Alias(value) => {
                let rhs = classes.find(*value_classes.get(&value.id)?)?;
                (
                    rhs,
                    EvpProofRef::Class(rhs),
                    SelectedSource::Value(value.id),
                    Replacement::Alias(value.clone()),
                )
            }
            Replacement::Constant(raw) => {
                let sort = packed_type(&event.result.ty)?;
                let raw = mask_raw(*raw, &event.result.ty);
                if let Some(existing) = nodes.iter().find(|node| {
                    node.operation == "const"
                        && node.sort == sort
                        && matches!(node.attributes.as_slice(), [(name, value)] if name == "value" && *value == raw)
                }) {
                    let output = node_values
                        .get(&existing.node)
                        .cloned()
                        .map(Replacement::Alias)
                        .unwrap_or(Replacement::Constant(raw));
                    (
                        classes.find(existing.owner)?,
                        EvpProofRef::Node(existing.node),
                        SelectedSource::Node(existing.node),
                        output,
                    )
                } else {
                    let node = u32::try_from(nodes.len()).ok()?;
                    let owner = classes.push()?;
                    let derived = EvpNode {
                        node,
                        owner,
                        source: EvpNodeSource::Derived,
                        operation: "const".to_owned(),
                        sort,
                        effect: EvpEffectClass::Pure,
                        attributes: vec![("value".to_owned(), raw)],
                        children: Vec::new(),
                    };
                    nodes.push(derived.clone());
                    events.push(EvpEvent::Node(derived));
                    (
                        owner,
                        EvpProofRef::Node(node),
                        SelectedSource::Node(node),
                        Replacement::Constant(raw),
                    )
                }
            }
        };
        if lhs == rhs {
            return None;
        }
        let result = lhs.min(rhs);
        events.push(EvpEvent::Rewrite(EvpRewrite {
            rule: rule.to_owned(),
            lhs_pre: lhs,
            rhs_pre: rhs,
            result,
            lhs: EvpProofRef::Node(node_id),
            rhs: rhs_ref,
            bindings,
        }));
        classes.union(lhs, rhs)?;
        if event.result.id == root_event.result.id {
            selected = Some(event_selected);
            normalized_root = Some(normalized_event);
            output_replacement = Some(event_output);
        }
    }
    let root_class = classes.find(replacement_root)?;
    let extractions = extraction_manifest(
        &nodes,
        &seeds,
        &value_classes,
        selected?,
        replacement_root,
        &classes,
    )?;
    let block_label = if block_index == 0 {
        "entry".to_owned()
    } else {
        format!("b{}", block.id)
    };
    let region_id = |index: usize| EvpRegionId {
        function: function.name.clone(),
        block: block_label.clone(),
        instruction: u64::try_from(index).expect("usize always fits u64 on supported targets"),
    };
    Some((
        EvpRegion {
            id: region_id(instruction_index),
            root_value: inst_results(instruction)[0].id,
            witnesses: witness_indices.into_iter().map(region_id).collect(),
            seeds,
            events,
            extractions,
            root_class,
        },
        normalized_root?,
        output_replacement?,
    ))
}

struct LocalClasses {
    parent: Vec<u32>,
}

impl LocalClasses {
    fn new(count: usize) -> Option<Self> {
        Some(Self {
            parent: (0..count)
                .map(|class| u32::try_from(class).ok())
                .collect::<Option<Vec<_>>>()?,
        })
    }

    fn push(&mut self) -> Option<u32> {
        let class = u32::try_from(self.parent.len()).ok()?;
        self.parent.push(class);
        Some(class)
    }

    fn find(&self, mut class: u32) -> Option<u32> {
        loop {
            let parent = *self.parent.get(class as usize)?;
            if parent == class {
                return Some(class);
            }
            class = parent;
        }
    }

    fn union(&mut self, left: u32, right: u32) -> Option<()> {
        let left = self.find(left)?;
        let right = self.find(right)?;
        if left == right {
            return None;
        }
        let root = left.min(right);
        let other = left.max(right);
        *self.parent.get_mut(other as usize)? = root;
        Some(())
    }
}

fn normalize_opportunity(
    event: &AppliedRewrite,
    nodes: &[EvpNode],
    value_classes: &HashMap<u32, u32>,
    classes: &LocalClasses,
) -> Option<RewriteOpportunity> {
    let original = RewriteOpportunity {
        rule: event.rule,
        replacement: event.replacement.clone(),
    };
    let required_before_constants = match (&event.instruction, event.rule, &event.replacement) {
        (Inst::Add(_, left, right), ScalarRewrite::AddZero, Replacement::Alias(value))
        | (Inst::Or(_, left, right), ScalarRewrite::OrZero, Replacement::Alias(value)) => {
            if value == left {
                vec![(right, 0)]
            } else if value == right {
                vec![(left, 0)]
            } else {
                return None;
            }
        }
        (Inst::Mul(_, left, right), ScalarRewrite::MulOne, Replacement::Alias(value)) => {
            if value == left {
                vec![(right, 1)]
            } else if value == right {
                vec![(left, 1)]
            } else {
                return None;
            }
        }
        (Inst::And(_, left, right), ScalarRewrite::AndZero, Replacement::Constant(0)) => {
            let mut values = Vec::new();
            if is_constant(left, &event.facts, 0) {
                values.push((left, 0));
            }
            if is_constant(right, &event.facts, 0) {
                values.push((right, 0));
            }
            values
        }
        _ => return Some(original),
    };
    if required_before_constants.into_iter().any(|(value, raw)| {
        class_has_before_constant(value, raw, nodes, value_classes, classes).unwrap_or(false)
    }) {
        Some(original)
    } else {
        constant_fold(&event.instruction, &event.facts)
    }
}

fn class_has_before_constant(
    value: &Value,
    raw: u64,
    nodes: &[EvpNode],
    value_classes: &HashMap<u32, u32>,
    classes: &LocalClasses,
) -> Option<bool> {
    let class = classes.find(*value_classes.get(&value.id)?)?;
    let raw = mask_raw(raw, &value.ty);
    Some(nodes.iter().any(|node| {
        node.source == EvpNodeSource::Before
            && node.operation == "const"
            && classes.find(node.owner) == Some(class)
            && matches!(node.attributes.as_slice(), [(name, value)] if name == "value" && *value == raw)
    }))
}

fn collect_closure(
    block: &evocc_evoir::Block,
    definitions: &HashMap<u32, usize>,
    index: usize,
    closure: &mut HashSet<usize>,
) -> Option<()> {
    if !closure.insert(index) {
        return Some(());
    }
    ingest_scalar_inst(&block.insts[index]).ok()?;
    for operand in scalar_operands(&block.insts[index]) {
        if let Some(definition) = definitions.get(&operand.id) {
            if *definition >= index {
                return None;
            }
            ingest_scalar_inst(&block.insts[*definition]).ok()?;
            collect_closure(block, definitions, *definition, closure)?;
        }
    }
    Some(())
}

fn before_node(
    instruction: &Inst,
    node: u32,
    owner: u32,
    values: &HashMap<u32, u32>,
) -> Option<EvpNode> {
    let result = inst_results(instruction).first().copied()?;
    let (operation, operands, attributes) = match instruction {
        Inst::Const(_, raw) => ("const", Vec::new(), vec![("value".to_owned(), *raw)]),
        Inst::Add(_, left, right) => ("add", vec![left, right], Vec::new()),
        Inst::Sub(_, left, right) => ("sub", vec![left, right], Vec::new()),
        Inst::Mul(_, left, right) => ("mul", vec![left, right], Vec::new()),
        Inst::And(_, left, right) => ("and", vec![left, right], Vec::new()),
        Inst::Or(_, left, right) => ("or", vec![left, right], Vec::new()),
        Inst::Xor(_, left, right) => ("xor", vec![left, right], Vec::new()),
        Inst::ICmp(_, comparison, left, right) => (
            match comparison {
                CmpOp::Eq => "icmp.eq",
                CmpOp::Ne => "icmp.ne",
                CmpOp::SLt => "icmp.slt",
                CmpOp::SLe => "icmp.sle",
                CmpOp::ULt => "icmp.ult",
                CmpOp::ULe => "icmp.ule",
            },
            vec![left, right],
            Vec::new(),
        ),
        _ => return None,
    };
    Some(EvpNode {
        node,
        owner,
        source: EvpNodeSource::Before,
        operation: operation.to_owned(),
        sort: packed_type(&result.ty)?,
        effect: EvpEffectClass::Pure,
        attributes,
        children: operands
            .into_iter()
            .map(|operand| values.get(&operand.id).copied())
            .collect::<Option<_>>()?,
    })
}

/// Where the node stream puts one rule variable.
///
/// Section 11.3 requires a binding to equal the class a checker derives from
/// the operation and children of the node the rewrite names, after the merges
/// recorded before it. A path into that node is therefore the only admissible
/// description of a variable; an operand of the instruction is not, because a
/// later replacement in the same witness closure may have remapped it onto a
/// different class holding an equal constant.
#[derive(Clone, Copy)]
struct BindingPath {
    child: usize,
    /// For a nested left pattern, the operation of the node the pattern passes
    /// through and the child of that node the variable names.
    through: Option<(&'static str, usize)>,
}

const LEFT: usize = 0;
const RIGHT: usize = 1;

impl BindingPath {
    const fn child(child: usize) -> Self {
        Self {
            child,
            through: None,
        }
    }

    const fn nested(child: usize, operation: &'static str, inner: usize) -> Self {
        Self {
            child,
            through: Some((operation, inner)),
        }
    }
}

/// Resolve one variable against the emitted node stream and the partition as it
/// stands before this rewrite.
fn binding_class(
    nodes: &[EvpNode],
    classes: &LocalClasses,
    node: u32,
    path: BindingPath,
) -> Option<u32> {
    let named = nodes.get(node as usize)?;
    let class = classes.find(*named.children.get(path.child)?)?;
    let Some((operation, inner)) = path.through else {
        return Some(class);
    };
    nodes
        .iter()
        .filter(|candidate| {
            candidate.source == EvpNodeSource::Before
                && candidate.operation == operation
                && classes.find(candidate.owner) == Some(class)
        })
        .find_map(|candidate| classes.find(*candidate.children.get(inner)?))
}

fn exact_rule(
    instruction: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    opportunity: &RewriteOpportunity,
) -> Option<(&'static str, Vec<(&'static str, BindingPath)>)> {
    let x = |child: usize| vec![("x", BindingPath::child(child))];
    let ab = || {
        vec![
            ("a", BindingPath::child(LEFT)),
            ("b", BindingPath::child(RIGHT)),
        ]
    };
    match (instruction, opportunity.rule, &opportunity.replacement) {
        (Inst::Add(_, left, right), ScalarRewrite::AddZero, Replacement::Alias(value)) => {
            if value == left { Some(("p0_001_add_zero_rhs", x(LEFT))) }
            else if value == right { Some(("p0_002_add_zero_lhs", x(RIGHT))) } else { None }
        }
        (Inst::Mul(_, left, right), ScalarRewrite::MulOne, Replacement::Alias(value)) => {
            if value == left { Some(("p0_003_mul_one_rhs", x(LEFT))) }
            else if value == right { Some(("p0_004_mul_one_lhs", x(RIGHT))) } else { None }
        }
        (Inst::And(_, left, right), ScalarRewrite::AndZero, Replacement::Constant(0)) => {
            if is_constant(right, facts, 0) { Some(("p0_005_and_zero_rhs", x(LEFT))) }
            else if is_constant(left, facts, 0) { Some(("p0_006_and_zero_lhs", x(RIGHT))) } else { None }
        }
        (Inst::Or(_, left, right), ScalarRewrite::OrZero, Replacement::Alias(value)) => {
            if value == left { Some(("p0_007_or_zero_rhs", x(LEFT))) }
            else if value == right { Some(("p0_008_or_zero_lhs", x(RIGHT))) } else { None }
        }
        (Inst::Sub(_, _, right), ScalarRewrite::DoubleNegation, Replacement::Alias(value)) => {
            matches!(facts.get(&right.id), Some(ScalarFact::Negation { operand }) if operand == value)
                .then(|| ("p0_009_double_neg", vec![("x", BindingPath::nested(RIGHT, "sub", RIGHT))]))
        }
        (Inst::ICmp(_, comparison, left, right), ScalarRewrite::IdentityComparison, Replacement::Constant(_)) if left == right => {
            Some((match comparison {
                CmpOp::Eq => "p0_010_icmp_eq_self", CmpOp::Ne => "p0_011_icmp_ne_self",
                CmpOp::SLt => "p0_012_icmp_slt_self", CmpOp::SLe => "p0_013_icmp_sle_self",
                CmpOp::ULt => "p0_014_icmp_ult_self", CmpOp::ULe => "p0_015_icmp_ule_self",
            }, x(LEFT)))
        }
        (instruction, ScalarRewrite::ConstantFold, Replacement::Constant(_)) => {
            let operands = scalar_operands(instruction);
            let [_left, _right] = operands.as_slice() else { return None; };
            let rule = match instruction {
                Inst::Add(..) => "p0_100_fold_add", Inst::Sub(..) => "p0_101_fold_sub",
                Inst::Mul(..) => "p0_102_fold_mul", Inst::And(..) => "p0_103_fold_and",
                Inst::Or(..) => "p0_104_fold_or", Inst::Xor(..) => "p0_105_fold_xor",
                Inst::ICmp(_, CmpOp::Eq, ..) => "p0_106_fold_icmp_eq",
                Inst::ICmp(_, CmpOp::Ne, ..) => "p0_107_fold_icmp_ne",
                Inst::ICmp(_, CmpOp::SLt, ..) => "p0_108_fold_icmp_slt",
                Inst::ICmp(_, CmpOp::SLe, ..) => "p0_109_fold_icmp_sle",
                Inst::ICmp(_, CmpOp::ULt, ..) => "p0_110_fold_icmp_ult",
                Inst::ICmp(_, CmpOp::ULe, ..) => "p0_111_fold_icmp_ule",
                _ => return None,
            };
            Some((rule, ab()))
        }
        _ => None,
    }
}

fn is_constant(value: &Value, facts: &HashMap<u32, ScalarFact>, raw: u64) -> bool {
    matches!(facts.get(&value.id), Some(ScalarFact::Constant { raw: actual, ty }) if ty == &value.ty && *actual == mask_raw(raw, ty))
}

enum SelectedSource {
    Value(u32),
    Node(u32),
}

fn extraction_manifest(
    nodes: &[EvpNode],
    seeds: &[EvpSeed],
    values: &HashMap<u32, u32>,
    selected: SelectedSource,
    replacement_root: u32,
    classes: &LocalClasses,
) -> Option<Vec<EvpExtraction>> {
    let (class, choice) = match selected {
        SelectedSource::Value(value) => {
            let class = *values.get(&value)?;
            if let Some(index) = seeds.iter().position(|seed| seed.value == value) {
                (class, EvpExtractionChoice::Seed(index as u32))
            } else {
                let node = nodes.iter().find(|node| node.owner == class)?.node;
                (class, EvpExtractionChoice::Node(node))
            }
        }
        SelectedSource::Node(node) => {
            let node = nodes.get(node as usize)?;
            (node.owner, EvpExtractionChoice::Node(node.node))
        }
    };
    let mut builder = ExtractionManifestBuilder {
        nodes,
        seeds,
        replacement_root,
        classes,
        seen: HashSet::new(),
        output: Vec::new(),
    };
    builder.append(class, choice)?;
    Some(builder.output)
}

struct ExtractionManifestBuilder<'a> {
    nodes: &'a [EvpNode],
    seeds: &'a [EvpSeed],
    replacement_root: u32,
    classes: &'a LocalClasses,
    seen: HashSet<u32>,
    output: Vec<EvpExtraction>,
}

impl ExtractionManifestBuilder<'_> {
    fn append(&mut self, class: u32, choice: EvpExtractionChoice) -> Option<()> {
        let final_class = self.classes.find(class)?;
        if self.seen.contains(&final_class) {
            return Some(());
        }
        if let EvpExtractionChoice::Node(node_id) = choice {
            let children = {
                let node = self.nodes.get(node_id as usize)?;
                let terminal_before_reuse =
                    node.source == EvpNodeSource::Before && node.owner != self.replacement_root;
                (!terminal_before_reuse).then(|| node.children.clone())
            };
            for child in children.into_iter().flatten() {
                let child_node = self.nodes.iter().find(|node| node.owner == child);
                let child_choice = if let Some(node) = child_node {
                    EvpExtractionChoice::Node(node.node)
                } else {
                    let seed = self.seeds.iter().position(|seed| seed.class == child)?;
                    EvpExtractionChoice::Seed(seed as u32)
                };
                self.append(child, child_choice)?;
            }
        }
        if self.seen.insert(final_class) {
            self.output.push(EvpExtraction {
                class: final_class,
                choice,
            });
        }
        Some(())
    }
}

fn apply(module: &Module, accepted: &[Accepted]) -> Result<(Module, ValueRemap), ()> {
    let mut output = module.clone();
    let mut value_remap = HashMap::new();
    for (function_index, function) in output.functions.iter_mut().enumerate() {
        let mut aliases = HashMap::new();
        for (block_index, block) in function.blocks.iter_mut().enumerate() {
            let mut rewritten = Vec::new();
            for (instruction_index, mut instruction) in
                std::mem::take(&mut block.insts).into_iter().enumerate()
            {
                let accepted = accepted.iter().find(|accepted| {
                    accepted.function == function_index
                        && accepted.block == block_index
                        && accepted.instruction == instruction_index
                });
                if let Some(accepted) = accepted {
                    if inst_results(&instruction)
                        .first()
                        .is_none_or(|result| result.id != accepted.root)
                    {
                        return Err(());
                    }
                    match &accepted.replacement {
                        Replacement::Alias(value) => {
                            aliases.insert(accepted.root, resolve_alias(value, &aliases));
                        }
                        Replacement::Constant(raw) => {
                            let result = inst_results(&instruction)[0].clone();
                            rewritten.push(Inst::Const(result.clone(), mask_raw(*raw, &result.ty)));
                        }
                    }
                } else {
                    remap_inst_uses(&mut instruction, &aliases);
                    rewritten.push(instruction);
                }
            }
            block.insts = rewritten;
            remap_inst_uses(&mut block.terminator, &aliases);
        }
        let identities = renumber(function)?;
        for (old, new) in identities {
            value_remap.insert((function.name.clone(), old), new);
        }
    }
    validate_module(&output).map_err(|_| ())?;
    Ok((output, value_remap))
}

fn renumber(function: &mut evocc_evoir::Function) -> Result<HashMap<u32, u32>, ()> {
    let mut identities = HashMap::new();
    let mut next = 0_u32;
    for block in &function.blocks {
        for argument in &block.args {
            if identities.insert(argument.value.id, next).is_some() {
                return Err(());
            }
            next = next.checked_add(1).ok_or(())?;
        }
        for instruction in &block.insts {
            for result in inst_results(instruction) {
                if identities.insert(result.id, next).is_some() {
                    return Err(());
                }
                next = next.checked_add(1).ok_or(())?;
            }
        }
    }
    for block in &mut function.blocks {
        for argument in &mut block.args {
            remap_value(&mut argument.value, &identities)?;
        }
        for instruction in &mut block.insts {
            remap_all(instruction, &identities)?;
        }
        remap_all(&mut block.terminator, &identities)?;
    }
    Ok(identities)
}

fn remap_value(value: &mut Value, identities: &HashMap<u32, u32>) -> Result<(), ()> {
    value.id = *identities.get(&value.id).ok_or(())?;
    Ok(())
}

fn remap_all(instruction: &mut Inst, identities: &HashMap<u32, u32>) -> Result<(), ()> {
    for value in all_values_mut(instruction) {
        remap_value(value, identities)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn all_values_mut(instruction: &mut Inst) -> Vec<&mut Value> {
    match instruction {
        Inst::Const(result, _) | Inst::Uninitialized(result) => vec![result],
        Inst::Add(result, left, right)
        | Inst::Sub(result, left, right)
        | Inst::Mul(result, left, right)
        | Inst::SRem(result, left, right)
        | Inst::URem(result, left, right)
        | Inst::Shl(result, left, right)
        | Inst::AShr(result, left, right)
        | Inst::LShr(result, left, right)
        | Inst::And(result, left, right)
        | Inst::Or(result, left, right)
        | Inst::Xor(result, left, right)
        | Inst::SignedAddDefined(result, left, right)
        | Inst::SignedSubDefined(result, left, right)
        | Inst::SignedMulDefined(result, left, right)
        | Inst::SignedDivRemNoOverflow(result, left, right)
        | Inst::SDiv(result, left, right)
        | Inst::UDiv(result, left, right)
        | Inst::ICmp(result, _, left, right) => vec![result, left, right],
        Inst::DivisorNonZero(result, divisor) => vec![result, divisor],
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            ..
        }
        | Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            ..
        } => vec![result, shifted, amount],
        Inst::UbAssume(condition, _) => vec![condition],
        Inst::Trap(_) => Vec::new(),
        Inst::SExt(result, source, _)
        | Inst::ZExt(result, source, _)
        | Inst::Trunc(result, source, _) => vec![result, source],
        Inst::PtrNull(result) | Inst::GlobalAddr { result, .. } | Inst::FuncAddr { result, .. } => {
            vec![result]
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            ..
        } => vec![result, base, index],
        Inst::Br(_, values) | Inst::Ret(values) => values.iter_mut().collect(),
        Inst::CondBr(condition, _, left, _, right) => {
            let mut values = vec![condition];
            values.extend(left);
            values.extend(right);
            values
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_in, world_in, mem_out, world_out],
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => vec![mem_in, world_in, pointer, mem_out, world_out],
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        }
        | Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            ..
        } => vec![mem_in, pointer, mem_out],
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, address, mem_in, world_in, mem_out, world_out],
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![address, value, mem_in, world_in, mem_out, world_out],
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![destination, source, mem_in, world_in, mem_out, world_out],
        Inst::Call {
            results,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.extend(args);
            values.extend([mem_in, world_in, mem_out, world_out]);
            values
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.push(callee);
            values.extend(args);
            values.extend([mem_in, world_in, mem_out, world_out]);
            values
        }
    }
}

fn packed_type(ty: &EvoIRType) -> Option<String> {
    match ty {
        EvoIRType::Bits(width @ 1..=64) => Some(format!("bits<{width}>")),
        _ => None,
    }
}
