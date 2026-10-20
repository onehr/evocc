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
    fn value_bits(self) -> u16;
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
    /// An incoming load or outgoing store, using the ABI's stack-word offset.
    fn stack_argument(&self) -> Option<SpillView<Self::Reg>>;
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
    /// Unchanged input, captured before boundary copies or spill rewriting.
    /// This is the program being checked, not an allocator analysis or hint.
    pub input: MachineFunction<I, T, C>,
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

/// Optional candidate order and local cleanup; constraints remain unchanged.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AllocationOptions {
    pub copy_hints: bool,
    pub reload_reuse: bool,
}

impl AllocationOptions {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut options = Self::default();
        for name in value.split(',') {
            let enabled = match name {
                "copy-hints" => &mut options.copy_hints,
                "reload-reuse" => &mut options.reload_reuse,
                _ => {
                    return Err(format!(
                        "unknown allocation variant {name:?}; supported: copy-hints,reload-reuse"
                    ))
                }
            };
            if *enabled {
                return Err(format!("duplicate allocation variant {name:?}"));
            }
            *enabled = true;
        }
        Ok(options)
    }

    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        match (self.copy_hints, self.reload_reuse) {
            (false, false) => None,
            (true, false) => Some("copy-hints"),
            (false, true) => Some("reload-reuse"),
            (true, true) => Some("copy-hints,reload-reuse"),
        }
    }
}

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
    allocate_with_options(target, function, AllocationOptions::default())
}

