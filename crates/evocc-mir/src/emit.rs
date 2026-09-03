//! Sections 6 and 8: the frame layout, the prologue and epilogue, and the
//! Darwin arm64 assembly text.
//!
//! Section 8: "The assembly text is the platform assembler's syntax. Functions
//! are emitted in module order, each as `.globl`/`.p2align 2`/label/body;
//! globals as data ... with their EvoIR initial bytes; symbols get the
//! platform prefix (`_` on Darwin)."

use std::collections::BTreeMap;
use std::fmt::Write as _;

use evocc_evoir::GlobalObject;

use crate::{
    linscan::physical, Allocated, LoadKind, MInst, MTerm, RegClass, StoreKind, SymbolKind, VReg,
};

/// Where every part of one frame lives, measured in bytes above `sp` after the
/// prologue has lowered it.
///
/// Section 6 lays the frame out "below the saved pair, in order: the
/// callee-saved save area, then one slot per `frame.slot` ..., then the spill
/// area", so the callee-saved words are highest and the spill area lowest.
///
/// Every offset here is taken from `sp`. Section 6 fixes the order and the
/// sizes but never says which register addresses a slot; with a frame this
/// revision guarantees is static, `sp` and `x29` differ only by a constant,
/// and `sp` keeps every offset non-negative and inside the unsigned
/// twelve-bit form. Recorded gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Total bytes the prologue subtracts, a multiple of sixteen.
    pub size: u64,
    /// Callee-saved register to its offset above `sp`.
    pub saved: BTreeMap<u8, u64>,
    /// Spill slot index to its offset above `sp`.
    pub spills: BTreeMap<u32, u64>,
    /// Each `frame.slot` instruction, in the order Section 6 lays them out, to
    /// its offset above `sp`.
    pub slots: BTreeMap<u64, u64>,
}

const fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

/// Compute the Section 6 layout of one allocated function.
#[must_use]
pub fn frame_of(allocated: &Allocated) -> Frame {
    let mut automatic: Vec<(u64, u64, u64)> = Vec::new();
    for block in &allocated.blocks {
        for inst in &block.insts {
            if let MInst::FrameSlot {
                origin,
                size,
                align,
                ..
            } = inst
            {
                automatic.push((*origin, *size, u64::from(*align)));
            }
        }
    }
    automatic.sort_unstable();
    automatic.dedup();

    let spill_bytes = u64::from(allocated.frame_slots) * 8;
    let automatic_bytes: u64 = automatic
        .iter()
        .map(|(_, size, align)| align_up(*size, *align))
        .sum();
    let saved_bytes = allocated.used_callee_saved.len() as u64 * 8;
    let size = align_up(spill_bytes + automatic_bytes + saved_bytes, 16);

    let mut spills = BTreeMap::new();
    for index in 0..allocated.frame_slots {
        spills.insert(index, u64::from(index) * 8);
    }

    // "laid out in origin order from the top of the frame downward": the
    // lowest origin takes the highest address of the automatic area.
    let mut slots = BTreeMap::new();
    let mut cursor = spill_bytes + automatic_bytes;
    for (origin, object, align) in &automatic {
        let bytes = align_up(*object, *align);
        cursor -= bytes;
        slots.insert(*origin, cursor);
    }

    let mut saved = BTreeMap::new();
    for (index, number) in allocated.used_callee_saved.iter().enumerate() {
        saved.insert(*number, spill_bytes + automatic_bytes + index as u64 * 8);
    }

    Frame {
        size,
        saved,
        spills,
        slots,
    }
}

/// The platform symbol prefix of Section 8.
const PREFIX: &str = "_";

