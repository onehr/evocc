//! Shared boundary pinning, linear scan, spill rewriting and slot compaction.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use super::abi::{AbiFacts, ArgumentCursor};
use super::machine::{MachInst, MachineBlock, MachineFunction};
use super::register::{RegisterBank, RegisterClass, VirtualReg};
use super::ssa::{SsaInst, SsaTerm, SCRATCH_BASE, SCRATCH_LIMIT};

pub trait AllocClass: RegisterClass + 'static {
    fn bank(self) -> u8;
    fn spill_words(self) -> u32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintValue<R> {
    Reg(R),
    Zero,
    Immediate,
    Symbol,
}

pub trait ConstraintOperand {
    type Reg: Copy;
    fn value(&self) -> ConstraintValue<Self::Reg>;
    fn set_register(&mut self, reg: Self::Reg);
    fn fixed_register(&self) -> Option<u8>;
    fn set_fixed_register(&mut self, number: Option<u8>);
    fn tied_output(&self) -> Option<u8>;
    fn early_clobber(&self) -> bool;
    fn read_write(&self) -> bool;
    fn memory(&self) -> bool;
    fn needs_register(&self) -> bool;
    fn bank(&self) -> u8;
    fn accepts_zero(&self) -> bool;
    fn accepts_symbol(&self) -> bool;
    fn spelling(&self) -> String;
}

pub trait ConstraintBody: Clone {
    type Reg: Copy;
    type Operand: ConstraintOperand<Reg = Self::Reg>;
    fn outputs(&self) -> &[Self::Operand];
    fn inputs(&self) -> &[Self::Operand];
    fn outputs_mut(&mut self) -> &mut [Self::Operand];
    fn inputs_mut(&mut self) -> &mut [Self::Operand];
    fn defined(&self) -> Vec<Self::Reg>;
    fn clobbered_registers(&self) -> Vec<u8>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpillKind {
    Store,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpillView<R> {
    pub kind: SpillKind,
    pub reg: R,
    pub slot: u32,
}

pub type CallOperands<'a, R> = (&'a [R], &'a [R]);
pub type CallOperandsMut<'a, R> = (&'a mut Vec<R>, &'a mut Vec<R>);
type PinnedFunction<I, T, C> = (MachineFunction<I, T, C>, BTreeMap<u32, u8>, u64);
type CallCopies<C> = (Vec<VirtualReg<C>>, Vec<(VirtualReg<C>, VirtualReg<C>)>);

pub trait AllocInst: SsaInst {
    type Body: ConstraintBody<Reg = Self::Reg>;
    fn copy_operands(&self) -> Option<(Self::Reg, Self::Reg)>;
    fn call_operands(&self) -> Option<CallOperands<'_, Self::Reg>>;
    fn call_operands_mut(&mut self) -> Option<CallOperandsMut<'_, Self::Reg>>;
    fn assembly(&self) -> Option<&Self::Body>;
    fn assembly_mut(&mut self) -> Option<&mut Self::Body>;
    fn is_indirect_result(&self) -> bool;
    fn exclusive_result(&self) -> Option<Self::Reg>;
    fn spill(&self) -> Option<SpillView<Self::Reg>>;
    fn spill_slot_mut(&mut self) -> Option<&mut u32>;
    fn spill_store(src: Self::Reg, slot: u32) -> Self;
    fn spill_reload(dst: Self::Reg, slot: u32) -> Self;
    fn incoming_arg(dst: Self::Reg, slot: u32) -> Self;
    fn outgoing_arg(src: Self::Reg, slot: u32) -> Self;
}

pub trait AllocTerm: SsaTerm {
    type Body: ConstraintBody<Reg = Self::Reg>;
    fn return_values(&self) -> Option<&[Self::Reg]>;
    fn return_values_mut(&mut self) -> Option<&mut Vec<Self::Reg>>;
    fn assembly(&self) -> Option<&Self::Body>;
    fn assembly_mut(&mut self) -> Option<&mut Self::Body>;
    fn defs(&self) -> Vec<Self::Reg>;
    fn allows_edge_spill_bridge(&self) -> bool;
}

pub trait AllocationTarget<C: AllocClass> {
    fn bank(&self, class: C) -> RegisterBank;
    fn candidates(&self, class: C, across_call: bool) -> &'static [u8];
    /// Boundary locations, preservation and stack facts of the selected ABI.
    fn abi(&self) -> &AbiFacts;
    fn cycle_scratch(&self, class: C) -> u8;
    fn flags_assignment_tag(&self) -> u8;
    fn fixed_assembly_registers(&self) -> &'static [u8];
    fn clobber_classes(&self) -> &'static [C];
    fn fixed_binding_spelling(&self, number: u8) -> String;
}

#[derive(Clone, PartialEq, Eq)]
pub struct Allocated<I, T, C> {
    pub name: String,
    pub blocks: Vec<MachineBlock<I, T, C>>,
    pub assignment: BTreeMap<u32, u8>,
    pub spilled: usize,
    pub frame_slots: u32,
    pub used_callee_saved: Vec<u8>,
    pub parameters: usize,
    pub outgoing: u64,
    pub indirect_result: bool,
    pub internal: bool,
}

impl<I: std::fmt::Debug, T: std::fmt::Debug, C: std::fmt::Debug> std::fmt::Debug
    for Allocated<I, T, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Allocated")
            .field("name", &self.name)
            .field("blocks", &self.blocks)
            .field("assignment", &self.assignment)
            .field("spilled", &self.spilled)
            .field("frame_slots", &self.frame_slots)
            .field("used_callee_saved", &self.used_callee_saved)
            .field("parameters", &self.parameters)
            .field("outgoing", &self.outgoing)
            .field("indirect_result", &self.indirect_result)
            .field("internal", &self.internal)
            .finish()
    }
}

