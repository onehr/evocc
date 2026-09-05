//! Section 8.5's two monotone fixed points, computed for the producer.
//!
//! Section 11.6 keeps eligibility and seed derivation unshared, because a
//! defect the producer and the checker held in common would make both agree
//! that an ineligible region was eligible and the certificate would be
//! accepted. This module is therefore the producer's own answer, written from
//! Section 8.5 and never read by the checker, which computes its own.
//!
//! Two facts about a value decide admission. It is *tainted* when it is the
//! result of `uninitialized`, when any operand it names is tainted, or when it
//! is a block argument and any terminator edge entering that block passes a
//! tainted value in its position. It is *excluded* when its type is `mem<R>`,
//! `world` or a pointer, when its defining instruction is absent from the
//! operator allowlist, may trap or carries a non-pure effect, or when
//! exclusion reaches it from an operand or across an edge. Both are least
//! fixed points of a monotone step, so any iteration order that runs to
//! quiescence reaches the same set; this one runs the passes of Section 8.6.

use std::collections::{HashMap, HashSet};

use evocc_evoir::{inst_results, BlockId, Function, Inst, Value};
use evocc_proof::EvpSeedOrigin;
use evocc_types::EvoIRType;

/// One function's admission facts, computed once before any region is formed.
///
/// Section 8.5 is explicit that taint is a property of the value rather than of
/// an expression extracted afterwards, so it is answered here and consulted,
/// never re-derived from a region.
pub(crate) struct Admission {
    tainted: HashSet<u32>,
    excluded: HashSet<u32>,
    block_arguments: HashSet<u32>,
}

impl Admission {
    pub(crate) fn of(function: &Function) -> Self {
        Self {
            tainted: tainted_values(function),
            excluded: excluded_values(function),
            block_arguments: function
                .blocks
                .iter()
                .flat_map(|block| block.args.iter().map(|argument| argument.value.id))
                .collect(),
        }
    }

    /// Whether a value may be the root, a witness result or a seed of a region.
    pub(crate) fn admits(&self, value: u32) -> bool {
        !self.tainted.contains(&value) && !self.excluded.contains(&value)
    }

    /// Section 8.7: `input` for a block argument, `definition` for a value
    /// defined earlier in the function.
    pub(crate) fn origin(&self, value: u32) -> EvpSeedOrigin {
        if self.block_arguments.contains(&value) {
            EvpSeedOrigin::Input
        } else {
            EvpSeedOrigin::Definition
        }
    }
}

/// Section 8.5's operator allowlist, as a judgment on one instruction.
///
/// The list is the authority rather than the reasoning behind it, so this is a
/// closed match with no default arm that admits. The sort conditions of the
/// same section are part of the judgment: this profile's roots and witnesses
/// all produce `bits<N>` with `1 <= N <= 64`, a comparison consumes one common
/// sort and produces `bits<1>`, a shift consumes its amount in any `bits<M>`,
/// and a width change consumes one sort and produces another, both in bound.
pub(crate) fn allowlisted(instruction: &Inst) -> bool {
    let shape = match instruction {
        Inst::Const(result, _) => in_bound(&result.ty),
        Inst::Add(result, left, right)
        | Inst::Sub(result, left, right)
        | Inst::Mul(result, left, right)
        | Inst::And(result, left, right)
        | Inst::Or(result, left, right)
        | Inst::Xor(result, left, right) => {
            in_bound(&result.ty) && left.ty == result.ty && right.ty == result.ty
        }
        Inst::ICmp(result, _, left, right) => {
            result.ty == EvoIRType::Bits(1) && in_bound(&left.ty) && left.ty == right.ty
        }
        Inst::Shl(result, shifted, amount)
        | Inst::LShr(result, shifted, amount)
        | Inst::AShr(result, shifted, amount) => {
            in_bound(&result.ty) && shifted.ty == result.ty && in_bound(&amount.ty)
        }
        Inst::ZExt(result, source, produced) | Inst::SExt(result, source, produced) => {
            in_bound(&result.ty)
                && result.ty == *produced
                && in_bound(&source.ty)
                && bit_width(&source.ty) <= bit_width(&result.ty)
        }
        Inst::Trunc(result, source, produced) => {
            in_bound(&result.ty)
                && result.ty == *produced
                && in_bound(&source.ty)
                && bit_width(&result.ty) <= bit_width(&source.ty)
        }
        _ => false,
    };
    shape && inst_results(instruction).len() == 1
}

fn in_bound(ty: &EvoIRType) -> bool {
    matches!(ty, EvoIRType::Bits(1..=64))
}

fn bit_width(ty: &EvoIRType) -> u16 {
    match ty {
        EvoIRType::Bits(width) => *width,
        _ => 0,
    }
}

/// Taint starts at `uninitialized` and travels the same way exclusion does.
///
/// The edge clause is not an elaboration: taint that propagated only along
/// operands would be laundered by a branch, and an expression built on poison
/// would then be admitted.
fn tainted_values(function: &Function) -> HashSet<u32> {
    let mut tainted = HashSet::new();
    for block in &function.blocks {
        for instruction in block.insts.iter().chain([&block.terminator]) {
            if matches!(instruction, Inst::Uninitialized(_)) {
                tainted.extend(inst_results(instruction).into_iter().map(|value| value.id));
            }
        }
    }
    close(function, &mut tainted);
    tainted
}