/// Emit one module as Darwin arm64 assembly.
#[must_use]
pub fn emit_module(functions: &[Allocated], globals: &[GlobalObject]) -> String {
    let mut text = String::new();
    text.push_str(".section __TEXT,__text,regular,pure_instructions\n");
    for function in functions {
        emit_function(&mut text, function);
    }
    if !globals.is_empty() {
        text.push_str("\n.section __DATA,__data\n");
        let mut ordered: Vec<&GlobalObject> = globals.iter().collect();
        ordered.sort_by_key(|global| global.origin);
        for global in ordered {
            let _ = writeln!(
                text,
                ".p2align {}",
                global.layout.align_bytes.trailing_zeros()
            );
            let _ = writeln!(text, ".globl {PREFIX}{}", global.symbol);
            let _ = writeln!(text, "{PREFIX}{}:", global.symbol);
            for byte in &global.bytes {
                let _ = writeln!(text, "    .byte {byte}");
            }
            if global.bytes.is_empty() {
                let _ = writeln!(text, "    .space {}", global.layout.size_bytes);
            }
        }
    }
    text
}

fn label(function: &str, block: u32) -> String {
    format!("L{function}_block{block}")
}

fn emit_function(text: &mut String, allocated: &Allocated) {
    let frame = frame_of(allocated);
    let _ = writeln!(text, "\n.globl {PREFIX}{}", allocated.name);
    let _ = writeln!(text, ".p2align 2");
    let _ = writeln!(text, "{PREFIX}{}:", allocated.name);
    emit_prologue(text, &frame);

    let name = |reg: VReg| -> String {
        allocated
            .assignment
            .get(&reg.id)
            .map_or_else(|| "xzr".to_owned(), |number| physical(reg.class, *number))
    };

    let order: Vec<u32> = allocated.blocks.iter().map(|block| block.id).collect();
    for (index, block) in allocated.blocks.iter().enumerate() {
        let _ = writeln!(text, "{}:", label(&allocated.name, block.id));
        for inst in &block.insts {
            emit_inst(text, inst, &frame, &name);
        }
        let next = order.get(index + 1).copied();
        emit_term(text, &allocated.name, &block.term, &frame, next, &name);
    }
}

fn emit_prologue(text: &mut String, frame: &Frame) {
    let _ = writeln!(text, "    stp x29, x30, [sp, #-16]!");
    let _ = writeln!(text, "    mov x29, sp");
    if frame.size != 0 {
        let _ = writeln!(text, "    sub sp, sp, #{}", frame.size);
    }
    for (number, offset) in &frame.saved {
        let _ = writeln!(text, "    str x{number}, [sp, #{offset}]");
    }
}

fn emit_epilogue(text: &mut String, frame: &Frame) {
    for (number, offset) in &frame.saved {
        let _ = writeln!(text, "    ldr x{number}, [sp, #{offset}]");
    }
    if frame.size != 0 {
        let _ = writeln!(text, "    add sp, sp, #{}", frame.size);
    }
    let _ = writeln!(text, "    ldp x29, x30, [sp], #16");
}

/// The `movz`/`movk` sequence Section 4 names for `mov rd, #imm`.
fn emit_immediate(text: &mut String, register: &str, class: RegClass, value: u64) {
    let chunks = if class == RegClass::Gpr64 { 4 } else { 2 };
    let mut written = false;
    for index in 0..chunks {
        let part = (value >> (16 * index)) & 0xffff;
        if part == 0 && written {
            continue;
        }
        if written {
            let _ = writeln!(text, "    movk {register}, #{part}, lsl #{}", 16 * index);
        } else {
            if index == 0 {
                let _ = writeln!(text, "    movz {register}, #{part}");
            } else {
                let _ = writeln!(text, "    movz {register}, #{part}, lsl #{}", 16 * index);
            }
            written = true;
        }
    }
    if !written {
        let _ = writeln!(text, "    movz {register}, #0");
    }
}

fn scratch(class: RegClass) -> String {
    physical(class, 16)
}

fn load_mnemonic(kind: LoadKind) -> &'static str {
    match kind {
        LoadKind::B => "ldrb",
        LoadKind::H => "ldrh",
        LoadKind::W | LoadKind::X | LoadKind::P | LoadKind::Pf => "ldr",
        LoadKind::Sb => "ldrsb",
        LoadKind::Sh => "ldrsh",
        LoadKind::Sw => "ldrsw",
    }
}

