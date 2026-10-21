//! Scalar FP shares the SIMD allocation bank; no host arithmetic is used.
use crate::isa::aarch64::regfile::{
    SCRATCH_UNIT_W, SCRATCH_UNIT_X, SCRATCH_W, SCRATCH_X, SIMD_SCRATCH_UNIT_D, SIMD_SCRATCH_UNIT_S,
    ZERO_GPR64,
};
use crate::{MBlock, MInst, MTerm, RegClass, VReg};
use evocc_evoir::{EvoIRType, FloatingInst, FloatingOp, Value};
use evocc_types::{floating::FloatBits, FloatKind};
use std::collections::BTreeMap;
use std::fmt::Write;
pub(crate) mod helpers;

pub(crate) fn hosted_conversion(symbol: &str) -> Option<(bool, bool, FloatKind)> {
    Some(match symbol {
        "__floattisf" => (true, true, FloatKind::F32),
        "__floattidf" => (true, true, FloatKind::F64),
        "__floatuntisf" => (true, false, FloatKind::F32),
        "__floatuntidf" => (true, false, FloatKind::F64),
        "__fixsfti" => (false, true, FloatKind::F32),
        "__fixdfti" => (false, true, FloatKind::F64),
        "__fixunssfti" => (false, false, FloatKind::F32),
        "__fixunsdfti" => (false, false, FloatKind::F64),
        _ => return None,
    })
}

fn needs_environment(inst: &MInst) -> u8 {
    match inst {
        MInst::Bl {
            fp_mode: Some(crate::FpCallMode::Strict),
            ..
        } => 1,
        MInst::Bl {
            fp_mode: Some(crate::FpCallMode::Fixed),
            ..
        } => 2,
        MInst::FpEnvCheck { strict } => {
            if *strict {
                1
            } else {
                2
            }
        }
        MInst::Floating { op, strict, .. }
            if !matches!(
                op,
                FloatingOp::Neg
                    | FloatingOp::ToBits
                    | FloatingOp::FromBits
                    | FloatingOp::ToIntDefined { .. }
            ) =>
        {
            if *strict {
                1
            } else {
                2
            }
        }
        _ => 0,
    }
}

fn transfers_environment(inst: &MInst, admitted: u8) -> u8 {
    if matches!(
        inst,
        MInst::Bl { .. } | MInst::Blr { .. } | MInst::Asm { .. }
    ) {
        0
    } else {
        admitted.max(needs_environment(inst))
    }
}

/// Must-analysis over the complete machine CFG. A successful check remains
/// valid only until opaque execution; entry and unreachable blocks never
/// inherit assumptions about the caller's FPCR. No check writes FPCR.
pub(crate) fn guard_points(blocks: &[MBlock]) -> BTreeMap<(u32, usize), bool> {
    let mut guards = BTreeMap::new();
    if !blocks
        .iter()
        .any(|block| block.insts.iter().any(|i| needs_environment(i) != 0))
    {
        return guards;
    }
    let indices: BTreeMap<_, _> = blocks
        .iter()
        .enumerate()
        .map(|(i, block)| (block.id, i))
        .collect();
    let mut predecessors = vec![Vec::new(); blocks.len()];
    for (i, block) in blocks.iter().enumerate() {
        for (target, _) in crate::term_edges(&block.term) {
            predecessors[indices[&target]].push(i);
        }
    }
    let mut reachable = vec![false; blocks.len()];
    let mut pending = vec![0];
    while let Some(i) = pending.pop() {
        if std::mem::replace(&mut reachable[i], true) {
            continue;
        }
        pending.extend(
            crate::term_edges(&blocks[i].term)
                .iter()
                .map(|(target, _)| indices[target]),
        );
    }
    // Top initialization is required for loop backedges in a must-analysis.
    let mut incoming = reachable
        .iter()
        .map(|v| if *v { 2 } else { 0 })
        .collect::<Vec<_>>();
    let mut outgoing = incoming.clone();
    loop {
        let mut changed = false;
        for (i, block) in blocks.iter().enumerate() {
            let before = if i != 0 && reachable[i] {
                predecessors[i]
                    .iter()
                    .filter(|pred| reachable[**pred])
                    .map(|pred| outgoing[*pred])
                    .min()
                    .unwrap_or(0)
            } else {
                0
            };
            let mut after = block.insts.iter().fold(before, |admitted, inst| {
                transfers_environment(inst, admitted)
            });
            if matches!(block.term, MTerm::AsmGoto { .. }) {
                after = 0;
            }
            changed |= incoming[i] != before || outgoing[i] != after;
            incoming[i] = before;
            outgoing[i] = after;
        }
        if !changed {
            break;
        }
    }
    for (i, block) in blocks.iter().enumerate() {
        let mut admitted = incoming[i];
        for (index, inst) in block.insts.iter().enumerate() {
            let required = needs_environment(inst);
            if required > admitted {
                guards.insert((block.id, index), required == 1);
            }
            admitted = transfers_environment(inst, admitted);
        }
    }
    guards
}

