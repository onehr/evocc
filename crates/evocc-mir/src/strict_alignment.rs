//! The strict-alignment check of Section 9.5.
//!
//! Under `-mstrict-align`, every load, store and pair that emission writes
//! must be aligned to the bytes it moves, to the element width for a pair.
//! Alignment checking (`SCTLR_ELx.A`) traps on any other access, and so does
//! code that runs with the MMU off, where all memory is Device memory.
//!
//! The check reads the allocated function, its post-SSA input and the frame
//! that emission lays out. It derives the alignment of each address from its
//! base object or slot and its offset; it does not read the lowering's choice
//! of access widths. It accepts an access's own alignment only where an EvoIR
//! guard proves it: an access that EvoIR performs carries the alignment of its
//! guard (Section 4.1), and the EvoIR interpreter traps when that guard fails.
//! A transport access moves record bytes that no EvoIR access reads, so it
//! has only the derived alignment. A refusal (`E4022`) prevents emission.

use std::collections::HashMap;

use evocc_evoir::GlobalObject;

use crate::emit::{Frame, StackGuard};
use crate::isa::aarch64::abi::Aarch64Abi;
use crate::isa::aarch64::insts::assembly_defs;
use crate::{
    inst_defs, term_edges, Allocated, BinOp, ExtOp, FlagOp, FramePlace, MBlock, MFunction, MInst,
    MTerm, RegClass, SymbolKind, Target, VReg, VerifierRefusal,
};

/// The largest power of two of a `u64`: the alignment of zero and the largest
/// modulus that the check keeps.
const ANY: u64 = 1 << 63;

/// Bytes that the Section 4.6 `stack.alloc` header and `stack.release` move.
const HEADER_WORD: u64 = 8;

/// The largest power of two that divides `value`, or `ANY` for zero.
const fn low_bit(value: u64) -> u64 {
    if value == 0 {
        ANY
    } else {
        1 << value.trailing_zeros()
    }
}

/// `2^(a + b)` for the alignments `2^a` and `2^b`, or `ANY` beyond it.
fn product(left: u64, right: u64) -> u64 {
    let bits = left.trailing_zeros() + right.trailing_zeros();
    if bits >= ANY.trailing_zeros() {
        ANY
    } else {
        1 << bits
    }
}

fn refusal(block: u32, detail: String) -> VerifierRefusal {
    VerifierRefusal {
        code: "E4022",
        name: "alignment: verifier-refused",
        block,
        detail,
    }
}

/// The bytes that one register of `class` moves to or from memory.
const fn register_bytes(class: RegClass) -> u64 {
    match class {
        RegClass::Gpr32 | RegClass::Simd32 => 4,
        RegClass::Gpr64 | RegClass::Simd64 | RegClass::Flags => 8,
        RegClass::Simd128 => 16,
    }
}

/// Check one allocated function under `-mstrict-align`.
///
/// `frame` is the frame that emission lays out for it, and `globals` gives the
/// alignment of the module's own data symbols.
pub fn verify_strict_alignment(
    target: Target,
    allocated: &Allocated,
    frame: &Frame,
    globals: &[GlobalObject],
) -> Result<(), VerifierRefusal> {
    let abi = target.aarch64_abi();
    let stack = abi.facts.stack_alignment_bytes;
    if !frame.alignment.is_power_of_two()
        || frame.alignment < stack
        || !frame.size.is_multiple_of(stack)
    {
        return Err(refusal(
            0,
            format!(
                "the frame of {} bytes at alignment {} does not keep sp {stack}-byte aligned",
                frame.size, frame.alignment
            ),
        ));
    }
    let symbols: HashMap<&str, u64> = globals
        .iter()
        .map(|global| {
            (
                global.symbol.as_str(),
                u64::from(global.layout.align_bytes).max(1),
            )
        })
        .collect();
    check_frame(abi, allocated, frame)?;
    let known = value_facts(&allocated.input, abi, frame, &symbols);
    check_values(&allocated.input, &known)
}

/// The alignment of the frame address `offset` bytes above the aligned base
/// of the frame (Sections 6.8 to 6.11).
fn in_frame(frame: &Frame, offset: u64) -> u64 {
    frame.alignment.min(low_bit(offset))
}

