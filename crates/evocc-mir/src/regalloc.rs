//! Section 7: out-of-SSA and linear-scan register allocation.
//!
//! "Block arguments are removed by placing, on each edge, the parallel copy
//! from the values the terminator passes to the target's arguments, resolved
//! into a sequence of moves with at most one scratch register (`x16`) for
//! cycles. Critical edges are split by inserting a block. This happens before
//! allocation, on virtual registers, as its own pass; the lowering of Section
//! 5 does not do it."

use std::collections::BTreeMap;

use crate::{term_edges, MBlock, MFunction, MInst, MTerm, RegClass, VReg};

/// Virtual registers at or above this number are the cycle-breaking scratch of
/// Section 7, pre-coloured to `x16`.
///
/// Section 7 names a physical register for a pass that runs "before
/// allocation, on virtual registers", so the scratch has to be a virtual
/// register the allocator pins. It is drawn from a separate range so the
/// allocator can recognise it without a side table, and `x16` is excluded from
/// the allocatable set, so nothing else can be given it.
pub const SCRATCH_BASE: u32 = 1 << 24;

/// Remove block arguments by placing each edge's parallel copy in its
/// predecessor, splitting critical edges first.
#[must_use]
pub fn out_of_ssa(function: &MFunction) -> MFunction {
    let split = split_critical_edges(function);
    let parameters: BTreeMap<u32, Vec<VReg>> = split
        .blocks
        .iter()
        .map(|block| (block.id, block.args.clone()))
        .collect();

    let mut next_scratch = SCRATCH_BASE;
    let mut blocks = Vec::new();
    for block in &split.blocks {
        let mut insts = block.insts.clone();
        let mut term = block.term.clone();
        // One edge at a time, so a block with two successors places two
        // sequences; a block with two successors and an argument-passing edge
        // cannot occur, because such an edge was split above.
        for (target, args) in term_edges(&block.term) {
            let destinations = parameters.get(&target).cloned().unwrap_or_default();
            let pairs: Vec<(VReg, VReg)> = destinations
                .iter()
                .copied()
                .zip(args.iter().copied())
                .collect();
            sequentialize(&pairs, &mut insts, &mut next_scratch);
        }
        clear_edge_args(&mut term);
        // The entry block's arguments are the function's parameters, bound by
        // the caller rather than by an edge, so no copy can remove them. They
        // are the fixed operands of the boundary that Section 7 hands the
        // allocator, and they disappear when it assigns them.
        let entry = split
            .blocks
            .first()
            .is_some_and(|first| first.id == block.id);
        blocks.push(MBlock {
            id: block.id,
            args: if entry {
                block.args.clone()
            } else {
                Vec::new()
            },
            insts,
            term,
        });
    }
    MFunction {
        name: function.name.clone(),
        blocks,
    }
}

fn clear_edge_args(term: &mut MTerm) {
    match term {
        MTerm::B { args, .. } => args.clear(),
        MTerm::BCond {
            taken_args,
            untaken_args,
            ..
        }
        | MTerm::Cb {
            taken_args,
            untaken_args,
            ..
        }
        | MTerm::Tb {
            taken_args,
            untaken_args,
            ..
        } => {
            taken_args.clear();
            untaken_args.clear();
        }
        MTerm::Ret { .. } | MTerm::Brk { .. } => {}
    }
}

fn retarget(term: &mut MTerm, from: usize, to: u32) {
    let mut index = 0;
    let mut visit = |target: &mut u32, args: &mut Vec<VReg>| {
        if index == from {
            *target = to;
            args.clear();
        }
        index += 1;
    };
    match term {
        MTerm::B { target, args } => visit(target, args),
        MTerm::BCond {
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        }
        | MTerm::Cb {
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        }
        | MTerm::Tb {
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        } => {
            visit(taken, taken_args);
            visit(untaken, untaken_args);
        }
        MTerm::Ret { .. } | MTerm::Brk { .. } => {}
    }
}

/// Split every edge whose source has more than one successor and whose target
/// has more than one predecessor, so that a copy has a block to live in.
fn split_critical_edges(function: &MFunction) -> MFunction {
    let mut predecessors: BTreeMap<u32, usize> = BTreeMap::new();
    for block in &function.blocks {
        for (target, _) in term_edges(&block.term) {
            *predecessors.entry(target).or_default() += 1;
        }
    }
    let mut next_block = function
        .blocks
        .iter()
        .map(|block| block.id)
        .max()
        .unwrap_or(0)
        + 1;

    let mut blocks = function.blocks.clone();
    let mut inserted = Vec::new();
    for block in &mut blocks {
        let edges = term_edges(&block.term);
        if edges.len() < 2 {
            continue;
        }
        let owned: Vec<(u32, Vec<VReg>)> = edges
            .into_iter()
            .map(|(target, args)| (target, args.to_vec()))
            .collect();
        for (position, (target, args)) in owned.into_iter().enumerate() {
            if predecessors.get(&target).copied().unwrap_or(0) < 2 {
                continue;
            }
            let id = next_block;
            next_block += 1;
            inserted.push(MBlock {
                id,
                args: Vec::new(),
                insts: Vec::new(),
                term: MTerm::B { target, args },
            });
            retarget(&mut block.term, position, id);
        }
    }
    blocks.extend(inserted);
    MFunction {
        name: function.name.clone(),
        blocks,
    }
}

/// Resolve one parallel copy into a sequence of moves.
///
/// A move is emitted as soon as nothing pending still reads its destination.
/// When every remaining move is blocked the pending set is a union of cycles;
/// one source is saved into the scratch, which frees exactly one move, and the
/// cycle unwinds. Section 7 allows "at most one scratch register for cycles",
/// and one is enough because a cycle is broken and consumed before the next is
/// begun.
fn sequentialize(pairs: &[(VReg, VReg)], insts: &mut Vec<MInst>, next_scratch: &mut u32) {
    let mut pending: Vec<(VReg, VReg)> = pairs
        .iter()
        .copied()
        .filter(|(dst, src)| dst.id != src.id)
        .collect();

    while !pending.is_empty() {
        let ready = pending
            .iter()
            .position(|(dst, _)| !pending.iter().any(|(_, src)| src.id == dst.id));
        let index = match ready {
            Some(index) => index,
            None => {
                // Every destination is still read: break one cycle.
                let (_, source) = pending[0];
                let scratch = VReg::new(*next_scratch, source.class);
                *next_scratch += 1;
                insts.push(MInst::Mov {
                    dst: scratch,
                    src: source,
                });
                for (_, src) in &mut pending {
                    if src.id == source.id {
                        *src = scratch;
                    }
                }
                continue;
            }
        };
        let (dst, src) = pending.remove(index);
        insts.push(MInst::Mov { dst, src });
    }
}

/// Whether a virtual register is the Section 7 cycle-breaking scratch.
#[must_use]
pub const fn is_scratch(reg: VReg) -> bool {
    reg.id >= SCRATCH_BASE && !matches!(reg.class, RegClass::Flags)
}