/// The class of a floating value. A half value is its 16 representation
/// bits in a `gpr32` register, with the bits above them unspecified, as for
/// a `bits<16>` value (Section 2.6).
pub(crate) fn class(kind: FloatKind) -> RegClass {
    match kind {
        FloatKind::F16 | FloatKind::BF16 => RegClass::Gpr32,
        FloatKind::F32 => RegClass::Simd32,
        FloatKind::F128 => RegClass::Simd128,
        FloatKind::F64 => RegClass::Simd64,
    }
}
fn ty(kind: FloatKind) -> EvoIRType {
    match kind {
        FloatKind::F16 => EvoIRType::F16,
        FloatKind::BF16 => EvoIRType::BF16,
        FloatKind::F32 => EvoIRType::F32,
        FloatKind::F128 => EvoIRType::F128,
        FloatKind::F64 => EvoIRType::F64,
    }
}

/// The format of the SIMD side of a half conversion, from its class.
fn simd_format(class: RegClass) -> Option<FloatKind> {
    match class {
        RegClass::Simd32 => Some(FloatKind::F32),
        RegClass::Simd64 => Some(FloatKind::F64),
        _ => None,
    }
}

/// A conversion with a half format on one side (Section 2.6). `kind` names
/// the half format; the class of the other register gives the other format:
/// `simd32` binary32, `simd64` binary64, and `gpr32` a 32-bit integer
/// source of `fp.from_signed` or `fp.from_unsigned`.
fn half_operation(
    dst: VReg,
    op: FloatingOp,
    kind: FloatKind,
    args: &[VReg],
) -> Result<FloatingInst, String> {
    use FloatingOp::*;
    let [arg] = args else {
        return Err("a half conversion takes one operand".into());
    };
    let value = |reg: VReg, ty| Value { id: reg.id, ty };
    let (input, result) = match op {
        Convert if arg.class == RegClass::Gpr32 => (
            value(*arg, ty(kind)),
            value(
                dst,
                ty(simd_format(dst.class).ok_or("half widening needs a SIMD result")?),
            ),
        ),
        Convert if dst.class == RegClass::Gpr32 => (
            value(
                *arg,
                ty(simd_format(arg.class).ok_or("half narrowing needs a SIMD source")?),
            ),
            value(dst, ty(kind)),
        ),
        FromSigned | FromUnsigned
            if arg.class == RegClass::Gpr32 && dst.class == RegClass::Gpr32 =>
        {
            (value(*arg, EvoIRType::Bits(32)), value(dst, ty(kind)))
        }
        _ => return Err("invalid machine half conversion".into()),
    };
    let instruction = FloatingInst {
        result,
        op,
        arguments: vec![input],
    };
    instruction.validate()?;
    Ok(instruction)
}