/// A value starts excluded when its sort is not one this profile reasons
/// about -- which is every `mem<R>`, `world`, pointer, floating, vector and
/// tuple sort, and every bit-vector outside the bound -- or when its defining
/// instruction is absent from the allowlist, which is where "may trap" and
/// "carries a non-pure effect" already live, since no instruction that does
/// either is on the list.
///
/// A block argument has no defining instruction, so only its sort decides
/// where it starts; an entry block argument of ordinary `bits<N>` type
/// therefore starts unexcluded, which is what makes an opaque scalar seed
/// legal at all.
fn excluded_values(function: &Function) -> HashSet<u32> {
    let mut excluded = HashSet::new();
    for block in &function.blocks {
        for argument in &block.args {
            if !in_bound(&argument.value.ty) {
                excluded.insert(argument.value.id);
            }
        }
        for instruction in block.insts.iter().chain([&block.terminator]) {
            let listed = allowlisted(instruction);
            for result in inst_results(instruction) {
                if !listed || !in_bound(&result.ty) {
                    excluded.insert(result.id);
                }
            }
        }
    }
    close(function, &mut excluded);
    excluded
}

/// The monotone step of both fixed points: from every operand to its result,
/// and across every terminator edge positionally to the target's arguments.
///
/// The pass only ever adds, so quiescence is the least fixed point and the
/// order the passes run in does not change it.
fn close(function: &Function, values: &mut HashSet<u32>) {
    let targets = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect::<HashMap<BlockId, usize>>();
    loop {
        let before = values.len();
        for block in &function.blocks {
            for instruction in block.insts.iter().chain([&block.terminator]) {
                if operands(instruction)
                    .iter()
                    .any(|operand| values.contains(&operand.id))
                {
                    values.extend(inst_results(instruction).into_iter().map(|value| value.id));
                }
            }
            match &block.terminator {
                Inst::AsmGoto {
                    labels,
                    fallthrough,
                    ..
                } => {
                    for edge in labels.iter().map(|label| &label.edge).chain([fallthrough]) {
                        cross(function, &targets, edge.target, &edge.arguments, values);
                    }
                }
                Inst::Br(target, arguments) => {
                    cross(function, &targets, *target, arguments, values)
                }
                Inst::CondBr(_, true_target, true_arguments, false_target, false_arguments) => {
                    cross(function, &targets, *true_target, true_arguments, values);
                    cross(function, &targets, *false_target, false_arguments, values);
                }
                _ => {}
            }
        }
        if values.len() == before {
            return;
        }
    }
}

fn cross(
    function: &Function,
    targets: &HashMap<BlockId, usize>,
    target: BlockId,
    arguments: &[Value],
    values: &mut HashSet<u32>,
) {
    let Some(block) = targets.get(&target).map(|index| &function.blocks[*index]) else {
        return;
    };
    for (argument, parameter) in arguments.iter().zip(&block.args) {
        if values.contains(&argument.id) {
            values.insert(parameter.value.id);
        }
    }
}

/// Every value an instruction names, whatever its sort.
///
/// Exclusion travels along a `mem`, `world` or pointer operand as readily as
/// along a scalar one, so this is the whole operand list rather than the
/// scalar subset the e-graph bridge reads. A terminator's operands are read
/// here too, because a value passed on an edge is used at the terminator.
fn operands(instruction: &Inst) -> Vec<&Value> {
    match instruction {
        Inst::VaAccess(access) => vec![&access.list, &access.mem_in, &access.world_in],
        Inst::PointerCast { operand, .. } => vec![operand],
        Inst::BitOperation { operand, .. } => vec![operand],
        Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => assembly.operands(),
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::Trap(..) => Vec::new(),
        Inst::Add(_, left, right)
        | Inst::Sub(_, left, right)
        | Inst::Mul(_, left, right)
        | Inst::SDiv(_, left, right)
        | Inst::UDiv(_, left, right)
        | Inst::SRem(_, left, right)
        | Inst::URem(_, left, right)
        | Inst::Shl(_, left, right)
        | Inst::AShr(_, left, right)
        | Inst::LShr(_, left, right)
        | Inst::And(_, left, right)
        | Inst::Or(_, left, right)
        | Inst::Xor(_, left, right)
        | Inst::ICmp(_, _, left, right)
        | Inst::SignedAddDefined(_, left, right)
        | Inst::SignedSubDefined(_, left, right)
        | Inst::SignedMulDefined(_, left, right)
        | Inst::SignedDivRemNoOverflow(_, left, right) => vec![left, right],
        Inst::DivisorNonZero(_, divisor) => vec![divisor],
        Inst::ZExt(_, source, _) | Inst::SExt(_, source, _) | Inst::Trunc(_, source, _) => {
            vec![source]
        }
        Inst::UbAssume(condition, _) => vec![condition],
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => vec![shifted, amount],
        Inst::PtrOffset { base, index, .. } => vec![base, index],
        Inst::Alloca {
            mem_in, world_in, ..
        } => vec![mem_in, world_in],
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
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => args.iter().chain([mem_in, world_in]).collect(),
        Inst::IndirectCall {
            callee,
            args,
            mem_in,
            world_in,
            ..
        } => [callee]
            .into_iter()
            .chain(args.iter())
            .chain([mem_in, world_in])
            .collect(),
        Inst::Br(_, arguments) | Inst::Ret(arguments) => arguments.iter().collect(),
        Inst::CondBr(condition, _, true_arguments, _, false_arguments) => [condition]
            .into_iter()
            .chain(true_arguments.iter())
            .chain(false_arguments.iter())
            .collect(),
    }
}
