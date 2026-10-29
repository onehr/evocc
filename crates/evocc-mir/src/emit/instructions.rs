//! Assembly emission for instruction families.

use super::*;
use std::fmt::Write as _;

#[allow(clippy::too_many_lines)]
pub(super) fn emit_inst(
    text: &mut String,
    symbols: &SymbolEmission<'_>,
    inst: &MInst,
    frame: &Frame,
    spell: &impl Fn(VReg, RegClass) -> String,
    function: &str,
    copies: &mut u32,
) {
    let target = symbols.target;
    let name = |reg: VReg| spell(reg, reg.class);
    // Every operand of a two-width form takes the form's own width.
    let at = |reg: VReg, width: Width| spell(reg, width.class());
    let vector_name = |reg: VReg| format!("{}{}", regfile::SIMD_ASM_VIEWS[0], &name(reg)[1..]);
    match inst {
        MInst::Floating { .. } | MInst::Aes { .. } | MInst::Vector { .. } => emit_simd(
            text,
            inst,
            function,
            copies,
            target,
            &name,
            &at,
            &vector_name,
        ),
        MInst::Mov { .. } | MInst::MovImm { .. } => {
            emit_moves(text, inst, frame, &name, &at, &vector_name)
        }
        MInst::Adr { .. } | MInst::Adrp { .. } | MInst::AddPageOff { .. } => {
            emit_addresses(text, symbols, inst, function, target, &name)
        }
        MInst::AtomicRmw { .. } | MInst::Atomic { .. } => {
            emit_atomics(text, inst, function, copies, target, &name, &at)
        }
        MInst::Load { .. } | MInst::Store { .. } => {
            emit_memory(text, inst, frame, spell, &name, &vector_name)
        }
        MInst::Binary { .. }
        | MInst::BinaryImm { .. }
        | MInst::Msub { .. }
        | MInst::Madd { .. }
        | MInst::CarryBinary { .. }
        | MInst::AddCarryFlags { .. }
        | MInst::SubCarryFlags { .. }
        | MInst::TstImm { .. } => emit_arithmetic(text, inst, &at),
        MInst::FlagBinary { .. }
        | MInst::WideMultiply { .. }
        | MInst::Extend { .. }
        | MInst::Unary { .. }
        | MInst::Bfx { .. } => emit_integer_operations(text, inst, spell, &name, &at),
        MInst::Cmp { .. } | MInst::CmpImm { .. } | MInst::Cset { .. } | MInst::Csel { .. } => {
            emit_conditions(text, inst, &name, &at)
        }
        MInst::Bl { .. } | MInst::Blr { .. } => emit_calls(text, inst, target, &name),
        MInst::FrameAddress { .. }
        | MInst::VaSaveArea { .. }
        | MInst::IndirectResult
        | MInst::FpEnvCheck { .. }
        | MInst::FrameSlot { .. } => {
            emit_frame_addresses(text, inst, frame, function, copies, target, &name)
        }
        MInst::StackAlloc { .. } | MInst::StackRelease { .. } => {
            emit_stack(text, inst, frame, function, copies, target, &name)
        }
        MInst::SpillStore { .. } | MInst::SpillReload { .. } => {
            emit_spills(text, inst, frame, &name)
        }
        MInst::MemCopy { .. } => emit_copy(text, inst, spell, function, copies),
        MInst::IncomingArg { .. } | MInst::OutgoingArg { .. } | MInst::Asm(..) => {
            emit_boundary_and_assembly(text, inst, spell, function, target, &name)
        }
        MInst::Ldp { .. } | MInst::Stp { .. } => emit_paired_memory(text, inst, &name),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_simd(
    text: &mut String,
    inst: &MInst,
    function: &str,
    copies: &mut u32,
    target: Target,
    name: &impl Fn(VReg) -> String,
    at: &impl Fn(VReg, Width) -> String,
    vector_name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::Floating { .. } => {
            let label = format!(
                "{}fp_{}_{}",
                if target == Target::Linux { ".L" } else { "L" },
                function,
                *copies
            );
            *copies += 1;
            crate::floating::emit(text, inst, &name, &label);
        }
        MInst::Aes { dst, op, args } => {
            if target == Target::Linux {
                let _ = writeln!(text, "    .arch_extension crypto");
            }
            if op.arity() == 2 {
                // AESE/AESD overwrite their first operand. Do not overwrite
                // a key coallocated with dst, nor a still-live source value.
                // v30 is the existing reserved SIMD scratch register.
                let _ = writeln!(
                    text,
                    "    mov {SIMD_SCRATCH_V}.16b, {}.16b\n    {} {SIMD_SCRATCH_V}.16b, {}.16b\n    mov {}.16b, {SIMD_SCRATCH_V}.16b",
                    vector_name(args[0]),
                    op.mnemonic(),
                    vector_name(args[1]),
                    vector_name(*dst)
                );
            } else {
                let _ = writeln!(
                    text,
                    "    {} {}.16b, {}.16b",
                    op.mnemonic(),
                    vector_name(*dst),
                    vector_name(args[0])
                );
            }
        }
        MInst::Vector {
            dst,
            extract,
            lane_bits,
            bytes,
            args,
        } => {
            let suffix = match lane_bits {
                8 => "b",
                16 => "h",
                32 => "s",
                64 => "d",
                _ => unreachable!("validated vector lane"),
            };
            if *extract {
                let source = args[0];
                let lanes = u16::from(*bytes) * 8 / lane_bits;
                let label = format!(
                    "{}vector_lane_{}_{}",
                    if target == Target::Linux { ".L" } else { "L" },
                    function,
                    *copies
                );
                *copies += 1;
                let _ = writeln!(
                    text,
                    "    cmp {}, #{lanes}\n    b.lo {label}\n    brk #3011\n{label}:",
                    at(args[1], Width::for_class(args[1].class).unwrap())
                );
                if *bytes <= 4 {
                    let shift = lane_bits.trailing_zeros();
                    let _ = writeln!(text, "    lsl {SCRATCH_W}, {}, #{shift}\n    lsr {SCRATCH_W}, {}, {SCRATCH_W}\n    ubfx {}, {SCRATCH_W}, #0, #{lane_bits}", at(args[1], Width::W), at(source, Width::W), at(*dst, Width::W));
                    return;
                }
                // TBL selects bytes without changing their order. v30/v31
                // and x16/x17 are reserved from allocation.
                let shift = (lane_bits / 8).trailing_zeros();
                let _ = writeln!(
                    text,
                    "    lsl {SCRATCH_W}, {}, #{shift}\n    dup {SIMD_SCRATCH_UNIT_V}.16b, {SCRATCH_W}",
                    at(args[1], Width::W)
                );
                emit_immediate(text, SCRATCH_UNIT_X, RegClass::Gpr64, 0x0706_0504_0302_0100);
                let _ = writeln!(text, "    fmov {SIMD_SCRATCH_D}, {SCRATCH_UNIT_X}");
                emit_immediate(text, SCRATCH_UNIT_X, RegClass::Gpr64, 0x0f0e_0d0c_0b0a_0908);
                let _ = writeln!(text, "    ins {SIMD_SCRATCH_V}.d[1], {SCRATCH_UNIT_X}\n    add {SIMD_SCRATCH_UNIT_V}.16b, {SIMD_SCRATCH_UNIT_V}.16b, {SIMD_SCRATCH_V}.16b\n    tbl {SIMD_SCRATCH_V}.16b, {{{}.16b}}, {SIMD_SCRATCH_UNIT_V}.16b\n    umov {}, {SIMD_SCRATCH_V}.{suffix}[0]", vector_name(source), at(*dst, if *lane_bits == 64 { Width::X } else { Width::W }));
            } else {
                if *bytes <= 4 {
                    let _ = writeln!(text, "    mov {SCRATCH_W}, {ZERO_GPR32}");
                    for (index, source) in args.iter().enumerate() {
                        let offset = index * usize::from(*lane_bits);
                        let _ = writeln!(
                            text,
                            "    bfi {SCRATCH_W}, {}, #{offset}, #{lane_bits}",
                            at(*source, Width::W)
                        );
                    }
                    let _ = writeln!(text, "    mov {}, {SCRATCH_W}", at(*dst, Width::W));
                    return;
                }
                let vector = vector_name(*dst);
                let _ = writeln!(
                    text,
                    "    movi {vector}.{}b, #0",
                    if dst.class == RegClass::Simd64 { 8 } else { 16 }
                );
                for (index, source) in args.iter().enumerate() {
                    let _ = writeln!(
                        text,
                        "    ins {vector}.{suffix}[{index}], {}",
                        at(*source, if *lane_bits == 64 { Width::X } else { Width::W })
                    );
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_moves(
    text: &mut String,
    inst: &MInst,
    frame: &Frame,
    name: &impl Fn(VReg) -> String,
    at: &impl Fn(VReg, Width) -> String,
    vector_name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::Mov { dst, src } => {
            // The two classes are one register file at two widths, so a move
            // between them is the thirty-two-bit form: it truncates when it
            // reads the low half and zero-extends when it writes it.
            if dst.class.is_simd() && src.class.is_simd() {
                let width = if dst.class == RegClass::Simd128 {
                    16
                } else {
                    8
                };
                let _ = writeln!(
                    text,
                    "    mov {}.{width}b, {}.{width}b",
                    vector_name(*dst),
                    vector_name(*src)
                );
            } else if dst.class == src.class {
                let _ = writeln!(text, "    mov {}, {}", name(*dst), name(*src));
            } else {
                let _ = writeln!(
                    text,
                    "    mov {}, {}",
                    at(*dst, Width::W),
                    at(*src, Width::W)
                );
            }
        }
        MInst::MovImm { dst, imm } => {
            if dst.class.is_simd() {
                let _ = writeln!(
                    text,
                    "    movi {}.{}b, #{}",
                    vector_name(*dst),
                    if dst.class == RegClass::Simd128 {
                        16
                    } else {
                        8
                    },
                    imm
                );
            } else {
                if frame.compact_constants {
                    let width = if dst.class == RegClass::Gpr64 {
                        Width::X
                    } else {
                        Width::W
                    };
                    if let Some(parts) = crate::isa::aarch64::constants::select(*imm, width) {
                        use crate::isa::aarch64::constants::Part;
                        let register = name(*dst);
                        for part in parts {
                            let (op, half, shift) = match part {
                                Part::Zero { half, shift } => ("movz", half, shift),
                                Part::Ones { half, shift } => ("movn", half, shift),
                                Part::Keep { half, shift } => ("movk", half, shift),
                                Part::Logical(value) => {
                                    let zero = if width == Width::X {
                                        ZERO_GPR64
                                    } else {
                                        ZERO_GPR32
                                    };
                                    let _ =
                                        writeln!(text, "    orr {register}, {zero}, #{value:#x}");
                                    continue;
                                }
                            };
                            if shift == 0 {
                                let _ = writeln!(text, "    {op} {register}, #{half}");
                            } else {
                                let _ =
                                    writeln!(text, "    {op} {register}, #{half}, lsl #{shift}");
                            }
                        }
                        return;
                    }
                }
                emit_immediate(text, &name(*dst), dst.class, *imm);
            }
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_addresses(
    text: &mut String,
    symbols: &SymbolEmission<'_>,
    inst: &MInst,
    function: &str,
    target: Target,
    name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::Adr { dst, block } => {
            let symbol = block_symbol(target, function, *block);
            let register = name(*dst);
            if symbols.long_block_addresses {
                let (page, offset) = match target {
                    Target::Linux => (symbol.clone(), format!(":lo12:{symbol}")),
                    Target::Darwin => (format!("{symbol}@PAGE"), format!("{symbol}@PAGEOFF")),
                };
                let _ = writeln!(text, "    adrp {register}, {page}");
                let _ = writeln!(text, "    add {register}, {register}, {offset}");
            } else {
                let _ = writeln!(text, "    adr {register}, {symbol}");
            }
        }
        MInst::Adrp { dst, symbol, kind } => {
            if symbols.tiny_addresses {
                if symbols
                    .indirect_addresses
                    .contains(&(*kind, symbol.as_str()))
                {
                    let _ = writeln!(text, "    ldr {}, :got:{symbol}", name(*dst));
                } else {
                    let _ = writeln!(
                        text,
                        "    adr {}, {}{symbol}",
                        name(*dst),
                        target.symbol_prefix()
                    );
                }
                return;
            }
            if symbols
                .indirect_addresses
                .contains(&(*kind, symbol.as_str()))
            {
                let operand = match target {
                    Target::Linux => format!(":got:{symbol}"),
                    Target::Darwin => format!("_{symbol}@GOTPAGE"),
                };
                let _ = writeln!(text, "    adrp {}, {operand}", name(*dst));
                return;
            }
            let page = match kind {
                SymbolKind::Global | SymbolKind::Function => "PAGE",
            };
            let _ = writeln!(
                text,
                "    adrp {}, {}",
                name(*dst),
                target.adrp_operand(symbol, page)
            );
        }
        MInst::AddPageOff {
            dst,
            src,
            symbol,
            kind,
        } => {
            if symbols.tiny_addresses {
                let _ = writeln!(text, "    mov {}, {}", name(*dst), name(*src));
                return;
            }
            if symbols
                .indirect_addresses
                .contains(&(*kind, symbol.as_str()))
            {
                let operand = match target {
                    Target::Linux => format!("#:got_lo12:{symbol}"),
                    Target::Darwin => format!("_{symbol}@GOTPAGEOFF"),
                };
                let _ = writeln!(text, "    ldr {}, [{}, {operand}]", name(*dst), name(*src));
                return;
            }
            let _ = writeln!(
                text,
                "    add {}, {}, {}",
                name(*dst),
                name(*src),
                target.pageoff_operand(symbol)
            );
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_atomics(
    text: &mut String,
    inst: &MInst,
    function: &str,
    copies: &mut u32,
    target: Target,
    name: &impl Fn(VReg) -> String,
    at: &impl Fn(VReg, Width) -> String,
) {
    match inst {
        MInst::AtomicRmw {
            dst,
            base,
            value,
            expected,
            operation,
            kind,
            order,
            ..
        } => {
            use evocc_memory::{AtomicOrder as O, AtomicRmwOp as R};
            let size = kind.size_bytes();
            let width = if size == 8 { Width::X } else { Width::W };
            let old = at(*dst, width);
            let operand = at(*value, width);
            let temporary = if size == 8 { SCRATCH_X } else { SCRATCH_W };
            let suffix = match size {
                1 => "b",
                2 => "h",
                _ => "",
            };
            let acquire = matches!(order, O::Consume | O::Acquire | O::AcqRel | O::SeqCst);
            let release = matches!(order, O::Release | O::AcqRel | O::SeqCst);
            let load = if acquire { "ldaxr" } else { "ldxr" };
            let store = if release { "stlxr" } else { "stxr" };
            let label = format!("{}{function}_atomic{}", target.label_prefix(), *copies);
            *copies += 1;
            if *order == O::SeqCst {
                text.push_str("    dmb ish\n");
            }
            let _ = writeln!(
                text,
                "{label}:\n    {load}{suffix} {old}, [{}]",
                name(*base)
            );
            if let Some(expected) = expected {
                if size < 4 {
                    let _ = writeln!(
                        text,
                        "    ubfx {temporary}, {}, #0, #{}",
                        at(*expected, width),
                        size * 8
                    );
                    let _ = writeln!(text, "    eor {temporary}, {old}, {temporary}");
                } else {
                    let _ = writeln!(text, "    eor {temporary}, {old}, {}", at(*expected, width));
                }
                let _ = writeln!(text, "    cbnz {temporary}, {label}_failed");
            }
            match operation {
                R::Exchange | R::CompareExchange => {
                    let _ = writeln!(text, "    mov {temporary}, {operand}");
                }
                operation => {
                    let instruction = match operation {
                        R::Add => "add",
                        R::Sub => "sub",
                        R::And | R::Nand => "and",
                        R::Or => "orr",
                        R::Xor => "eor",
                        _ => unreachable!(),
                    };
                    let _ = writeln!(text, "    {instruction} {temporary}, {old}, {operand}");
                    if *operation == R::Nand {
                        let _ = writeln!(text, "    mvn {temporary}, {temporary}");
                    }
                }
            }
            let _ = writeln!(
                text,
                "    {store}{suffix} {SCRATCH_UNIT_W}, {temporary}, [{}]\n    cbnz {SCRATCH_UNIT_W}, {label}",
                name(*base)
            );
            if expected.is_some() {
                let _ = writeln!(
                    text,
                    "    b {label}_done\n{label}_failed:\n    clrex\n{label}_done:"
                );
            }
            if *order == O::SeqCst {
                text.push_str("    dmb ish\n");
            }
        }
        MInst::Atomic {
            load,
            value,
            base,
            kind,
            order,
            ..
        } => {
            let mnemonic = match (
                *load,
                *order == evocc_memory::AtomicOrder::Relaxed,
                kind.size_bytes(),
            ) {
                (true, true, 1) => "ldrb",
                (true, true, 2) => "ldrh",
                (true, true, _) => "ldr",
                (true, false, 1) => "ldarb",
                (true, false, 2) => "ldarh",
                (true, false, _) => "ldar",
                (false, true, 1) => "strb",
                (false, true, 2) => "strh",
                (false, true, _) => "str",
                (false, false, 1) => "stlrb",
                (false, false, 2) => "stlrh",
                (false, false, _) => "stlr",
            };
            let _ = writeln!(text, "    {mnemonic} {}, [{}]", name(*value), name(*base));
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_memory(
    text: &mut String,
    inst: &MInst,
    frame: &Frame,
    spell: &impl Fn(VReg, RegClass) -> String,
    name: &impl Fn(VReg) -> String,
    vector_name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::Load {
            dst,
            base,
            offset,
            kind,
            align,
            ..
        } => {
            if frame.strict_alignment && *align != 0 && u64::from(*align) < kind.size_bytes() {
                if dst.class.is_simd() {
                    let vector = vector_name(*dst);
                    let _ = writeln!(text, "    movi {vector}.16b, #0");
                    for byte in 0..kind.size_bytes() {
                        let _ = writeln!(
                            text,
                            "    ldrb {SCRATCH_W}, [{}, #{}]\n    ins {vector}.b[{byte}], {SCRATCH_W}",
                            name(*base),
                            offset + byte as i64
                        );
                    }
                    return;
                }
                let _ = writeln!(text, "    mov {SCRATCH_X}, #0");
                for byte in 0..kind.size_bytes() {
                    let _ = writeln!(
                        text,
                        "    ldrb {SCRATCH_UNIT_W}, [{}, #{}]\n    orr {SCRATCH_X}, {SCRATCH_X}, {SCRATCH_UNIT_X}, lsl #{}",
                        name(*base),
                        offset + byte as i64,
                        byte * 8
                    );
                }
                if *kind == LoadKind::Sw {
                    let _ = writeln!(text, "    sxtw {}, {SCRATCH_W}", name(*dst));
                } else {
                    let _ = writeln!(
                        text,
                        "    mov {}, {}",
                        spell(*dst, load_class(*kind, dst.class)),
                        if load_class(*kind, dst.class) == RegClass::Gpr32 {
                            SCRATCH_W
                        } else {
                            SCRATCH_X
                        }
                    );
                }
                return;
            }
            let _ = writeln!(
                text,
                "    {} {}, [{}, #{offset}]",
                load_mnemonic(*kind),
                half_view(
                    *kind == LoadKind::Hv,
                    spell(*dst, load_class(*kind, dst.class))
                ),
                spell(*base, RegClass::Gpr64)
            );
        }
        MInst::Store {
            src,
            base,
            offset,
            kind,
            align,
            ..
        } => {
            if frame.strict_alignment && *align != 0 && u64::from(*align) < kind.size_bytes() {
                if src.class.is_simd() {
                    let vector = vector_name(*src);
                    for byte in 0..kind.size_bytes() {
                        let _ = writeln!(
                            text,
                            "    umov {SCRATCH_W}, {vector}.b[{byte}]\n    strb {SCRATCH_W}, [{}, #{}]",
                            name(*base),
                            offset + byte as i64
                        );
                    }
                    return;
                }
                for byte in 0..kind.size_bytes() {
                    let _ = writeln!(
                        text,
                        "    lsr {SCRATCH_X}, {}, #{}\n    strb {SCRATCH_W}, [{}, #{}]",
                        spell(*src, RegClass::Gpr64),
                        byte * 8,
                        name(*base),
                        offset + byte as i64
                    );
                }
                return;
            }
            let _ = writeln!(
                text,
                "    {} {}, [{}, #{offset}]",
                store_mnemonic(*kind),
                half_view(*kind == StoreKind::Hv, spell(*src, store_class(*kind))),
                spell(*base, RegClass::Gpr64)
            );
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_arithmetic(
    text: &mut String,
    inst: &MInst,
    at: &impl Fn(VReg, Width) -> String,
) {
    match inst {
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, #{imm}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width)
            );
        }
        MInst::Msub {
            dst,
            n,
            m,
            a,
            width,
        } => {
            let _ = writeln!(
                text,
                "    msub {}, {}, {}, {}",
                at(*dst, *width),
                at(*n, *width),
                at(*m, *width),
                at(*a, *width)
            );
        }
        MInst::Madd {
            dst,
            n,
            m,
            a,
            width,
        } => {
            let _ = writeln!(
                text,
                "    madd {}, {}, {}, {}",
                at(*dst, *width),
                at(*n, *width),
                at(*m, *width),
                at(*a, *width)
            );
        }
        MInst::CarryBinary {
            op,
            dst,
            lhs,
            rhs,
            width,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        // The final carry chain keeps flags, not its arithmetic result.
        MInst::AddCarryFlags {
            lhs, rhs, width, ..
        }
        | MInst::SubCarryFlags {
            lhs, rhs, width, ..
        } => {
            let zero = if matches!(width, Width::X) {
                ZERO_GPR64
            } else {
                ZERO_GPR32
            };
            let _ = writeln!(
                text,
                "    {} {zero}, {}, {}",
                if matches!(inst, MInst::AddCarryFlags { .. }) {
                    "adcs"
                } else {
                    "sbcs"
                },
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        MInst::TstImm {
            lhs, imm, width, ..
        } => {
            let _ = writeln!(text, "    tst {}, #{imm}", at(*lhs, *width));
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_integer_operations(
    text: &mut String,
    inst: &MInst,
    spell: &impl Fn(VReg, RegClass) -> String,
    name: &impl Fn(VReg) -> String,
    at: &impl Fn(VReg, Width) -> String,
) {
    match inst {
        MInst::FlagBinary {
            op,
            dst,
            lhs,
            rhs,
            width,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        // Section 4 fixes both widths of each wide multiply: `smull xd, wn,
        // wm` and `smulh xd, xn, xm`.
        MInst::WideMultiply { op, dst, lhs, rhs } => {
            let operands = match op {
                WideMul::Smull => RegClass::Gpr32,
                WideMul::Smulh | WideMul::Umulh => RegClass::Gpr64,
            };
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                spell(*dst, RegClass::Gpr64),
                spell(*lhs, operands),
                spell(*rhs, operands)
            );
        }
        // `sxtb`/`sxth`/`uxtb`/`uxth`/`sxtw` all read a thirty-two-bit source.
        MInst::Extend { op, dst, src } => {
            let _ = writeln!(
                text,
                "    {} {}, {}",
                op.mnemonic(),
                name(*dst),
                spell(*src, RegClass::Gpr32)
            );
        }
        MInst::Unary {
            op,
            dst,
            src,
            width,
        } => {
            let operands = width.class();
            let _ = writeln!(
                text,
                "    {} {}, {}",
                op.mnemonic(),
                spell(*dst, operands),
                spell(*src, operands)
            );
        }
        MInst::Bfx {
            signed,
            dst,
            src,
            lsb,
            bits,
            width,
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, #{lsb}, #{bits}",
                if *signed { "sbfx" } else { "ubfx" },
                at(*dst, *width),
                at(*src, *width)
            );
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_conditions(
    text: &mut String,
    inst: &MInst,
    name: &impl Fn(VReg) -> String,
    at: &impl Fn(VReg, Width) -> String,
) {
    match inst {
        MInst::Cmp {
            lhs, rhs, width, ..
        } => {
            let _ = writeln!(text, "    cmp {}, {}", at(*lhs, *width), at(*rhs, *width));
        }
        MInst::CmpImm {
            lhs, imm, width, ..
        } => {
            let _ = writeln!(text, "    cmp {}, #{imm}", at(*lhs, *width));
        }
        MInst::Cset { dst, cond, .. } => {
            let _ = writeln!(text, "    cset {}, {}", name(*dst), cond.mnemonic());
        }
        MInst::Csel {
            dst,
            lhs,
            rhs,
            cond,
            width,
            ..
        } => {
            let _ = writeln!(
                text,
                "    csel {}, {}, {}, {}",
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width),
                cond.mnemonic()
            );
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_calls(
    text: &mut String,
    inst: &MInst,
    target: Target,
    name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::Bl { symbol, .. } => {
            let _ = writeln!(text, "    bl {}{symbol}", target.symbol_prefix());
        }
        MInst::Blr { target, .. } => {
            let _ = writeln!(text, "    blr {}", name(*target));
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_frame_addresses(
    text: &mut String,
    inst: &MInst,
    frame: &Frame,
    function: &str,
    copies: &mut u32,
    target: Target,
    name: &impl Fn(VReg) -> String,
) {
    match inst {
        // Section 6's variadic places: `stack` is "`x29 + 16` (the first
        // incoming stack slot)" and `gr_top` is "save area + 64".
        MInst::FrameAddress { dst, place } => match place {
            crate::FramePlace::IncomingStack => {
                let offset = target.abi().facts.incoming_argument_offset(0);
                emit_add_immediate(text, "add", &name(*dst), FRAME_POINTER_X, offset);
            }
            crate::FramePlace::RegisterSaveTop => {
                let top = frame
                    .register_save
                    .map_or(0, |(_, base)| base + target.abi().general_save_area_bytes());
                emit_frame_address(text, &name(*dst), top, frame);
            }
            // Section 5's activation-address rows (revision 0.17). The
            // prologue's `stp x29, x30, [sp, #-16]!` puts the saved link
            // register at `x29 + 8`, and the frame pointer is `x29` itself.
            crate::FramePlace::SavedLinkRegister => {
                if frame.sign_returns {
                    let _ = writeln!(text, "    mov {SCRATCH_UNIT_X}, {LINK_REGISTER_X}\n    ldr {LINK_REGISTER_X}, [{FRAME_POINTER_X}, #8]\n    xpaclri\n    mov {}, {LINK_REGISTER_X}\n    mov {LINK_REGISTER_X}, {SCRATCH_UNIT_X}", name(*dst));
                } else {
                    let _ = writeln!(text, "    ldr {}, [{FRAME_POINTER_X}, #8]", name(*dst));
                }
            }
            crate::FramePlace::FramePointer => {
                let _ = writeln!(text, "    mov {}, {FRAME_POINTER_X}", name(*dst));
            }
            crate::FramePlace::AncestorFrame(depth) => {
                let destination = name(*dst);
                let _ = writeln!(text, "    mov {destination}, {FRAME_POINTER_X}");
                if depth.get() <= 2 {
                    for _ in 0..depth.get() {
                        let _ = writeln!(text, "    ldr {destination}, [{destination}]");
                    }
                } else {
                    // x16 is reserved from allocation. A non-flag-setting sub
                    // and cbnz preserve live NZCV across this pseudo-form.
                    let label = format!("{}{function}_frame{}", target.label_prefix(), *copies);
                    *copies += 1;
                    emit_immediate(text, SCRATCH_X, RegClass::Gpr64, u64::from(depth.get()));
                    let _ = writeln!(text, "{label}:\n    ldr {destination}, [{destination}]\n    sub {SCRATCH_X}, {SCRATCH_X}, #1\n    cbnz {SCRATCH_X}, {label}");
                }
            }
        },
        // The area itself is reserved by the frame layout and filled by the
        // prologue, so the marker emits nothing of its own.
        MInst::VaSaveArea { .. } => {}
        // Section 6 (revision 0.18): allocation has already pinned the
        // register the marker names to `x8`, so emission writes nothing.
        MInst::IndirectResult => {}
        // emit_blocks places this check unless every incoming path already
        // checked FPCR without intervening opaque execution.
        MInst::FpEnvCheck { .. } => {}
        // Section 6: "one slot per `frame.slot` (its declared size, rounded to
        // its declared alignment ...)".
        MInst::FrameSlot { dst, origin, .. } => {
            let offset = frame.slots.get(origin).copied().unwrap_or(0);
            emit_frame_address(text, &name(*dst), offset, frame);
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_stack(
    text: &mut String,
    inst: &MInst,
    frame: &Frame,
    function: &str,
    copies: &mut u32,
    target: Target,
    name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::StackAlloc {
            dst, size, align, ..
        } => {
            // Keep a fresh outgoing-argument area below each allocation. It
            // must never overlap either the new object or a previous one.
            let _ = writeln!(text, "    mov {SCRATCH_UNIT_X}, {STACK_POINTER}");
            let _ = writeln!(
                text,
                "    sub {SCRATCH_X}, {STACK_POINTER}, {}",
                name(*size)
            );
            let _ = writeln!(
                text,
                "    and {SCRATCH_X}, {SCRATCH_X}, #-{}",
                (*align).max(16)
            );
            let _ = writeln!(text, "    stur {SCRATCH_UNIT_X}, [{SCRATCH_X}, #-16]");
            let _ = writeln!(text, "    mov {}, {SCRATCH_X}", name(*dst));
            emit_add_immediate(
                text,
                "sub",
                STACK_POINTER,
                SCRATCH_X,
                align_up(frame.outgoing, 16) + 16,
            );
        }
        MInst::StackRelease { address } => {
            let done = format!("{}{function}_release{}", target.label_prefix(), *copies);
            *copies += 1;
            // Non-flag-setting arithmetic preserves a live NZCV value.
            let _ = writeln!(
                text,
                "    mov {SCRATCH_X}, {STACK_POINTER}\n    sub {SCRATCH_X}, {}, {SCRATCH_X}",
                name(*address)
            );
            emit_add_immediate(
                text,
                "sub",
                SCRATCH_X,
                SCRATCH_X,
                align_up(frame.outgoing, 16) + 16,
            );
            let _ = writeln!(
                text,
                "    cbnz {SCRATCH_X}, {done}\n    ldur {SCRATCH_X}, [{}, #-16]\n    mov {STACK_POINTER}, {SCRATCH_X}\n{done}:",
                name(*address)
            );
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_spills(
    text: &mut String,
    inst: &MInst,
    frame: &Frame,
    name: &impl Fn(VReg) -> String,
) {
    match inst {
        MInst::SpillStore { src, slot } => {
            let offset = frame.spills.get(slot).copied().unwrap_or(0);
            emit_frame_access(text, "str", &name(*src), offset, frame);
        }
        MInst::SpillReload { dst, slot } => {
            let offset = frame.spills.get(slot).copied().unwrap_or(0);
            emit_frame_access(text, "ldr", &name(*dst), offset, frame);
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_copy(
    text: &mut String,
    inst: &MInst,
    spell: &impl Fn(VReg, RegClass) -> String,
    function: &str,
    copies: &mut u32,
) {
    match inst {
        // Section 4: emission expands the copy "to `ldr`/`str` pairs of the
        // largest aligned width, unrolled up to 64 bytes and a counted loop
        // above". The two intra-procedure scratch registers of Section 2 are
        // what the expansion may use: one carries each unit, and above the
        // ceiling the other is the counter.
        MInst::MemCopy {
            dst,
            src,
            size,
            align,
            ..
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
            let source = spell(*src, RegClass::Gpr64);
            let destination = spell(*dst, RegClass::Gpr64);
            if *size <= UNROLL_CEILING {
                let temporary = scratch(class);
                let mut offset = 0;
                while offset < *size {
                    let _ = writeln!(text, "    {load} {temporary}, [{source}, #{offset}]");
                    let _ = writeln!(text, "    {store} {temporary}, [{destination}, #{offset}]");
                    offset += unit;
                }
            } else {
                // The counter runs down from `size` to zero in units, so the
                // loop needs no comparison and leaves `nzcv` alone; the offset
                // register is the second scratch, and the two addresses are
                // read but never written, since the allocator may still have
                // uses for them after the copy.
                let counter = physical(RegClass::Gpr64, SCRATCH_OFFSET);
                let temporary = physical(class, SCRATCH_UNIT);
                let body = format!("L{function}_copy{copies}");
                *copies += 1;
                emit_immediate(text, &counter, RegClass::Gpr64, *size);
                let _ = writeln!(text, "{body}:");
                let _ = writeln!(text, "    sub {counter}, {counter}, #{unit}");
                let _ = writeln!(text, "    {load} {temporary}, [{source}, {counter}]");
                let _ = writeln!(text, "    {store} {temporary}, [{destination}, {counter}]");
                let _ = writeln!(text, "    cbnz {counter}, {body}");
            }
        }
        _ => unreachable!(),
    }
}

pub(super) fn emit_boundary_and_assembly(
    text: &mut String,
    inst: &MInst,
    spell: &impl Fn(VReg, RegClass) -> String,
    function: &str,
    target: Target,
    name: &impl Fn(VReg) -> String,
) {
    match inst {
        // Section 6: "the callee reads slot `i` at `x29 + 16 + 8 * i`, since
        // the prologue saves the pair first", and the caller writes its own
        // at increasing addresses from the `sp` it calls with.
        MInst::IncomingArg { dst, slot } => {
            let offset = target.abi().facts.incoming_argument_offset(*slot);
            let _ = writeln!(
                text,
                "    ldr {}, [{FRAME_POINTER_X}, #{offset}]",
                name(*dst)
            );
        }
        MInst::OutgoingArg { src, slot } => {
            let offset = target.abi().facts.outgoing_argument_offset(*slot);
            let _ = writeln!(text, "    str {}, [{STACK_POINTER}, #{offset}]", name(*src));
        }
        MInst::Asm(body) => emit_assembly(text, target, body, &[], function, spell),
        _ => unreachable!(),
    }
}

pub(super) fn emit_paired_memory(text: &mut String, inst: &MInst, name: &impl Fn(VReg) -> String) {
    match inst {
        MInst::Ldp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
            ..
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
            ..
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
        _ => unreachable!(),
    }
}