/// Independent machine shape check before allocation and after reconstruction.
pub(crate) fn operation(
    dst: VReg,
    op: FloatingOp,
    kind: FloatKind,
    args: &[VReg],
    integer_bits: u16,
) -> Result<FloatingInst, String> {
    use FloatingOp::*;
    if kind.is_half() {
        if integer_bits != 0 {
            return Err("a half conversion has no integer width".into());
        }
        return half_operation(dst, op, kind, args);
    }
    if integer_bits != 0
        && (!matches!(op, ToSigned | ToUnsigned)
            || integer_bits > dst.class.bit_width().unwrap_or(0))
    {
        return Err("invalid semantic integer conversion width".into());
    }
    if !matches!(kind, FloatKind::F32 | FloatKind::F64) || matches!(op, Constant(_)) {
        return Err("invalid machine floating format/constant".into());
    }
    let source_integer = matches!(op, FromSigned | FromUnsigned | FromBits);
    let mut inputs = Vec::new();
    for arg in args {
        let input_ty = if source_integer {
            match arg.class {
                RegClass::Gpr32 => EvoIRType::Bits(32),
                RegClass::Gpr64 => EvoIRType::Bits(64),
                _ => return Err("floating conversion source must use GPR bank".into()),
            }
        } else {
            if arg.class != class(kind) {
                return Err("floating operand register bank/width mismatch".into());
            }
            ty(kind)
        };
        inputs.push(Value {
            id: arg.id,
            ty: input_ty,
        });
    }
    let result_ty = match op {
        ToIntDefined { .. }
        | Oeq
        | Une
        | Olt
        | Ole
        | Ogt
        | Oge
        | QuietOlt
        | QuietOle
        | QuietOgt
        | QuietOge => {
            if dst.class != RegClass::Gpr32 {
                return Err("FP predicate result requires gpr32".into());
            }
            EvoIRType::Bits(1)
        }
        ToSigned | ToUnsigned | ToBits => match dst.class {
            RegClass::Gpr32 => EvoIRType::Bits(if integer_bits == 0 { 32 } else { integer_bits }),
            RegClass::Gpr64 => EvoIRType::Bits(if integer_bits == 0 { 64 } else { integer_bits }),
            _ => return Err("floating integer result requires GPR bank".into()),
        },
        Convert => match dst.class {
            RegClass::Simd32 => EvoIRType::F32,
            RegClass::Simd64 => EvoIRType::F64,
            _ => return Err("floating conversion result bank/width mismatch".into()),
        },
        _ => {
            if dst.class != class(kind) {
                return Err("floating result bank/width mismatch".into());
            }
            ty(kind)
        }
    };
    let instruction = FloatingInst {
        result: Value {
            id: dst.id,
            ty: result_ty,
        },
        op,
        arguments: inputs,
    };
    instruction.validate()?;
    Ok(instruction)
}

fn constant(text: &mut String, kind: FloatKind, value: FloatBits) {
    let raw = value.bits() as u64;
    let _ = writeln!(text, "    movz {SCRATCH_UNIT_X}, #{}", raw & 65535);
    for shift in [16, 32, 48] {
        if raw >> shift & 65535 != 0 {
            let _ = writeln!(
                text,
                "    movk {SCRATCH_UNIT_X}, #{}, lsl #{shift}",
                raw >> shift & 65535
            );
        }
    }
    let _ = writeln!(
        text,
        "    fmov {}, {}",
        if kind == FloatKind::F32 {
            SIMD_SCRATCH_UNIT_S
        } else {
            SIMD_SCRATCH_UNIT_D
        },
        if kind == FloatKind::F32 {
            SCRATCH_UNIT_W
        } else {
            SCRATCH_UNIT_X
        }
    );
}

