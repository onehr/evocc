//! Shared SSA destruction, transport copies and the existing demand analysis.
//!
//! The instruction family supplies operand facts and constructors only. The
//! forwarding worklist, edge splitting, copy ordering and DCE are owned here.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::machine::{MachInst, MachTerm, MachineBlock, MachineFunction};
use super::register::{RegisterClass, VirtualReg};

pub trait SsaInst: MachInst + Clone {
    fn copy(dst: Self::Reg, src: Self::Reg) -> Self;
    fn has_fixed_binding(&self) -> bool;
}

pub trait SsaTerm: MachTerm + Clone {
    fn indirect_target(&self) -> Option<Self::Reg>;
    fn set_indirect_target(&mut self, reg: Self::Reg);
    fn clear_edge_args(&mut self);
    fn retarget(&mut self, index: usize, target: u32);
    fn jump(target: u32, args: Vec<Self::Reg>) -> Self;
    fn prevents_forwarding(&self) -> bool;
}

/// The original disjoint virtual range for cycle-breaking transport scratch.
pub const SCRATCH_BASE: u32 = 1 << 24;
pub const SCRATCH_LIMIT: u32 = 1 << 25;

pub fn out_of_ssa_with_dce<I, T, C>(
    function: &MachineFunction<I, T, C>,
    eliminate_dead: bool,
    forwarding_input_valid: bool,
) -> MachineFunction<I, T, C>
where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
    T: SsaTerm<Reg = VirtualReg<C>>,
{
    let forwarding = forwarded_parameters(function, forwarding_input_valid);
    let source = |reg: VirtualReg<C>| forwarding.get(&reg.id).copied().unwrap_or(reg);
    let split = split_critical_edges(function);
    let parameters: BTreeMap<u32, Vec<VirtualReg<C>>> = split
        .blocks
        .iter()
        .map(|block| (block.id, block.args.clone()))
        .collect();
    let needed = needed_edge_values(&split, &parameters, !eliminate_dead);
    let mut predecessors: BTreeMap<u32, usize> = BTreeMap::new();
    for block in &split.blocks {
        for (target, _) in block.term.edges() {
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
                .chain(block.insts.iter().flat_map(MachInst::defs))
        })
        .map(|value| value.id)
        .max()
        .unwrap_or(0)
        + 1;
    let mut indirect_targets = BTreeMap::new();
    let mut suffix: BTreeMap<u32, Vec<I>> = BTreeMap::new();
    let mut prefix: BTreeMap<u32, Vec<I>> = BTreeMap::new();

    for block in &split.blocks {
        if let Some(target) = block.term.indirect_target() {
            let edges = block.term.edges();
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
                let saved = VirtualReg::new(next_transport, target.class);
                next_transport += 1;
                moves.push(I::copy(saved, target));
                indirect_targets.insert(block.id, saved);
            }
            sequentialize(&pairs, &mut moves, &mut next_scratch);
            suffix.entry(block.id).or_default().extend(moves);
            continue;
        }
        let successors = block.term.edges().len();
        for (target, args) in block.term.edges() {
            let destinations = parameters.get(&target).cloned().unwrap_or_default();
            let pairs: Vec<(VirtualReg<C>, VirtualReg<C>)> = destinations
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
                        || !inst.discardable()
                        || inst.defs().iter().any(|dst| needed.contains(&dst.id))
                })
                .cloned(),
        );
        insts.extend(suffix.remove(&block.id).unwrap_or_default());
        let mut term = block.term.clone();
        if let Some(saved) = indirect_targets.get(&block.id) {
            term.set_indirect_target(*saved);
        }
        term.clear_edge_args();
        for inst in &mut insts {
            for slot in inst.operand_slots(false) {
                *slot = source(*slot);
            }
        }
        for slot in term.operand_slots() {
            *slot = source(*slot);
        }
        // The entry block's arguments are the function's parameters, bound by
        // the caller rather than by an edge, so no copy can remove them. They
        // are the fixed operands of the boundary that Section 7 hands the
        // allocator, and they disappear when it assigns them.
        blocks.push(MachineBlock {
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
    MachineFunction {
        name: function.name.clone(),
        blocks,
    }
}

fn forwarded_parameters<I, T, C>(
    function: &MachineFunction<I, T, C>,
    forwarding_input_valid: bool,
) -> BTreeMap<u32, VirtualReg<C>>
where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
    T: SsaTerm<Reg = VirtualReg<C>>,
{
    fn resolve<C: RegisterClass>(
        aliases: &BTreeMap<u32, VirtualReg<C>>,
        mut value: VirtualReg<C>,
    ) -> VirtualReg<C> {
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
    // forms which redefine a virtual register do not pass this SSA validation. Fixed asm
    // bindings remain excluded: two parameters can transport one value into
    // different physical registers, so identifying their uses before pinning
    // would incorrectly impose conflicting constraints on one virtual register.
    // Indirect dispatch also stays unchanged. Its union copy can overwrite a
    // self-edge source even when a different target is selected; replacing that
    // target's saved binding with the source would observe the wrong value.
    if !forwarding_input_valid
        || function
            .blocks
            .iter()
            .any(|b| b.term.prevents_forwarding() || b.insts.iter().any(SsaInst::has_fixed_binding))
    {
        return BTreeMap::new();
    }
    let mut parameters = BTreeMap::new();
    let mut incoming: BTreeMap<u32, Vec<VirtualReg<C>>> = BTreeMap::new();
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
        for (target, args) in block.term.edges() {
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

fn needed_edge_values<I, T, C>(
    function: &MachineFunction<I, T, C>,
    parameters: &BTreeMap<u32, Vec<VirtualReg<C>>>,
    retain_dead: bool,
) -> BTreeSet<u32>
where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
    T: SsaTerm<Reg = VirtualReg<C>>,
{
    let mut incoming: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut needed = BTreeSet::new();
    for block in &function.blocks {
        // A flags producer and its immediately following consumer form one
        // legality bundle. Keeping an arithmetic result while dropping its
        // otherwise unused condition consumer leaves an unconsumed flags
        // definition. Propagate demand forward within the bundle as well as
        // backward through normal uses. Entire dead bundles still disappear.
        for pair in block.insts.windows(2) {
            let definitions = pair[0].defs();
            let uses = pair[1].uses();
            if definitions
                .iter()
                .any(|reg| reg.class.is_flags() && uses.contains(reg))
            {
                let consumer_definitions = pair[1].defs();
                for dst in definitions {
                    incoming
                        .entry(dst.id)
                        .or_default()
                        .extend(consumer_definitions.iter().map(|reg| reg.id));
                }
            }
        }
        for inst in &block.insts {
            let uses = inst.uses();
            if inst.discardable() && !retain_dead {
                for dst in inst.defs() {
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
        control.clear_edge_args();
        needed.extend(control.uses().into_iter().map(|reg| reg.id));
        for (target, args) in block.term.edges() {
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

pub(crate) fn unused_parameters<I, T, C>(function: &MachineFunction<I, T, C>) -> Vec<bool>
where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
    T: SsaTerm<Reg = VirtualReg<C>>,
{
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

pub fn dead_computation_count<I, T, C>(function: &MachineFunction<I, T, C>) -> usize
where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
    T: SsaTerm<Reg = VirtualReg<C>>,
{
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
        .filter(|i| i.discardable() && i.defs().iter().all(|dst| !needed.contains(&dst.id)))
        .count()
}

fn split_critical_edges<I, T, C>(function: &MachineFunction<I, T, C>) -> MachineFunction<I, T, C>
where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
    T: SsaTerm<Reg = VirtualReg<C>>,
{
    let mut predecessors: BTreeMap<u32, usize> = BTreeMap::new();
    for block in &function.blocks {
        for (target, _) in block.term.edges() {
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
        let edges = block.term.edges();
        // A computed address still denotes the original label: retargeting
        // just its CFG metadata to a copy block would bypass the copies.
        if edges.len() < 2 || block.term.indirect_target().is_some() {
            continue;
        }
        let owned: Vec<(u32, Vec<VirtualReg<C>>)> = edges
            .into_iter()
            .map(|(target, args)| (target, args.to_vec()))
            .collect();
        for (position, (target, args)) in owned.into_iter().enumerate() {
            if predecessors.get(&target).copied().unwrap_or(0) < 2 {
                continue;
            }
            let id = next_block;
            next_block += 1;
            inserted.push(MachineBlock {
                id,
                args: Vec::new(),
                insts: Vec::new(),
                term: T::jump(target, args),
            });
            block.term.retarget(position, id);
        }
    }
    blocks.extend(inserted);
    MachineFunction {
        name: function.name.clone(),
        blocks,
    }
}

fn sequentialize<I, C>(
    pairs: &[(VirtualReg<C>, VirtualReg<C>)],
    insts: &mut Vec<I>,
    next_scratch: &mut u32,
) where
    C: RegisterClass,
    I: SsaInst<Reg = VirtualReg<C>>,
{
    let mut pending: Vec<(VirtualReg<C>, VirtualReg<C>)> = pairs
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
                let scratch = VirtualReg::new(*next_scratch, source.class);
                *next_scratch += 1;
                insts.push(I::copy(scratch, source));
                for (_, src) in &mut pending {
                    if src.id == source.id {
                        *src = scratch;
                    }
                }
                continue;
            }
        };
        let (dst, src) = pending.remove(index);
        insts.push(I::copy(dst, src));
    }
}
