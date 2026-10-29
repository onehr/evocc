//! EvoIR value aliases.

use super::*;

/// Follow an alias chain to the value it ends at.
///
/// The visited set is what makes a cyclic alias table terminate rather than
/// hang; a cycle is a caller's defect and resolving it to the value the walk
/// re-entered is the answer that keeps every other operand usable.
pub fn resolve_alias(value: &Value, aliases: &HashMap<u32, Value>) -> Value {
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

pub(super) fn remap(value: &mut Value, aliases: &HashMap<u32, Value>) {
    *value = resolve_alias(value, aliases);
}

#[allow(clippy::too_many_lines)]
pub fn remap_inst_uses(inst: &mut Inst, aliases: &HashMap<u32, Value>) {
    match inst {
        Inst::StrictFloating(operation) => {
            remap(&mut operation.world_in, aliases);
            for value in &mut operation.instruction.arguments {
                remap(value, aliases);
            }
        }
        Inst::Floating(operation) => {
            for value in &mut operation.arguments {
                remap(value, aliases);
            }
        }
        Inst::TlsAddr(a) => {
            remap(&mut a.mem_in, aliases);
            remap(&mut a.world_in, aliases);
        }
        Inst::Vector(operation) => {
            for value in &mut operation.arguments {
                remap(value, aliases);
            }
        }
        Inst::Atomic(access) => {
            for value in access.operands_mut() {
                remap(value, aliases);
            }
        }
        Inst::IndirectBr { target, edges } => {
            remap(target, aliases);
            for value in edges.iter_mut().flat_map(|edge| &mut edge.arguments) {
                remap(value, aliases);
            }
        }
        Inst::PointerCast { operand, .. } => remap(operand, aliases),
        Inst::BitOperation { operand, .. } => remap(operand, aliases),
        Inst::VaAccess(access) => {
            remap(&mut access.list, aliases);
            remap(&mut access.mem_in, aliases);
            remap(&mut access.world_in, aliases);
            if let VariadicOp::ArgRecord { destination, .. } = &mut access.op {
                remap(destination, aliases);
            }
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
        | Inst::FuncAddr { .. }
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. } => {}
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
        Inst::Alloca {
            mem_in, world_in, ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
        }
        Inst::DynamicAlloca {
            mem_in,
            world_in,
            size,
            ..
        } => {
            remap(mem_in, aliases);
            remap(world_in, aliases);
            remap(size, aliases);
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