fn operand_register<R: Copy>(value: ConstraintValue<R>) -> Option<R> {
    if let ConstraintValue::Reg(reg) = value {
        Some(reg)
    } else {
        None
    }
}

/// Place one boundary value of `class`; a class occupies its spill width in
/// stack slots.
fn place<C: AllocClass>(cursor: &mut ArgumentCursor<'_>, class: C) -> Result<u8, u32> {
    cursor.place(usize::from(class.bank()), class.spill_words())
}

const FRESH_BASE: u32 = SCRATCH_LIMIT;

pub fn allocate<I, T, C, A>(
    target: &A,
    function: &MachineFunction<I, T, C>,
) -> Result<Allocated<I, T, C>, String>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
    A: AllocationTarget<C>,
{
    let mut next = FRESH_BASE;
    let parameters = function.blocks.first().map_or(0, |block| block.args.len());
    let indirect_result = function.blocks.first().is_some_and(|block| {
        block
            .insts
            .first()
            .is_some_and(AllocInst::is_indirect_result)
    });
    let (mut current, pins, outgoing) = pin_boundaries(target, function, &mut next)?;
    let mut slots: BTreeMap<u32, u32> = BTreeMap::new();
    let mut spillable: Option<BTreeSet<u32>> = None;
    for _ in 0..64 {
        match scan(target, &current, &pins) {
            Ok(mut assignment) => {
                let mut compacted = MachineFunction {
                    name: current.name.clone(),
                    blocks: drop_identity_moves(&current, &mut assignment),
                };
                let frame_slots = compact_spill_slots(&mut compacted);
                let used_callee_saved = assignment
                    .values()
                    .copied()
                    .filter(|number| target.abi().callee_saved.contains(number))
                    .collect::<BTreeSet<u8>>()
                    .into_iter()
                    .collect();
                return Ok(Allocated {
                    name: compacted.name,
                    blocks: compacted.blocks,
                    parameters,
                    assignment,
                    spilled: slots.len(),
                    frame_slots,
                    used_callee_saved,
                    outgoing,
                    indirect_result,
                    internal: false,
                });
            }
            Err(victims) => {
                // Splitting a value creates the short local ranges needed to
                // reload/use and define/store it. Spilling those ranges again
                // cannot make an overconstrained use fit; it recursively grows
                // spill code. Snapshot the pre-rewrite values only on the first
                // failure, and require every retry to remove a new one.
                let remaining = spillable.get_or_insert_with(|| {
                    current
                        .blocks
                        .iter()
                        .flat_map(|block| {
                            block
                                .insts
                                .iter()
                                .flat_map(|inst| inst.uses().into_iter().chain(inst.defs()))
                                .chain(block.term.uses())
                                .chain(block.term.defs())
                        })
                        .map(|reg| reg.id)
                        .collect()
                });
                let victims: Vec<_> = victims
                    .into_iter()
                    .filter(|value| remaining.remove(value))
                    .collect();
                if victims.is_empty() {
                    return Err("the allocator cannot place an unspillable spill temporary".into());
                }
                for victim in &victims {
                    let next_slot = u32::try_from(slots.len()).unwrap_or(u32::MAX);
                    slots.entry(*victim).or_insert(next_slot);
                }
                current = insert_spill_code(&current, &slots, &victims, &mut next);
            }
        }
    }
    Err("the allocator did not settle".to_owned())
}