fn store_mnemonic(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::B => "strb",
        StoreKind::H => "strh",
        StoreKind::W | StoreKind::X | StoreKind::P => "str",
    }
}

#[allow(clippy::too_many_lines)]
fn emit_inst(text: &mut String, inst: &MInst, frame: &Frame, name: &impl Fn(VReg) -> String) {
    match inst {
        MInst::Mov { dst, src } => {
            let _ = writeln!(text, "    mov {}, {}", name(*dst), name(*src));
        }
        MInst::MovImm { dst, imm } => emit_immediate(text, &name(*dst), dst.class, *imm),
        MInst::Adrp { dst, symbol, kind } => {
            let page = match kind {
                SymbolKind::Global | SymbolKind::Function => "PAGE",
            };
            let _ = writeln!(text, "    adrp {}, {PREFIX}{symbol}@{page}", name(*dst));
        }
        MInst::AddPageOff {
            dst, src, symbol, ..
        } => {
            let _ = writeln!(
                text,
                "    add {}, {}, {PREFIX}{symbol}@PAGEOFF",
                name(*dst),
                name(*src)
            );
        }
        MInst::Load {
            dst,
            base,
            offset,
            kind,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, [{}, #{offset}]",
                load_mnemonic(*kind),
                name(*dst),
                name(*base)
            );
        }
        MInst::Store {
            src,
            base,
            offset,
            kind,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, [{}, #{offset}]",
                store_mnemonic(*kind),
                name(*src),
                name(*base)
            );
        }
        MInst::Binary {
            op, dst, lhs, rhs, ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                name(*dst),
                name(*lhs),
                name(*rhs)
            );
        }
        MInst::BinaryImm {
            op, dst, lhs, imm, ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, #{imm}",
                op.mnemonic(),
                name(*dst),
                name(*lhs)
            );
        }
        MInst::Msub { dst, n, m, a, .. } => {
            let _ = writeln!(
                text,
                "    msub {}, {}, {}, {}",
                name(*dst),
                name(*n),
                name(*m),
                name(*a)
            );
        }
        MInst::FlagBinary {
            op, dst, lhs, rhs, ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                name(*dst),
                name(*lhs),
                name(*rhs)
            );
        }
        MInst::WideMultiply { op, dst, lhs, rhs } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                name(*dst),
                name(*lhs),
                name(*rhs)
            );
        }
        MInst::Extend { op, dst, src } => {
            let _ = writeln!(text, "    {} {}, {}", op.mnemonic(), name(*dst), name(*src));
        }
        MInst::Bfx {
            signed,
            dst,
            src,
            lsb,
            bits,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, #{lsb}, #{bits}",
                if *signed { "sbfx" } else { "ubfx" },
                name(*dst),
                name(*src)
            );
        }
        MInst::Cmp { lhs, rhs, .. } => {
            let _ = writeln!(text, "    cmp {}, {}", name(*lhs), name(*rhs));
        }
        MInst::CmpImm { lhs, imm, .. } => {
            let _ = writeln!(text, "    cmp {}, #{imm}", name(*lhs));
        }
        MInst::Cset { dst, cond, .. } => {
            let _ = writeln!(text, "    cset {}, {}", name(*dst), cond.mnemonic());
        }
        MInst::Csel {
            dst,
            lhs,
            rhs,
            cond,
            ..
        } => {
            let _ = writeln!(
                text,
                "    csel {}, {}, {}, {}",
                name(*dst),
                name(*lhs),
                name(*rhs),
                cond.mnemonic()
            );
        }
        MInst::Bl { symbol, .. } => {
            let _ = writeln!(text, "    bl {PREFIX}{symbol}");
        }
        MInst::Blr { target, .. } => {
            let _ = writeln!(text, "    blr {}", name(*target));
        }
        // Section 6: "one slot per `frame.slot` (its declared size, rounded to
        // its declared alignment ...)".
        MInst::FrameSlot { dst, origin, .. } => {
            let offset = frame.slots.get(origin).copied().unwrap_or(0);
            let _ = writeln!(text, "    add {}, sp, #{offset}", name(*dst));
        }
        MInst::SpillStore { src, slot } => {
            let offset = frame.spills.get(slot).copied().unwrap_or(0);
            let _ = writeln!(text, "    str {}, [sp, #{offset}]", name(*src));
        }
        MInst::SpillReload { dst, slot } => {
            let offset = frame.spills.get(slot).copied().unwrap_or(0);
            let _ = writeln!(text, "    ldr {}, [sp, #{offset}]", name(*dst));
        }
        // Section 4: emission expands the copy "to `ldr`/`str` pairs of the
        // largest aligned width". The intra-procedure scratch of Section 2 is
        // what carries each word.
        MInst::MemCopy {
            dst,
            src,
            size,
            align,
        } => {
            let unit = [8_u64, 4, 2, 1]
                .into_iter()
                .find(|unit| u64::from(*align) >= *unit && size.is_multiple_of(*unit))
                .unwrap_or(1);
            let (load, store, class) = match unit {
                8 => ("ldr", "str", RegClass::Gpr64),
                4 => ("ldr", "str", RegClass::Gpr32),
                2 => ("ldrh", "strh", RegClass::Gpr32),
                _ => ("ldrb", "strb", RegClass::Gpr32),
            };
            let temporary = scratch(class);
            let mut offset = 0;
            while offset < *size {
                let _ = writeln!(text, "    {load} {temporary}, [{}, #{offset}]", name(*src));
                let _ = writeln!(text, "    {store} {temporary}, [{}, #{offset}]", name(*dst));
                offset += unit;
            }
        }
        MInst::Ldp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
        } => {
            let bang = if *writeback { "!" } else { "" };
            let _ = writeln!(
                text,
                "    ldp {}, {}, [{}, #{offset}]{bang}",
                name(*rt1),
                name(*rt2),
                name(*base)
            );
        }
        MInst::Stp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
        } => {
            let bang = if *writeback { "!" } else { "" };
            let _ = writeln!(
                text,
                "    stp {}, {}, [{}, #{offset}]{bang}",
                name(*rt1),
                name(*rt2),
                name(*base)
            );
        }
    }
}

