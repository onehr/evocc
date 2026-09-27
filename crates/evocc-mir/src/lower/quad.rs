//! Hosted binary128 lowering. Representation work stays in the existing SIMD
//! bank; numerical helpers are ordinary calls with explicit clobbers.
use super::*;
use evocc_evoir::{FloatingInst, FloatingOp};
use evocc_types::{
    floating::{FloatBits, FpRounding},
    FloatKind,
};

fn pack(l: &mut Lowerer, out: &mut Vec<MInst>, lo: VReg, hi: VReg) -> VReg {
    let dst = l.fresh(RegClass::Simd128);
    out.push(MInst::Vector {
        dst,
        extract: false,
        lane_bits: 64,
        bytes: 16,
        args: vec![lo, hi],
    });
    dst
}
fn unpack(l: &mut Lowerer, out: &mut Vec<MInst>, value: VReg) -> (VReg, VReg) {
    let (lo, hi) = l.fresh_pair();
    for (dst, index) in [(lo, 0), (hi, 1)] {
        let index = l.word(out, index);
        out.push(MInst::Vector {
            dst,
            extract: true,
            lane_bits: 64,
            bytes: 16,
            args: vec![value, index],
        });
    }
    (lo, hi)
}
fn constant(l: &mut Lowerer, out: &mut Vec<MInst>, bits: u128) -> VReg {
    let lo = l.word(out, bits as u64);
    let hi = l.word(out, (bits >> 64) as u64);
    pack(l, out, lo, hi)
}
fn select_zero(l: &mut Lowerer, out: &mut Vec<MInst>, value: VReg, predicate: VReg) -> VReg {
    let (lo, hi) = unpack(l, out, value);
    let lo = floating_select_zero(l, out, lo, predicate);
    let hi = floating_select_zero(l, out, hi, predicate);
    pack(l, out, lo, hi)
}
fn magnitude_less(l: &mut Lowerer, out: &mut Vec<MInst>, lo: VReg, hi: VReg, bound: u128) -> VReg {
    let blo = l.word(out, bound as u64);
    let bhi = l.word(out, (bound >> 64) as u64);
    let high_less = l.compare_into(out, hi, bhi, Width::X, Cond::Lo);
    let high_equal = l.compare_into(out, hi, bhi, Width::X, Cond::Eq);
    let low_less = l.compare_into(out, lo, blo, Width::X, Cond::Lo);
    let tied = l.both(out, high_equal, low_less);
    l.binary(out, BinOp::Orr, high_less, tied, Width::W)
}
/// Truncation is defined on a half-open interval. Compare encodings only, so
/// even signaling NaNs do not change FPSR while evaluating the C UB predicate.
fn defined(l: &mut Lowerer, out: &mut Vec<MInst>, input: VReg, width: u16, signed: bool) -> VReg {
    let (lo, hi) = unpack(l, out, input);
    let magnitude = l.word_binary_imm(out, BinOp::And, hi, 0x7fffffffffffffff);
    let exponent = width - u16::from(signed);
    let positive = FloatBits::parse(FloatKind::F128, &format!("0x1p{exponent}"))
        .unwrap()
        .bits();
    let negative = if signed {
        FloatBits::from_unsigned_round(FloatKind::F128, (1u128 << exponent) + 1, FpRounding::Upward)
            .value
            .bits()
    } else {
        FloatBits::parse(FloatKind::F128, "1").unwrap().bits()
    };
    let yes_positive = magnitude_less(l, out, lo, magnitude, positive);
    let yes_negative = magnitude_less(l, out, lo, magnitude, negative);
    let flags = l.fresh(RegClass::Flags);
    out.push(MInst::CmpImm {
        dst: flags,
        lhs: hi,
        imm: 0,
        width: Width::X,
    });
    let dst = l.fresh(RegClass::Gpr32);
    out.push(MInst::Csel {
        dst,
        flags,
        lhs: yes_negative,
        rhs: yes_positive,
        cond: Cond::Lt,
        width: Width::W,
    });
    dst
}
fn call(
    l: &mut Lowerer,
    out: &mut Vec<MInst>,
    symbol: &str,
    args: Vec<VReg>,
    strict: bool,
) -> Result<Vec<VReg>, String> {
    let helper =
        crate::floating::helpers::qualified(symbol).ok_or("unqualified binary128 helper")?;
    let (_, classes) = helper.classes();
    let results = classes.into_iter().map(|c| l.fresh(c)).collect::<Vec<_>>();
    out.push(MInst::FpEnvCheck { strict });
    out.push(MInst::Bl {
        symbol: symbol.into(),
        args,
        results: results.clone(),
        fp_mode: Some(if strict {
            crate::FpCallMode::Strict
        } else {
            crate::FpCallMode::Fixed
        }),
    });
    Ok(results)
}
fn raise_invalid(l: &mut Lowerer, out: &mut Vec<MInst>, valid: VReg) {
    let zero = l.fresh(RegClass::Gpr32);
    out.push(MInst::MovImm { dst: zero, imm: 0 });
    let invalid = l.compare_into(out, valid, zero, Width::W, Cond::Eq);
    let nan = l.fresh(RegClass::Gpr32);
    out.push(MInst::MovImm {
        dst: nan,
        imm: 0x7fc00000,
    });
    let bits = floating_select_zero(l, out, nan, invalid);
    let src = l.fresh(RegClass::Simd32);
    out.push(MInst::Floating {
        dst: src,
        op: FloatingOp::FromBits,
        kind: FloatKind::F32,
        args: vec![bits],
        strict: false,
        integer_bits: 0,
    });
    let dst = l.fresh(RegClass::Gpr32);
    out.push(MInst::Floating {
        dst,
        op: FloatingOp::ToSigned,
        kind: FloatKind::F32,
        args: vec![src],
        strict: true,
        integer_bits: 32,
    });
}