fn pin_boundaries<I, T, C, A>(
    target: &A,
    function: &MachineFunction<I, T, C>,
    next: &mut u32,
) -> Result<PinnedFunction<I, T, C>, String>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
    A: AllocationTarget<C>,
{
    let mut pins: BTreeMap<u32, u8> = BTreeMap::new();
    let mut blocks = Vec::new();
    let mut outgoing_slots: u32 = 0;
    let mut next_block = function.blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    for (index, block) in function.blocks.iter().enumerate() {
        let mut insts = Vec::new();
        if index == 0 {
            let mut cursor = ArgumentCursor::new(target.abi().argument_banks);
            let indirect = block
                .insts
                .first()
                .is_some_and(AllocInst::is_indirect_result)
                .then(|| block.args.len().saturating_sub(1));
            for (position, parameter) in block.args.iter().enumerate() {
                if indirect == Some(position) {
                    let incoming = fresh(next, parameter.class);
                    pins.insert(incoming.id, target.abi().indirect_result);
                    insts.push(I::copy(*parameter, incoming));
                    continue;
                }
                let location = place(&mut cursor, parameter.class);
                if let Err(slot) = location {
                    insts.push(I::incoming_arg(*parameter, slot));
                    continue;
                }
                let incoming = fresh(next, parameter.class);
                pins.insert(incoming.id, location.unwrap());
                insts.push(I::copy(*parameter, incoming));
            }
        }
        let mut indirect_call = false;
        for (offset, inst) in block.insts.iter().enumerate() {
            if index == 0 && offset == 0 && inst.is_indirect_result() {
                insts.push(inst.clone());
                continue;
            }
            if inst.is_indirect_result() {
                indirect_call = true;
                insts.push(inst.clone());
            } else if let Some((args, results)) = inst.call_operands() {
                let (moved, outputs) = pin_call(
                    target,
                    &mut insts,
                    args,
                    results,
                    next,
                    &mut pins,
                    &mut outgoing_slots,
                    std::mem::take(&mut indirect_call),
                );
                let mut call = inst.clone();
                let (args, results) = call
                    .call_operands_mut()
                    .expect("call view has mutable operands");
                *args = moved;
                *results = outputs.iter().map(|(pinned, _)| *pinned).collect();
                insts.push(call);
                for (pinned, original) in outputs {
                    insts.push(I::copy(original, pinned));
                }
            } else if let Some(body) = inst.assembly() {
                let (body, after) = pin_assembly(target, body, &mut insts, next, &mut pins)?;
                let mut assembly = inst.clone();
                *assembly
                    .assembly_mut()
                    .expect("assembly view has mutable body") = body;
                insts.push(assembly);
                insts.extend(after);
            } else {
                insts.push(inst.clone());
            }
        }
        let mut bridges = Vec::new();
        let mut term = block.term.clone();
        if let Some(body) = block.term.assembly() {
            let (body, after) = pin_assembly(target, body, &mut insts, next, &mut pins)?;
            *term.assembly_mut().expect("assembly view has mutable body") = body;
            if !after.is_empty() {
                for (index, (target, args)) in block.term.edges().into_iter().enumerate() {
                    let id = next_block;
                    next_block += 1;
                    bridges.push(MachineBlock {
                        id,
                        args: Vec::new(),
                        insts: after.clone(),
                        term: T::jump(target, args.to_vec()),
                    });
                    term.retarget(index, id);
                }
            }
        } else if let Some(values) = block.term.return_values() {
            let mut moved = Vec::new();
            let mut cursor = ArgumentCursor::new(target.abi().result_banks);
            for value in values {
                let outgoing = fresh(next, value.class);
                pins.insert(
                    outgoing.id,
                    place(&mut cursor, value.class).map_err(|_| "too many return registers")?,
                );
                insts.push(I::copy(outgoing, *value));
                moved.push(outgoing);
            }
            *term
                .return_values_mut()
                .expect("return view has mutable values") = moved;
        }
        blocks.push(MachineBlock {
            id: block.id,
            args: Vec::new(),
            insts,
            term,
        });
        blocks.extend(bridges);
    }
    for block in &blocks {
        for reg in block
            .insts
            .iter()
            .flat_map(MachInst::defs)
            .chain(block.insts.iter().flat_map(MachInst::uses))
        {
            if reg.id >= SCRATCH_BASE && reg.id < SCRATCH_LIMIT && !reg.class.is_flags() {
                pins.insert(reg.id, target.cycle_scratch(reg.class));
            }
        }
    }
    Ok((
        MachineFunction {
            name: function.name.clone(),
            blocks,
        },
        pins,
        target.abi().outgoing_area_bytes(outgoing_slots),
    ))
}
fn pin_assembly<I, C, A>(
    target: &A,
    source: &I::Body,
    before: &mut Vec<I>,
    next: &mut u32,
    pins: &mut BTreeMap<u32, u8>,
) -> Result<(I::Body, Vec<I>), String>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    A: AllocationTarget<C>,
{
    // A value is not a physical operand occurrence: `a` may be supplied in
    // both fixed locations. Split independent fixed inputs only where their
    // location requirements differ. Tied operands keep the output's identity
    // and must still reject contradictory bindings. Equal (value, location)
    // occurrences can share their boundary copy.
    let mut locations: BTreeMap<u32, BTreeSet<u8>> = BTreeMap::new();
    for operand in source.outputs().iter().chain(source.inputs()) {
        if let (Some(reg), Some(number)) =
            (operand_register(operand.value()), operand.fixed_register())
        {
            locations.entry(reg.id).or_default().insert(number);
        }
    }
    let mut separated = source.clone();
    let mut input_copies = BTreeMap::new();
    for operand in separated.inputs_mut() {
        if operand.tied_output().is_some() {
            continue;
        }
        if let (Some(reg), Some(number)) =
            (operand_register(operand.value()), operand.fixed_register())
        {
            if locations[&reg.id].len() > 1 {
                let copy = *input_copies.entry((reg, number)).or_insert_with(|| {
                    let copy = fresh(next, reg.class);
                    before.push(I::copy(copy, reg));
                    copy
                });
                operand.set_register(copy);
            }
        }
    }
    let source = &separated;
    let mut requested = BTreeMap::new();
    for operand in source.outputs().iter().chain(source.inputs()) {
        if let Some(number) = operand.fixed_register() {
            if !target.fixed_assembly_registers().contains(&number) {
                return Err(format!(
                    "unsupported assembly register binding {}",
                    target.fixed_binding_spelling(number)
                ));
            }
            let reg = operand_register(operand.value())
                .ok_or("fixed assembly operand has no register")?;
            if requested
                .insert(reg.id, number)
                .is_some_and(|old| old != number)
            {
                return Err("conflicting tied assembly register bindings".into());
            }
        }
    }
    let mut copies = BTreeMap::new();
    for operand in source.outputs().iter().chain(source.inputs()) {
        if let Some(reg) = operand_register(operand.value()) {
            if let Some(number) = requested.get(&reg.id) {
                copies.entry(reg.id).or_insert_with(|| {
                    let pinned = fresh(next, reg.class);
                    pins.insert(pinned.id, *number);
                    pinned
                });
            }
        }
    }
    let mut initialized = BTreeSet::new();
    for operand in source
        .inputs()
        .iter()
        .chain(source.outputs().iter().filter(|o| o.read_write()))
    {
        if let Some(reg) = operand_register(operand.value()) {
            if let Some(pinned) = copies.get(&reg.id) {
                if initialized.insert(reg.id) {
                    before.push(I::copy(*pinned, reg));
                }
            }
        }
    }
    let after = source
        .defined()
        .into_iter()
        .filter_map(|reg| copies.get(&reg.id).map(|pinned| I::copy(reg, *pinned)))
        .collect();
    let mut body = source.clone();
    let rewrite = |operand: &mut <I::Body as ConstraintBody>::Operand| {
        if let Some(reg) = operand_register(operand.value()) {
            if let Some(pinned) = copies.get(&reg.id) {
                operand.set_register(*pinned);
                operand.set_fixed_register(requested.get(&reg.id).copied());
            }
        }
    };
    for operand in body.outputs_mut() {
        rewrite(operand);
    }
    for operand in body.inputs_mut() {
        rewrite(operand);
    }
    Ok((body, after))
}