pub fn allocate_with_options<I, T, C, A>(
    target: &A,
    function: &MachineFunction<I, T, C>,
    options: AllocationOptions,
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
        match scan_with_options(target, &current, &pins, options) {
            Ok(mut assignment) => {
                let mut compacted = MachineFunction {
                    name: current.name.clone(),
                    blocks: drop_identity_moves(&current, &mut assignment),
                };
                if options.reload_reuse {
                    reuse_local_reloads(target, &mut compacted, &mut assignment);
                }
                let frame_slots = compact_spill_slots(&mut compacted);
                // A callee-saved register is saved when an allocated value
                // occupies it or when an assembly body clobbers it.
                let used_callee_saved = assignment
                    .values()
                    .copied()
                    .chain(assembly_clobbers(&compacted.blocks))
                    .filter(|number| target.abi().callee_saved.contains(number))
                    .collect::<BTreeSet<u8>>()
                    .into_iter()
                    .collect();
                return Ok(Allocated {
                    input: function.clone(),
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
            // Machine-level inputs may already contain outgoing stores. They
            // need the same reserved area as stores introduced for a call.
            if let Some(access) = inst.stack_argument() {
                if access.kind == SpillKind::Store {
                    outgoing_slots = outgoing_slots.max(
                        access
                            .slot
                            .checked_add(access.reg.class.spill_words())
                            .ok_or("outgoing argument area overflows")?,
                    );
                }
            }
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
    // Source SSA keeps an input distinct from the value produced by asm.
    // After SSA destruction, give each tied operand a private two-address
    // occurrence. Do not coalesce the original input: it can be live after
    // the asm, or supplied to another output at a different location.
    let mut normalized = source.clone();
    let mut initialized_ties = BTreeMap::new();
    for index in 0..normalized.inputs().len() {
        let input = &normalized.inputs()[index];
        let Some(output_index) = input.tied_output() else {
            continue;
        };
        let output = normalized
            .outputs()
            .get(usize::from(output_index))
            .and_then(|operand| operand_register(operand.value()))
            .ok_or("tied assembly operand has no register output")?;
        let incoming =
            operand_register(input.value()).ok_or("tied assembly operand has no input register")?;
        if initialized_ties
            .insert(output.id, incoming)
            .is_some_and(|old| old != incoming)
        {
            return Err("conflicting tied assembly input values".into());
        }
        if incoming != output {
            before.push(I::copy(output, incoming));
            normalized.inputs_mut()[index].set_register(output);
        }
    }
    let source = &normalized;
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
    let tied_outputs: BTreeSet<_> = source
        .inputs()
        .iter()
        .filter_map(|operand| operand.tied_output().map(usize::from))
        .collect();
    let rewrite = |operand: &mut <I::Body as ConstraintBody>::Operand, tied: bool| {
        if let Some(reg) = operand_register(operand.value()) {
            if let Some(pinned) = copies.get(&reg.id) {
                operand.set_register(*pinned);
                // Only a tie implies the other operand's binding. Sharing a
                // value with a fixed input can reuse its register, but must
                // not add a constraint to an independent input or memory use.
                if tied {
                    operand.set_fixed_register(requested.get(&reg.id).copied());
                }
            }
        }
    };
    for (index, operand) in body.outputs_mut().iter_mut().enumerate() {
        rewrite(operand, tied_outputs.contains(&index));
    }
    for operand in body.inputs_mut() {
        let tied = operand.tied_output().is_some();
        rewrite(operand, tied);
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

/// Remove only same-location reloads whose previous value is still present.
/// Logical spill slots have not been compacted, so distinct slots cannot alias.
/// Replacements point directly to a retained definition, never an alias chain.
fn reuse_local_reloads<I, T, C, A>(
    target: &A,
    function: &mut MachineFunction<I, T, C>,
    assignment: &mut BTreeMap<u32, u8>,
) where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
    A: AllocationTarget<C>,
{
    if !function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|inst| inst.spill().is_some_and(|s| s.kind == SpillKind::Reload))
    {
        return;
    }
    let mut definitions = BTreeMap::<u32, usize>::new();
    let mut use_block = BTreeMap::<u32, Option<u32>>::new();
    for block in &function.blocks {
        for reg in block
            .args
            .iter()
            .copied()
            .chain(block.insts.iter().flat_map(MachInst::defs))
            .chain(block.term.defs())
        {
            *definitions.entry(reg.id).or_default() += 1;
        }
        for reg in block
            .insts
            .iter()
            .flat_map(MachInst::uses)
            .chain(block.term.uses())
        {
            use_block
                .entry(reg.id)
                .and_modify(|owner| {
                    if *owner != Some(block.id) {
                        *owner = None;
                    }
                })
                .or_insert(Some(block.id));
        }
        // Even an edge operand spelled in this block transports a value out
        // of it. Keep those reloads; this cleanup has no cross-edge domain.
        for (_, args) in block.term.edges() {
            for reg in args {
                use_block.insert(reg.id, None);
            }
        }
    }
    for block in &mut function.blocks {
        let mut available = BTreeMap::<u32, VirtualReg<C>>::new();
        let mut aliases = BTreeMap::<u32, VirtualReg<C>>::new();
        let mut kept = Vec::with_capacity(block.insts.len());
        for mut inst in std::mem::take(&mut block.insts) {
            for reg in inst.operand_slots(false) {
                if let Some(source) = aliases.get(&reg.id) {
                    *reg = *source;
                }
            }
            let spill = inst.spill();
            if let Some(SpillView {
                kind: SpillKind::Reload,
                reg,
                slot,
            }) = spill
            {
                if definitions.get(&reg.id) == Some(&1)
                    && use_block.get(&reg.id) == Some(&Some(block.id))
                {
                    if let Some(source) = available.get(&slot).copied().filter(|source| {
                        source.class == reg.class
                            && assignment.get(&reg.id).is_some_and(|location| {
                                assignment.get(&source.id) == Some(location)
                            })
                    }) {
                        aliases.insert(reg.id, source);
                        assignment.remove(&reg.id);
                        continue;
                    }
                }
            }
            if spill.is_none() && !inst.discardable() {
                available.clear();
            }
            for reg in inst.defs() {
                if reg.class.is_flags() {
                    continue;
                }
                if let Some(location) = assignment.get(&reg.id) {
                    available.retain(|_, held| {
                        held.class.bank() != reg.class.bank()
                            || assignment.get(&held.id) != Some(location)
                    });
                } else {
                    available.clear();
                }
            }
            if let Some(spill) = spill {
                match spill.kind {
                    SpillKind::Store => {
                        available.remove(&spill.slot);
                    }
                    SpillKind::Reload => {
                        if !spill.reg.class.is_flags()
                            && definitions.get(&spill.reg.id) == Some(&1)
                            && assignment.get(&spill.reg.id).is_some_and(|location| {
                                // Reserved expansion scratch may be overwritten
                                // by a spill pseudo-op without an ordinary def.
                                target.candidates(spill.reg.class, false).contains(location)
                                    || target.candidates(spill.reg.class, true).contains(location)
                            })
                        {
                            available.insert(spill.slot, spill.reg);
                        }
                    }
                }
            }
            kept.push(inst);
        }
        block.insts = kept;
        for reg in block.term.operand_slots() {
            if let Some(source) = aliases.get(&reg.id) {
                *reg = *source;
            }
        }
    }
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

#[derive(Debug, Clone)]
struct Interval<C> {
    value: u32,
    class: C,
    start: usize,
    end: usize,
    /// Sorted inclusive segments. The hull orders and expires intervals; only
    /// these segments establish interference or liveness across a call.
    segments: Vec<(usize, usize)>,
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

    let mut local_bounds = BTreeMap::new();
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
        let bounds = local_bounds.entry(block.id).or_insert_with(BTreeMap::new);
        let (start, end) = ranges[&block.id];
        for (offset, inst) in block.insts.iter().enumerate() {
            let read = start + 2 * offset;
            let write = read + 1;
            if inst.is_call() {
                calls.push(read);
            }
            for reg in inst.uses() {
                touch(reg, read, bounds);
            }
            for reg in inst.defs() {
                touch(reg, write, bounds);
            }
            // Section 7: `=&r` and `+&r` want "a register no input operand of
            // the same `asm` holds". An ordinary destination may reuse an
            // input's register because the read happens before the write; an
            // early clobber may not, so its interval starts where the reads do.
            if let Some(body) = inst.assembly() {
                for reg in early_clobbers(body) {
                    touch(reg, read, bounds);
                }
            }
            if let Some(dst) = inst.exclusive_result() {
                // The exclusive retry loop writes its old-value result
                // before consuming the address, comparison and update inputs.
                touch(dst, read, bounds);
            }
        }
        for reg in block.term.uses() {
            touch(reg, end - 2, bounds);
        }
        if let Some(body) = block.term.assembly() {
            for reg in body.defined() {
                touch(reg, end - 1, bounds);
            }
            for reg in early_clobbers(body) {
                touch(reg, end - 2, bounds);
            }
        }
    }

    // A value has a segment only in blocks that define, read or carry it.
    // Extend live-in values to entry and live-out values to exit, including
    // empty pass-through blocks and back edges. Discover every class first:
    // layout order need not place a definition before a block that carries it.
    let classes: BTreeMap<_, _> = local_bounds
        .values()
        .flat_map(|bounds| bounds.iter().map(|(&id, &(_, _, class))| (id, class)))
        .collect();
    for block in &function.blocks {
        let bounds = local_bounds.get_mut(&block.id).unwrap();
        let (start, end) = ranges[&block.id];
        let live_out: BTreeSet<u32> = block
            .term
            .edges()
            .into_iter()
            .filter_map(|(target, _)| live_in.get(&target))
            .flat_map(|entering| entering.iter().copied())
            .collect();
        for value in &live_in[&block.id] {
            touch(VirtualReg::new(*value, classes[value]), start, bounds);
        }
        for value in &live_out {
            touch(VirtualReg::new(*value, classes[value]), end - 1, bounds);
        }
    }

    let mut segments: BTreeMap<u32, Vec<(usize, usize)>> = BTreeMap::new();
    // Visit layout order, not block-ID order. Adjacent segments have no hole.
    for block in &function.blocks {
        for (&value, &(start, end, _)) in &local_bounds[&block.id] {
            let parts = segments.entry(value).or_default();
            if let Some(last) = parts
                .last_mut()
                .filter(|last| last.1 >= start.saturating_sub(1))
            {
                last.1 = end;
            } else {
                parts.push((start, end));
            }
        }
    }
    let mut result: Vec<Interval<C>> = segments
        .into_iter()
        .map(|(value, segments)| Interval {
            value,
            class: classes[&value],
            start: segments[0].0,
            end: segments.last().unwrap().1,
            spans_call: segments.iter().any(|&(start, end)| {
                calls
                    .get(calls.partition_point(|at| *at < start))
                    .is_some_and(|at| *at < end)
            }),
            segments,
        })
        .collect();
    // Walk by start point, ties by virtual register number.
    result.sort_by_key(|interval| (interval.start, interval.value));
    result
}

/// Every register that an assembly body of `blocks` clobbers, in an
/// instruction or in a terminator.
pub fn assembly_clobbers<I, T, C>(blocks: &[MachineBlock<I, T, C>]) -> BTreeSet<u8>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    blocks
        .iter()
        .flat_map(|block| {
            block
                .insts
                .iter()
                .filter_map(AllocInst::assembly)
                .chain(block.term.assembly())
                .flat_map(ConstraintBody::clobbered_registers)
        })
        .collect()
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
                    segments: vec![(start, end)],
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

#[cfg(test)]
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
    scan_with_options(target, function, pins, AllocationOptions::default())
}

fn scan_with_options<I, T, C, A>(
    target: &A,
    function: &MachineFunction<I, T, C>,
    pins: &BTreeMap<u32, u8>,
    options: AllocationOptions,
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
    let mut peers = BTreeMap::<u32, Vec<u32>>::new();
    if options.copy_hints {
        for inst in function.blocks.iter().flat_map(|block| &block.insts) {
            if let Some((dst, src)) = inst.copy_operands() {
                if dst.class == src.class && !dst.class.is_flags() && dst.id != src.id {
                    peers.entry(dst.id).or_default().push(src.id);
                    peers.entry(src.id).or_default().push(dst.id);
                }
            }
        }
    }
    let mut assignment: BTreeMap<u32, u8> = BTreeMap::new();
    let mut fixed = asm_interferences(target, function);
    let mut held: Vec<(&Interval<C>, u8)> = Vec::new();
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
            fixed.push((interval.clone(), number));
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
            .extend(interval.segments);
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
                held.push((interval, number));
                continue;
            }
        }
        let mut blockers: BTreeMap<u8, usize> = BTreeMap::new();
        for (_, number) in held.iter().filter(|(other, _)| overlaps(other, interval)) {
            *blockers.entry(*number).or_default() += 1;
        }
        let mut taken: BTreeSet<u8> = blockers.keys().copied().collect();
        let mut pinned_here = BTreeSet::new();
        for (number, ranges) in &reserved {
            if interval.segments.iter().any(|&(start, end)| {
                ranges
                    .get(ranges.partition_point(|(_, last)| *last < start))
                    .is_some_and(|(first, _)| *first <= end)
            }) {
                taken.insert(*number);
                pinned_here.insert(*number);
            }
        }
        // Section 7: "a call clobbers every caller-saved register, so an
        // interval live across a call is assigned a callee-saved register or
        // spilled."
        let candidates = target.candidates(interval.class, interval.spans_call);
        let ordinary = candidates
            .iter()
            .copied()
            .find(|number| !taken.contains(number));
        let selected = ordinary.map(|fallback| {
            if !options.copy_hints {
                return fallback;
            }
            // Hints only name locations already present in assignment. Thus
            // no new callee-save location is opened solely to honor a hint.
            peers
                .get(&interval.value)
                .into_iter()
                .flatten()
                .filter_map(|peer| assignment.get(peer).copied())
                .find(|number| candidates.contains(number) && !taken.contains(number))
                .unwrap_or(fallback)
        });
        match selected {
            Some(number) => {
                assignment.insert(interval.value, number);
                held.push((interval, number));
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
                            // A register can now hold several disjoint ranges.
                            // Evicting one frees it only if it is the sole range
                            // that conflicts with the incoming value.
                            && blockers[number] == 1
                    })
                    .max_by_key(|(other, _)| priority(other.value))
                    .map(|(other, number)| (*other, *number));
                match furthest {
                    Some((other, number)) if priority(other.value) > priority(interval.value) => {
                        assignment.remove(&other.value);
                        held.retain(|(entry, _)| entry.value != other.value);
                        victims.push(other.value);
                        assignment.insert(interval.value, number);
                        held.push((interval, number));
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

fn overlaps<C>(left: &Interval<C>, right: &Interval<C>) -> bool {
    if left.start > right.end || right.start > left.end {
        return false;
    }
    let mut right_parts = right.segments.iter().peekable();
    for &(start, end) in &left.segments {
        while right_parts.peek().is_some_and(|(_, last)| *last < start) {
            right_parts.next();
        }
        if right_parts.peek().is_some_and(|(first, _)| *first <= end) {
            return true;
        }
    }
    false
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
                    // The destination can have different incoming copies on
                    // different edges. Unite both classes of names, not only
                    // the last written destination. Every member already has
                    // the same physical location and class. Rewriting all its
                    // definitions preserves the value on each incoming path.
                    let destination = resolve(&mut merged, dst);
                    let source = resolve(&mut merged, src);
                    if destination != source {
                        merged.insert(destination.id, source);
                    }
                }
            }
        }
    }

    // Flatten before rewriting. Roots are united only with distinct roots,
    // so the forest is acyclic and needs no arbitrary chain-length limit.
    let members: Vec<_> = merged
        .iter()
        .map(|(&id, reg)| VirtualReg::new(id, reg.class))
        .collect();
    for member in members {
        resolve(&mut merged, member);
    }
    let representative = |reg: VirtualReg<C>| merged.get(&reg.id).copied().unwrap_or(reg);
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
                    *slot = representative(*slot);
                }
                for slot in rewritten.operand_slots(true) {
                    *slot = representative(*slot);
                }
                insts.push(rewritten);
            }
            let mut term = block.term.clone();
            for slot in term.operand_slots() {
                *slot = representative(*slot);
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
    merged: &mut BTreeMap<u32, VirtualReg<C>>,
    reg: VirtualReg<C>,
) -> VirtualReg<C> {
    let mut current = reg;
    let mut path = Vec::new();
    while let Some(next) = merged.get(&current.id) {
        path.push(current.id);
        current = *next;
    }
    for id in path {
        merged.insert(id, current);
    }
    current
}
