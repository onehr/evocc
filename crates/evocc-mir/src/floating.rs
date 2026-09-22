//! Scalar FP shares the SIMD allocation bank; no host arithmetic is used.
use crate::{MBlock, MInst, MTerm, RegClass, VReg};
use evocc_evoir::{EvoIRType, FloatingInst, FloatingOp, Value};
use evocc_types::{floating::FloatBits, FloatKind};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

fn needs_environment(inst: &MInst) -> bool {
    matches!(inst, MInst::FpEnvCheck)
        || matches!(inst, MInst::Floating { op, .. } if !matches!(op, FloatingOp::ToBits | FloatingOp::FromBits))
}

fn transfers_environment(inst: &MInst, admitted: bool) -> bool {
    if matches!(
        inst,
        MInst::Bl { .. } | MInst::Blr { .. } | MInst::Asm { .. }
    ) {
        false
    } else {
        admitted || needs_environment(inst)
    }
}

/// Must-analysis over the complete machine CFG. A successful check remains
/// valid only until opaque execution; entry and unreachable blocks never
/// inherit assumptions about the caller's FPCR. No check writes FPCR.
pub(crate) fn guard_points(blocks: &[MBlock]) -> BTreeSet<(u32, usize)> {
    let mut guards = BTreeSet::new();
    if !blocks
        .iter()
        .any(|block| block.insts.iter().any(needs_environment))
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
    let mut incoming = reachable.clone();
    let mut outgoing = reachable.clone();
    loop {
        let mut changed = false;
        for (i, block) in blocks.iter().enumerate() {
            let before = i != 0
                && reachable[i]
                && predecessors[i]
                    .iter()
                    .filter(|pred| reachable[**pred])
                    .all(|pred| outgoing[*pred]);
            let mut after = block.insts.iter().fold(before, |admitted, inst| {
                transfers_environment(inst, admitted)
            });
            if matches!(block.term, MTerm::AsmGoto { .. }) {
                after = false;
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
            if needs_environment(inst) && !admitted {
                guards.insert((block.id, index));
            }
            admitted = transfers_environment(inst, admitted);
        }
    }
    guards
}

pub(crate) fn class(kind: FloatKind) -> RegClass {
    if kind == FloatKind::F32 {
        RegClass::Simd32
    } else {
        RegClass::Simd64
    }
}
fn ty(kind: FloatKind) -> EvoIRType {
    if kind == FloatKind::F32 {
        EvoIRType::F32
    } else {
        EvoIRType::F64
    }
}

/// Independent machine shape check before allocation and after reconstruction.
pub(crate) fn operation(
    dst: VReg,
    op: FloatingOp,
    kind: FloatKind,
    args: &[VReg],
) -> Result<FloatingInst, String> {
    use FloatingOp::*;
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
        ToIntDefined { .. } | Oeq | Une | Olt | Ole | Ogt | Oge => {
            if dst.class != RegClass::Gpr32 {
                return Err("FP predicate result requires gpr32".into());
            }
            EvoIRType::Bits(1)
        }
        ToSigned | ToUnsigned | ToBits => match dst.class {
            RegClass::Gpr32 => EvoIRType::Bits(32),
            RegClass::Gpr64 => EvoIRType::Bits(64),
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
    let _ = writeln!(text, "    movz x17, #{}", raw & 65535);
    for shift in [16, 32, 48] {
        if raw >> shift & 65535 != 0 {
            let _ = writeln!(
                text,
                "    movk x17, #{}, lsl #{shift}",
                raw >> shift & 65535
            );
        }
    }
    let _ = writeln!(
        text,
        "    fmov {}31, {}17",
        if kind == FloatKind::F32 { "s" } else { "d" },
        if kind == FloatKind::F32 { "w" } else { "x" }
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
    let upper_exponent = width - u16::from(signed);
    let upper = FloatBits::parse(kind, &format!("0x1p{upper_exponent}")).unwrap();
    constant(text, kind, upper);
    let scratch = if kind == FloatKind::F32 { "s31" } else { "d31" };
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
    dst: VReg,
    op: FloatingOp,
    kind: FloatKind,
    args: &[VReg],
    name: &impl Fn(VReg) -> String,
    label: &str,
) {
    use FloatingOp::*;
    let dest = name(dst);
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
        Oeq | Une | Olt | Ole | Ogt | Oge => {
            let cond = match op {
                Oeq => "eq",
                Une => "ne",
                Olt => "mi",
                Ole => "ls",
                Ogt => "gt",
                Oge => "ge",
                _ => unreachable!(),
            };
            let _ = writeln!(
                text,
                "    fcmp {}, {}\n    cset {dest}, {cond}",
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
                dst.class.bit_width().unwrap(),
                op == ToSigned,
                &name(args[0]),
                &format!("{label}_invalid"),
            );
            let _ = writeln!(text, "    fcvtz{} {dest}, {}\n    b {label}_done\n{label}_invalid:\n    mov {dest}, #0\n{label}_done:", if op == ToSigned { "s" } else { "u" }, name(args[0]));
        }
        Constant(_) => unreachable!("constants lower via exact bit moves"),
    }
}