#[allow(clippy::too_many_arguments)]
fn pin_call<I, C, A>(
    target: &A,
    insts: &mut Vec<I>,
    args: &[VirtualReg<C>],
    results: &[VirtualReg<C>],
    next: &mut u32,
    pins: &mut BTreeMap<u32, u8>,
    outgoing_slots: &mut u32,
    indirect_result: bool,
) -> CallCopies<C>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    A: AllocationTarget<C>,
{
    // Section 6 (revision 0.18): with the marker, the call's last argument is
    // the destination of a record result and uses its indirect-result location.
    let indirect = indirect_result.then(|| args.len().saturating_sub(1));
    let mut moved = Vec::new();
    let mut preparation = Vec::new();
    let mut stack_classes = Vec::new();
    let mut cursor = ArgumentCursor::new(target.abi().argument_banks);
    for (position, arg) in args.iter().enumerate() {
        if indirect == Some(position) {
            let outgoing = fresh(next, arg.class);
            pins.insert(outgoing.id, target.abi().indirect_result);
            preparation.push((I::copy(outgoing, *arg), false));
            moved.push(outgoing);
            continue;
        }
        // Stack slots are assigned in argument order at increasing addresses
        // from the caller's stack pointer at the call.
        let location = place(&mut cursor, arg.class);
        if let Err(slot) = location {
            preparation.push((I::outgoing_arg(*arg, slot), true));
            stack_classes.push(arg.class);
            *outgoing_slots = (*outgoing_slots).max(cursor.stack_words());
            continue;
        }
        let outgoing = fresh(next, arg.class);
        pins.insert(outgoing.id, location.unwrap());
        preparation.push((I::copy(outgoing, *arg), false));
        moved.push(outgoing);
    }
    let pinned: BTreeSet<_> = moved.iter().map(|reg| pins[&reg.id]).collect();
    let saturated = stack_classes.iter().any(|class| {
        let candidates = target.candidates(*class, false);
        !candidates.is_empty() && candidates.iter().all(|number| pinned.contains(number))
    });
    if saturated {
        // A late outgoing reload needs a register before the call consumes
        // its fixed operands. Store the already-evaluated stack arguments
        // before occupying the entire pool with argument pins. Both internal
        // orders and all ABI offsets remain unchanged.
        let (stores, copies): (Vec<_>, Vec<_>) =
            preparation.into_iter().partition(|(_, stack)| *stack);
        insts.extend(stores.into_iter().chain(copies).map(|(inst, _)| inst));
    } else {
        insts.extend(preparation.into_iter().map(|(inst, _)| inst));
    }
    let mut outputs = Vec::new();
    let mut cursor = ArgumentCursor::new(target.abi().result_banks);
    for result in results {
        let incoming = fresh(next, result.class);
        pins.insert(
            incoming.id,
            place(&mut cursor, result.class).unwrap_or(target.abi().result_overflow),
        );
        outputs.push((incoming, *result));
    }
    (moved, outputs)
}

pub(crate) fn compact_spill_slots<I, T, C>(function: &mut MachineFunction<I, T, C>) -> u32
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut sizes = BTreeMap::new();
    let mut uses = BTreeMap::new();
    let mut defs = BTreeMap::new();
    for block in &function.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            let Some(spill) = inst.spill() else {
                continue;
            };
            sizes.insert(spill.slot, spill.reg.class.spill_words());
            match spill.kind {
                SpillKind::Reload => {
                    if !written.contains(&spill.slot) {
                        read.insert(spill.slot);
                    }
                }
                SpillKind::Store => {
                    written.insert(spill.slot);
                }
            }
        }
        uses.insert(block.id, read);
        defs.insert(block.id, written);
    }
    if sizes.is_empty() {
        return 0;
    }
    let slots: Vec<_> = sizes.keys().copied().collect();
    let indices: BTreeMap<_, _> = slots
        .iter()
        .enumerate()
        .map(|(i, slot)| (*slot, i))
        .collect();
    // Repeated reloads used to reinsert the same quadratic number of tree
    // edges at every instruction. Union the identical live-slot relation a
    // machine word at a time, then symmetrize once. Sparse slot IDs are mapped
    // to dense indices; neither liveness nor the deterministic coloring changes.
    // Store only nonempty 64-slot chunks. A large function may have hundreds
    // of thousands of short, disjoint spill lifetimes; a dense NxN matrix
    // would allocate gigabytes even though its interference graph is sparse.
    let mut conflicts: Vec<BTreeMap<usize, u64>> = vec![BTreeMap::new(); slots.len()];
    let live_in = live_on_entry(function, &uses, &defs);
    for block in &function.blocks {
        let mut live: BTreeMap<usize, u64> = BTreeMap::new();
        for (target, _) in block.term.edges() {
            if let Some(slots) = live_in.get(&target) {
                for slot in slots {
                    let index = indices[slot];
                    *live.entry(index / 64).or_default() |= 1 << (index % 64);
                }
            }
        }
        for inst in block.insts.iter().rev() {
            let Some(spill) = inst.spill() else {
                continue;
            };
            let slot = spill.slot;
            let index = indices[&slot];
            // A store conflicts with values live afterwards even if its own
            // result is dead: the emitted store still overwrites its slot.
            // A reload also keeps its value distinct from every other value
            // live at that point, including across loop backedges.
            for (word, bits) in &live {
                *conflicts[index].entry(*word).or_default() |= bits;
            }
            if spill.kind == SpillKind::Store {
                if let Some(bits) = live.get_mut(&(index / 64)) {
                    *bits &= !(1 << (index % 64));
                    if *bits == 0 {
                        live.remove(&(index / 64));
                    }
                }
            } else {
                *live.entry(index / 64).or_default() |= 1 << (index % 64);
            }
        }
    }
    for index in 0..slots.len() {
        if let Some(bits) = conflicts[index].get_mut(&(index / 64)) {
            *bits &= !(1 << (index % 64));
            if *bits == 0 {
                conflicts[index].remove(&(index / 64));
            }
        }
        let row: Vec<_> = conflicts[index]
            .iter()
            .map(|(word, bits)| (*word, *bits))
            .collect();
        for (word, mut pending) in row {
            while pending != 0 {
                let other = word * 64 + pending.trailing_zeros() as usize;
                pending &= pending - 1;
                *conflicts[other].entry(index / 64).or_default() |= 1 << (index % 64);
            }
        }
    }
    let degrees: Vec<u32> = conflicts
        .iter()
        .map(|row| row.values().map(|word| word.count_ones()).sum())
        .collect();
    let mut order: Vec<_> = (0..slots.len()).collect();
    order.sort_unstable_by_key(|index| (Reverse(degrees[*index]), slots[*index]));
    let mut mapping = BTreeMap::new();
    let mut count = 0;
    for index in order {
        let slot = slots[index];
        let mut occupied = BTreeSet::new();
        for (word, pending) in &conflicts[index] {
            let mut pending = *pending;
            while pending != 0 {
                let other = slots[word * 64 + pending.trailing_zeros() as usize];
                pending &= pending - 1;
                if let Some(start) = mapping.get(&other) {
                    occupied.extend(*start..*start + sizes[&other]);
                }
            }
        }
        let size = sizes[&slot];
        let location = (0_u32..)
            .find(|start| {
                start.is_multiple_of(size)
                    && (*start..*start + size).all(|word| !occupied.contains(&word))
            })
            .unwrap();
        count = count.max(location + size);
        mapping.insert(slot, location);
    }
    for inst in function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.insts)
    {
        if let Some(slot) = inst.spill_slot_mut() {
            *slot = mapping[slot];
        }
    }
    count
}

