//! Section 7's linear scan and the allocated form it produces.
//!
//! The fixed operands of Section 7 are made short-lived first: a copy is
//! placed on each side of every call and of the return, and the entry
//! parameters are copied out of their boundary registers at the top of the
//! function. Each boundary register is then live across one move and the call
//! or return itself, so a pre-assignment can never collide with another, and
//! the scan below only has to respect them rather than reconcile them.
//!
//! Spilling is a rewrite followed by a rerun: an interval that cannot be given
//! a register is written to its frame slot after each definition and read back
//! before each use, through registers whose live ranges are one instruction
//! long, and the scan is run again over the rewritten function. That is the
//! same allocation Section 7 describes, arrived at without a second
//! interference structure.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{
    inst_defs, inst_uses, regalloc, term_edges, term_uses, AsmBody, AsmConstraint, AsmOperand,
    AsmValue, MBlock, MFunction, MInst, MTerm, RegClass, Target, VReg,
};

/// Section 6: "`x0` to `x17` are caller-saved", less the two intra-procedure
/// scratch registers, which Section 2 keeps for emitted sequences.
pub const CALLER_SAVED: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

/// Section 6: "`x19` to `x28` ... are callee-saved".
pub const CALLEE_SAVED: [u8; 10] = [19, 20, 21, 22, 23, 24, 25, 26, 27, 28];

/// Section 2: "`x16` and `x17` are the intra-procedure-call scratch
/// registers, used only by emitted sequences".
pub const SCRATCH: u8 = 16;

/// SIMD numbers occupy a disjoint allocator bank. v30/v31 are emission
/// scratch; v8-v15 are deliberately not allocated because only their low
/// halves are callee saved. Live-across-call vectors spill at full width.
pub const SIMD_BASE: u8 = 64;
pub const SIMD_SCRATCH: u8 = SIMD_BASE + 30;
pub const SIMD_CALLER_SAVED: [u8; 22] = [
    64, 65, 66, 67, 68, 69, 70, 71, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
];

/// Independent integer and SIMD argument banks share one stack cursor.
#[derive(Default)]
pub(crate) struct ArgumentCursor {
    general: u8,
    simd: u8,
    stack: u32,
}

impl ArgumentCursor {
    pub(crate) fn extent(&self) -> (u8, u8, u64) {
        (self.general, self.simd, u64::from(self.stack) * 8)
    }
    /// Once the GPR bank is exhausted, alignment is relative to the shared
    /// stack cursor, which may already include spilled SIMD arguments.
    pub(crate) fn general_pair_padding(&self) -> usize {
        if self.general < 8 {
            usize::from(self.general % 2)
        } else {
            (self.stack % 2) as usize
        }
    }
    pub(crate) fn place(&mut self, class: RegClass) -> Result<u8, u32> {
        let (next, base) = if class.is_simd() {
            (&mut self.simd, SIMD_BASE)
        } else {
            (&mut self.general, 0)
        };
        if *next < 8 {
            let number = base + *next;
            *next += 1;
            Ok(number)
        } else {
            let words = if class == RegClass::Simd128 { 2 } else { 1 };
            self.stack = self.stack.next_multiple_of(words);
            let slot = self.stack;
            self.stack += words;
            Err(slot)
        }
    }
}

/// Section 6: arguments and results occupy `x0` to `x7`.
const BOUNDARY: u8 = 8;

/// Section 6 (revision 0.18): "above 16 bytes the destination pointer travels
/// in `x8`, not in an argument register".
const INDIRECT_RESULT: u8 = 8;

/// Virtual registers this pass creates start above the out-of-SSA scratch
/// range, so that a boundary copy is never mistaken for a cycle break.
const FRESH_BASE: u32 = regalloc::SCRATCH_LIMIT;

/// One allocated function: the same blocks with every virtual register
/// assigned, no block arguments, and a frame slot for each spilled value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocated {
    pub name: String,
    pub blocks: Vec<MBlock>,
    /// Virtual register identity to physical register number.
    pub assignment: BTreeMap<u32, u8>,
    /// How many virtual registers went to the frame.
    pub spilled: usize,
    /// Eight-byte slots in the spill area.
    pub frame_slots: u32,
    /// Callee-saved registers the body uses, which the prologue must save.
    pub used_callee_saved: Vec<u8>,
    /// How many arguments the function takes, which Section 6 puts in `x0`
    /// onwards and which are therefore defined when the function is entered.
    pub parameters: usize,
    /// Bytes of outgoing stack arguments this function's calls need, which
    /// Section 6's frame layout reserves at the bottom of the frame (Linux
    /// only, revision 0.10).
    pub outgoing: u64,
    /// Whether the last parameter is the destination of a record result above
    /// sixteen bytes and therefore arrives in `x8` (Section 6, revision 0.18).
    pub indirect_result: bool,
    /// Whether the EvoIR function this was allocated from has internal
    /// linkage (Section 8, revision 0.16), which emission answers by writing
    /// no `.globl` for its symbol.
    ///
    /// Allocation is a property of the body alone, so it never sets this;
    /// whoever knows the EvoIR function does, before emission.
    pub internal: bool,
}

/// Assign a physical register to every virtual register of an out-of-SSA
/// function, spilling to the frame when a class runs out.
pub fn allocate(function: &MFunction) -> Result<Allocated, String> {
    allocate_for(Target::Darwin, function)
}

