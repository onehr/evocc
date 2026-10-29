//! Lowering value ids.

use super::*;

pub(super) fn replace_inst_target(instruction: &mut Inst, old: BlockId, new: BlockId) {
    match instruction {
        Inst::Br(target, _) => {
            if *target == old {
                *target = new;
            }
        }
        Inst::CondBr(_, true_target, _, false_target, _) => {
            if *true_target == old {
                *true_target = new;
            }
            if *false_target == old {
                *false_target = new;
            }
        }
        _ => {}
    }
}

pub(super) fn renumber_function(function: &mut Function) -> Result<HashMap<u32, u32>, LowerError> {
    let block_map = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            u32::try_from(index)
                .map(|new| (block.id, new))
                .map_err(|_| LowerError::InvalidGeneratedSsa {
                    message: "too many blocks to number".into(),
                })
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut value_map = HashMap::new();
    let mut next = 0_u32;
    for block in &function.blocks {
        for argument in &block.args {
            value_map.insert(argument.value.id, next);
            next = next
                .checked_add(1)
                .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                    message: "too many values to number".into(),
                })?;
        }
        for instruction in block.insts.iter().chain([&block.terminator]) {
            for result in inst_results(instruction) {
                value_map.insert(result.id, next);
                next = next
                    .checked_add(1)
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "too many values to number".into(),
                    })?;
            }
        }
    }
    for block in &mut function.blocks {
        block.id = *block_map
            .get(&block.id)
            .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                message: "block remap is incomplete".into(),
            })?;
        for argument in &mut block.args {
            remap_value(&mut argument.value, &value_map)?;
        }
        for instruction in &mut block.insts {
            remap_inst(instruction, &block_map, &value_map)?;
        }
        remap_inst(&mut block.terminator, &block_map, &value_map)?;
    }
    Ok(value_map)
}

pub(super) fn remap_value(
    value: &mut Value,
    value_map: &HashMap<u32, u32>,
) -> Result<(), LowerError> {
    value.id = *value_map
        .get(&value.id)
        .ok_or_else(|| LowerError::InvalidGeneratedSsa {
            message: format!("value {} has no final identity", value.id),
        })?;
    Ok(())
}

pub(super) fn remap_inst(
    instruction: &mut Inst,
    block_map: &HashMap<BlockId, BlockId>,
    value_map: &HashMap<u32, u32>,
) -> Result<(), LowerError> {
    for value in inst_values_mut(instruction) {
        remap_value(value, value_map)?;
    }
    match instruction {
        Inst::IndirectBr { edges, .. } => {
            for edge in edges.iter_mut() {
                edge.target = *block_map.get(&edge.target).ok_or_else(|| {
                    LowerError::InvalidGeneratedSsa {
                        message: "indirect edge has no final identity".into(),
                    }
                })?;
            }
            edges.sort_by_key(|edge| edge.target);
        }
        Inst::BlockAddr { block, .. } => {
            *block = *block_map
                .get(block)
                .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                    message: "code address has no final block identity".into(),
                })?;
        }
        Inst::AsmGoto {
            labels,
            fallthrough,
            ..
        } => {
            for edge in labels
                .iter_mut()
                .map(|label| &mut label.edge)
                .chain([fallthrough])
            {
                edge.target = *block_map.get(&edge.target).ok_or_else(|| {
                    LowerError::InvalidGeneratedSsa {
                        message: "assembly edge has no final identity".into(),
                    }
                })?;
            }
        }
        Inst::Br(target, _) => {
            *target = *block_map
                .get(target)
                .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                    message: format!("branch target {target} has no final identity"),
                })?;
        }
        Inst::CondBr(_, true_target, _, false_target, _) => {
            *true_target =
                *block_map
                    .get(true_target)
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "true branch has no final identity".into(),
                    })?;
            *false_target =
                *block_map
                    .get(false_target)
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "false branch has no final identity".into(),
                    })?;
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub(super) fn inst_values_mut(instruction: &mut Inst) -> Vec<&mut Value> {
    match instruction {
        Inst::Vector(operation) => operation.values_mut(),
        Inst::Floating(operation) => operation.values_mut(),
        Inst::StrictFloating(operation) => operation.values_mut(),
        Inst::Atomic(access) => access.values_mut(),
        Inst::TlsAddr(access) => access.values_mut(),
        Inst::IndirectBr { target, edges } => std::iter::once(target)
            .chain(edges.iter_mut().flat_map(|edge| &mut edge.arguments))
            .collect(),
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
            match &mut access.op {
                evocc_evoir::VariadicOp::Arg(result) => values.push(result),
                evocc_evoir::VariadicOp::ArgRecord { destination, .. } => values.push(destination),
                _ => {}
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
        Inst::PtrOffset {
            result,
            base,
            index,
            ..
        } => vec![result, base, index],
        Inst::PtrNull(result)
        | Inst::GlobalAddr { result, .. }
        | Inst::FuncAddr { result, .. }
        | Inst::BlockAddr { result, .. }
        | Inst::ReturnAddress { result }
        | Inst::FrameAddress { result, .. } => {
            vec![result]
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_in, world_in, mem_out, world_out],
        Inst::DynamicAlloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            size,
            ..
        } => vec![result, mem_in, world_in, mem_out, world_out, size],
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
        Inst::Br(_, arguments) | Inst::Ret(arguments) => arguments.iter_mut().collect(),
        Inst::CondBr(condition, _, true_args, _, false_args) => {
            let mut values = vec![condition];
            values.extend(true_args.iter_mut());
            values.extend(false_args.iter_mut());
            values
        }
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
            values.extend(args.iter_mut());
            values.push(mem_in);
            values.push(world_in);
            values.push(mem_out);
            values.push(world_out);
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
            ..
        } => {
            let mut values = results.iter_mut().collect::<Vec<_>>();
            values.push(callee);
            values.extend(args.iter_mut());
            values.push(mem_in);
            values.push(world_in);
            values.push(mem_out);
            values.push(world_out);
            values
        }
        Inst::SExt(result, source, _)
        | Inst::ZExt(result, source, _)
        | Inst::Trunc(result, source, _) => {
            vec![result, source]
        }
    }
}