pub(crate) fn insert_spill_code<I, T, C>(
    function: &MachineFunction<I, T, C>,
    slots: &BTreeMap<u32, u32>,
    victims: &[u32],
    next: &mut u32,
) -> MachineFunction<I, T, C>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let spilled: BTreeSet<u32> = victims.iter().copied().collect();
    let mut blocks = Vec::new();
    let mut next_block = function
        .blocks
        .iter()
        .map(|block| block.id)
        .max()
        .unwrap_or(0)
        + 1;
    for block in &function.blocks {
        let mut insts = Vec::new();
        let mut stores = Vec::new();
        let mut cursor = 0;
        while cursor < block.insts.len() {
            let mut before = Vec::new();
            let mut after = Vec::new();
            let mut chain = Vec::new();
            let mut temporaries = BTreeMap::new();
            let ends_in_flags = loop {
                let inst = &block.insts[cursor];
                cursor += 1;
                let mut rewritten = inst.clone();
                for reg in inst.uses() {
                    if spilled.contains(&reg.id) {
                        let temporary = *temporaries.entry(reg.id).or_insert_with(|| {
                            let temporary = fresh(next, reg.class);
                            before.push(I::spill_reload(temporary, slots[&reg.id]));
                            temporary
                        });
                        replace_use(&mut rewritten, reg, temporary);
                    }
                }
                let mut defines_flags = false;
                for reg in inst.defs() {
                    defines_flags |= reg.class.is_flags();
                    if spilled.contains(&reg.id) {
                        let temporary = fresh(next, reg.class);
                        temporaries.insert(reg.id, temporary);
                        replace_def(&mut rewritten, reg, temporary);
                        after.push(I::spill_store(temporary, slots[&reg.id]));
                    }
                }
                chain.push(rewritten);
                if !defines_flags || cursor == block.insts.len() {
                    break defines_flags;
                }
            };
            insts.extend(before);
            insts.extend(chain);
            if ends_in_flags {
                // The last flags value belongs to the conditional terminator.
                // Its ordinary results must be saved on the outgoing edges.
                stores.extend(after);
            } else {
                insts.extend(after);
            }
        }
        let mut term = block.term.clone();
        let mut temporaries = BTreeMap::new();
        for reg in block.term.uses() {
            if spilled.contains(&reg.id) && !temporaries.contains_key(&reg.id) {
                let temporary = fresh(next, reg.class);
                temporaries.insert(reg.id, temporary);
                insts.push(I::spill_reload(temporary, slots[&reg.id]));
                replace_term_use(&mut term, reg, temporary);
            }
        }
        if let Some(body) = block.term.assembly() {
            for reg in body.defined() {
                if spilled.contains(&reg.id) {
                    let temporary = *temporaries
                        .entry(reg.id)
                        .or_insert_with(|| fresh(next, reg.class));
                    replace_term_use(&mut term, reg, temporary);
                    stores.push(I::spill_store(temporary, slots[&reg.id]));
                }
            }
        }
        // Assembly outputs and deferred flags-chain stores belong to every
        // outgoing edge, not to a shared successor another path can enter.
        let mut bridges = Vec::new();
        if !stores.is_empty() {
            let edges: Vec<_> = if term.allows_edge_spill_bridge() {
                term.edges()
                    .into_iter()
                    .map(|(target, args)| (target, args.to_vec()))
                    .collect()
            } else {
                Vec::new()
            };
            for (index, (target, args)) in edges.into_iter().enumerate() {
                let id = next_block;
                next_block += 1;
                bridges.push(MachineBlock {
                    id,
                    args: Vec::new(),
                    insts: stores.clone(),
                    term: T::jump(target, args),
                });
                term.retarget(index, id);
            }
        }
        blocks.push(MachineBlock {
            id: block.id,
            args: Vec::new(),
            insts,
            term,
        });
        blocks.extend(bridges);
    }
    MachineFunction {
        name: function.name.clone(),
        blocks,
    }
}

