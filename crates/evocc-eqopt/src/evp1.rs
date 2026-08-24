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
    ingest_scalar_inst, mask_raw, optimize_scalar_module_with_events, record_fact, remap_inst_uses,
    resolve_alias, rewrite_opportunity, scalar_operands, Replacement, RewriteOpportunity,
    ScalarFact,
};
use crate::{ProofAuthority, ScalarEqBudget, ScalarEqReport, ScalarRewrite};

const REGISTRY_NAME: &str = "p0.scalar.v1";
const REGISTRY_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertifiedScalarEqOutcome {
    pub module: Module,
    pub certificate: EvpCertificate,
    pub report: ScalarEqReport,
}

/// Optimize only regions that can carry a self-contained current-transaction
/// EVP1 proof. More speculative or history-dependent opportunities are simply
/// omitted; optimization is never required for correctness.
#[must_use]
pub fn optimize_scalar_module_with_evp1(
    module: &Module,
    budget: ScalarEqBudget,
) -> CertifiedScalarEqOutcome {
    let input = render_module(module);
    let mut report = optimize_scalar_module_with_events(module, budget).0.report;
    report.receipts.clear();
    report.merges = 0;
    report.optimized_values = 0;
    report.proof_authority = ProofAuthority::Reviewed {
        registry: REGISTRY_NAME,
    };
    if report.fell_back {
        return zero(module, input, report);
    }

    let mut regions = Vec::new();
    let mut accepted = Vec::new();
    let mut proof_nodes = 0_usize;
    for (function_index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            let mut facts = HashMap::new();
            for (instruction_index, instruction) in block.insts.iter().enumerate() {
                let opportunity = rewrite_opportunity(instruction, &facts);
                if let Some(opportunity) = opportunity {
                    if let Some(region) = build_region(
                        module,
                        function_index,
                        block_index,
                        instruction_index,
                        instruction,
                        &facts,
                        &opportunity,
                    ) {
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
                            return zero(module, input, report);
                        }
                        proof_nodes += new_nodes;
                        accepted.push(Accepted {
                            function: function_index,
                            block: block_index,
                            instruction: instruction_index,
                            root: inst_results(instruction)[0].id,
                            replacement: opportunity.replacement.clone(),
                        });
                        regions.push(region);
                    }
                }
                record_fact(instruction, &mut facts);
            }
        }
    }
    let output = match apply(module, &accepted) {
        Ok(output) => output,
        Err(()) => {
            report.proof_replay_failed = true;
            report.fell_back = true;
            return zero(module, input, report);
        }
    };
    let output_text = render_module(&output);
    report.regions_admitted = regions.len();
    report.graph_nodes = proof_nodes;
    report.epochs = regions.len();
    report.merges = regions.len();
    report.optimized_values = regions.len();
    CertifiedScalarEqOutcome {
        module: output,
        certificate: EvpCertificate {
            registry_name: REGISTRY_NAME.to_owned(),
            registry_digest: REGISTRY_SHA256.to_owned(),
            input_digest: sha256_hex(input.as_bytes()),
            regions,
            output_digest: sha256_hex(output_text.as_bytes()),
        },
        report,
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

#[allow(clippy::too_many_arguments)]
fn build_region(
    module: &Module,
    function_index: usize,
    block_index: usize,
    instruction_index: usize,
    instruction: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    opportunity: &RewriteOpportunity,
) -> Option<EvpRegion> {
    ingest_scalar_inst(instruction).ok()?;
    let function = &module.functions[function_index];
    let block = &function.blocks[block_index];
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
    let mut nodes = Vec::new();
    for index in &witness_indices {
        let instruction = &block.insts[*index];
        let node_id = u32::try_from(nodes.len()).ok()?;
        let owner = u32::try_from(seeds.len() + nodes.len()).ok()?;
        let node = before_node(instruction, node_id, owner, &value_classes)?;
        value_classes.insert(inst_results(instruction)[0].id, owner);
        nodes.push(node);
    }
    let root_node = witness_indices
        .iter()
        .position(|index| *index == instruction_index)? as u32;
    let lhs = nodes[root_node as usize].owner;
    let (rule, binding_values) = exact_rule(instruction, facts, opportunity)?;
    let bindings = binding_values
        .iter()
        .map(|(name, value)| Some(((*name).to_owned(), *value_classes.get(&value.id)?)))
        .collect::<Option<Vec<_>>>()?;
    let (rhs, rhs_ref, selected) = match &opportunity.replacement {
        Replacement::Alias(value) => {
            let rhs = *value_classes.get(&value.id)?;
            (
                rhs,
                EvpProofRef::Class(rhs),
                SelectedSource::Value(value.id),
            )
        }
        Replacement::Constant(raw) => {
            let result = inst_results(instruction)[0];
            let node = u32::try_from(nodes.len()).ok()?;
            let owner = u32::try_from(seeds.len() + nodes.len()).ok()?;
            nodes.push(EvpNode {
                node,
                owner,
                source: EvpNodeSource::Derived,
                operation: "const".to_owned(),
                sort: packed_type(&result.ty)?,
                effect: EvpEffectClass::Pure,
                attributes: vec![("value".to_owned(), mask_raw(*raw, &result.ty))],
                children: Vec::new(),
            });
            (owner, EvpProofRef::Node(node), SelectedSource::Node(node))
        }
    };
    if lhs == rhs {
        return None;
    }
    let result = lhs.min(rhs);
    let mut events = nodes
        .iter()
        .cloned()
        .map(EvpEvent::Node)
        .collect::<Vec<_>>();
    events.push(EvpEvent::Rewrite(EvpRewrite {
        rule: rule.to_owned(),
        lhs_pre: lhs,
        rhs_pre: rhs,
        result,
        lhs: EvpProofRef::Node(root_node),
        rhs: rhs_ref,
        bindings,
    }));
    let extractions =
        extraction_manifest(&nodes, &seeds, &value_classes, selected, lhs, rhs, result)?;
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
    Some(EvpRegion {
        id: region_id(instruction_index),
        root_value: inst_results(instruction)[0].id,
        witnesses: witness_indices.into_iter().map(region_id).collect(),
        seeds,
        events,
        extractions,
        root_class: result,
    })
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

fn exact_rule<'a>(
    instruction: &Inst,
    facts: &HashMap<u32, ScalarFact>,
    opportunity: &RewriteOpportunity,
) -> Option<(&'static str, Vec<(&'a str, Value)>)> {
    let x = |value: &Value| vec![("x", value.clone())];
    let ab = |left: &Value, right: &Value| vec![("a", left.clone()), ("b", right.clone())];
    match (instruction, opportunity.rule, &opportunity.replacement) {
        (Inst::Add(_, left, right), ScalarRewrite::AddZero, Replacement::Alias(value)) => {
            if value == left { Some(("p0_001_add_zero_rhs", x(left))) }
            else if value == right { Some(("p0_002_add_zero_lhs", x(right))) } else { None }
        }
        (Inst::Mul(_, left, right), ScalarRewrite::MulOne, Replacement::Alias(value)) => {
            if value == left { Some(("p0_003_mul_one_rhs", x(left))) }
            else if value == right { Some(("p0_004_mul_one_lhs", x(right))) } else { None }
        }
        (Inst::And(_, left, right), ScalarRewrite::AndZero, Replacement::Constant(0)) => {
            if is_constant(right, facts, 0) { Some(("p0_005_and_zero_rhs", x(left))) }
            else if is_constant(left, facts, 0) { Some(("p0_006_and_zero_lhs", x(right))) } else { None }
        }
        (Inst::Or(_, left, right), ScalarRewrite::OrZero, Replacement::Alias(value)) => {
            if value == left { Some(("p0_007_or_zero_rhs", x(left))) }
            else if value == right { Some(("p0_008_or_zero_lhs", x(right))) } else { None }
        }
        (Inst::Sub(_, _, right), ScalarRewrite::DoubleNegation, Replacement::Alias(value)) => {
            matches!(facts.get(&right.id), Some(ScalarFact::Negation { operand }) if operand == value)
                .then(|| ("p0_009_double_neg", x(value)))
        }
        (Inst::ICmp(_, comparison, left, right), ScalarRewrite::IdentityComparison, Replacement::Constant(_)) if left == right => {
            Some((match comparison {
                CmpOp::Eq => "p0_010_icmp_eq_self", CmpOp::Ne => "p0_011_icmp_ne_self",
                CmpOp::SLt => "p0_012_icmp_slt_self", CmpOp::SLe => "p0_013_icmp_sle_self",
                CmpOp::ULt => "p0_014_icmp_ult_self", CmpOp::ULe => "p0_015_icmp_ule_self",
            }, x(left)))
        }
        (instruction, ScalarRewrite::ConstantFold, Replacement::Constant(_)) => {
            let operands = scalar_operands(instruction);
            let [left, right] = operands.as_slice() else { return None; };
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
            Some((rule, ab(left, right)))
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
    lhs: u32,
    rhs: u32,
    result: u32,
) -> Option<Vec<EvpExtraction>> {
    let canonical = |class: u32| {
        if class == lhs || class == rhs {
            result
        } else {
            class
        }
    };
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
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    append_extraction(
        class,
        choice,
        nodes,
        seeds,
        &canonical,
        &mut seen,
        &mut output,
    )?;
    Some(output)
}

fn append_extraction(
    class: u32,
    choice: EvpExtractionChoice,
    nodes: &[EvpNode],
    seeds: &[EvpSeed],
    canonical: &impl Fn(u32) -> u32,
    seen: &mut HashSet<u32>,
    output: &mut Vec<EvpExtraction>,
) -> Option<()> {
    let final_class = canonical(class);
    if seen.contains(&final_class) {
        return Some(());
    }
    if let EvpExtractionChoice::Node(node_id) = choice {
        let node = nodes.get(node_id as usize)?;
        for child in &node.children {
            let child_node = nodes.iter().find(|node| node.owner == *child);
            let child_choice = if let Some(node) = child_node {
                EvpExtractionChoice::Node(node.node)
            } else {
                let seed = seeds.iter().position(|seed| seed.class == *child)?;
                EvpExtractionChoice::Seed(seed as u32)
            };
            append_extraction(*child, child_choice, nodes, seeds, canonical, seen, output)?;
        }
    }
    if seen.insert(final_class) {
        output.push(EvpExtraction {
            class: final_class,
            choice,
        });
    }
    Some(())
}

fn apply(module: &Module, accepted: &[Accepted]) -> Result<Module, ()> {
    let mut output = module.clone();
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
        renumber(function)?;
    }
    validate_module(&output).map_err(|_| ())?;
    Ok(output)
}

fn renumber(function: &mut evocc_evoir::Function) -> Result<(), ()> {
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
    Ok(())
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
