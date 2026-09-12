//! Section 7: out-of-SSA and linear-scan register allocation.
//!
//! "Block arguments are removed by placing, on each edge, the parallel copy
//! from the values the terminator passes to the target's arguments, resolved
//! into a sequence of moves with at most one scratch register (`x16`) for
//! cycles. Critical edges are split by inserting a block. This happens before
//! allocation, on virtual registers, as its own pass; the lowering of Section
//! 5 does not do it."

use std::collections::BTreeMap;

use crate::{inst_defs, term_edges, MBlock, MFunction, MInst, MTerm, RegClass, VReg};

/// Virtual registers at or above this number are the cycle-breaking scratch of
/// Section 7, pre-coloured to `x16`.
///
/// Section 7 names a physical register for a pass that runs "before
/// allocation, on virtual registers", so the scratch has to be a virtual
/// register the allocator pins. It is drawn from a separate range so the
/// allocator can recognise it without a side table, and `x16` is excluded from
/// the allocatable set, so nothing else can be given it.
pub const SCRATCH_BASE: u32 = 1 << 24;

/// One past the scratch range, so a later pass can take a range of its own.
pub const SCRATCH_LIMIT: u32 = 1 << 25;

/// Remove block arguments by placing each edge's parallel copy where it can
/// only run on that edge, splitting the edges where nowhere will do.
///
/// A copy may sit at the end of the predecessor only when the predecessor has
/// one successor, and at the top of the target only when the target has one
/// predecessor; an edge with neither is the critical edge Section 7 splits. A
/// copy appended to a block whose terminator reads the flags would also stand
/// between the comparison and its consumer, which Section 2 forbids, and the
/// same rule keeps that from arising.
#[must_use]
pub fn out_of_ssa(function: &MFunction) -> MFunction {
    let split = split_critical_edges(function);
    let parameters: BTreeMap<u32, Vec<VReg>> = split
        .blocks
        .iter()
        .map(|block| (block.id, block.args.clone()))
        .collect();
    let mut predecessors: BTreeMap<u32, usize> = BTreeMap::new();
    for block in &split.blocks {
        for (target, _) in term_edges(&block.term) {
            *predecessors.entry(target).or_default() += 1;
        }
    }

    let mut next_scratch = SCRATCH_BASE;
    let mut next_transport = split
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .args
                .iter()
                .copied()
                .chain(block.insts.iter().flat_map(inst_defs))
        })
        .map(|value| value.id)
        .max()
        .unwrap_or(0)
        + 1;
    let mut indirect_targets = BTreeMap::new();
    let mut suffix: BTreeMap<u32, Vec<MInst>> = BTreeMap::new();
    let mut prefix: BTreeMap<u32, Vec<MInst>> = BTreeMap::new();

    for block in &split.blocks {
        if let MTerm::Indirect { target, edges } = &block.term {
            // One union parallel copy, BEFORE allocation. Target parameters
            // have distinct virtual identities and are all live out here;
            // scan must give them non-conflicting registers or spill slots.
            // The branch register is another live use, not copy scratch.
            let pairs: Vec<_> = edges
                .iter()
                .flat_map(|(target, args)| {
                    parameters[target].iter().copied().zip(args.iter().copied())
                })
                .collect();
            let mut moves = Vec::new();
            if pairs
                .iter()
                .any(|(dst, src)| dst.id == target.id && dst.id != src.id)
            {
                // A self-edge may redefine the very block argument holding
                // the computed destination. Capture it before the joint copy,
                // in an ordinary live virtual register (not cycle scratch).
                let saved = VReg::new(next_transport, target.class);
                next_transport += 1;
                moves.push(MInst::Mov {
                    dst: saved,
                    src: *target,
                });
                indirect_targets.insert(block.id, saved);
            }
            sequentialize(&pairs, &mut moves, &mut next_scratch);
            suffix.entry(block.id).or_default().extend(moves);
            continue;
        }
        let successors = term_edges(&block.term).len();
        for (target, args) in term_edges(&block.term) {
            let destinations = parameters.get(&target).cloned().unwrap_or_default();
            let pairs: Vec<(VReg, VReg)> = destinations
                .iter()
                .copied()
                .zip(args.iter().copied())
                .collect();
            if pairs.is_empty() {
                continue;
            }
            let mut moves = Vec::new();
            sequentialize(&pairs, &mut moves, &mut next_scratch);
            if successors == 1 {
                suffix.entry(block.id).or_default().extend(moves);
            } else {
                prefix.entry(target).or_default().extend(moves);
            }
        }
    }

    let entry = split.blocks.first().map(|block| block.id);
    let mut blocks = Vec::new();
    for block in &split.blocks {
        let mut insts = prefix.remove(&block.id).unwrap_or_default();
        insts.extend(block.insts.iter().cloned());
        insts.extend(suffix.remove(&block.id).unwrap_or_default());
        let mut term = block.term.clone();
        if let MTerm::Indirect { target, .. } = &mut term {
            if let Some(saved) = indirect_targets.get(&block.id) {
                *target = *saved;
            }
        }
        clear_edge_args(&mut term);
        // The entry block's arguments are the function's parameters, bound by
        // the caller rather than by an edge, so no copy can remove them. They
        // are the fixed operands of the boundary that Section 7 hands the
        // allocator, and they disappear when it assigns them.
        blocks.push(MBlock {
            id: block.id,
            args: if entry == Some(block.id) {
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
        MTerm::Indirect { edges, .. } => {
            for (_, args) in edges {
                args.clear();
            }
        }
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
        MTerm::AsmGoto {
            labels,
            fallthrough_args,
            ..
        } => {
            for (_, args) in labels {
                args.clear();
            }
            fallthrough_args.clear();
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
        MTerm::Indirect { .. } => {
            unreachable!("computed edges cannot be retargeted without address routing")
        }
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
        MTerm::AsmGoto {
            labels,
            fallthrough,
            fallthrough_args,
            ..
        } => {
            for (target, args) in labels {
                visit(target, args);
            }
            visit(fallthrough, fallthrough_args);
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
        // A computed address still denotes the original label: retargeting
        // just its CFG metadata to a copy block would bypass the copies.
        if edges.len() < 2 || matches!(block.term, MTerm::Indirect { .. }) {
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
    reg.id >= SCRATCH_BASE && reg.id < SCRATCH_LIMIT && !matches!(reg.class, RegClass::Flags)
}