/// Refuse a frame access of `width` bytes at alignment `alignment`.
fn frame_access(
    block: u32,
    place: &dyn Fn() -> String,
    alignment: u64,
    width: u64,
) -> Result<(), VerifierRefusal> {
    if alignment >= width {
        Ok(())
    } else {
        Err(refusal(
            block,
            format!("{} moves {width} bytes at alignment {alignment}", place()),
        ))
    }
}

/// The frame record, the save slots, the canary, the variadic save areas,
/// the fixed objects, the spill slots and the stack arguments.
fn check_frame(
    abi: &Aarch64Abi,
    allocated: &Allocated,
    frame: &Frame,
) -> Result<(), VerifierRefusal> {
    // The prologue stores the frame record at the incoming `sp`, which the
    // ABI keeps 16-byte aligned, and every other slot below it.
    for (number, offset) in &frame.saved {
        frame_access(
            0,
            &|| format!("the save slot of x{number} at frame offset {offset}"),
            in_frame(frame, *offset),
            8,
        )?;
    }
    if !frame.general_registers_only {
        for (number, offset) in &frame.saved_vectors {
            frame_access(
                0,
                &|| format!("the save slot of d{number} at frame offset {offset}"),
                in_frame(frame, *offset),
                8,
            )?;
        }
    }
    if let Some((offset, guard)) = &frame.stack_guard {
        frame_access(
            0,
            &|| format!("the canary slot at frame offset {offset}"),
            in_frame(frame, *offset),
            8,
        )?;
        // The global guard is a pointer-sized object that its GOT entry
        // names; a system-register guard sits at a fixed offset from a
        // thread pointer that its platform keeps 8-byte aligned.
        if let StackGuard::SystemRegister { register, offset } = guard {
            frame_access(
                0,
                &|| format!("the guard at {register} + {offset}"),
                low_bit(*offset as u64),
                8,
            )?;
        }
    }
    if let Some((named, base)) = frame.register_save {
        let general = abi.general_arguments() as u64;
        for number in u64::from(named)..general {
            let offset = base + number * abi.general_save_bytes();
            frame_access(
                0,
                &|| format!("the variadic save slot of x{number} at frame offset {offset}"),
                in_frame(frame, offset),
                8,
            )?;
        }
        if !frame.general_registers_only {
            let vectors = abi.vector_arguments() as u64;
            for number in 0..vectors {
                let offset =
                    base + abi.general_save_area_bytes() + number * abi.vector_save_bytes();
                frame_access(
                    0,
                    &|| format!("the variadic save slot of q{number} at frame offset {offset}"),
                    in_frame(frame, offset),
                    16,
                )?;
            }
        }
    }
    let stack = abi.facts.stack_alignment_bytes;
    for block in &allocated.blocks {
        for inst in &block.insts {
            match inst {
                MInst::FrameSlot { origin, align, .. } => {
                    let Some(offset) = frame.slots.get(origin) else {
                        return Err(refusal(
                            block.id,
                            format!("frame object {origin} has no offset in the frame"),
                        ));
                    };
                    let alignment = in_frame(frame, *offset);
                    if alignment < u64::from(*align) {
                        return Err(refusal(
                            block.id,
                            format!(
                                "frame object {origin} at frame offset {offset} has alignment \
                                 {alignment}, not its declared {align}"
                            ),
                        ));
                    }
                }
                MInst::SpillStore { src: reg, slot } | MInst::SpillReload { dst: reg, slot } => {
                    let Some(offset) = frame.spills.get(slot) else {
                        return Err(refusal(
                            block.id,
                            format!("spill slot {slot} has no offset in the frame"),
                        ));
                    };
                    frame_access(
                        block.id,
                        &|| {
                            format!(
                                "the {} access of spill slot {slot} at frame offset {offset}",
                                reg.class.mnemonic()
                            )
                        },
                        in_frame(frame, *offset),
                        register_bytes(reg.class),
                    )?;
                }
                // The incoming area starts at `x29 + 16`, and a caller's
                // outgoing area at its `sp`; the ABI keeps both 16-byte aligned.
                MInst::IncomingArg { dst, slot } => {
                    let offset = abi.facts.incoming_argument_offset(*slot);
                    frame_access(
                        block.id,
                        &|| format!("incoming stack argument {slot} at x29 + {offset}"),
                        stack.min(low_bit(offset)),
                        register_bytes(dst.class),
                    )?;
                }
                MInst::OutgoingArg { src, slot } => {
                    let offset = abi.facts.outgoing_argument_offset(*slot);
                    frame_access(
                        block.id,
                        &|| format!("outgoing stack argument {slot} at sp + {offset}"),
                        stack.min(low_bit(offset)),
                        register_bytes(src.class),
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// What the check knows of a value: it is `residue` modulo `modulus`, a power
/// of two. A modulus of one knows nothing. The alignment of an address is the
/// largest power of two that divides every value admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Residue {
    modulus: u64,
    residue: u64,
}

impl Residue {
    const UNKNOWN: Self = Self {
        modulus: 1,
        residue: 0,
    };

    /// `value` modulo `modulus`.
    fn new(modulus: u64, value: u64) -> Self {
        let modulus = modulus.clamp(1, ANY);
        Self {
            modulus,
            residue: value & (modulus - 1),
        }
    }

    /// A multiple of `alignment`.
    fn aligned(alignment: u64) -> Self {
        Self::new(alignment, 0)
    }

    fn alignment(self) -> u64 {
        self.modulus.min(low_bit(self.residue))
    }

    /// The value plus `delta`, with the wrapping arithmetic of the machine.
    fn offset(self, delta: u64) -> Self {
        Self::new(self.modulus, self.residue.wrapping_add(delta))
    }

    fn add(self, other: Self) -> Self {
        Self::new(
            self.modulus.min(other.modulus),
            self.residue.wrapping_add(other.residue),
        )
    }

    fn sub(self, other: Self) -> Self {
        Self::new(
            self.modulus.min(other.modulus),
            self.residue.wrapping_sub(other.residue),
        )
    }

    /// The values that either admits: the meet where paths join.
    fn join(self, other: Self) -> Self {
        let modulus = self
            .modulus
            .min(other.modulus)
            .min(low_bit(self.residue.wrapping_sub(other.residue)));
        Self::new(modulus, self.residue)
    }

    /// The value seen through its low `bits` bits, as an extension or a
    /// 32-bit operation sees it.
    fn truncated(self, bits: u32) -> Self {
        Self::new(self.modulus.min(1 << bits.min(63)), self.residue)
    }
}

/// What is known of each virtual register of `function` on every path.
///
/// An entry argument is unknown. A block argument joins its incoming edges,
/// and any other definition follows its operation (`definitions`); a
/// definition waits until its operands are known. A join only weakens a fact,
/// so the iteration ends.
fn value_facts(
    function: &MFunction,
    abi: &Aarch64Abi,
    frame: &Frame,
    symbols: &HashMap<&str, u64>,
) -> HashMap<u32, Residue> {
    let blocks: HashMap<u32, &MBlock> = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect();
    let mut known: HashMap<u32, Residue> = HashMap::new();
    let meet = |known: &mut HashMap<u32, Residue>, reg: VReg, fact: Residue| -> bool {
        match known.get(&reg.id) {
            Some(old) if old.join(fact) == *old => false,
            Some(old) => {
                let joined = old.join(fact);
                known.insert(reg.id, joined);
                true
            }
            None => {
                known.insert(reg.id, fact);
                true
            }
        }
    };
    if let Some(entry) = function.blocks.first() {
        for reg in &entry.args {
            meet(&mut known, *reg, Residue::UNKNOWN);
        }
    }
    loop {
        let mut changed = false;
        for block in &function.blocks {
            for inst in &block.insts {
                for (reg, fact) in definitions(inst, &known, abi, frame, symbols) {
                    changed |= meet(&mut known, reg, fact);
                }
            }
            if let MTerm::AsmGoto { body, .. } = &block.term {
                for reg in assembly_defs(body) {
                    changed |= meet(&mut known, reg, Residue::UNKNOWN);
                }
            }
            for (target, args) in term_edges(&block.term) {
                let Some(successor) = blocks.get(&target) else {
                    continue;
                };
                for (parameter, argument) in successor.args.iter().zip(args) {
                    if let Some(fact) = known.get(&argument.id).copied() {
                        changed |= meet(&mut known, *parameter, fact);
                    }
                }
            }
        }
        if !changed {
            return known;
        }
    }
}

/// The fact of each register that `inst` defines, once its operands are
/// known.
fn definitions(
    inst: &MInst,
    known: &HashMap<u32, Residue>,
    abi: &Aarch64Abi,
    frame: &Frame,
    symbols: &HashMap<&str, u64>,
) -> Vec<(VReg, Residue)> {
    let of = |reg: &VReg| known.get(&reg.id).copied();
    let stack = abi.facts.stack_alignment_bytes;
    let fact = match inst {
        MInst::Mov { src, .. } => of(src),
        MInst::Extend { op, src, .. } => of(src).map(|fact| {
            fact.truncated(match op {
                ExtOp::Sxtb | ExtOp::Uxtb => 8,
                ExtOp::Sxth | ExtOp::Uxth => 16,
                ExtOp::Sxtw => 32,
            })
        }),
        MInst::MovImm { imm, .. } => Some(Residue::new(ANY, *imm)),
        MInst::BinaryImm { op, lhs, imm, .. } => of(lhs).map(|fact| match op {
            BinOp::Add => fact.offset(*imm),
            BinOp::Sub => fact.offset(imm.wrapping_neg()),
            // Bits that the mask clears are zero, below the modulus too.
            BinOp::And => Residue::new(fact.modulus.max(low_bit(*imm)), fact.residue & imm),
            BinOp::Orr => Residue::new(fact.modulus, fact.residue | imm),
            BinOp::Eor => Residue::new(fact.modulus, fact.residue ^ imm),
            BinOp::Lsl if *imm < 64 => Residue::new(
                if *imm <= u64::from(fact.modulus.leading_zeros()) {
                    fact.modulus << imm
                } else {
                    ANY
                },
                fact.residue << imm,
            ),
            BinOp::Mul => Residue::aligned(product(fact.alignment(), low_bit(*imm))),
            _ => Residue::UNKNOWN,
        }),
        MInst::Binary { op, lhs, rhs, .. } => of(lhs).zip(of(rhs)).map(|(left, right)| match op {
            BinOp::Add => left.add(right),
            BinOp::Sub => left.sub(right),
            BinOp::And => Residue::aligned(left.alignment().max(right.alignment())),
            BinOp::Orr | BinOp::Eor => Residue::aligned(left.alignment().min(right.alignment())),
            BinOp::Mul => Residue::aligned(product(left.alignment(), right.alignment())),
            BinOp::Lsl => Residue::aligned(left.alignment()),
            _ => Residue::UNKNOWN,
        }),
        MInst::Csel { lhs, rhs, .. } => of(lhs).zip(of(rhs)).map(|(left, right)| left.join(right)),
        // Each object lies at its laid-out offset above the aligned frame
        // base, wherever emission addresses it from (Sections 6.8 to 6.11).
        MInst::FrameSlot { origin, .. } => Some(Residue::new(
            frame.alignment,
            frame.slots.get(origin).copied().unwrap_or(0),
        )),
        MInst::StackAlloc { align, .. } => Some(Residue::aligned(u64::from(*align).max(stack))),
        MInst::FrameAddress { place, .. } => Some(match place {
            FramePlace::IncomingStack => Residue::new(stack, abi.facts.incoming_argument_offset(0)),
            FramePlace::RegisterSaveTop => Residue::new(
                frame.alignment,
                frame
                    .register_save
                    .map_or(0, |(_, base)| base + abi.general_save_area_bytes()),
            ),
            FramePlace::FramePointer => Residue::aligned(stack),
            FramePlace::SavedLinkRegister | FramePlace::AncestorFrame(_) => Residue::UNKNOWN,
        }),
        MInst::AddPageOff {
            symbol,
            kind: SymbolKind::Global,
            ..
        } => Some(
            symbols
                .get(symbol.as_str())
                .map_or(Residue::UNKNOWN, |align| Residue::aligned(*align)),
        ),
        _ => Some(Residue::UNKNOWN),
    };
    let defined = inst_defs(inst);
    let value = |reg: VReg, fact: Residue| match reg.class {
        RegClass::Gpr64 => fact,
        RegClass::Gpr32 => fact.truncated(32),
        _ => Residue::UNKNOWN,
    };
    match inst {
        // The flags that an `adds` or `subs` also defines are no address.
        MInst::FlagBinary {
            op, dst, lhs, rhs, ..
        } => {
            let Some((left, right)) = of(lhs).zip(of(rhs)) else {
                return Vec::new();
            };
            let sum = match op {
                FlagOp::Adds => left.add(right),
                FlagOp::Subs => left.sub(right),
            };
            defined
                .into_iter()
                .map(|reg| {
                    if reg == *dst {
                        (reg, value(reg, sum))
                    } else {
                        (reg, Residue::UNKNOWN)
                    }
                })
                .collect()
        }
        _ if defined.len() == 1 => fact
            .map(|fact| vec![(defined[0], value(defined[0], fact))])
            .unwrap_or_default(),
        _ => defined
            .into_iter()
            .map(|reg| (reg, Residue::UNKNOWN))
            .collect(),
    }
}

/// Every access of the post-SSA program, which allocation keeps unchanged.
fn check_values(
    function: &MFunction,
    known: &HashMap<u32, Residue>,
) -> Result<(), VerifierRefusal> {
    // The alignment of the address `offset` bytes from `base`.
    let at = |base: &VReg, offset: i64| {
        known
            .get(&base.id)
            .copied()
            .unwrap_or(Residue::UNKNOWN)
            .offset(offset as u64)
            .alignment()
    };
    for block in &function.blocks {
        let refuse = |what: String, alignment: u64, width: u64| {
            refusal(
                block.id,
                format!("{what} moves {width} bytes at alignment {alignment}"),
            )
        };
        for inst in &block.insts {
            match inst {
                MInst::Load {
                    align,
                    dst,
                    base,
                    offset,
                    kind,
                    transport,
                    ..
                } => {
                    let width = kind.size_bytes();
                    if let Some(alignment) = scalar(*align, width, *transport, at(base, *offset)) {
                        let what = format!(
                            "{} %v{}, [%v{}, #{offset}]{}",
                            kind.mnemonic(),
                            dst.id,
                            base.id,
                            if *transport { " [transport]" } else { "" }
                        );
                        return Err(refuse(what, alignment, width));
                    }
                }
                MInst::Store {
                    align,
                    src,
                    base,
                    offset,
                    kind,
                    transport,
                    ..
                } => {
                    let width = kind.size_bytes();
                    if let Some(alignment) = scalar(*align, width, *transport, at(base, *offset)) {
                        let what = format!(
                            "{} %v{}, [%v{}, #{offset}]{}",
                            kind.mnemonic(),
                            src.id,
                            base.id,
                            if *transport { " [transport]" } else { "" }
                        );
                        return Err(refuse(what, alignment, width));
                    }
                }
                // `stack.release` reads the header below its object.
                MInst::StackRelease { address } => {
                    let alignment = at(address, -16);
                    if alignment < HEADER_WORD {
                        return Err(refuse(
                            format!("stack.release %v{}", address.id),
                            alignment,
                            HEADER_WORD,
                        ));
                    }
                }
                // A pair moves the halves of a `bits<128>` access at
                // alignment 8 or more, or two words of a variadic area, so
                // each element has its natural alignment (Section 4.1). The
                // unit of a `mem.copy` never exceeds the alignment of its
                // EvoIR `copy` or of the variadic area that it reads, and an
                // atomic access is naturally aligned by its guard (Section
                // 4.2). An `asm` template is the program's own text, which
                // the check does not read.
                _ => {}
            }
        }
    }
    Ok(())
}

/// The alignment of a scalar access that emission writes as one transfer
/// below its width, or `None` when the access is aligned.
///
/// An `[align=N]` below the width makes emission move the bytes one at a
/// time (Section 8.9). Otherwise an access that EvoIR performs is aligned by
/// its guard, natural without a suffix. A transport access has only
/// `derived`, the alignment of its base object or slot and its offset.
fn scalar(align: u32, width: u64, transport: bool, derived: u64) -> Option<u64> {
    if align != 0 && u64::from(align) < width {
        return None;
    }
    let proven = if transport {
        derived
    } else if align == 0 {
        derived.max(width)
    } else {
        derived.max(u64::from(align))
    };
    (proven < width).then_some(proven)
}