fn bounds(
    text: &mut String,
    kind: FloatKind,
    width: u16,
    signed: bool,
    input: &str,
    invalid: &str,
) {
    // A definedness predicate is pure. FCMP on an sNaN would set FPSR.INVALID,
    // so classify every NaN through its representation before any FP compare.
    // The strict conversion itself raises INVALID on the rejected path below.
    let (scratch_gpr, scratch_unit, signless, infinity) = if kind == FloatKind::F32 {
        (SCRATCH_W, SCRATCH_UNIT_W, "0x7fffffff", "0x7f800000")
    } else {
        (
            SCRATCH_X,
            SCRATCH_UNIT_X,
            "0x7fffffffffffffff",
            "0x7ff0000000000000",
        )
    };
    let _ = writeln!(text, "    fmov {scratch_gpr}, {input}\n    and {scratch_gpr}, {scratch_gpr}, #{signless}\n    mov {scratch_unit}, #{infinity}\n    cmp {scratch_gpr}, {scratch_unit}\n    b.hi {invalid}");
    let upper_exponent = width - u16::from(signed);
    let upper = FloatBits::parse(kind, &format!("0x1p{upper_exponent}")).unwrap();
    constant(text, kind, upper);
    let scratch = if kind == FloatKind::F32 {
        SIMD_SCRATCH_UNIT_S
    } else {
        SIMD_SCRATCH_UNIT_D
    };
    // HS also rejects unordered (NaN), whose FCMP carry flag is set.
    let _ = writeln!(text, "    fcmp {input}, {scratch}\n    b.hs {invalid}");
    let (lower, strict) = if !signed {
        (FloatBits::from_signed(kind, -1), true)
    } else if upper_exponent < if kind == FloatKind::F32 { 24 } else { 53 } {
        (
            FloatBits::from_signed(kind, -(1i128 << upper_exponent) - 1),
            true,
        )
    } else {
        (upper.negated(), false)
    };
    constant(text, kind, lower);
    let _ = writeln!(
        text,
        "    fcmp {input}, {scratch}\n    b.{} {invalid}",
        if strict { "le" } else { "lt" }
    );
}

pub(crate) fn emit(
    text: &mut String,
    instruction: &MInst,
    name: &impl Fn(VReg) -> String,
    label: &str,
) {
    let MInst::Floating {
        dst,
        op,
        kind,
        args,
        strict,
        integer_bits,
    } = instruction
    else {
        unreachable!("floating emitter receives only floating instructions")
    };
    let (dst, op, kind, strict, integer_bits) = (*dst, *op, *kind, *strict, *integer_bits);
    use FloatingOp::*;
    let dest = name(dst);
    if kind.is_half() {
        let source = name(args[0]);
        emit_half(
            text,
            op,
            kind,
            (dst.class, &dest),
            (args[0].class, &source),
            strict,
            label,
        );
        return;
    }
    match op {
        Add | Sub | Mul | Div => {
            let _ = writeln!(
                text,
                "    f{} {dest}, {}, {}",
                op.mnemonic(),
                name(args[0]),
                name(args[1])
            );
        }
        Fma => {
            let _ = writeln!(
                text,
                "    fmadd {dest}, {}, {}, {}",
                name(args[0]),
                name(args[1]),
                name(args[2])
            );
        }
        Neg => {
            let _ = writeln!(text, "    fneg {dest}, {}", name(args[0]));
        }
        Convert => {
            let _ = writeln!(text, "    fcvt {dest}, {}", name(args[0]));
        }
        FromSigned | FromUnsigned => {
            let _ = writeln!(
                text,
                "    {}cvtf {dest}, {}",
                if op == FromSigned { "s" } else { "u" },
                name(args[0])
            );
        }
        ToBits | FromBits => {
            let _ = writeln!(text, "    fmov {dest}, {}", name(args[0]));
        }
        Oeq | Une | Olt | Ole | Ogt | Oge | QuietOlt | QuietOle | QuietOgt | QuietOge => {
            let cond = match op {
                Oeq => "eq",
                Une => "ne",
                Olt | QuietOlt => "mi",
                Ole | QuietOle => "ls",
                Ogt | QuietOgt => "gt",
                Oge | QuietOge => "ge",
                _ => unreachable!(),
            };
            let _ = writeln!(
                text,
                "    fcmp{} {}, {}\n    cset {dest}, {cond}",
                if strict && matches!(op, Olt | Ole | Ogt | Oge) {
                    "e"
                } else {
                    ""
                },
                name(args[0]),
                name(args[1])
            );
        }
        ToIntDefined { width, signed } => {
            bounds(
                text,
                kind,
                width,
                signed,
                &name(args[0]),
                &format!("{label}_invalid"),
            );
            let _ = writeln!(text, "    mov {dest}, #1\n    b {label}_done\n{label}_invalid:\n    mov {dest}, #0\n{label}_done:");
        }
        ToSigned | ToUnsigned => {
            bounds(
                text,
                kind,
                if integer_bits == 0 {
                    dst.class.bit_width().unwrap()
                } else {
                    integer_bits
                },
                op == ToSigned,
                &name(args[0]),
                &format!("{label}_invalid"),
            );
            let invalid_status = if strict {
                format!("    fmov {SIMD_SCRATCH_UNIT_D}, {ZERO_GPR64}\n    fdiv {SIMD_SCRATCH_UNIT_D}, {SIMD_SCRATCH_UNIT_D}, {SIMD_SCRATCH_UNIT_D}\n")
            } else {
                String::new()
            };
            let _ = writeln!(text, "    fcvtz{} {dest}, {}\n    b {label}_done\n{label}_invalid:\n{invalid_status}    mov {dest}, #0\n{label}_done:", if op == ToSigned { "s" } else { "u" }, name(args[0]));
        }
        Constant(_) => unreachable!("constants lower via exact bit moves"),
        ComplexMul { .. } | ComplexDiv { .. } => {
            unreachable!("complex parts lower to helper calls")
        }
    }
}

