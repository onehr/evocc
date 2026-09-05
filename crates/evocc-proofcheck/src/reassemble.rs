//! Mechanical multi-region reassembly and canonical value renumbering.

use std::collections::{BTreeMap, HashMap, HashSet};

use evocc_evoir::{inst_results, validate_module, CmpOp, EvoIRType, Inst, Module, Value};

use crate::replay::{RegionReplacement, Selected};

pub(crate) fn reassemble(
    before: &Module,
    replacements: Vec<RegionReplacement>,
) -> Result<Module, ()> {
    let mut module = before.clone();
    let mut grouped: HashMap<(String, String), BTreeMap<usize, RegionReplacement>> = HashMap::new();
    for replacement in replacements {
        let key = (replacement.function.clone(), replacement.block.clone());
        if grouped
            .entry(key)
            .or_default()
            .insert(replacement.instruction, replacement)
            .is_some()
        {
            return Err(());
        }
    }

    for function in &mut module.functions {
        let mut next = function
            .blocks
            .iter()
            .flat_map(|block| {
                block.args.iter().map(|argument| argument.value.id).chain(
                    block.insts.iter().flat_map(|instruction| {
                        inst_results(instruction).into_iter().map(|value| value.id)
                    }),
                )
            })
            .max()
            .map_or(Ok(0), |maximum| maximum.checked_add(1).ok_or(()))?;
        let mut aliases = HashMap::new();
        for (block_index, block) in function.blocks.iter_mut().enumerate() {
            let label = if block_index == 0 {
                "entry".to_owned()
            } else {
                format!("b{}", block.id)
            };
            let plans = grouped
                .remove(&(function.name.clone(), label))
                .unwrap_or_default();
            if plans.keys().any(|index| *index >= block.insts.len()) {
                return Err(());
            }
            let old = std::mem::take(&mut block.insts);
            let mut rewritten = Vec::new();
            for (instruction_index, mut instruction) in old.into_iter().enumerate() {
                if let Some(plan) = plans.get(&instruction_index) {
                    if inst_results(&instruction)
                        .first()
                        .is_none_or(|result| result.id != plan.root_value)
                    {
                        return Err(());
                    }
                    let mut selected = HashMap::new();
                    for selection in &plan.selections {
                        let value = match &selection.choice {
                            Selected::Existing(value) => resolve(value, &aliases),
                            Selected::Node(node) => {
                                let children = node
                                    .children
                                    .iter()
                                    .map(|class| selected.get(class).cloned().ok_or(()))
                                    .collect::<Result<Vec<_>, _>>()?;
                                let result = Value {
                                    id: next,
                                    ty: parse_bits(&node.sort)?,
                                };
                                next = next.checked_add(1).ok_or(())?;
                                rewritten.push(materialize(node, result.clone(), &children)?);
                                result
                            }
                        };
                        if selected.insert(selection.class, value).is_some() {
                            return Err(());
                        }
                    }
                    let replacement = selected.get(&plan.root_class).cloned().ok_or(())?;
                    aliases.insert(plan.root_value, replacement);
                } else {
                    remap_uses(&mut instruction, &aliases);
                    rewritten.push(instruction);
                }
            }
            block.insts = rewritten;
            remap_uses(&mut block.terminator, &aliases);
        }
        if grouped.keys().any(|(name, _)| name == &function.name) {
            return Err(());
        }
        renumber(function)?;
    }
    if !grouped.is_empty() {
        return Err(());
    }
    validate_module(&module).map_err(|_| ())?;
    Ok(module)
}

fn materialize(node: &evocc_proof::EvpNode, result: Value, children: &[Value]) -> Result<Inst, ()> {
    let binary = || -> Result<(Value, Value), ()> {
        let [left, right] = children else {
            return Err(());
        };
        Ok((left.clone(), right.clone()))
    };
    Ok(match node.operation.as_str() {
        "const" => {
            let [(name, raw)] = node.attributes.as_slice() else {
                return Err(());
            };
            if name != "value" || !children.is_empty() {
                return Err(());
            }
            Inst::Const(result, *raw)
        }
        "add" => {
            let (left, right) = binary()?;
            Inst::Add(result, left, right)
        }
        "sub" => {
            let (left, right) = binary()?;
            Inst::Sub(result, left, right)
        }
        "mul" => {
            let (left, right) = binary()?;
            Inst::Mul(result, left, right)
        }
        "and" => {
            let (left, right) = binary()?;
            Inst::And(result, left, right)
        }
        "or" => {
            let (left, right) = binary()?;
            Inst::Or(result, left, right)
        }
        "xor" => {
            let (left, right) = binary()?;
            Inst::Xor(result, left, right)
        }
        // Admitted to the Section 8.5 allowlist at revision 1.9.44 and emitted
        // by `p2_302_add_self_shl`. None of the three may trap; a shift's
        // definedness guard is a separate predicate and is not part of the
        // instruction this materializes.
        "shl" => {
            let (left, right) = binary()?;
            Inst::Shl(result, left, right)
        }
        "lshr" => {
            let (left, right) = binary()?;
            Inst::LShr(result, left, right)
        }
        "ashr" => {
            let (left, right) = binary()?;
            Inst::AShr(result, left, right)
        }
        operation if operation.starts_with("icmp.") => {
            let comparison = match operation {
                "icmp.eq" => CmpOp::Eq,
                "icmp.ne" => CmpOp::Ne,
                "icmp.slt" => CmpOp::SLt,
                "icmp.sle" => CmpOp::SLe,
                "icmp.ult" => CmpOp::ULt,
                "icmp.ule" => CmpOp::ULe,
                _ => return Err(()),
            };
            let (left, right) = binary()?;
            Inst::ICmp(result, comparison, left, right)
        }
        _ => return Err(()),
    })
}

