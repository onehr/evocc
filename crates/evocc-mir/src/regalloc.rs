//! Section 7: out-of-SSA and linear-scan register allocation.
//!
//! "Block arguments are removed by placing, on each edge, the parallel copy
//! from the values the terminator passes to the target's arguments, resolved
//! into a sequence of moves with at most one scratch register (`x16`) for
//! cycles. Critical edges are split by inserting a block. This happens before
//! allocation, on virtual registers, as its own pass; the lowering of Section
//! 5 does not do it."

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    inst_defs, inst_uses, term_edges, term_uses, MBlock, MFunction, MInst, MTerm, RegClass, VReg,
};

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
    out_of_ssa_with_dce(function, true)
}

/// Attribution control for the existing baseline DCE, not an EqSat pass.
/// Production callers use `out_of_ssa`; disabling deletion also keeps the edge
/// transports needed by the retained computations, rather than leaving uses of
/// dropped block arguments. No memory/effect/control operation is discarded.
#[must_use]
pub fn out_of_ssa_with_dce(function: &MFunction, eliminate_dead: bool) -> MFunction {
    let forwarding = forwarded_parameters(function);
    let source = |reg: VReg| forwarding.get(&reg.id).copied().unwrap_or(reg);
    let split = split_critical_edges(function);
    let parameters: BTreeMap<u32, Vec<VReg>> = split
        .blocks
        .iter()
        .map(|block| (block.id, block.args.clone()))
        .collect();
    let needed = needed_edge_values(&split, &parameters, !eliminate_dead);
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
                .filter(|(dst, _)| needed.contains(&dst.id))
                .filter(|(dst, _)| !forwarding.contains_key(&dst.id))
                .map(|(dst, src)| (dst, source(src)))
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
                .filter(|(dst, _)| needed.contains(&dst.id))
                .filter(|(dst, _)| !forwarding.contains_key(&dst.id))
                .map(|(dst, src)| (dst, source(src)))
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
        insts.extend(
            block
                .insts
                .iter()
                .filter(|inst| {
                    !eliminate_dead
                        || !discardable_computation(inst)
                        || inst_defs(inst).iter().any(|dst| needed.contains(&dst.id))
                })
                .cloned(),
        );
        insts.extend(suffix.remove(&block.id).unwrap_or_default());
        let mut term = block.term.clone();
        if let MTerm::Indirect { target, .. } = &mut term {
            if let Some(saved) = indirect_targets.get(&block.id) {
                *target = *saved;
            }
        }
        clear_edge_args(&mut term);
        for inst in &mut insts {
            for slot in crate::linscan::operand_slots(inst, false) {
                *slot = source(*slot);
            }
        }
        for slot in crate::linscan::term_slots(&mut term) {
            *slot = source(*slot);
        }
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

/// Eliminate a trivial phi: apart from passing itself unchanged, every incoming
/// edge must transport the same value. Repeat through the dependent parameters
/// to simplify forwarding chains and unchanged loop cycles. A nontrivial phi is
/// still a definition, so an inner loop may forward an outer induction value
/// without incorrectly identifying that value with its initial constant.
/// This is SSA identity, NOT physical coalescing or equality of constant bits.
fn forwarded_parameters(function: &MFunction) -> BTreeMap<u32, VReg> {
    fn resolve(aliases: &BTreeMap<u32, VReg>, mut value: VReg) -> VReg {
        // Only canonical, distinct roots are linked below, so this cannot
        // cycle. Do not truncate a long forwarding chain at an arbitrary cap.
        while let Some(&next) = aliases.get(&value.id) {
            value = next;
        }
        value
    }

    // This is a pre-SSA-destruction analysis, so malformed/non-SSA test inputs
    // must not receive an identity proof. Ordinary asm can retain its effects
    // and constraints while its input uses are rewritten. Read/write or tied
    // forms which redefine a VReg do not pass this SSA validation. Fixed asm
    // bindings remain excluded: two parameters can transport one value into
    // different physical registers, so identifying their uses before pinning
    // would incorrectly impose conflicting constraints on one VReg.
    // Indirect dispatch also stays unchanged. Its union copy can overwrite a
    // self-edge source even when a different target is selected; replacing that
    // target's saved binding with the source would observe the wrong value.
    if crate::validate::validate_function(function).is_err()
        || function.blocks.iter().any(|b| {
            matches!(b.term, MTerm::AsmGoto { .. } | MTerm::Indirect { .. })
                || b.insts.iter().any(|i| {
                    matches!(i, MInst::Asm(body)
                        if body.numbered().iter().any(|operand| operand.fixed_register.is_some()))
                })
        })
    {
        return BTreeMap::new();
    }
    let mut parameters = BTreeMap::new();
    let mut incoming: BTreeMap<u32, Vec<VReg>> = BTreeMap::new();
    let mut users: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (index, block) in function.blocks.iter().enumerate() {
        parameters.insert(block.id, &block.args);
        for &arg in &block.args {
            if index != 0 {
                incoming.insert(arg.id, Vec::new());
            }
        }
    }
    for block in &function.blocks {
        for (target, args) in term_edges(&block.term) {
            // An entry backedge is outside this bounded rule: entry values are
            // caller bindings, not a loop phi with an additional hidden edge.
            if target == function.blocks[0].id {
                return BTreeMap::new();
            }
            for (param, &arg) in parameters[&target].iter().zip(args) {
                incoming.get_mut(&param.id).unwrap().push(arg);
                users.entry(arg.id).or_default().push(param.id);
            }
        }
    }
    let mut aliases = BTreeMap::new();
    let mut pending: VecDeque<_> = incoming.keys().copied().collect();
    let mut queued: BTreeSet<_> = incoming.keys().copied().collect();
    while let Some(id) = pending.pop_front() {
        queued.remove(&id);
        if aliases.contains_key(&id) {
            continue;
        }
        let distinct: BTreeSet<_> = incoming[&id]
            .iter()
            .map(|&value| resolve(&aliases, value))
            .filter(|value| value.id != id)
            .collect();
        if distinct.len() != 1 {
            continue;
        }
        let root = *distinct.first().unwrap();
        aliases.insert(id, root);
        // A later replacement of the root must also revisit users of this
        // alias. Transfer these dependencies rather than scanning all phis.
        if let Some(dependents) = users.remove(&id) {
            for &dependent in &dependents {
                if queued.insert(dependent) {
                    pending.push_back(dependent);
                }
            }
            users.entry(root.id).or_default().extend(dependents);
        }
    }
    aliases
        .iter()
        .map(|(&id, &value)| (id, resolve(&aliases, value)))
        .collect()
}

/// Only total register computations can be discarded. In particular, every
/// memory access (even nonvolatile), call, assembly, allocation, ABI marker
/// and lifetime action stays. UB guards are control flow and stay as well.
fn discardable_computation(inst: &MInst) -> bool {
    matches!(
        inst,
        MInst::Mov { .. }
            | MInst::MovImm { .. }
            | MInst::Adrp { .. }
            | MInst::AddPageOff { .. }
            | MInst::Adr { .. }
            | MInst::Binary { .. }
            | MInst::BinaryImm { .. }
            | MInst::FlagBinary { .. }
            | MInst::WideMultiply { .. }
            | MInst::Msub { .. }
            | MInst::Madd { .. }
            | MInst::CarryBinary { .. }
            | MInst::AddCarryFlags { .. }
            | MInst::SubCarryFlags { .. }
            | MInst::TstImm { .. }
            | MInst::Unary { .. }
            | MInst::Extend { .. }
            | MInst::Bfx { .. }
            | MInst::Cmp { .. }
            | MInst::CmpImm { .. }
            | MInst::Cset { .. }
            | MInst::Csel { .. }
    )
}

/// Follow value demand through pure computations and ALL incoming edges.
/// An unused address, including one forwarded around a dead parameter cycle,
/// must not create a runtime relocation into a metadata/discarded section.
/// Global declarations and their sections are not changed by this pass.
fn needed_edge_values(
    function: &MFunction,
    parameters: &BTreeMap<u32, Vec<VReg>>,
    retain_dead: bool,
) -> BTreeSet<u32> {
    let mut incoming: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut needed = BTreeSet::new();
    for block in &function.blocks {
        // A flags producer and its immediately following consumer form one
        // legality bundle. In particular, keeping the arithmetic result of
        // `adds` while dropping its unused `cset` leaves an unconsumed flags
        // definition. Propagate demand forward within the bundle as well as
        // backward through normal uses. Entire dead bundles still disappear.
        for pair in block.insts.windows(2) {
            let definitions = inst_defs(&pair[0]);
            let uses = inst_uses(&pair[1]);
            if definitions
                .iter()
                .any(|reg| reg.class == RegClass::Flags && uses.contains(reg))
            {
                let consumer_definitions = inst_defs(&pair[1]);
                for dst in definitions {
                    incoming
                        .entry(dst.id)
                        .or_default()
                        .extend(consumer_definitions.iter().map(|reg| reg.id));
                }
            }
        }
        for inst in &block.insts {
            let uses = inst_uses(inst);
            if discardable_computation(inst) && !retain_dead {
                for dst in inst_defs(inst) {
                    incoming
                        .entry(dst.id)
                        .or_default()
                        .extend(uses.iter().map(|reg| reg.id));
                }
            } else {
                needed.extend(uses.iter().map(|reg| reg.id));
            }
        }
        let mut control = block.term.clone();
        clear_edge_args(&mut control);
        needed.extend(term_uses(&control).into_iter().map(|reg| reg.id));
        for (target, args) in term_edges(&block.term) {
            if let Some(params) = parameters.get(&target) {
                for (param, arg) in params.iter().zip(args) {
                    incoming.entry(param.id).or_default().push(arg.id);
                }
            }
        }
    }
    let mut pending: Vec<_> = needed.iter().copied().collect();
    while let Some(value) = pending.pop() {
        if let Some(arguments) = incoming.get(&value) {
            for &argument in arguments {
                if needed.insert(argument) {
                    pending.push(argument);
                }
            }
        }
    }
    needed
}

/// The entry is an ABI boundary, so unused arguments still keep their fixed
/// positions. A direct private caller may supply an arbitrary value in a
/// position the callee provably never reads, after evaluating the source
/// argument normally. No signature or incoming-register convention changes.
pub(crate) fn unused_parameters(function: &MFunction) -> Vec<bool> {
    let parameters = function
        .blocks
        .iter()
        .map(|b| (b.id, b.args.clone()))
        .collect();
    let needed = needed_edge_values(function, &parameters, false);
    function.blocks.first().map_or_else(Vec::new, |entry| {
        entry.args.iter().map(|a| !needed.contains(&a.id)).collect()
    })
}

/// Instructions the existing baseline demand analysis can discard. This is
/// producer telemetry, not an independent semantic or native-code audit.
#[must_use]
pub fn dead_computation_count(function: &MFunction) -> usize {
    let parameters = function
        .blocks
        .iter()
        .map(|b| (b.id, b.args.clone()))
        .collect();
    let needed = needed_edge_values(function, &parameters, false);
    function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|i| {
            discardable_computation(i) && inst_defs(i).iter().all(|dst| !needed.contains(&dst.id))
        })
        .count()
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