fn fresh<C: AllocClass>(next: &mut u32, class: C) -> VirtualReg<C> {
    let reg = VirtualReg::new(*next, class);
    *next += 1;
    reg
}

fn swap_reg<C: AllocClass>(slot: &mut VirtualReg<C>, from: VirtualReg<C>, to: VirtualReg<C>) {
    if slot.id == from.id {
        *slot = to;
    }
}

fn replace_use<I, C>(inst: &mut I, from: VirtualReg<C>, to: VirtualReg<C>)
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    for reg in inst.operand_slots(false) {
        swap_reg(reg, from, to);
    }
}

fn replace_def<I, C>(inst: &mut I, from: VirtualReg<C>, to: VirtualReg<C>)
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    for reg in inst.operand_slots(true) {
        swap_reg(reg, from, to);
    }
}

fn replace_term_use<T, C>(term: &mut T, from: VirtualReg<C>, to: VirtualReg<C>)
where
    C: AllocClass,
    T: AllocTerm<Reg = VirtualReg<C>>,
{
    for slot in term.operand_slots() {
        swap_reg(slot, from, to);
    }
}
const CLOBBER_BASE: u32 = u32::MAX / 2;
fn numbering<I, T, C>(function: &MachineFunction<I, T, C>) -> BTreeMap<u32, (usize, usize)>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut ranges = BTreeMap::new();
    let mut position = 0;
    for block in &function.blocks {
        let start = position;
        position += 2 * (block.insts.len() + 1);
        ranges.insert(block.id, (start, position));
    }
    ranges
}

fn liveness<I, T, C>(function: &MachineFunction<I, T, C>) -> BTreeMap<u32, BTreeSet<u32>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut uses: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut defs: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for block in &function.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            for reg in inst.uses() {
                if !written.contains(&reg.id) {
                    read.insert(reg.id);
                }
            }
            for reg in inst.defs() {
                written.insert(reg.id);
            }
        }
        for reg in block.term.uses() {
            if !written.contains(&reg.id) {
                read.insert(reg.id);
            }
        }
        if let Some(body) = block.term.assembly() {
            written.extend(body.defined().iter().map(|reg| reg.id));
        }
        uses.insert(block.id, read);
        defs.insert(block.id, written);
    }
    live_on_entry(function, &uses, &defs)
}

pub(crate) fn live_on_entry<I, T, C>(
    function: &MachineFunction<I, T, C>,
    uses: &BTreeMap<u32, BTreeSet<u32>>,
    defs: &BTreeMap<u32, BTreeSet<u32>>,
) -> BTreeMap<u32, BTreeSet<u32>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut live_in: BTreeMap<u32, BTreeSet<u32>> = function
        .blocks
        .iter()
        .map(|block| (block.id, BTreeSet::new()))
        .collect();
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let mut out: BTreeSet<u32> = BTreeSet::new();
            for (target, _) in block.term.edges() {
                if let Some(entering) = live_in.get(&target) {
                    out.extend(entering.iter().copied());
                }
            }
            let mut entering = uses[&block.id].clone();
            for value in out {
                if !defs[&block.id].contains(&value) {
                    entering.insert(value);
                }
            }
            if live_in[&block.id] != entering {
                live_in.insert(block.id, entering);
                changed = true;
            }
        }
        if !changed {
            return live_in;
        }
    }
}

fn read_positions<I, T, C>(function: &MachineFunction<I, T, C>) -> BTreeMap<u32, Vec<usize>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let ranges = numbering(function);
    let mut reads = BTreeMap::<u32, Vec<usize>>::new();
    for block in &function.blocks {
        let (start, end) = ranges[&block.id];
        for (offset, inst) in block.insts.iter().enumerate() {
            for reg in inst.uses() {
                reads.entry(reg.id).or_default().push(start + 2 * offset);
            }
        }
        for reg in block.term.uses() {
            reads.entry(reg.id).or_default().push(end - 2);
        }
    }
    reads
}

#[derive(Debug, Clone, Copy)]
struct Interval<C> {
    value: u32,
    class: C,
    start: usize,
    end: usize,
    spans_call: bool,
}