fn emit_term(
    text: &mut String,
    function: &str,
    term: &MTerm,
    frame: &Frame,
    next: Option<u32>,
    name: &impl Fn(VReg) -> String,
) {
    let branch = |text: &mut String, target: u32| {
        if next != Some(target) {
            let _ = writeln!(text, "    b {}", label(function, target));
        }
    };
    match term {
        MTerm::B { target, .. } => branch(text, *target),
        MTerm::BCond {
            cond,
            taken,
            untaken,
            ..
        } => {
            let _ = writeln!(
                text,
                "    b.{} {}",
                cond.mnemonic(),
                label(function, *taken)
            );
            branch(text, *untaken);
        }
        MTerm::Cb {
            nonzero,
            src,
            taken,
            untaken,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}",
                if *nonzero { "cbnz" } else { "cbz" },
                name(*src),
                label(function, *taken)
            );
            branch(text, *untaken);
        }
        MTerm::Tb {
            nonzero,
            src,
            bit,
            taken,
            untaken,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, #{bit}, {}",
                if *nonzero { "tbnz" } else { "tbz" },
                name(*src),
                label(function, *taken)
            );
            branch(text, *untaken);
        }
        MTerm::Ret { .. } => {
            emit_epilogue(text, frame);
            let _ = writeln!(text, "    ret");
        }
        MTerm::Brk { imm } => {
            let _ = writeln!(text, "    brk #{imm}");
        }
    }
}