/// The same, for a named target.
///
/// Only Section 6's "Stack arguments" rule distinguishes them: on Linux an
/// argument that finds no register goes to the frame, and on Darwin more than
/// eight are refused in lowering and never reach here.
pub fn allocate_for(target: Target, function: &MFunction) -> Result<Allocated, String> {
    let mut next = FRESH_BASE;
    let parameters = function.blocks.first().map_or(0, |block| block.args.len());
    let indirect_result = function.blocks.first().is_some_and(|block| {
        block
            .insts
            .first()
            .is_some_and(|inst| matches!(inst, MInst::IndirectResult))
    });
    let (mut current, pins, outgoing) = pin_boundaries(target, function, &mut next)?;
    let mut slots: BTreeMap<u32, u32> = BTreeMap::new();

    // Each round either succeeds or sends at least one interval to the frame,
    // and a value that has been spilled is never a candidate again, so the
    // loop is bounded by the number of virtual registers.
    for _ in 0..64 {
        match scan(&current, &pins) {
            Ok(mut assignment) => {
                let mut compacted = MFunction {
                    name: current.name.clone(),
                    blocks: drop_identity_moves(&current, &mut assignment),
                };
                let frame_slots = compact_spill_slots(&mut compacted);
                let used_callee_saved = assignment
                    .values()
                    .copied()
                    .filter(|number| CALLEE_SAVED.contains(number))
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

// ---------------------------------------------------------------------------
// Fixed operands
// ---------------------------------------------------------------------------

fn fresh(next: &mut u32, class: RegClass) -> VReg {
    let reg = VReg::new(*next, class);
    *next += 1;
    reg
}

/// Copy every boundary value into or out of a register of its own, and record
/// which physical register each of those short-lived registers must have.
fn pin_boundaries(
    platform: Target,
    function: &MFunction,
    next: &mut u32,
) -> Result<(MFunction, BTreeMap<u32, u8>, u64), String> {
    let mut pins: BTreeMap<u32, u8> = BTreeMap::new();
    let mut blocks = Vec::new();
    let mut outgoing_slots: u32 = 0;
    let mut next_block = function.blocks.iter().map(|b| b.id).max().unwrap_or(0) + 1;

    for (index, block) in function.blocks.iter().enumerate() {
        let mut insts = Vec::new();

        if index == 0 {
            let mut cursor = ArgumentCursor::default();
            // Section 6 (revision 0.18): an `indirect.result` opening the
            // entry block says the last parameter is the destination a record
            // result above sixteen bytes is written through, and that it
            // arrives in `x8` rather than in an argument register. Every other
            // position is unchanged, because the destination is last.
            let indirect = block
                .insts
                .first()
                .is_some_and(|inst| matches!(inst, MInst::IndirectResult))
                .then(|| block.args.len().saturating_sub(1));
            // Section 6: the parameters arrive in x0 to x7. Each is copied out
            // at once, so the boundary register is live over one move.
            for (position, parameter) in block.args.iter().enumerate() {
                if indirect == Some(position) {
                    let incoming = fresh(next, parameter.class);
                    pins.insert(incoming.id, INDIRECT_RESULT);
                    insts.push(MInst::Mov {
                        dst: *parameter,
                        src: incoming,
                    });
                    continue;
                }
                // Section 6: an argument past the eighth "goes to the stack in
                // an 8-byte slot ... the callee reads slot `i` at `x29 + 16 +
                // 8 * i`". Darwin refuses those in lowering, so `position` is
                // always inside the register file there.
                let location = cursor.place(parameter.class);
                if let Err(slot) = location {
                    insts.push(MInst::IncomingArg {
                        dst: *parameter,
                        slot,
                    });
                    continue;
                }
                let incoming = fresh(next, parameter.class);
                pins.insert(incoming.id, location.unwrap());
                insts.push(MInst::Mov {
                    dst: *parameter,
                    src: incoming,
                });
            }
        }

        let mut indirect_call = false;
        for (offset, inst) in block.insts.iter().enumerate() {
            // The entry block's own marker names this function's last
            // parameter, not the next call's, and it was read above.
            if index == 0 && offset == 0 && matches!(inst, MInst::IndirectResult) {
                insts.push(inst.clone());
                continue;
            }
            match inst {
                MInst::IndirectResult => {
                    indirect_call = true;
                    insts.push(inst.clone());
                }
                MInst::Bl {
                    symbol,
                    args,
                    results,
                } => {
                    let (moved, outputs) = pin_call(
                        platform,
                        &mut insts,
                        args,
                        results,
                        next,
                        &mut pins,
                        &mut outgoing_slots,
                        std::mem::take(&mut indirect_call),
                    );
                    insts.push(MInst::Bl {
                        symbol: symbol.clone(),
                        args: moved,
                        results: outputs.iter().map(|(pinned, _)| *pinned).collect(),
                    });
                    for (pinned, original) in outputs {
                        insts.push(MInst::Mov {
                            dst: original,
                            src: pinned,
                        });
                    }
                }
                MInst::Blr {
                    target,
                    args,
                    results,
                } => {
                    let (moved, outputs) = pin_call(
                        platform,
                        &mut insts,
                        args,
                        results,
                        next,
                        &mut pins,
                        &mut outgoing_slots,
                        std::mem::take(&mut indirect_call),
                    );
                    insts.push(MInst::Blr {
                        target: *target,
                        args: moved,
                        results: outputs.iter().map(|(pinned, _)| *pinned).collect(),
                    });
                    for (pinned, original) in outputs {
                        insts.push(MInst::Mov {
                            dst: original,
                            src: pinned,
                        });
                    }
                }
                MInst::Asm(body) => {
                    let (body, after) = pin_assembly(body, &mut insts, next, &mut pins)?;
                    insts.push(MInst::Asm(body));
                    insts.extend(after);
                }
                other => insts.push(other.clone()),
            }
        }

        let mut bridges = Vec::new();
        let term = match &block.term {
            MTerm::AsmGoto {
                body,
                labels,
                fallthrough,
                fallthrough_args,
            } => {
                let (body, after) = pin_assembly(body, &mut insts, next, &mut pins)?;
                let mut edge = |target, args: &Vec<VReg>| {
                    if after.is_empty() {
                        return (target, args.clone());
                    }
                    let id = next_block;
                    next_block += 1;
                    bridges.push(MBlock {
                        id,
                        args: Vec::new(),
                        insts: after.clone(),
                        term: MTerm::B {
                            target,
                            args: args.clone(),
                        },
                    });
                    (id, Vec::new())
                };
                let labels = labels
                    .iter()
                    .map(|(target, args)| edge(*target, args))
                    .collect();
                let (fallthrough, fallthrough_args) = edge(*fallthrough, fallthrough_args);
                MTerm::AsmGoto {
                    body,
                    labels,
                    fallthrough,
                    fallthrough_args,
                }
            }
            MTerm::Ret { values } => {
                let mut moved = Vec::new();
                let mut cursor = ArgumentCursor::default();
                for value in values {
                    let outgoing = fresh(next, value.class);
                    pins.insert(
                        outgoing.id,
                        cursor
                            .place(value.class)
                            .map_err(|_| "too many return registers")?,
                    );
                    insts.push(MInst::Mov {
                        dst: outgoing,
                        src: *value,
                    });
                    moved.push(outgoing);
                }
                MTerm::Ret { values: moved }
            }
            other => other.clone(),
        };

        blocks.push(MBlock {
            id: block.id,
            args: Vec::new(),
            insts,
            term,
        });
        blocks.extend(bridges);
    }

    // Section 7's cycle scratch is pinned where Section 2 puts it.
    for block in &blocks {
        for reg in block
            .insts
            .iter()
            .flat_map(inst_defs)
            .chain(block.insts.iter().flat_map(inst_uses))
        {
            if regalloc::is_scratch(reg) {
                pins.insert(
                    reg.id,
                    if reg.class.is_simd() {
                        SIMD_SCRATCH
                    } else {
                        SCRATCH
                    },
                );
            }
        }
    }

    Ok((
        MFunction {
            name: function.name.clone(),
            blocks,
        },
        pins,
        // `sp` stays 16-byte aligned at every call, so the area is rounded up.
        u64::from(outgoing_slots).next_multiple_of(2) * 8,
    ))
}

/// A binding constrains only the asm, never the whole C object's lifetime.
/// Tied/read-write operands already share a VReg after machine lowering.
fn pin_assembly(
    source: &AsmBody,
    before: &mut Vec<MInst>,
    next: &mut u32,
    pins: &mut BTreeMap<u32, u8>,
) -> Result<(AsmBody, Vec<MInst>), String> {
    let mut requested = BTreeMap::new();
    for operand in source.numbered() {
        if let Some(number) = operand.fixed_register {
            if !CALLER_SAVED.contains(&number)
                && !CALLEE_SAVED.contains(&number)
                && number != 30
                && !SIMD_CALLER_SAVED.contains(&number)
            {
                return Err(format!("unsupported assembly register binding x{number}"));
            }
            let reg = operand
                .register()
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
    for operand in source.numbered() {
        if let Some(reg) = operand.register() {
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
    for operand in source.inputs.iter().chain(
        source
            .outputs
            .iter()
            .filter(|o| o.constraint.direction() == crate::AsmDirection::ReadWrite),
    ) {
        if let Some(reg) = operand.register() {
            if let Some(pinned) = copies.get(&reg.id) {
                if initialized.insert(reg.id) {
                    before.push(MInst::Mov {
                        dst: *pinned,
                        src: reg,
                    });
                }
            }
        }
    }
    let after = source
        .defined()
        .into_iter()
        .filter_map(|reg| {
            copies.get(&reg.id).map(|pinned| MInst::Mov {
                dst: reg,
                src: *pinned,
            })
        })
        .collect();
    let mut body = source.clone();
    for operand in body.outputs.iter_mut().chain(&mut body.inputs) {
        if let Some(reg) = operand.register() {
            if let Some(pinned) = copies.get(&reg.id) {
                operand.value = AsmValue::Reg(*pinned);
                operand.fixed_register = requested.get(&reg.id).copied();
            }
        }
    }
    Ok((body, after))
}

type PinnedResults = Vec<(VReg, VReg)>;

#[allow(clippy::too_many_arguments)]
fn pin_call(
    _platform: Target,
    insts: &mut Vec<MInst>,
    args: &[VReg],
    results: &[VReg],
    next: &mut u32,
    pins: &mut BTreeMap<u32, u8>,
    outgoing_slots: &mut u32,
    indirect_result: bool,
) -> (Vec<VReg>, PinnedResults) {
    // Section 6 (revision 0.18): with the marker, the call's last argument is
    // the destination of a record result above sixteen bytes and goes to `x8`.
    let indirect = indirect_result.then(|| args.len().saturating_sub(1));
    let mut moved = Vec::new();
    let mut cursor = ArgumentCursor::default();
    for (position, arg) in args.iter().enumerate() {
        if indirect == Some(position) {
            let outgoing = fresh(next, arg.class);
            pins.insert(outgoing.id, INDIRECT_RESULT);
            insts.push(MInst::Mov {
                dst: outgoing,
                src: *arg,
            });
            moved.push(outgoing);
            continue;
        }
        // Section 6: the slots are "assigned in argument order at increasing
        // addresses from the caller's `sp` at the call".
        let location = cursor.place(arg.class);
        if let Err(slot) = location {
            insts.push(MInst::OutgoingArg { src: *arg, slot });
            *outgoing_slots = (*outgoing_slots).max(cursor.stack);
            continue;
        }
        let outgoing = fresh(next, arg.class);
        pins.insert(outgoing.id, location.unwrap());
        insts.push(MInst::Mov {
            dst: outgoing,
            src: *arg,
        });
        moved.push(outgoing);
    }
    let mut outputs = Vec::new();
    let mut cursor = ArgumentCursor::default();
    for result in results {
        let incoming = fresh(next, result.class);
        pins.insert(incoming.id, cursor.place(result.class).unwrap_or(BOUNDARY));
        outputs.push((incoming, *result));
    }
    (moved, outputs)
}

// ---------------------------------------------------------------------------
// Numbering, liveness and intervals
// ---------------------------------------------------------------------------

/// The half-open position range of each block, in the block order of Section 2.
///
/// Each instruction owns two positions: its operands are read at the even one
/// and its results are written at the odd one. Without that a move's
/// destination could never be given its source's register, and the only
/// coalescing Section 7 admits -- "removing a move whose two operands got the
/// same register" -- would never fire.
fn numbering(function: &MFunction) -> BTreeMap<u32, (usize, usize)> {
    let mut ranges = BTreeMap::new();
    let mut position = 0;
    for block in &function.blocks {
        let start = position;
        position += 2 * (block.insts.len() + 1);
        ranges.insert(block.id, (start, position));
    }
    ranges
}

/// Backward dataflow to the least fixed point: what each block reads before it
/// writes, and what its successors need.
fn liveness(function: &MFunction) -> BTreeMap<u32, BTreeSet<u32>> {
    let mut uses: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut defs: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for block in &function.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            for reg in inst_uses(inst) {
                if !written.contains(&reg.id) {
                    read.insert(reg.id);
                }
            }
            for reg in inst_defs(inst) {
                written.insert(reg.id);
            }
        }
        for reg in term_uses(&block.term) {
            if !written.contains(&reg.id) {
                read.insert(reg.id);
            }
        }
        if let MTerm::AsmGoto { body, .. } = &block.term {
            written.extend(body.defined().iter().map(|reg| reg.id));
        }
        uses.insert(block.id, read);
        defs.insert(block.id, written);
    }
    live_on_entry(function, &uses, &defs)
}

fn live_on_entry(
    function: &MFunction,
    uses: &BTreeMap<u32, BTreeSet<u32>>,
    defs: &BTreeMap<u32, BTreeSet<u32>>,
) -> BTreeMap<u32, BTreeSet<u32>> {
    let mut live_in: BTreeMap<u32, BTreeSet<u32>> = function
        .blocks
        .iter()
        .map(|block| (block.id, BTreeSet::new()))
        .collect();
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let mut out: BTreeSet<u32> = BTreeSet::new();
            for (target, _) in term_edges(&block.term) {
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

#[derive(Debug, Clone, Copy)]
struct Interval {
    value: u32,
    class: RegClass,
    start: usize,
    end: usize,
    spans_call: bool,
}

fn intervals(function: &MFunction) -> Vec<Interval> {
    let ranges = numbering(function);
    let live_in = liveness(function);

    let mut bounds: BTreeMap<u32, (usize, usize, RegClass)> = BTreeMap::new();
    let touch = |value: VReg, at: usize, bounds: &mut BTreeMap<u32, (usize, usize, RegClass)>| {
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
            if matches!(inst, MInst::Bl { .. } | MInst::Blr { .. }) {
                calls.push(read);
            }
            for reg in inst_uses(inst) {
                touch(reg, read, &mut bounds);
            }
            for reg in inst_defs(inst) {
                touch(reg, write, &mut bounds);
            }
            // Section 7: `=&r` and `+&r` want "a register no input operand of
            // the same `asm` holds". An ordinary destination may reuse an
            // input's register because the read happens before the write; an
            // early clobber may not, so its interval starts where the reads do.
            if let MInst::Asm(body) = inst {
                for reg in early_clobbers(body) {
                    touch(reg, read, &mut bounds);
                }
            }
            if let MInst::AtomicRmw { dst, .. } = inst {
                // The exclusive retry loop writes its old-value result
                // before consuming the address, comparison and update inputs.
                touch(*dst, read, &mut bounds);
            }
        }
        for reg in term_uses(&block.term) {
            touch(reg, end - 2, &mut bounds);
        }
        if let MTerm::AsmGoto { body, .. } = &block.term {
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
        let live_out: BTreeSet<u32> = term_edges(&block.term)
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

    let mut result: Vec<Interval> = bounds
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

// ---------------------------------------------------------------------------
// The scan
// ---------------------------------------------------------------------------

/// Assign registers, or report the intervals that have to go to the frame.
///
/// The fixed operands are taken first and held for the whole of their (short)
/// intervals, so the walk never has to displace one; a register a fixed
/// interval holds is simply unavailable while it does.
/// The registers an `asm`'s early-clobber operands occupy.
fn early_clobbers(body: &AsmBody) -> Vec<VReg> {
    body.outputs
        .iter()
        .filter(|operand| operand.constraint.early_clobber())
        .filter_map(AsmOperand::register)
        .collect()
}

/// Section 7: "A clobber names a register the `asm` writes: the allocator
/// treats the whole instruction as defining it, so no live value may occupy it
/// across the instruction."
///
/// The clobber is not a value with an interval; it is a register that is
/// unavailable at one point. It reaches the scan as a fixed interference: an
/// interval nothing owns, holding the register over the instruction, which
/// every overlapping interval must avoid.
fn asm_interferences(function: &MFunction) -> Vec<(Interval, u8)> {
    let ranges = numbering(function);
    let mut fixed = Vec::new();
    let mut next = CLOBBER_BASE;
    let mut record = |start: usize, end: usize, number: u8, next: &mut u32| {
        for class in [RegClass::Gpr32, RegClass::Gpr64] {
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
            if let MInst::Asm(body) = inst {
                let read = start + 2 * offset;
                for number in body.clobbered_registers() {
                    record(read, read + 1, number, &mut next);
                }
            }
        }
        if let MTerm::AsmGoto { body, .. } = &block.term {
            for number in body.clobbered_registers() {
                record(end - 2, end - 1, number, &mut next);
            }
        }
    }
    fixed
}

/// Interference intervals start their identities here, above any virtual
/// register a function can hold, so the scan can tell them from values.
const CLOBBER_BASE: u32 = u32::MAX / 2;

/// Section 7: a digit constraint means "the same register as the output that
/// digit names (a tied operand); the two intervals are one".
fn tied_pairs(function: &MFunction) -> BTreeMap<u32, u32> {
    let mut pairs = BTreeMap::new();
    let mut collect = |body: &AsmBody| {
        for input in &body.inputs {
            let AsmConstraint::Tied(digit) = input.constraint else {
                continue;
            };
            let (Some(input_reg), Some(output)) = (
                input.register(),
                body.outputs
                    .get(usize::from(digit))
                    .and_then(AsmOperand::register),
            ) else {
                continue;
            };
            pairs.insert(input_reg.id, output.id);
        }
    };
    for block in &function.blocks {
        for inst in &block.insts {
            if let MInst::Asm(body) = inst {
                collect(body);
            }
        }
        if let MTerm::AsmGoto { body, .. } = &block.term {
            collect(body);
        }
    }
    pairs
}

/// Read positions use the same numbering as intervals, including terminators.
/// The final interval endpoint is not the next read: a value can be needed
/// immediately and again much later, with other values unused in between.
fn read_positions(function: &MFunction) -> BTreeMap<u32, Vec<usize>> {
    let ranges = numbering(function);
    let mut reads = BTreeMap::<u32, Vec<usize>>::new();
    for block in &function.blocks {
        let (start, end) = ranges[&block.id];
        for (offset, inst) in block.insts.iter().enumerate() {
            for reg in inst_uses(inst) {
                reads.entry(reg.id).or_default().push(start + 2 * offset);
            }
        }
        for reg in term_uses(&block.term) {
            reads.entry(reg.id).or_default().push(end - 2);
        }
    }
    reads
}

fn scan(function: &MFunction, pins: &BTreeMap<u32, u8>) -> Result<BTreeMap<u32, u8>, Vec<u32>> {
    let all = intervals(function);
    let reads = read_positions(function);
    let tied = tied_pairs(function);
    let mut assignment: BTreeMap<u32, u8> = BTreeMap::new();
    let mut fixed = asm_interferences(function);
    let mut held: Vec<(Interval, u8)> = Vec::new();
    let mut victims: Vec<u32> = Vec::new();

    for interval in &all {
        if interval.class == RegClass::Flags {
            // Section 2 gives the class one register, and the adjacency rule
            // keeps every live range inside one pair of instructions.
            assignment.insert(interval.value, 0);
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
        if interval.class == RegClass::Flags || pins.contains_key(&interval.value) {
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
        let candidates: Vec<u8> = if interval.class.is_simd() {
            if interval.spans_call {
                Vec::new()
            } else {
                SIMD_CALLER_SAVED.to_vec()
            }
        } else if interval.spans_call {
            CALLEE_SAVED.to_vec()
        } else {
            CALLER_SAVED
                .iter()
                .chain(CALLEE_SAVED.iter())
                .copied()
                .collect()
        };
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

const fn overlaps(left: &Interval, right: &Interval) -> bool {
    left.start <= right.end && right.start <= left.end
}

/// Section 7: "No coalescing beyond removing a move whose two operands got the
/// same register."
///
/// Removing the move leaves its destination without a definition, so the two
/// names are merged: every later mention of the destination becomes the
/// source. The physical result is identical and the function still reads as
/// one definition per value, which is what the verifier of Section 9 checks.
fn drop_identity_moves(function: &MFunction, assignment: &mut BTreeMap<u32, u8>) -> Vec<MBlock> {
    let mut merged: BTreeMap<u32, VReg> = BTreeMap::new();
    for block in &function.blocks {
        for inst in &block.insts {
            if let MInst::Mov { dst, src } = inst {
                if dst.class == src.class && assignment.get(&dst.id) == assignment.get(&src.id) {
                    let source = resolve(&merged, *src);
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
                if let MInst::Mov { dst, src } = inst {
                    if dst.class == src.class && assignment.get(&dst.id) == assignment.get(&src.id)
                    {
                        continue;
                    }
                }
                let mut rewritten = inst.clone();
                for slot in operand_slots(&mut rewritten, false) {
                    *slot = resolve(&merged, *slot);
                }
                for slot in operand_slots(&mut rewritten, true) {
                    *slot = resolve(&merged, *slot);
                }
                insts.push(rewritten);
            }
            let mut term = block.term.clone();
            for slot in term_slots(&mut term) {
                *slot = resolve(&merged, *slot);
            }
            MBlock {
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

fn resolve(merged: &BTreeMap<u32, VReg>, reg: VReg) -> VReg {
    let mut current = reg;
    for _ in 0..64 {
        match merged.get(&current.id) {
            Some(next) if next.id != current.id => current = *next,
            _ => return VReg::new(current.id, reg.class),
        }
    }
    VReg::new(current.id, reg.class)
}

fn term_slots(term: &mut MTerm) -> Vec<&mut VReg> {
    match term {
        MTerm::Indirect { target, edges } => std::iter::once(target)
            .chain(edges.iter_mut().flat_map(|(_, args)| args))
            .collect(),
        MTerm::B { .. } | MTerm::Brk { .. } => Vec::new(),
        MTerm::BCond { flags, .. } => vec![flags],
        MTerm::Cb { src, .. } | MTerm::Tb { src, .. } => vec![src],
        MTerm::Ret { values } => values.iter_mut().collect(),
        // Coalescing must rename constrained operands too: their old virtual
        // identities are removed from the assignment map after this pass.
        MTerm::AsmGoto { body, .. } => body
            .outputs
            .iter_mut()
            .chain(body.inputs.iter_mut())
            .filter_map(|operand| match &mut operand.value {
                AsmValue::Reg(reg) => Some(reg),
                _ => None,
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Spill code
// ---------------------------------------------------------------------------

/// Reuse eight-byte spill locations only when their CFG live ranges do not
/// overlap. A convex interval in block-layout order would make unrelated
/// branch arms interfere, especially when edge-copy blocks are appended.
/// Virtual spill identities stay separate throughout allocation; physical
/// slot reuse happens once, after the final rewrite.
fn compact_spill_slots(function: &mut MFunction) -> u32 {
    let mut sizes = BTreeMap::new();
    let mut uses = BTreeMap::new();
    let mut defs = BTreeMap::new();
    for block in &function.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            if let MInst::SpillReload { dst: reg, slot } | MInst::SpillStore { src: reg, slot } =
                inst
            {
                sizes.insert(
                    *slot,
                    if reg.class == RegClass::Simd128 {
                        2_u32
                    } else {
                        1
                    },
                );
            }
            match inst {
                MInst::SpillReload { slot, .. } => {
                    if !written.contains(slot) {
                        read.insert(*slot);
                    }
                }
                MInst::SpillStore { slot, .. } => {
                    written.insert(*slot);
                }
                _ => continue,
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
        for (target, _) in term_edges(&block.term) {
            if let Some(slots) = live_in.get(&target) {
                for slot in slots {
                    let index = indices[slot];
                    *live.entry(index / 64).or_default() |= 1 << (index % 64);
                }
            }
        }
        for inst in block.insts.iter().rev() {
            let slot = match inst {
                MInst::SpillStore { slot, .. } | MInst::SpillReload { slot, .. } => *slot,
                _ => continue,
            };
            let index = indices[&slot];
            // A store conflicts with values live afterwards even if its own
            // result is dead: the emitted store still overwrites its slot.
            // A reload also keeps its value distinct from every other value
            // live at that point, including across loop backedges.
            for (word, bits) in &live {
                *conflicts[index].entry(*word).or_default() |= bits;
            }
            if matches!(inst, MInst::SpillStore { .. }) {
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
        if let MInst::SpillStore { slot, .. } | MInst::SpillReload { slot, .. } = inst {
            *slot = mapping[slot];
        }
    }
    count
}

/// Spill around indivisible flags producer/consumer chains. Reloads precede
/// the whole chain, stores follow it, and a value defined inside the chain
/// is forwarded to its consumers instead of being reloaded from its old slot.
fn insert_spill_code(
    function: &MFunction,
    slots: &BTreeMap<u32, u32>,
    victims: &[u32],
    next: &mut u32,
) -> MFunction {
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
                for reg in inst_uses(inst) {
                    if spilled.contains(&reg.id) {
                        let temporary = *temporaries.entry(reg.id).or_insert_with(|| {
                            let temporary = fresh(next, reg.class);
                            before.push(MInst::SpillReload {
                                dst: temporary,
                                slot: slots[&reg.id],
                            });
                            temporary
                        });
                        replace_use(&mut rewritten, reg, temporary);
                    }
                }
                let mut defines_flags = false;
                for reg in inst_defs(inst) {
                    defines_flags |= reg.class == RegClass::Flags;
                    if spilled.contains(&reg.id) {
                        let temporary = fresh(next, reg.class);
                        temporaries.insert(reg.id, temporary);
                        replace_def(&mut rewritten, reg, temporary);
                        after.push(MInst::SpillStore {
                            src: temporary,
                            slot: slots[&reg.id],
                        });
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
        for reg in term_uses(&block.term) {
            if spilled.contains(&reg.id) && !temporaries.contains_key(&reg.id) {
                let temporary = fresh(next, reg.class);
                temporaries.insert(reg.id, temporary);
                insts.push(MInst::SpillReload {
                    dst: temporary,
                    slot: slots[&reg.id],
                });
                replace_term_use(&mut term, reg, temporary);
            }
        }
        if let MTerm::AsmGoto { body, .. } = &block.term {
            for reg in body.defined() {
                if spilled.contains(&reg.id) {
                    let temporary = *temporaries
                        .entry(reg.id)
                        .or_insert_with(|| fresh(next, reg.class));
                    replace_term_use(&mut term, reg, temporary);
                    stores.push(MInst::SpillStore {
                        src: temporary,
                        slot: slots[&reg.id],
                    });
                }
            }
        }
        // Assembly outputs and deferred flags-chain stores belong to every
        // outgoing edge, not to a shared successor another path can enter.
        let mut bridges = Vec::new();
        if !stores.is_empty() {
            let edges: Vec<_> = match &mut term {
                MTerm::AsmGoto {
                    labels,
                    fallthrough,
                    fallthrough_args,
                    ..
                } => labels
                    .iter_mut()
                    .map(|(target, args)| (target, args))
                    .chain(std::iter::once((fallthrough, fallthrough_args)))
                    .collect(),
                MTerm::BCond {
                    taken,
                    taken_args,
                    untaken,
                    untaken_args,
                    ..
                } => vec![(taken, taken_args), (untaken, untaken_args)],
                _ => Vec::new(),
            };
            for (target, args) in edges {
                let id = next_block;
                next_block += 1;
                bridges.push(MBlock {
                    id,
                    args: Vec::new(),
                    insts: stores.clone(),
                    term: MTerm::B {
                        target: *target,
                        args: std::mem::take(args),
                    },
                });
                *target = id;
            }
        }
        blocks.push(MBlock {
            id: block.id,
            args: Vec::new(),
            insts,
            term,
        });
        blocks.extend(bridges);
    }
    MFunction {
        name: function.name.clone(),
        blocks,
    }
}

fn swap_reg(slot: &mut VReg, from: VReg, to: VReg) {
    if slot.id == from.id {
        *slot = to;
    }
}

fn replace_use(inst: &mut MInst, from: VReg, to: VReg) {
    for reg in operand_slots(inst, false) {
        swap_reg(reg, from, to);
    }
}

fn replace_def(inst: &mut MInst, from: VReg, to: VReg) {
    for reg in operand_slots(inst, true) {
        swap_reg(reg, from, to);
    }
}

/// Mutable access to an instruction's operand slots, definitions or uses.
fn operand_slots(inst: &mut MInst, definitions: bool) -> Vec<&mut VReg> {
    match inst {
        MInst::Vector { dst, args, .. }
        | MInst::Aes { dst, args, .. }
        | MInst::Floating { dst, args, .. } => {
            if definitions {
                vec![dst]
            } else {
                args.iter_mut().collect()
            }
        }
        // An `asm`'s operands are named by their constraints and rewritten
        // through the operand list, never through a spill: Section 7 requires
        // each to be in the register its constraint asks for, and a reload
        // placed between the constraint and the template would not be one.
        MInst::Asm(body) => {
            let mut slots = Vec::new();
            for operand in body.outputs.iter_mut().chain(body.inputs.iter_mut()) {
                let is_definition = !matches!(operand.constraint, AsmConstraint::Memory { .. })
                    && operand.constraint.direction().writes();
                if is_definition != definitions {
                    continue;
                }
                if let AsmValue::Reg(reg) = &mut operand.value {
                    slots.push(reg);
                }
            }
            slots
        }
        MInst::Mov { dst, src } | MInst::StackAlloc { dst, size: src, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![src]
            }
        }
        MInst::MovImm { dst, .. }
        | MInst::Adrp { dst, .. }
        | MInst::Adr { dst, .. }
        | MInst::FrameSlot { dst, .. }
        | MInst::FrameAddress { dst, .. } => {
            if definitions {
                vec![dst]
            } else {
                Vec::new()
            }
        }
        MInst::VaSaveArea { .. } | MInst::IndirectResult | MInst::FpEnvCheck => Vec::new(),
        MInst::AddPageOff { dst, src, .. }
        | MInst::Extend { dst, src, .. }
        | MInst::Unary { dst, src, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![src]
            }
        }
        MInst::Atomic {
            load, value, base, ..
        } => {
            if definitions {
                if *load {
                    vec![value]
                } else {
                    Vec::new()
                }
            } else if *load {
                vec![base]
            } else {
                vec![value, base]
            }
        }
        MInst::StackRelease { address } => {
            if definitions {
                Vec::new()
            } else {
                vec![address]
            }
        }
        MInst::AtomicRmw {
            dst,
            base,
            value,
            expected,
            ..
        } => {
            if definitions {
                vec![dst]
            } else {
                let mut values = vec![base, value];
                values.extend(expected.iter_mut());
                values
            }
        }
        MInst::Load { dst, base, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![base]
            }
        }
        MInst::Store { src, base, .. } => {
            if definitions {
                Vec::new()
            } else {
                vec![src, base]
            }
        }
        MInst::Binary { dst, lhs, rhs, .. } | MInst::Cmp { dst, lhs, rhs, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![lhs, rhs]
            }
        }
        MInst::BinaryImm { dst, lhs, .. }
        | MInst::CmpImm { dst, lhs, .. }
        | MInst::Bfx { dst, src: lhs, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![lhs]
            }
        }
        MInst::Msub { dst, n, m, a, .. } | MInst::Madd { dst, n, m, a, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![n, m, a]
            }
        }
        MInst::CarryBinary {
            dst,
            flags,
            lhs,
            rhs,
            ..
        }
        | MInst::SubCarryFlags {
            dst,
            flags_in: flags,
            lhs,
            rhs,
            ..
        }
        | MInst::AddCarryFlags {
            dst,
            flags_in: flags,
            lhs,
            rhs,
            ..
        } => {
            if definitions {
                vec![dst]
            } else {
                vec![flags, lhs, rhs]
            }
        }
        MInst::TstImm { dst, lhs, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![lhs]
            }
        }
        MInst::FlagBinary {
            dst,
            flags,
            lhs,
            rhs,
            ..
        } => {
            if definitions {
                vec![dst, flags]
            } else {
                vec![lhs, rhs]
            }
        }
        MInst::WideMultiply { dst, lhs, rhs, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![lhs, rhs]
            }
        }
        MInst::Cset { dst, flags, .. } => {
            if definitions {
                vec![dst]
            } else {
                vec![flags]
            }
        }
        MInst::Csel {
            dst,
            flags,
            lhs,
            rhs,
            ..
        } => {
            if definitions {
                vec![dst]
            } else {
                vec![flags, lhs, rhs]
            }
        }
        MInst::MemCopy { dst, src, .. } => {
            if definitions {
                Vec::new()
            } else {
                vec![dst, src]
            }
        }
        MInst::Bl { args, results, .. } => {
            if definitions {
                results.iter_mut().collect()
            } else {
                args.iter_mut().collect()
            }
        }
        MInst::Blr {
            target,
            args,
            results,
        } => {
            if definitions {
                results.iter_mut().collect()
            } else {
                let mut slots = vec![target];
                slots.extend(args.iter_mut());
                slots
            }
        }
        MInst::SpillStore { src, .. } => {
            if definitions {
                Vec::new()
            } else {
                vec![src]
            }
        }
        MInst::SpillReload { dst, .. } => {
            if definitions {
                vec![dst]
            } else {
                Vec::new()
            }
        }
        MInst::IncomingArg { dst, .. } => {
            if definitions {
                vec![dst]
            } else {
                Vec::new()
            }
        }
        MInst::OutgoingArg { src, .. } => {
            if definitions {
                Vec::new()
            } else {
                vec![src]
            }
        }
        MInst::Ldp { rt1, rt2, base, .. } => {
            if definitions {
                vec![rt1, rt2]
            } else {
                vec![base]
            }
        }
        MInst::Stp { rt1, rt2, base, .. } => {
            if definitions {
                Vec::new()
            } else {
                vec![rt1, rt2, base]
            }
        }
    }
}

fn replace_term_use(term: &mut MTerm, from: VReg, to: VReg) {
    match term {
        MTerm::Indirect { target, edges } => {
            swap_reg(target, from, to);
            for value in edges.iter_mut().flat_map(|(_, args)| args) {
                swap_reg(value, from, to);
            }
        }
        MTerm::B { .. } | MTerm::Brk { .. } => {}
        MTerm::BCond { flags, .. } => swap_reg(flags, from, to),
        MTerm::Cb { src, .. } | MTerm::Tb { src, .. } => swap_reg(src, from, to),
        MTerm::Ret { values } => {
            for value in values {
                swap_reg(value, from, to);
            }
        }
        MTerm::AsmGoto { body, .. } => {
            for operand in body.outputs.iter_mut().chain(body.inputs.iter_mut()) {
                if let AsmValue::Reg(reg) = &mut operand.value {
                    swap_reg(reg, from, to);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The post-allocation textual form
// ---------------------------------------------------------------------------

/// The physical spelling of one assigned virtual register.
#[must_use]
pub fn physical(class: RegClass, number: u8) -> String {
    match class {
        RegClass::Gpr32 => format!("w{number}"),
        RegClass::Gpr64 => format!("x{number}"),
        RegClass::Simd32 => format!("s{}", number - SIMD_BASE),
        RegClass::Simd64 => format!("d{}", number - SIMD_BASE),
        RegClass::Simd128 => format!("q{}", number - SIMD_BASE),
        RegClass::Flags => "nzcv".to_owned(),
    }
}

/// Render an allocated function in the post-allocation spelling of Appendix A.
#[must_use]
pub fn render_allocated(allocated: &Allocated) -> String {
    // A use is written without its class, so the physical spelling of one is
    // recovered from the definition that owns it.
    let mut classes: BTreeMap<u32, RegClass> = BTreeMap::new();
    for block in &allocated.blocks {
        for reg in block
            .insts
            .iter()
            .flat_map(inst_defs)
            .chain(block.insts.iter().flat_map(inst_uses))
            .chain(term_uses(&block.term))
        {
            classes.insert(reg.id, reg.class);
        }
    }
    let name = |reg: VReg| -> String {
        let class = classes.get(&reg.id).copied().unwrap_or(reg.class);
        allocated.assignment.get(&reg.id).map_or_else(
            || format!("%v{}", reg.id),
            |number| physical(class, *number),
        )
    };
    let mut text = String::new();
    let _ = writeln!(text, "func @{} {{", allocated.name);
    for (index, block) in allocated.blocks.iter().enumerate() {
        if index != 0 {
            text.push('\n');
        }
        let label = if index == 0 {
            "entry".to_owned()
        } else {
            format!("b{}", block.id)
        };
        let _ = writeln!(text, "{label}:");
        for inst in &block.insts {
            let _ = writeln!(text, "    {}", render_inst(inst, &name));
        }
        let _ = writeln!(text, "    {}", render_term(allocated, &block.term, &name));
    }
    text.push_str("}\n");
    text
}

fn render_inst(inst: &MInst, name: &impl Fn(VReg) -> String) -> String {
    // The pre-allocation renderer already spells every form; the only
    // difference here is that an operand is a physical register, that a
    // definition carries no class, and that the two spill pseudo-forms name a
    // frame slot rather than an address.
    match inst {
        MInst::SpillStore { src, slot } => format!("str.x {}, [slot #{slot}]", name(*src)),
        MInst::SpillReload { dst, slot } => format!("{} = ldr.x [slot #{slot}]", name(*dst)),
        other => {
            let pre = crate::text::render_inst(other);
            rename(&pre, name)
        }
    }
}

fn render_term(allocated: &Allocated, term: &MTerm, name: &impl Fn(VReg) -> String) -> String {
    match term {
        // Section 6 returns in x0 onwards, so the registers are already fixed
        // and Appendix A prints a bare `ret`.
        MTerm::Ret { .. } => "ret".to_owned(),
        other => {
            let function = MFunction {
                name: allocated.name.clone(),
                blocks: allocated.blocks.clone(),
            };
            let pre = crate::text::render_term(&function, other);
            rename(&pre, name)
        }
    }
}

/// Replace every `%vN` in a pre-allocation rendering with its physical name.
fn rename(text: &str, name: &impl Fn(VReg) -> String) -> String {
    let mut out = String::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == '%' && bytes.get(index + 1) == Some(&'v') {
            let mut end = index + 2;
            let mut id = 0_u32;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                id = id * 10 + (bytes[end] as u32 - '0' as u32);
                end += 1;
            }
            // A definition carries its class after a colon; a use does not.
            // A definition carries `:class`, which the physical spelling
            // replaces; a use carries nothing and is looked up by identity.
            let skip = if bytes.get(end) == Some(&':') {
                let mut tail = end + 1;
                while tail < bytes.len() && bytes[tail].is_ascii_alphanumeric() {
                    tail += 1;
                }
                tail
            } else {
                end
            };
            out.push_str(&name(VReg::new(id, RegClass::Gpr32)));
            index = skip;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

#[cfg(test)]
mod spill_flag_tests {
    use super::*;
    use crate::{CarryOp, Cond, FlagOp, Width};

    fn word(id: u32) -> VReg {
        VReg::new(id, RegClass::Gpr64)
    }
    fn flags(id: u32) -> VReg {
        VReg::new(id, RegClass::Flags)
    }

    #[test]
    fn spill_victim_uses_next_read_instead_of_interval_end() {
        let count = (CALLER_SAVED.len() + CALLEE_SAVED.len()) as u32;
        let mut insts: Vec<_> = (0..=count)
            .map(|id| MInst::MovImm {
                dst: word(id),
                imm: u64::from(id),
            })
            .collect();
        // All registers are occupied when `count` is defined. Value zero
        // has the latest LAST read, but must be used immediately. Value one
        // has the most distant NEXT read and is the Section 7 spill victim.
        for id in [0, count].into_iter().chain(2..count).chain([1, 0]) {
            insts.push(MInst::SpillStore {
                src: word(id),
                slot: id,
            });
        }
        let function = MFunction {
            name: "next_read_pressure".into(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts,
                term: MTerm::Ret { values: Vec::new() },
            }],
        };
        assert_eq!(scan(&function, &BTreeMap::new()), Err(vec![1]));
    }

    #[test]
    fn next_read_victim_cannot_lend_a_register_with_a_future_pin() {
        let count = (CALLER_SAVED.len() + CALLEE_SAVED.len()) as u32;
        let mut insts: Vec<_> = (0..=count)
            .map(|id| MInst::MovImm {
                dst: word(id),
                imm: u64::from(id),
            })
            .collect();
        // Zero occupies x0 until just before its future fixed use. Although
        // zero's next read is furthest, `count` remains live beyond that pin
        // and cannot take x0. One is the furthest eligible victim instead.
        for id in [count].into_iter().chain(2..count).chain([1, 0]) {
            insts.push(MInst::SpillStore {
                src: word(id),
                slot: id,
            });
        }
        insts.extend([
            MInst::MovImm {
                dst: word(count + 1),
                imm: 0,
            },
            MInst::SpillStore {
                src: word(count + 1),
                slot: count + 1,
            },
            MInst::SpillStore {
                src: word(count),
                slot: count,
            },
        ]);
        let function = MFunction {
            name: "next_read_fixed_pressure".into(),
            blocks: vec![MBlock {
                id: 0,
                args: Vec::new(),
                insts,
                term: MTerm::Ret { values: Vec::new() },
            }],
        };
        assert_eq!(
            scan(&function, &BTreeMap::from([(count + 1, 0)])),
            Err(vec![1])
        );
    }

    // Independent set-based reference for the packed conflict representation.
    // Keep the deliberately small oracle separate from production bitsets.
    fn reference_spill_mapping(function: &MFunction) -> (BTreeMap<u32, u32>, u32) {
        let mut sizes = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let mut defs = BTreeMap::new();
        for block in &function.blocks {
            let mut read = BTreeSet::new();
            let mut written = BTreeSet::new();
            for inst in &block.insts {
                match inst {
                    MInst::SpillReload { dst, slot } => {
                        sizes.insert(*slot, if dst.class == RegClass::Simd128 { 2 } else { 1 });
                        if !written.contains(slot) {
                            read.insert(*slot);
                        }
                    }
                    MInst::SpillStore { src, slot } => {
                        sizes.insert(*slot, if src.class == RegClass::Simd128 { 2 } else { 1 });
                        written.insert(*slot);
                    }
                    _ => {}
                }
            }
            uses.insert(block.id, read);
            defs.insert(block.id, written);
        }
        let live_in = live_on_entry(function, &uses, &defs);
        let mut graph: BTreeMap<u32, BTreeSet<u32>> =
            sizes.keys().map(|slot| (*slot, BTreeSet::new())).collect();
        for block in &function.blocks {
            let mut live: BTreeSet<_> = term_edges(&block.term)
                .iter()
                .flat_map(|(target, _)| live_in[target].iter().copied())
                .collect();
            for inst in block.insts.iter().rev() {
                let slot = match inst {
                    MInst::SpillReload { slot, .. } | MInst::SpillStore { slot, .. } => *slot,
                    _ => continue,
                };
                for other in live.iter().copied().filter(|other| *other != slot) {
                    graph.get_mut(&slot).unwrap().insert(other);
                    graph.get_mut(&other).unwrap().insert(slot);
                }
                if matches!(inst, MInst::SpillStore { .. }) {
                    live.remove(&slot);
                } else {
                    live.insert(slot);
                }
            }
        }
        let mut order: Vec<_> = sizes.keys().copied().collect();
        order.sort_unstable_by_key(|slot| (Reverse(graph[slot].len()), *slot));
        let mut mapping = BTreeMap::new();
        let mut count = 0;
        for slot in order {
            let occupied: BTreeSet<_> = graph[&slot]
                .iter()
                .flat_map(|other| {
                    mapping
                        .get(other)
                        .into_iter()
                        .flat_map(|start| *start..*start + sizes[other])
                })
                .collect();
            let size = sizes[&slot];
            let start = (0_u32..)
                .find(|start| {
                    start.is_multiple_of(size)
                        && (*start..*start + size).all(|at| !occupied.contains(&at))
                })
                .unwrap();
            count = count.max(start + size);
            mapping.insert(slot, start);
        }
        (mapping, count)
    }

    #[test]
    fn packed_spill_conflicts_match_set_graph_across_backedges_and_dead_stores() {
        let mut random = 13_u64;
        for case in 0..128_u32 {
            let mut blocks = Vec::new();
            for id in 0..3 {
                let mut insts = Vec::new();
                for _ in 0..128 {
                    random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let slot = (random >> 32) as u32 % (case % 31 + 1);
                    let reg = VReg::new(
                        slot,
                        if slot.is_multiple_of(3) {
                            RegClass::Simd128
                        } else {
                            RegClass::Gpr64
                        },
                    );
                    let slot = 17 + slot * 3;
                    insts.push(if random & 1 == 0 {
                        MInst::SpillStore { src: reg, slot }
                    } else {
                        MInst::SpillReload { dst: reg, slot }
                    });
                }
                blocks.push(MBlock {
                    id,
                    args: vec![],
                    insts,
                    term: if id == 2 {
                        MTerm::BCond {
                            flags: flags(100),
                            cond: Cond::Ne,
                            taken: 1,
                            taken_args: vec![],
                            untaken: 0,
                            untaken_args: vec![],
                        }
                    } else {
                        MTerm::B {
                            target: id + 1,
                            args: vec![],
                        }
                    },
                });
            }
            let mut function = MFunction {
                name: "spill_graph".into(),
                blocks,
            };
            let (mapping, count) = reference_spill_mapping(&function);
            let mut expected = function.clone();
            for inst in expected
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.insts)
            {
                if let MInst::SpillReload { slot, .. } | MInst::SpillStore { slot, .. } = inst {
                    *slot = mapping[slot];
                }
            }
            assert_eq!(compact_spill_slots(&mut function), count);
            assert_eq!(function, expected);
        }
    }

    #[test]
    fn repeated_dense_vector_spills_keep_every_live_slot_distinct() {
        let mut insts = Vec::new();
        for slot in 0..256 {
            insts.push(MInst::SpillStore {
                src: VReg::new(slot, RegClass::Simd128),
                slot,
            });
        }
        for _ in 0..256 {
            for slot in 0..256 {
                insts.push(MInst::SpillReload {
                    dst: VReg::new(slot, RegClass::Simd128),
                    slot,
                });
            }
        }
        let mut function = MFunction {
            name: "dense_spills".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![],
                insts,
                term: MTerm::Ret { values: vec![] },
            }],
        };
        assert_eq!(compact_spill_slots(&mut function), 512);
        for inst in &function.blocks[0].insts {
            match inst {
                MInst::SpillStore { src, slot } | MInst::SpillReload { dst: src, slot } => {
                    assert_eq!(*slot, 2 * src.id)
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn disjoint_vector_spills_reuse_one_slot_despite_many_sparse_identities() {
        let insts = (0..16_384)
            .flat_map(|id| {
                let reg = VReg::new(id, RegClass::Simd128);
                let slot = 29 + id * 17;
                [
                    MInst::SpillStore { src: reg, slot },
                    MInst::SpillReload { dst: reg, slot },
                ]
            })
            .collect();
        let mut function = MFunction {
            name: "disjoint_spills".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![],
                insts,
                term: MTerm::Ret { values: vec![] },
            }],
        };
        assert_eq!(compact_spill_slots(&mut function), 2);
        for inst in &function.blocks[0].insts {
            match inst {
                MInst::SpillStore { slot, .. } | MInst::SpillReload { slot, .. } => {
                    assert_eq!(*slot, 0);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn spill_reload_and_store_preserve_carry_adjacency() {
        for chain in [false, true] {
            let mut insts = vec![
                MInst::MovImm {
                    dst: word(0),
                    imm: 1,
                },
                MInst::MovImm {
                    dst: word(1),
                    imm: u64::MAX,
                },
                MInst::MovImm {
                    dst: word(2),
                    imm: 7,
                },
                MInst::FlagBinary {
                    op: FlagOp::Adds,
                    dst: word(3),
                    flags: flags(4),
                    lhs: word(0),
                    rhs: word(1),
                    width: Width::X,
                },
            ];
            if chain {
                insts.push(MInst::SubCarryFlags {
                    dst: flags(5),
                    flags_in: flags(4),
                    lhs: word(3),
                    rhs: word(2),
                    width: Width::X,
                });
                insts.push(MInst::Cset {
                    dst: word(6),
                    flags: flags(5),
                    cond: Cond::Hi,
                });
            } else {
                insts.push(MInst::CarryBinary {
                    op: CarryOp::Adc,
                    dst: word(6),
                    flags: flags(4),
                    lhs: word(3),
                    rhs: word(2),
                    width: Width::X,
                });
            }
            let function = MFunction {
                name: "carry_spill".into(),
                blocks: vec![MBlock {
                    id: 0,
                    args: vec![],
                    insts,
                    term: MTerm::Ret {
                        values: vec![word(6)],
                    },
                }],
            };
            for victims in [vec![2], vec![3], vec![2, 3, 6]] {
                let slots = victims
                    .iter()
                    .enumerate()
                    .map(|(slot, reg)| (*reg, slot as u32))
                    .collect();
                let rewritten = insert_spill_code(&function, &slots, &victims, &mut 100);
                crate::validate::validate_function(&rewritten).unwrap();
                let mut allocated = allocate(&rewritten).unwrap();
                allocated.frame_slots = slots.len() as u32;
                crate::verify_allocation(&allocated).unwrap();
            }
        }
    }

    #[test]
    fn spill_stores_after_a_flag_setting_branch_execute_on_each_edge() {
        let function = MFunction {
            name: "branch_spill".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        MInst::MovImm {
                            dst: word(0),
                            imm: 1,
                        },
                        MInst::MovImm {
                            dst: word(1),
                            imm: 2,
                        },
                        MInst::FlagBinary {
                            op: FlagOp::Adds,
                            dst: word(3),
                            flags: flags(4),
                            lhs: word(0),
                            rhs: word(1),
                            width: Width::X,
                        },
                    ],
                    term: MTerm::BCond {
                        flags: flags(4),
                        cond: Cond::Eq,
                        taken: 1,
                        taken_args: vec![],
                        untaken: 2,
                        untaken_args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: vec![],
                    term: MTerm::Ret {
                        values: vec![word(3)],
                    },
                },
                MBlock {
                    id: 2,
                    args: vec![],
                    insts: vec![],
                    term: MTerm::Ret {
                        values: vec![word(3)],
                    },
                },
            ],
        };
        let rewritten = insert_spill_code(&function, &BTreeMap::from([(3, 0)]), &[3], &mut 100);
        let mut allocated = allocate(&rewritten).unwrap();
        allocated.frame_slots = 1;
        crate::verify_allocation(&allocated).unwrap();
        assert_eq!(
            rewritten
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .filter(|i| matches!(i, MInst::SpillStore { .. }))
                .count(),
            2
        );
    }
}