fn intervals<I, T, C>(function: &MachineFunction<I, T, C>) -> Vec<Interval<C>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let ranges = numbering(function);
    let live_in = liveness(function);

    let mut bounds: BTreeMap<u32, (usize, usize, C)> = BTreeMap::new();
    let touch = |value: VirtualReg<C>, at: usize, bounds: &mut BTreeMap<u32, (usize, usize, C)>| {
        bounds
            .entry(value.id)
            .and_modify(|(start, end, _)| {
                *start = (*start).min(at);
                *end = (*end).max(at);
            })
            .or_insert((at, at, value.class));
    };

    let mut calls: Vec<usize> = Vec::new();
    for block in &function.blocks {
        let (start, end) = ranges[&block.id];
        for (offset, inst) in block.insts.iter().enumerate() {
            let read = start + 2 * offset;
            let write = read + 1;
            if inst.is_call() {
                calls.push(read);
            }
            for reg in inst.uses() {
                touch(reg, read, &mut bounds);
            }
            for reg in inst.defs() {
                touch(reg, write, &mut bounds);
            }
            // Section 7: `=&r` and `+&r` want "a register no input operand of
            // the same `asm` holds". An ordinary destination may reuse an
            // input's register because the read happens before the write; an
            // early clobber may not, so its interval starts where the reads do.
            if let Some(body) = inst.assembly() {
                for reg in early_clobbers(body) {
                    touch(reg, read, &mut bounds);
                }
            }
            if let Some(dst) = inst.exclusive_result() {
                // The exclusive retry loop writes its old-value result
                // before consuming the address, comparison and update inputs.
                touch(dst, read, &mut bounds);
            }
        }
        for reg in block.term.uses() {
            touch(reg, end - 2, &mut bounds);
        }
        if let Some(body) = block.term.assembly() {
            for reg in body.defined() {
                touch(reg, end - 1, &mut bounds);
            }
            for reg in early_clobbers(body) {
                touch(reg, end - 2, &mut bounds);
            }
        }
    }

    // Section 7: an interval is "extended across any block in which it is live
    // on entry to cover the whole block". A value live on entry that dies
    // inside the block covers only as far as its last read: the sentence is
    // about a value passing through, and stretching one that ends here would
    // hold its register for the whole block. The boundary registers of Section
    // 6 are exactly such values, and stretching them would leave the parameters
    // unable to keep the registers they arrive in.
    //
    // This is a pass of its own rather than the tail of the one above, because
    // a value is only extendable once its definitions and uses are all known.
    // Folded into the discovery pass, the extension reached only the values
    // some earlier block had already mentioned, and a value first mentioned
    // after the block it lives through kept an interval that began inside it:
    // thirty-two of the day-7 programs handed one register to a pointer that
    // lived across a block and to a dead flag-setting result inside it.
    for block in &function.blocks {
        let (start, end) = ranges[&block.id];
        let live_out: BTreeSet<u32> = block
            .term
            .edges()
            .into_iter()
            .filter_map(|(target, _)| live_in.get(&target))
            .flat_map(|entering| entering.iter().copied())
            .collect();
        for value in &live_in[&block.id] {
            if let Some((first, _, _)) = bounds.get_mut(value) {
                *first = (*first).min(start);
            }
        }
        // A value that leaves the block on any edge holds its register to the
        // end of the block, whether it entered the block or was defined in it.
        // Reaching only the values that entered leaves a value defined in a
        // block and read after the back edge expiring at that definition, and
        // the register goes to the next interval while the loop still needs it:
        // the allocation verifier found exactly that, on a pointer carried
        // around a loop whose register a spill store then took.
        for value in &live_out {
            if let Some((_, last, _)) = bounds.get_mut(value) {
                *last = (*last).max(end - 1);
            }
        }
    }

    let mut result: Vec<Interval<C>> = bounds
        .into_iter()
        .map(|(value, (start, end, class))| Interval {
            value,
            class,
            start,
            end,
            spans_call: calls
                .get(calls.partition_point(|at| *at < start))
                .is_some_and(|at| *at < end),
        })
        .collect();
    // Walk by start point, ties by virtual register number.
    result.sort_by_key(|interval| (interval.start, interval.value));
    result
}

fn early_clobbers<B, C>(body: &B) -> Vec<VirtualReg<C>>
where
    C: AllocClass,
    B: ConstraintBody<Reg = VirtualReg<C>>,
{
    body.outputs()
        .iter()
        .filter(|operand| operand.early_clobber())
        .filter_map(|operand| operand_register(operand.value()))
        .collect()
}

fn asm_interferences<I, T, C, A>(
    target: &A,
    function: &MachineFunction<I, T, C>,
) -> Vec<(Interval<C>, u8)>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
    A: AllocationTarget<C>,
{
    let ranges = numbering(function);
    let mut fixed = Vec::new();
    let mut next = CLOBBER_BASE;
    let mut record = |start: usize, end: usize, number: u8, next: &mut u32| {
        for &class in target.clobber_classes() {
            fixed.push((
                Interval {
                    value: *next,
                    class,
                    start,
                    end,
                    spans_call: false,
                },
                number,
            ));
            *next += 1;
        }
    };
    for block in &function.blocks {
        let (start, end) = ranges[&block.id];
        for (offset, inst) in block.insts.iter().enumerate() {
            if let Some(body) = inst.assembly() {
                let read = start + 2 * offset;
                for number in body.clobbered_registers() {
                    record(read, read + 1, number, &mut next);
                }
            }
        }
        if let Some(body) = block.term.assembly() {
            for number in body.clobbered_registers() {
                record(end - 2, end - 1, number, &mut next);
            }
        }
    }
    fixed
}

fn tied_pairs<I, T, C>(function: &MachineFunction<I, T, C>) -> BTreeMap<u32, u32>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut pairs = BTreeMap::new();
    let mut collect = |body: &I::Body| {
        for input in body.inputs() {
            let Some(digit) = input.tied_output() else {
                continue;
            };
            let (Some(input_reg), Some(output)) = (
                operand_register(input.value()),
                body.outputs()
                    .get(usize::from(digit))
                    .and_then(|operand| operand_register(operand.value())),
            ) else {
                continue;
            };
            pairs.insert(input_reg.id, output.id);
        }
    };
    for block in &function.blocks {
        for inst in &block.insts {
            if let Some(body) = inst.assembly() {
                collect(body);
            }
        }
        if let Some(body) = block.term.assembly() {
            collect(body);
        }
    }
    pairs
}

