//! EvoIR instruction values.

use super::*;

/// Returns all SSA values defined by an instruction, in textual result order.
#[must_use]
pub fn inst_results(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::TlsAddr(access) => access.results(),
        Inst::Vector(operation) => vec![&operation.result],
        Inst::Floating(operation) => vec![&operation.result],
        Inst::StrictFloating(operation) => {
            vec![&operation.instruction.result, &operation.world_out]
        }
        Inst::IndirectBr { .. } => vec![],
        Inst::PointerCast { result, .. } => vec![result],
        Inst::BitOperation { result, .. } => vec![result],
        Inst::VaAccess(access) => access.results(),
        Inst::Atomic(access) => access.results(),
        Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => assembly.results(),
        Inst::Const(v, _)
        | Inst::Uninitialized(v)
        | Inst::Add(v, ..)
        | Inst::Sub(v, ..)
        | Inst::Mul(v, ..)
        | Inst::SRem(v, ..)
        | Inst::URem(v, ..)
        | Inst::Shl(v, ..)
        | Inst::AShr(v, ..)
        | Inst::LShr(v, ..)
        | Inst::And(v, ..)
        | Inst::Or(v, ..)
        | Inst::Xor(v, ..)
        | Inst::SignedAddDefined(v, ..)
        | Inst::SignedSubDefined(v, ..)
        | Inst::SignedMulDefined(v, ..)
        | Inst::DivisorNonZero(v, ..)
        | Inst::SignedDivRemNoOverflow(v, ..)
        | Inst::SDiv(v, ..)
        | Inst::UDiv(v, ..)
        | Inst::ICmp(v, ..)
        | Inst::PtrNull(v)
        | Inst::SExt(v, ..)
        | Inst::ZExt(v, ..)
        | Inst::Trunc(v, ..) => vec![v],
        Inst::GlobalAddr { result, .. }
        | Inst::FuncAddr { result, .. }
        | Inst::BlockAddr { result, .. }
        | Inst::ReturnAddress { result }
        | Inst::FrameAddress { result, .. }
        | Inst::PtrOffset { result, .. } => vec![result],
        Inst::ShiftCountDefined { result, .. } | Inst::SignedShlDefined { result, .. } => {
            vec![result]
        }
        Inst::Load {
            result,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_out, world_out],
        Inst::Store {
            mem_out, world_out, ..
        }
        | Inst::Copy {
            mem_out, world_out, ..
        } => vec![mem_out, world_out],
        Inst::Alloca {
            result,
            mem_out,
            world_out,
            ..
        }
        | Inst::DynamicAlloca {
            result,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_out, world_out],
        Inst::LifetimeEnd {
            mem_out, world_out, ..
        } => vec![mem_out, world_out],
        Inst::PointerValid { mem_out, .. } | Inst::AccessInBounds { mem_out, .. } => vec![mem_out],
        Inst::Call {
            results,
            mem_out,
            world_out,
            ..
        }
        | Inst::IndirectCall {
            results,
            mem_out,
            world_out,
            ..
        } => {
            let mut definitions: Vec<&Value> = results.iter().collect();
            definitions.push(mem_out);
            definitions.push(world_out);
            definitions
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Trap(..) | Inst::UbAssume(..) => {
            vec![]
        }
    }
}

/// Returns all `Value` operands that this instruction *uses* (reads).
#[must_use]
pub fn inst_uses(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::TlsAddr(access) => access.operands(),
        Inst::Vector(operation) => operation.arguments.iter().collect(),
        Inst::Floating(operation) => operation.arguments.iter().collect(),
        Inst::StrictFloating(operation) => operation.operands(),
        Inst::IndirectBr { target, edges } => std::iter::once(target)
            .chain(edges.iter().flat_map(|edge| &edge.arguments))
            .collect(),
        Inst::PointerCast { operand, .. } => vec![operand],
        Inst::BitOperation { operand, .. } => vec![operand],
        Inst::VaAccess(access) => access.operands(),
        Inst::Atomic(access) => access.operands(),
        Inst::Asm(assembly) => assembly.operands(),
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            let mut uses = assembly.operands();
            for label in labels {
                uses.extend(&label.edge.arguments);
            }
            uses.extend(&fallthrough.arguments);
            uses
        }
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Trap(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. } => vec![],

        Inst::Add(_, lhs, rhs)
        | Inst::Sub(_, lhs, rhs)
        | Inst::Mul(_, lhs, rhs)
        | Inst::SRem(_, lhs, rhs)
        | Inst::URem(_, lhs, rhs)
        | Inst::Shl(_, lhs, rhs)
        | Inst::AShr(_, lhs, rhs)
        | Inst::LShr(_, lhs, rhs)
        | Inst::And(_, lhs, rhs)
        | Inst::Or(_, lhs, rhs)
        | Inst::Xor(_, lhs, rhs)
        | Inst::SignedAddDefined(_, lhs, rhs)
        | Inst::SignedSubDefined(_, lhs, rhs)
        | Inst::SignedMulDefined(_, lhs, rhs)
        | Inst::SignedDivRemNoOverflow(_, lhs, rhs)
        | Inst::SDiv(_, lhs, rhs)
        | Inst::UDiv(_, lhs, rhs) => vec![lhs, rhs],

        Inst::DivisorNonZero(_, divisor) => vec![divisor],
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => vec![shifted, amount],

        Inst::UbAssume(condition, _) => vec![condition],

        Inst::ICmp(_, _, lhs, rhs) => vec![lhs, rhs],

        Inst::Br(_, args) => args.iter().collect(),
        Inst::CondBr(cond, _, targs, _, fargs) => {
            let mut uses: Vec<&Value> = vec![cond];
            uses.extend(targs);
            uses.extend(fargs);
            uses
        }
        Inst::Ret(values) => values.iter().collect(),

        Inst::Load {
            address,
            mem_in,
            world_in,
            ..
        } => vec![address, mem_in, world_in],
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            ..
        } => vec![address, value, mem_in, world_in],
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            ..
        } => vec![destination, source, mem_in, world_in],
        Inst::Alloca {
            mem_in, world_in, ..
        } => vec![mem_in, world_in],
        Inst::DynamicAlloca {
            mem_in,
            world_in,
            size,
            ..
        } => vec![mem_in, world_in, size],
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            ..
        } => vec![mem_in, world_in, pointer],
        Inst::PointerValid {
            mem_in, pointer, ..
        }
        | Inst::AccessInBounds {
            mem_in, pointer, ..
        } => vec![mem_in, pointer],
        Inst::PtrOffset { base, index, .. } => vec![base, index],
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut uses: Vec<&Value> = args.iter().collect();
            uses.push(mem_in);
            uses.push(world_in);
            uses
        }
        Inst::IndirectCall {
            callee,
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut uses: Vec<&Value> = vec![callee];
            uses.extend(args);
            uses.push(mem_in);
            uses.push(world_in);
            uses
        }
        Inst::SExt(_, src, _) | Inst::ZExt(_, src, _) | Inst::Trunc(_, src, _) => vec![src],
    }
}