/// A half conversion (Section 2.6). A binary16 value moves through the `h`
/// view of the scratch register `v31` and converts with FCVT, which observes
/// FPCR and raises status as IEEE 754 requires. A bfloat16 value widens by
/// a shift into the high half of a binary32 representation; FMAX of the value
/// with itself then quiets a signaling NaN and raises INVALID for it, and
/// changes no other value. Narrowing to bfloat16 rounds the binary32 bits in
/// general registers. A binary64 source narrows first with FCVTXN, which
/// rounds to odd: twenty-four bits are more than eight plus two, so the
/// second rounding is exact rounding of the binary64 value, and the status
/// of the two steps is the status of one IEEE conversion.
fn emit_half(
    text: &mut String,
    op: FloatingOp,
    kind: FloatKind,
    (dst_class, dest): (RegClass, &str),
    (src_class, source): (RegClass, &str),
    strict: bool,
    label: &str,
) {
    use crate::isa::aarch64::regfile::SIMD_SCRATCH_UNIT_H;
    let s31 = SIMD_SCRATCH_UNIT_S;
    let d31 = SIMD_SCRATCH_UNIT_D;
    match op {
        FloatingOp::Convert if src_class == RegClass::Gpr32 => {
            if kind == FloatKind::F16 {
                let _ = writeln!(
                    text,
                    "    fmov {s31}, {source}\n    fcvt {dest}, {SIMD_SCRATCH_UNIT_H}"
                );
            } else if dst_class == RegClass::Simd32 {
                let _ = writeln!(
                    text,
                    "    lsl {SCRATCH_W}, {source}, #16\n    fmov {dest}, {SCRATCH_W}\n    fmax {dest}, {dest}, {dest}"
                );
            } else {
                let _ = writeln!(
                    text,
                    "    lsl {SCRATCH_W}, {source}, #16\n    fmov {s31}, {SCRATCH_W}\n    fcvt {dest}, {s31}"
                );
            }
        }
        FloatingOp::Convert => {
            if kind == FloatKind::F16 {
                let _ = writeln!(
                    text,
                    "    fcvt {SIMD_SCRATCH_UNIT_H}, {source}\n    fmov {dest}, {s31}"
                );
            } else if src_class == RegClass::Simd64 {
                let _ = writeln!(text, "    fcvtxn {s31}, {source}");
                bfloat_narrow(text, dest, s31, strict, label);
            } else {
                bfloat_narrow(text, dest, source, strict, label);
            }
        }
        FloatingOp::FromSigned | FloatingOp::FromUnsigned => {
            // A 32-bit integer is exact in binary64.
            let _ = writeln!(
                text,
                "    {}cvtf {d31}, {source}",
                if op == FloatingOp::FromSigned {
                    "s"
                } else {
                    "u"
                }
            );
            if kind == FloatKind::F16 {
                let _ = writeln!(
                    text,
                    "    fcvt {SIMD_SCRATCH_UNIT_H}, {d31}\n    fmov {dest}, {s31}"
                );
            } else {
                let _ = writeln!(text, "    fcvtxn {s31}, {d31}");
                bfloat_narrow(text, dest, s31, strict, label);
            }
        }
        _ => unreachable!("validated half conversion"),
    }
}