fn parse_bits(sort: &str) -> Result<EvoIRType, ()> {
    let width = sort
        .strip_prefix("bits<")
        .and_then(|sort| sort.strip_suffix('>'))
        .and_then(|width| width.parse::<u16>().ok())
        .filter(|width| (1..=64).contains(width))
        .ok_or(())?;
    Ok(EvoIRType::Bits(width))
}

fn resolve(value: &Value, aliases: &HashMap<u32, Value>) -> Value {
    let mut current = value.clone();
    let mut visited = HashSet::new();
    while visited.insert(current.id) {
        let Some(next) = aliases.get(&current.id) else {
            break;
        };
        current = next.clone();
    }
    current
}

fn remap(value: &mut Value, aliases: &HashMap<u32, Value>) {
    *value = resolve(value, aliases);
}

#[allow(clippy::too_many_lines)]
fn remap_uses(instruction: &mut Inst, aliases: &HashMap<u32, Value>) {
    match instruction {
        Inst::PointerCast { operand, .. } => remap(operand, aliases),
        Inst::BitOperation { operand, .. } => remap(operand, aliases),
        Inst::VaAccess(access) => {
            remap(&mut access.list, aliases);
            remap(&mut access.mem_in, aliases);
            remap(&mut access.world_in, aliases);
        }
        Inst::Asm(assembly) => {
            for value in assembly.operands_mut() {
                remap(value, aliases);
            }
        }
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            for value in assembly.operands_mut() {
                remap(value, aliases);
            }
            for edge in labels
                .iter_mut()
                .map(|label| &mut label.edge)
                .chain([fallthrough])
            {
                for value in &mut edge.arguments {
                    remap(value, aliases);
                }
            }
        }
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
        Inst::DivisorNonZero(_, value) | Inst::UbAssume(value, _) => remap(value, aliases),
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => {
            remap(shifted, aliases);
            remap(amount, aliases);
        }
        Inst::SExt(_, source, _) | Inst::ZExt(_, source, _) | Inst::Trunc(_, source, _) => {
            remap(source, aliases)
        }
        Inst::Br(_, arguments) | Inst::Ret(arguments) => {
            arguments.iter_mut().for_each(|value| remap(value, aliases))
        }
        Inst::CondBr(condition, _, true_arguments, _, false_arguments) => {
            remap(condition, aliases);
            true_arguments
                .iter_mut()
                .chain(false_arguments)
                .for_each(|value| remap(value, aliases));
        }
        Inst::PtrOffset { base, index, .. } => {
            remap(base, aliases);
            remap(index, aliases);
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
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            ..
        } => {
            remap(destination, aliases);
            remap(source, aliases);
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            args.iter_mut().for_each(|value| remap(value, aliases));
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
            args.iter_mut().for_each(|value| remap(value, aliases));
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
    }
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
        for instruction in block.insts.iter().chain([&block.terminator]) {
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
            remap_identity(&mut argument.value, &identities)?;
        }
        for instruction in &mut block.insts {
            remap_all(instruction, &identities)?;
        }
        remap_all(&mut block.terminator, &identities)?;
    }
    Ok(())
}

fn remap_identity(value: &mut Value, identities: &HashMap<u32, u32>) -> Result<(), ()> {
    value.id = *identities.get(&value.id).ok_or(())?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn all_values_mut(instruction: &mut Inst) -> Vec<&mut Value> {
    match instruction {
        Inst::PointerCast { result, operand } => vec![result, operand],
        Inst::BitOperation {
            result, operand, ..
        } => vec![result, operand],
        Inst::VaAccess(access) => {
            let mut values = vec![
                &mut access.list,
                &mut access.mem_in,
                &mut access.world_in,
                &mut access.mem_out,
                &mut access.world_out,
            ];
            if let evocc_evoir::VariadicOp::Arg(result) = &mut access.op {
                values.push(result);
            }
            values
        }
        Inst::Asm(assembly) => assembly.values_mut(),
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            let mut values = assembly.values_mut();
            for edge in labels
                .iter_mut()
                .map(|label| &mut label.edge)
                .chain([fallthrough])
            {
                values.extend(edge.arguments.iter_mut());
            }
            values
        }
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

fn remap_all(instruction: &mut Inst, identities: &HashMap<u32, u32>) -> Result<(), ()> {
    for value in all_values_mut(instruction) {
        remap_identity(value, identities)?;
    }
    Ok(())
}
