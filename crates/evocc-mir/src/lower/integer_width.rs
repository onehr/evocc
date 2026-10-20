//! Remove extensions whose input already has zero upper bits.
//!
//! These are machine representation facts, not EvoIR range assumptions. Entry
//! values and unhandled definitions start at their full register width. A join
//! takes the maximum of every incoming width. Iteration can only decrease widths;
//! each register changes at most 64 times, including in cyclic control flow.
use std::collections::{BTreeMap, VecDeque};

use crate::{inst_defs, inst_uses, term_edges, BinOp, ExtOp, MBlock, MInst, RegClass, VReg};

fn width(reg: VReg) -> u16 {
    match reg.class {
        RegClass::Gpr32 => 32,
        _ => 64,
    }
}

fn significant_bits(value: u64) -> u16 {
    (64 - value.leading_zeros()) as u16
}

fn extension(op: ExtOp) -> (u16, bool) {
    match op {
        ExtOp::Sxtb => (8, true),
        ExtOp::Sxth => (16, true),
        ExtOp::Sxtw => (32, true),
        ExtOp::Uxtb => (8, false),
        ExtOp::Uxth => (16, false),
    }
}

enum Definition<'a> {
    Instruction(&'a MInst),
    Join(Vec<VReg>),
}

fn facts(blocks: &[MBlock]) -> BTreeMap<VReg, u16> {
    let mut definitions = Vec::new();
    let mut incoming: BTreeMap<u32, Vec<&[VReg]>> = BTreeMap::new();
    for block in blocks {
        for (target, args) in term_edges(&block.term) {
            incoming.entry(target).or_default().push(args);
        }
        for inst in &block.insts {
            let defs = inst_defs(inst);
            if defs.len() == 1 && matches!(defs[0].class, RegClass::Gpr32 | RegClass::Gpr64) {
                definitions.push((defs[0], Definition::Instruction(inst)));
            }
        }
    }
    // Entry arguments also have an external incoming edge with unknown values.
    for block in blocks.iter().skip(1) {
        if let Some(edges) = incoming.get(&block.id) {
            for (index, arg) in block.args.iter().enumerate() {
                if matches!(arg.class, RegClass::Gpr32 | RegClass::Gpr64) {
                    definitions.push((
                        *arg,
                        Definition::Join(edges.iter().map(|e| e[index]).collect()),
                    ));
                }
            }
        }
    }
    let mut users: BTreeMap<VReg, Vec<usize>> = BTreeMap::new();
    for (index, (_, definition)) in definitions.iter().enumerate() {
        let inputs = match definition {
            Definition::Instruction(inst) => inst_uses(inst),
            Definition::Join(args) => args.clone(),
        };
        for input in inputs {
            users.entry(input).or_default().push(index);
        }
    }
    let mut known = BTreeMap::new();
    let mut pending: VecDeque<_> = (0..definitions.len()).collect();
    let mut queued = vec![true; definitions.len()];
    while let Some(index) = pending.pop_front() {
        queued[index] = false;
        let (dst, definition) = &definitions[index];
        let bound = |reg: VReg| known.get(&reg).copied().unwrap_or_else(|| width(reg));
        let bits = match definition {
            Definition::Join(args) => args.iter().map(|r| bound(*r)).max().unwrap_or(width(*dst)),
            Definition::Instruction(inst) => match inst {
                MInst::Cset { .. } => 1,
                MInst::MovImm { imm, .. } => significant_bits(*imm),
                MInst::Mov { src, .. } => bound(*src),
                MInst::BinaryImm {
                    op: BinOp::And,
                    lhs,
                    imm,
                    width,
                    ..
                } => bound(*lhs).min(significant_bits(*imm & width.mask())),
                MInst::Extend { op, src, .. } => {
                    let (bits, signed) = extension(*op);
                    if !signed {
                        bound(*src).min(bits)
                    } else if bound(*src) < bits {
                        bound(*src)
                    } else {
                        width(*dst)
                    }
                }
                _ => width(*dst),
            },
        }
        .min(width(*dst));
        if bits < bound(*dst) {
            known.insert(*dst, bits);
            for user in users.get(dst).into_iter().flatten() {
                if !queued[*user] {
                    queued[*user] = true;
                    pending.push_back(*user);
                }
            }
        }
    }
    known
}

pub(super) fn select(blocks: &mut [MBlock]) {
    let known = facts(blocks);
    let bound = |reg: VReg| known.get(&reg).copied().unwrap_or_else(|| width(reg));
    for inst in blocks.iter_mut().flat_map(|block| &mut block.insts) {
        let copy = match inst {
            MInst::Extend { op, dst, src } => {
                let (bits, signed) = extension(*op);
                (bound(*src) <= bits - u16::from(signed)).then_some((*dst, *src))
            }
            MInst::BinaryImm {
                op: BinOp::And,
                dst,
                lhs,
                imm,
                ..
            } => {
                let bits = bound(*lhs);
                let needed = u64::MAX.checked_shr(u32::from(64 - bits)).unwrap_or(0);
                (*imm & needed == needed).then_some((*dst, *lhs))
            }
            _ => None,
        };
        if let Some((dst, src)) = copy {
            // Keep the destination identity and register class. W destinations
            // still truncate; a W-to-X move still zero-extends. Allocation can
            // coalesce the copy, but this pass never changes edges or effects.
            *inst = MInst::Mov { dst, src };
        }
    }
}