pub(crate) fn scan<I, T, C, A>(
    target: &A,
    function: &MachineFunction<I, T, C>,
    pins: &BTreeMap<u32, u8>,
) -> Result<BTreeMap<u32, u8>, Vec<u32>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
    A: AllocationTarget<C>,
{
    let all = intervals(function);
    let reads = read_positions(function);
    let tied = tied_pairs(function);
    let mut assignment: BTreeMap<u32, u8> = BTreeMap::new();
    let mut fixed = asm_interferences(target, function);
    let mut held: Vec<(Interval<C>, u8)> = Vec::new();
    let mut victims: Vec<u32> = Vec::new();

    for interval in &all {
        if interval.class.is_flags() {
            // Section 2 gives the class one register, and the adjacency rule
            // keeps every live range inside one pair of instructions.
            assignment.insert(interval.value, target.flags_assignment_tag());
            continue;
        }
        if let Some(number) = pins.get(&interval.value).copied() {
            assignment.insert(interval.value, number);
            fixed.push((*interval, number));
        }
    }

    // Fixed ranges can start after the current interval, so simply expiring
    // old entries is not enough. Index their union by physical register and
    // search the first range that has not ended. This preserves interference
    // with future call operands and assembly clobbers without scanning every
    // fixed operand in a large function for every value.
    let mut reserved: BTreeMap<u8, Vec<(usize, usize)>> = BTreeMap::new();
    for (interval, number) in fixed {
        reserved
            .entry(number)
            .or_default()
            .push((interval.start, interval.end));
    }
    for ranges in reserved.values_mut() {
        ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in ranges.drain(..) {
            if let Some(last) = merged.last_mut().filter(|last| start <= last.1) {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        *ranges = merged;
    }

    for interval in &all {
        if interval.class.is_flags() || pins.contains_key(&interval.value) {
            continue;
        }
        // The remaining intervals were assigned in start order. Once one
        // ends before this start it cannot interfere with any later value.
        held.retain(|(other, _)| other.end >= interval.start);
        // A tied input takes the register its output was given, whichever of
        // the two the walk reaches first: the pair is one location.
        if let Some(output) = tied.get(&interval.value) {
            if let Some(number) = assignment.get(output).copied() {
                assignment.insert(interval.value, number);
                held.push((*interval, number));
                continue;
            }
        }
        let mut taken: BTreeSet<u8> = held
            .iter()
            .filter(|(other, _)| overlaps(other, interval))
            .map(|(_, number)| *number)
            .collect();
        let mut pinned_here = BTreeSet::new();
        for (number, ranges) in &reserved {
            if ranges
                .get(ranges.partition_point(|(_, end)| *end < interval.start))
                .is_some_and(|(start, _)| *start <= interval.end)
            {
                taken.insert(*number);
                pinned_here.insert(*number);
            }
        }
        // Section 7: "a call clobbers every caller-saved register, so an
        // interval live across a call is assigned a callee-saved register or
        // spilled."
        let candidates = target.candidates(interval.class, interval.spans_call);
        match candidates
            .iter()
            .copied()
            .find(|number| !taken.contains(number))
        {
            Some(number) => {
                assignment.insert(interval.value, number);
                held.push((*interval, number));
            }
            None => {
                // Spill the overlapping interval whose next use is furthest
                // away, ties by virtual register number; if this one reaches
                // further than any of them, it is the one that goes.
                let priority = |value: u32| {
                    let next = reads
                        .get(&value)
                        .and_then(|positions| {
                            positions.get(positions.partition_point(|at| *at < interval.start))
                        })
                        .copied()
                        .unwrap_or(usize::MAX);
                    (next, Reverse(value))
                };
                let furthest = held
                    .iter()
                    .filter(|(other, number)| {
                        overlaps(other, interval)
                            && candidates.contains(number)
                            && !pinned_here.contains(number)
                            && !pins.contains_key(&other.value)
                            && other.value < CLOBBER_BASE
                    })
                    .max_by_key(|(other, _)| priority(other.value))
                    .map(|(other, number)| (*other, *number));
                match furthest {
                    Some((other, number)) if priority(other.value) > priority(interval.value) => {
                        assignment.remove(&other.value);
                        held.retain(|(entry, _)| entry.value != other.value);
                        victims.push(other.value);
                        assignment.insert(interval.value, number);
                        held.push((*interval, number));
                    }
                    _ => victims.push(interval.value),
                }
            }
        }
    }

    // The walk is by start point, so a tied output may come after its input.
    for (input, output) in &tied {
        if let Some(number) = assignment.get(output).copied() {
            assignment.insert(*input, number);
        } else if let Some(number) = assignment.get(input).copied() {
            assignment.insert(*output, number);
        }
    }

    if victims.is_empty() {
        Ok(assignment)
    } else {
        victims.sort_unstable();
        victims.dedup();
        Err(victims)
    }
}

const fn overlaps<C>(left: &Interval<C>, right: &Interval<C>) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn drop_identity_moves<I, T, C>(
    function: &MachineFunction<I, T, C>,
    assignment: &mut BTreeMap<u32, u8>,
) -> Vec<MachineBlock<I, T, C>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut merged: BTreeMap<u32, VirtualReg<C>> = BTreeMap::new();
    for block in &function.blocks {
        for inst in &block.insts {
            if let Some((dst, src)) = inst.copy_operands() {
                if dst.class == src.class && assignment.get(&dst.id) == assignment.get(&src.id) {
                    let source = resolve(&merged, src);
                    merged.insert(dst.id, source);
                }
            }
        }
    }

    let blocks = function
        .blocks
        .iter()
        .map(|block| {
            let mut insts = Vec::new();
            for inst in &block.insts {
                if let Some((dst, src)) = inst.copy_operands() {
                    if dst.class == src.class && assignment.get(&dst.id) == assignment.get(&src.id)
                    {
                        continue;
                    }
                }
                let mut rewritten = inst.clone();
                for slot in rewritten.operand_slots(false) {
                    *slot = resolve(&merged, *slot);
                }
                for slot in rewritten.operand_slots(true) {
                    *slot = resolve(&merged, *slot);
                }
                insts.push(rewritten);
            }
            let mut term = block.term.clone();
            for slot in term.operand_slots() {
                *slot = resolve(&merged, *slot);
            }
            MachineBlock {
                id: block.id,
                args: Vec::new(),
                insts,
                term,
            }
        })
        .collect();

    for name in merged.keys() {
        assignment.remove(name);
    }
    blocks
}

fn resolve<C: AllocClass>(
    merged: &BTreeMap<u32, VirtualReg<C>>,
    reg: VirtualReg<C>,
) -> VirtualReg<C> {
    let mut current = reg;
    for _ in 0..64 {
        match merged.get(&current.id) {
            Some(next) if next.id != current.id => current = *next,
            _ => return VirtualReg::new(current.id, reg.class),
        }
    }
    VirtualReg::new(current.id, reg.class)
}
