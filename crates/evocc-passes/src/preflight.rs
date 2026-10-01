//! Read-only opportunity inspection for the frozen N03b/N03e slices.
use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{inst_results, Block, Function, GlobalVisibility, Inst, Linkage, Module, Value};
use evocc_randgate::Observation;
use evocc_types::{AddressSpace, EvoIRType, Region};

use crate::{ExecutionInput, ValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePermission {
    Allowed,
    Barrier,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpportunityCounts {
    pub candidates: u64,
    pub stages: BTreeMap<&'static str, u64>,
    pub structural_sites: u64,
    pub admitted_sites: u64,
    pub declined: BTreeMap<&'static str, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    pub inline: OpportunityCounts,
    pub edge_reuse: OpportunityCounts,
    pub module_admission: &'static str,
    pub executed_inputs: u64,
    pub admission_detail: Option<String>,
}

/// Inspect the frozen input only. `admitted_sites` means structurally eligible
/// sites in a module admitted on the supplied inputs, NOT dynamic site coverage
/// and NOT acceptance of a transformation. This function never modifies IR.
pub fn inspect(
    module: &Module,
    source_permissions: &BTreeMap<String, SourcePermission>,
    inputs: &[ExecutionInput],
) -> Result<PreflightReport, ValidationError> {
    evocc_evoir::validate_module(module).map_err(ValidationError::InvalidInput)?;
    let mut report = PreflightReport {
        inline: inspect_calls(module, source_permissions),
        edge_reuse: inspect_edges(module),
        module_admission: "NO_EXECUTION_INPUTS",
        executed_inputs: 0,
        admission_detail: None,
    };
    if let Err(error) = super::admission(module) {
        report.module_admission = "OPAQUE";
        report.admission_detail = Some(error.to_string());
        return Ok(report);
    }
    if inputs.is_empty() {
        return Ok(report);
    }
    let mut ids = BTreeSet::new();
    let mut defined = 0;
    for input in inputs {
        if input.id.is_empty() || !ids.insert(&input.id) {
            return Err(ValidationError::InvalidInput(
                "empty or duplicate execution input ID".into(),
            ));
        }
        match super::execute(module, input) {
            Ok((observation, _)) => {
                report.executed_inputs += 1;
                defined += u64::from(matches!(observation, Observation::Defined(_)));
            }
            Err(error) => {
                report.module_admission = "EXECUTION_INELIGIBLE";
                report.admission_detail = Some(error.to_string());
                return Ok(report);
            }
        }
    }
    if defined == 0 {
        report.module_admission = "DIAGNOSTIC_INPUTS_ONLY";
        return Ok(report);
    }
    report.module_admission = "ADMITTED_ON_INPUTS";
    report.inline.admitted_sites = report.inline.structural_sites;
    report.edge_reuse.admitted_sites = report.edge_reuse.structural_sites;
    Ok(report)
}

impl OpportunityCounts {
    fn stage(&mut self, name: &'static str) {
        *self.stages.entry(name).or_default() += 1;
    }

    fn decline(&mut self, name: &'static str) {
        *self.declined.entry(name).or_default() += 1;
    }

    pub fn accumulate(&mut self, other: &Self) {
        self.candidates += other.candidates;
        self.structural_sites += other.structural_sites;
        self.admitted_sites += other.admitted_sites;
        for (key, count) in &other.stages {
            *self.stages.entry(key).or_default() += count;
        }
        for (key, count) in &other.declined {
            *self.declined.entry(key).or_default() += count;
        }
    }

    pub fn render_json(&self) -> String {
        fn counts(values: &BTreeMap<&str, u64>) -> String {
            values
                .iter()
                .map(|(key, count)| format!("{}:{count}", super::json_string(key)))
                .collect::<Vec<_>>()
                .join(",")
        }
        format!("{{\"candidates\":{},\"stages\":{{{}}},\"structural_sites\":{},\"admitted_sites\":{},\"declined\":{{{}}}}}", self.candidates, counts(&self.stages), self.structural_sites, self.admitted_sites, counts(&self.declined))
    }
}

impl PreflightReport {
    pub fn render_json(&self) -> String {
        let detail = self
            .admission_detail
            .as_deref()
            .map_or_else(|| "null".into(), super::json_string);
        format!("{{\"schema\":\"evocc-opportunity-preflight-v1\",\"transforms\":0,\"module_admission\":{},\"admission_detail\":{detail},\"executed_inputs\":{},\"inline\":{},\"edge_reuse\":{}}}", super::json_string(self.module_admission), self.executed_inputs, self.inline.render_json(), self.edge_reuse.render_json())
    }
}

fn ordinary(ty: &EvoIRType) -> bool {
    matches!(
        ty,
        EvoIRType::Bits(1..=128) | EvoIRType::Ptr(AddressSpace::Generic)
    )
}

fn signature(types: &[EvoIRType]) -> bool {
    types.len() >= 2
        && types[types.len() - 2..] == [EvoIRType::Mem(Region::All), EvoIRType::World]
        && types[..types.len() - 2].iter().all(ordinary)
}

fn inspect_calls(
    module: &Module,
    permissions: &BTreeMap<String, SourcePermission>,
) -> OpportunityCounts {
    let mut report = OpportunityCounts::default();
    let definitions: BTreeMap<_, _> = module
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let mut uses = BTreeMap::<&str, u64>::new();
    let mut other_uses = BTreeSet::new();
    let mut poison = false;
    let mut total = 0;
    for function in &module.functions {
        for block in &function.blocks {
            total += block.insts.len();
            for inst in &block.insts {
                match inst {
                    Inst::Call { callee, .. } => *uses.entry(callee).or_default() += 1,
                    Inst::FuncAddr { symbol, .. } | Inst::GlobalAddr { symbol, .. } => {
                        other_uses.insert(symbol.as_str());
                    }
                    Inst::Uninitialized(_) => poison = true,
                    _ => {}
                }
            }
        }
    }
    for alias in &module.aliases {
        other_uses.insert(alias.target.as_str());
        other_uses.insert(alias.name.as_str());
    }
    for global in &module.globals {
        for relocation in &global.relocs {
            other_uses.extend(relocation.symbols());
        }
    }
    for registration in &module.lifecycle {
        other_uses.insert(registration.function.as_str());
    }
    let mut growth = 0;
    for function in &module.functions {
        for inst in function.blocks.iter().flat_map(|b| &b.insts) {
            let Inst::Call {
                callee, arg_marks, ..
            } = inst
            else {
                continue;
            };
            report.candidates += 1;
            let shape = definitions
                .get(callee.as_str())
                .copied()
                .ok_or("external-or-alias")
                .and_then(|target| {
                    if target.linkage != Linkage::Strong
                        || target.visibility != GlobalVisibility::Internal
                    {
                        return Err("not-internal-strong");
                    }
                    if uses.get(callee.as_str()) != Some(&1) {
                        return Err("multiple-direct-sites");
                    }
                    if other_uses.contains(callee.as_str()) || !module.assembly.is_empty() {
                        return Err("other-symbol-reference");
                    }
                    report.stage("internal-strong-single-site");
                    leaf_shape(target, arg_marks.is_empty())
                });
            if shape.is_ok() {
                report.stage("leaf-no-frame-objects");
            }
            match permissions.get(callee) {
                None => {
                    report.decline("source-policy-unknown");
                    continue;
                }
                Some(SourcePermission::Barrier) => {
                    report.decline("source-barrier");
                    continue;
                }
                Some(SourcePermission::Allowed) => report.stage("source-policy-allowed"),
            }
            let size = match shape {
                Ok(size) => size,
                Err(reason) => {
                    report.decline(reason);
                    continue;
                }
            };
            if poison {
                report.decline("module-poison-source");
                continue;
            }
            if report.structural_sites >= 32 || growth + size > 1024 || growth + size > total / 4 {
                report.decline("structural-growth-budget");
                continue;
            }
            growth += size;
            report.stage("structural-budget-admitted");
            report.structural_sites += 1;
        }
    }
    report
}

fn leaf_shape(function: &Function, ordinary_call: bool) -> Result<usize, &'static str> {
    let params = function
        .params
        .iter()
        .map(|p| p.ty.clone())
        .collect::<Vec<_>>();
    if function.variadic
        || !function.param_marks.is_empty()
        || !ordinary_call
        || !signature(&params)
        || !signature(&function.ret_tys)
    {
        return Err("callee-signature");
    }
    if function.blocks.len() > 4 {
        return Err("callee-block-budget");
    }
    let count: usize = function.blocks.iter().map(|b| b.insts.len()).sum();
    if count > 32 {
        return Err("callee-instruction-budget");
    }
    if !cyclic_blocks(function).is_empty() {
        return Err("callee-cycle");
    }
    for block in &function.blocks {
        if !matches!(
            block.terminator,
            Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(_)
        ) {
            return Err("callee-terminator");
        }
        for inst in &block.insts {
            if matches!(inst, Inst::Call { .. } | Inst::IndirectCall { .. }) {
                return Err("callee-not-leaf");
            }
            if matches!(
                inst,
                Inst::Alloca { .. }
                    | Inst::DynamicAlloca { .. }
                    | Inst::LifetimeEnd { .. }
                    | Inst::FrameAddress { .. }
                    | Inst::ReturnAddress { .. }
            ) {
                return Err("callee-frame-sensitive");
            }
            if !matches!(
                inst,
                Inst::Const(..)
                    | Inst::Add(..)
                    | Inst::Sub(..)
                    | Inst::Mul(..)
                    | Inst::And(..)
                    | Inst::Or(..)
                    | Inst::Xor(..)
                    | Inst::Shl(..)
                    | Inst::AShr(..)
                    | Inst::LShr(..)
                    | Inst::SDiv(..)
                    | Inst::UDiv(..)
                    | Inst::SRem(..)
                    | Inst::URem(..)
                    | Inst::ICmp(..)
                    | Inst::SignedAddDefined(..)
                    | Inst::SignedSubDefined(..)
                    | Inst::SignedMulDefined(..)
                    | Inst::DivisorNonZero(..)
                    | Inst::SignedDivRemNoOverflow(..)
                    | Inst::ShiftCountDefined { .. }
                    | Inst::SignedShlDefined { .. }
                    | Inst::UbAssume(..)
                    | Inst::SExt(..)
                    | Inst::ZExt(..)
                    | Inst::Trunc(..)
                    | Inst::BitOperation { .. }
                    | Inst::PtrNull(_)
                    | Inst::GlobalAddr { .. }
                    | Inst::PointerCast { .. }
                    | Inst::PtrOffset { .. }
                    | Inst::PointerValid { .. }
                    | Inst::AccessInBounds { .. }
                    | Inst::Load {
                        volatile: false,
                        ..
                    }
                    | Inst::Store {
                        volatile: false,
                        ..
                    }
            ) {
                return Err("callee-opcode");
            }
            if inst_results(inst).into_iter().any(|v| {
                !ordinary(&v.ty) && !matches!(v.ty, EvoIRType::Mem(Region::All) | EvoIRType::World)
            }) {
                return Err("callee-value-type");
            }
        }
    }
    Ok(count)
}

fn direct_edges(block: &Block) -> Vec<(u32, &[Value])> {
    match &block.terminator {
        Inst::Br(target, args) => vec![(*target, args)],
        Inst::CondBr(_, yes, yes_args, no, no_args) => vec![(*yes, yes_args), (*no, no_args)],
        _ => Vec::new(),
    }
}

fn cyclic_blocks(function: &Function) -> BTreeSet<u32> {
    let outgoing: BTreeMap<_, _> = function
        .blocks
        .iter()
        .map(|block| {
            (
                block.id,
                direct_edges(block)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut cyclic = BTreeSet::new();
    for block in &function.blocks {
        let mut pending = outgoing[&block.id].clone();
        let mut seen = BTreeSet::new();
        while let Some(next) = pending.pop() {
            if next == block.id {
                cyclic.insert(block.id);
                break;
            }
            if seen.insert(next) {
                if let Some(edges) = outgoing.get(&next) {
                    pending.extend(edges);
                }
            }
        }
    }
    cyclic
}

fn binary(inst: &Inst) -> Option<(u8, &Value, &Value, &Value)> {
    let (op, result, a, b) = match inst {
        Inst::Add(r, a, b) => (0, r, a, b),
        Inst::Sub(r, a, b) => (1, r, a, b),
        Inst::Mul(r, a, b) => (2, r, a, b),
        Inst::And(r, a, b) => (3, r, a, b),
        Inst::Or(r, a, b) => (4, r, a, b),
        Inst::Xor(r, a, b) => (5, r, a, b),
        _ => return None,
    };
    (matches!(result.ty, EvoIRType::Bits(1..=64)) && a.ty == result.ty && b.ty == result.ty)
        .then_some((op, result, a, b))
}

fn inspect_edges(module: &Module) -> OpportunityCounts {
    let mut report = OpportunityCounts::default();
    for function in &module.functions {
        let mut local = OpportunityCounts::default();
        let count: usize = function.blocks.iter().map(|b| b.insts.len()).sum();
        let cap = function.blocks.len() > 128 || count > 4096;
        let opaque_edges = function
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Inst::IndirectBr { .. } | Inst::AsmGoto { .. }));
        let cycles = if cap {
            BTreeSet::new()
        } else {
            cyclic_blocks(function)
        };
        let mut checks = 0;
        let mut exhausted = false;
        let mut matches = Vec::new();
        for (block_index, block) in function.blocks.iter().enumerate() {
            for inst in &block.insts {
                let Some((op, root, a, b)) = binary(inst) else {
                    continue;
                };
                local.candidates += 1;
                let reason = if cap {
                    Some("function-scan-budget")
                } else if exhausted {
                    Some("edge-check-budget")
                } else if block_index == 0 {
                    Some("entry-block")
                } else if opaque_edges {
                    Some("indirect-incoming-edges")
                } else if cycles.contains(&block.id) {
                    Some("cyclic-destination")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    local.decline(reason);
                    continue;
                }
                let Some(ai) = block.args.iter().position(|p| p.value == *a) else {
                    local.decline("operands-not-block-parameters");
                    continue;
                };
                let Some(bi) = block.args.iter().position(|p| p.value == *b) else {
                    local.decline("operands-not-block-parameters");
                    continue;
                };
                let parameters: Vec<_> = block
                    .args
                    .iter()
                    .enumerate()
                    .filter(|(i, p)| *i != ai && *i != bi && p.value.ty == root.ty)
                    .map(|(i, _)| i)
                    .collect();
                if parameters.is_empty() {
                    local.decline("no-existing-result-parameter");
                    continue;
                }
                local.stage("existing-result-parameter");
                let incoming: Vec<_> = function
                    .blocks
                    .iter()
                    .flat_map(|predecessor| {
                        direct_edges(predecessor)
                            .into_iter()
                            .filter(move |(target, _)| *target == block.id)
                            .map(move |(_, args)| (predecessor, args))
                    })
                    .collect();
                if incoming.is_empty() {
                    local.decline("no-incoming-edges");
                    continue;
                }
                if incoming.iter().any(|(p, _)| cycles.contains(&p.id)) {
                    local.decline("cyclic-predecessor");
                    continue;
                }
                let mut found = None;
                for pi in parameters {
                    let mut producers = Vec::new();
                    for (predecessor, args) in &incoming {
                        checks += 1;
                        if checks > 16384 {
                            exhausted = true;
                            break;
                        }
                        let supplied = &args[pi];
                        let matching = predecessor.insts.iter().filter_map(binary).find(
                            |(source_op, result, left, right)| {
                                *source_op == op
                                    && *result == supplied
                                    && result.ty == root.ty
                                    && *left == &args[ai]
                                    && *right == &args[bi]
                            },
                        );
                        let Some((_, producer, _, _)) = matching else {
                            break;
                        };
                        producers.push(producer.id);
                    }
                    if exhausted {
                        break;
                    }
                    if producers.len() == incoming.len() {
                        found = Some(producers);
                        break;
                    }
                }
                if exhausted {
                    local.decline("edge-check-budget");
                } else if let Some(producers) = found {
                    local.stage("all-edge-structural-matches");
                    matches.push((root.id, producers));
                } else {
                    local.decline("incoming-edge-mismatch");
                }
            }
        }
        // Exhaustion declines the entire function, including earlier matches.
        // Protect all original producers before selecting any deletion root.
        let protected: BTreeSet<_> = matches
            .iter()
            .flat_map(|(_, producers)| producers.iter().copied())
            .collect();
        for (root, _) in matches {
            if exhausted {
                local.decline("edge-check-budget");
            } else if protected.contains(&root) {
                local.decline("supporting-producer-protected");
                local.stage("producer-roots-protected");
            } else if report.structural_sites + local.structural_sites >= 64 {
                local.decline("module-site-budget");
            } else {
                local.structural_sites += 1;
            }
        }
        local.stages.insert("edge-binding-checks", checks);
        report.accumulate(&local);
    }
    report
}