pub(super) fn lower(
    l: &mut Lowerer,
    out: &mut Vec<MInst>,
    operation: &FloatingInst,
    strict: bool,
) -> Result<(), String> {
    use FloatingOp::*;
    let result = &operation.result;
    let op = operation.op;
    match op {
        Constant(bits) => {
            let dst = constant(l, out, bits);
            l.bind(result, dst);
        }
        FromBits => {
            let (lo, hi) = l.pair(&operation.arguments[0])?;
            let dst = pack(l, out, lo, hi);
            l.bind(result, dst);
        }
        ToBits => {
            let input = l.reg(&operation.arguments[0])?;
            let (lo, hi) = unpack(l, out, input);
            l.bind_pair(result, lo, hi);
        }
        Neg => {
            let input = l.reg(&operation.arguments[0])?;
            let (lo, hi) = unpack(l, out, input);
            let hi = l.word_binary_imm(out, BinOp::Eor, hi, 1u64 << 63);
            let dst = pack(l, out, lo, hi);
            l.bind(result, dst);
        }
        ToIntDefined { width, signed } => {
            let input = l.reg(&operation.arguments[0])?;
            let dst = defined(l, out, input, width, signed);
            l.bind(result, dst);
        }
        FromSigned | FromUnsigned => {
            let input = &operation.arguments[0];
            let bits = significant_bits(&input.ty);
            let args = if bits == 128 {
                l.registers(input)?
            } else {
                let input = l.reg(input)?;
                let width = width_of(input.class);
                vec![if op == FromSigned {
                    l.sign_extend(out, input, bits, width)
                } else {
                    l.zero_extend(out, input, bits, width)
                }]
            };
            let suffix = match bits {
                1..=32 => "si",
                33..=64 => "di",
                _ => "ti",
            };
            let symbol = format!(
                "__float{}{suffix}tf",
                if op == FromUnsigned { "un" } else { "" }
            );
            let dst = call(l, out, &symbol, args, strict)?[0];
            l.bind(result, dst);
        }
        ToSigned | ToUnsigned => {
            let bits = significant_bits(&result.ty);
            let input = l.reg(&operation.arguments[0])?;
            let valid = defined(l, out, input, bits, op == ToSigned);
            if strict {
                raise_invalid(l, out, valid);
            }
            let input = select_zero(l, out, input, valid);
            let suffix = match bits {
                1..=32 => "si",
                33..=64 => "di",
                _ => "ti",
            };
            let symbol = format!(
                "__fix{}tf{suffix}",
                if op == ToUnsigned { "uns" } else { "" }
            );
            let outputs = call(l, out, &symbol, vec![input], strict)?;
            if bits == 128 {
                l.bind_pair(result, outputs[0], outputs[1]);
            } else {
                let dst = l.zero_extend(out, outputs[0], bits, width_of(outputs[0].class));
                l.bind(result, dst);
            }
        }
        Convert => {
            let source = &operation.arguments[0];
            if source.ty == result.ty {
                let input = l.reg(source)?;
                l.bind(result, input);
                return Ok(());
            }
            let symbol = match (&source.ty, &result.ty) {
                (EvoIRType::F32, EvoIRType::F128) => "__extendsftf2",
                (EvoIRType::F64, EvoIRType::F128) => "__extenddftf2",
                (EvoIRType::F128, EvoIRType::F32) => "__trunctfsf2",
                (EvoIRType::F128, EvoIRType::F64) => "__trunctfdf2",
                _ => return Err("unqualified quad format conversion".into()),
            };
            let input = l.reg(source)?;
            let dst = call(l, out, symbol, vec![input], strict)?[0];
            l.bind(result, dst);
        }
        Add | Sub | Mul | Div | Fma => {
            let args = operation
                .arguments
                .iter()
                .map(|a| l.reg(a))
                .collect::<Result<Vec<_>, _>>()?;
            let symbol = match op {
                Add => "__addtf3",
                Sub => "__subtf3",
                Mul => "__multf3",
                Div => "__divtf3",
                _ => "fmal",
            };
            let dst = call(l, out, symbol, args, strict)?[0];
            l.bind(result, dst);
        }
        Oeq | Une | Olt | Ole | Ogt | Oge | QuietOlt | QuietOle | QuietOgt | QuietOge => {
            let mut args = operation
                .arguments
                .iter()
                .map(|a| l.reg(a))
                .collect::<Result<Vec<_>, _>>()?;
            let mut ordered = None;
            if matches!(op, QuietOlt | QuietOle | QuietOgt | QuietOge) {
                let unordered = call(l, out, "__unordtf2", args.clone(), strict)?[0];
                let zero = l.fresh(RegClass::Gpr32);
                out.push(MInst::MovImm { dst: zero, imm: 0 });
                let valid = l.compare_into(out, unordered, zero, Width::W, Cond::Eq);
                args = args
                    .into_iter()
                    .map(|a| select_zero(l, out, a, valid))
                    .collect();
                ordered = Some(valid);
            }
            let (symbol, cond) = match op {
                Oeq => ("__eqtf2", Cond::Eq),
                Une => ("__eqtf2", Cond::Ne),
                Olt | QuietOlt => ("__letf2", Cond::Lt),
                Ole | QuietOle => ("__letf2", Cond::Le),
                Ogt | QuietOgt => ("__getf2", Cond::Gt),
                _ => ("__getf2", Cond::Ge),
            };
            let raw = call(l, out, symbol, args, strict)?[0];
            let zero = l.fresh(RegClass::Gpr32);
            out.push(MInst::MovImm { dst: zero, imm: 0 });
            let mut dst = l.compare_into(out, raw, zero, Width::W, cond);
            if let Some(valid) = ordered {
                dst = l.both(out, dst, valid);
            }
            l.bind(result, dst);
        }
    }
    Ok(())
}