/// Round the binary32 value in `source` to bfloat16 bits in `dest`. A NaN
/// keeps its sign and its leading payload bits and becomes quiet; the quiet
/// comparison FCMP raises INVALID for a signaling NaN, as the conversion
/// must. A baseline conversion rounds to nearest, ties to even: it adds
/// 0x7fff and the lowest kept bit, and an overflow carries into infinity. A
/// strict one reads the rounding direction from FPCR and ORs its status into
/// FPSR: INEXACT for a discarded bit, UNDERFLOW when the value is tiny before
/// rounding, and OVERFLOW when it rounds to infinity. Writing FPSR raises a
/// status in this non-trapping profile.
fn bfloat_narrow(text: &mut String, dest: &str, source: &str, strict: bool, label: &str) {
    let w16 = SCRATCH_W;
    let w17 = SCRATCH_UNIT_W;
    let x17 = SCRATCH_UNIT_X;
    let _ = writeln!(
        text,
        "    fcmp {source}, {source}\n    fmov {w16}, {source}\n    b.vs {label}_bnan"
    );
    if strict {
        let _ = writeln!(
            text,
            "    tst {w16}, #0xffff\n    b.eq {label}_bexact\n    mov {dest}, #16\n    tst {w16}, #0x7f800000\n    b.ne {label}_bnormal\n    mov {dest}, #24\n{label}_bnormal:\n    mrs {x17}, fpcr\n    ubfx {w17}, {w17}, #22, #2\n    cbz {w17}, {label}_bnearest\n    cmp {w17}, #3\n    b.eq {label}_bzero\n    cmp {w17}, #1\n    cset {w17}, eq\n    eor {w17}, {w17}, {w16}, lsr #31\n    b {label}_bround\n{label}_bzero:\n    mov {w17}, #0\n    b {label}_bround\n{label}_bnearest:\n    ubfx {w17}, {w16}, #16, #1\n    add {w17}, {w17}, {w16}, uxth\n    add {w17}, {w17}, #7, lsl #12\n    add {w17}, {w17}, #0xfff\n    lsr {w17}, {w17}, #16\n{label}_bround:\n    add {w16}, {w17}, {w16}, lsr #16\n    mvn {w17}, {w16}\n    tst {w17}, #0x7f80\n    b.ne {label}_bstatus\n    orr {dest}, {dest}, #4\n{label}_bstatus:\n    mrs {x17}, fpsr\n    orr {w17}, {w17}, {dest}\n    msr fpsr, {x17}\n    mov {dest}, {w16}\n    b {label}_bdone\n{label}_bexact:\n    lsr {dest}, {w16}, #16\n    b {label}_bdone"
        );
    } else {
        let _ = writeln!(
            text,
            "    ubfx {w17}, {w16}, #16, #1\n    add {w16}, {w16}, {w17}\n    mov {w17}, #0x7fff\n    add {w16}, {w16}, {w17}\n    lsr {dest}, {w16}, #16\n    b {label}_bdone"
        );
    }
    let _ = writeln!(
        text,
        "{label}_bnan:\n    lsr {dest}, {w16}, #16\n    orr {dest}, {dest}, #0x40\n{label}_bdone:"
    );
}
