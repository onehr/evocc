//! Independent SoftFloat result/status stream. NaN policy and tininess must
//! match the target specialization. Scalar hardware FMA uses the native corpus
//! separately; binary128's hosted FMA priority agrees with this specialization.
use super::*;
use evocc_types::floating::{FpResult, FpStatus};

fn fp(name: &str) -> Option<FloatKind> {
    match name {
        "f32" => Some(FloatKind::F32),
        "f64" => Some(FloatKind::F64),
        "f128" => Some(FloatKind::F128),
        _ => None,
    }
}
fn integer(name: &str) -> Option<(u16, bool)> {
    match name {
        "i32" => Some((32, true)),
        "i64" => Some((64, true)),
        "ui32" => Some((32, false)),
        "ui64" => Some((64, false)),
        _ => None,
    }
}
fn result(value: FpResult<FloatBits>) -> FpResult<Option<u128>> {
    FpResult {
        value: Some(value.value.bits()),
        status: value.status,
    }
}

pub fn check(operation: &str, rounding: FpRounding, line: &str) -> Result<(), String> {
    let fields = line
        .split_whitespace()
        .map(|s| u128::from_str_radix(s, 16).map_err(|_| "invalid hexadecimal field"))
        .collect::<Result<Vec<_>, _>>()?;
    let status = |flags: u128| -> Result<FpStatus, String> {
        if flags > 31 {
            return Err("invalid reference status".into());
        }
        Ok(FpStatus::from_bits(
            ((flags & 1) << 4
                | (flags & 2) << 2
                | (flags & 4)
                | (flags & 8) >> 2
                | (flags & 16) >> 4) as u8,
        )
        .unwrap())
    };
    let (actual, expected, expected_status) = if let Some((from, to)) = operation.split_once("_to_")
    {
        if fields.len() != 3 {
            return Err("conversion requires three fields".into());
        }
        let expected_status = status(fields[2])?;
        let actual = if let Some(kind) = fp(from) {
            let a = FloatBits::from_bits(kind, fields[0]).ok_or("overwide input")?;
            if let Some(kind) = fp(to) {
                result(a.target_convert_round(kind, rounding).unwrap())
            } else if let Some((width, signed)) = integer(to) {
                if rounding != FpRounding::TowardZero {
                    return Err("C integer conversion requires toward-zero reference".into());
                }
                let mut r = a.to_integer_status(width, signed).unwrap();
                r.value = r.value.map(|v| v & (u128::MAX >> (128 - width)));
                r
            } else {
                return Err("unknown conversion target".into());
            }
        } else {
            let (width, signed) = integer(from).ok_or("unknown conversion source")?;
            if fields[0] >> width != 0 {
                return Err("overwide integer input".into());
            }
            let kind = fp(to).ok_or("unknown floating target")?;
            result(if signed {
                FloatBits::from_signed_round(
                    kind,
                    ((fields[0] << (128 - width)) as i128) >> (128 - width),
                    rounding,
                )
            } else {
                FloatBits::from_unsigned_round(kind, fields[0], rounding)
            })
        };
        let expected = (!(integer(to).is_some() && expected_status.contains(FpStatus::INVALID)))
            .then_some(fields[1]);
        (actual, expected, expected_status)
    } else if operation == "f128_mulAdd" {
        if fields.len() != 5 {
            return Err("fused operation requires five fields".into());
        }
        let a = FloatBits::from_bits(FloatKind::F128, fields[0]).unwrap();
        let b = FloatBits::from_bits(FloatKind::F128, fields[1]).unwrap();
        let c = FloatBits::from_bits(FloatKind::F128, fields[2]).unwrap();
        let flags = status(fields[4])?;
        let expected = if !a.is_nan()
            && !b.is_nan()
            && fields[3] == 0x7fff8000000000000000000000000000
            && flags == FpStatus::INVALID
        {
            0x7fffffffffffffffffffffffffffffff
        } else {
            fields[3]
        };
        (
            result(a.target_fma_round(b, c, rounding).unwrap()),
            Some(expected),
            flags,
        )
    } else {
        if fields.len() != 4 {
            return Err("binary operation requires four fields".into());
        }
        let (format, op) = operation.split_once('_').ok_or("missing operation")?;
        let kind = fp(format).ok_or("unknown format")?;
        let a = FloatBits::from_bits(kind, fields[0]).ok_or("overwide a")?;
        let b = FloatBits::from_bits(kind, fields[1]).ok_or("overwide b")?;
        let actual = match op {
            "add" | "sub" | "mul" | "div" => result(
                a.target_binary_round(
                    b,
                    match op {
                        "add" => FiniteOp::Add,
                        "sub" => FiniteOp::Sub,
                        "mul" => FiniteOp::Mul,
                        _ => FiniteOp::Div,
                    },
                    rounding,
                )
                .unwrap(),
            ),
            "eq" | "lt" | "le" => {
                let r = a.target_compare(b, op != "eq").unwrap();
                let yes = match op {
                    "eq" => r.value == Some(Ordering::Equal),
                    "lt" => r.value == Some(Ordering::Less),
                    _ => matches!(r.value, Some(Ordering::Less | Ordering::Equal)),
                };
                FpResult {
                    value: Some(u128::from(yes)),
                    status: r.status,
                }
            }
            _ => return Err("unsupported strict reference operation".into()),
        };
        // SoftFloat ARM-VFPv2 and the hosted AArch64 TFmode runtime differ
        // only in the default NaN fraction for invalid non-NaN arithmetic.
        // Adapt precisely that profile choice, not arbitrary NaN mismatches.
        // Native helper probes independently check this representation.
        let expected = if kind == FloatKind::F128
            && matches!(op, "add" | "sub" | "mul" | "div")
            && !a.is_nan()
            && !b.is_nan()
            && fields[2] == 0x7fff8000000000000000000000000000
            && status(fields[3])? == FpStatus::INVALID
        {
            0x7fffffffffffffffffffffffffffffff
        } else {
            fields[2]
        };
        (actual, Some(expected), status(fields[3])?)
    };
    if actual.value != expected || actual.status != expected_status {
        return Err(format!(
            "{operation} {rounding:?}: {line}; actual {actual:x?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_and_invalid_domain_are_not_ignored() {
        let r = FpRounding::TowardZero;
        assert!(check("f32_to_i32", r, "3fc00000 00000001 01").is_ok());
        assert!(check("f32_to_i32", r, "3fc00000 00000001 00").is_err());
        assert!(check("f32_to_i32", r, "7fc00000 80000000 10").is_ok());
        assert!(check("f32_to_i32", r, "7fc00000 80000000 00").is_err());
        assert!(check("f32_add", r, "00000000 00000000 00000000 20").is_err());
    }
    #[test]
    fn quad_default_nan_adaptation_does_not_hide_other_bit_or_status_changes() {
        let r = FpRounding::NearestEven;
        assert!(check("f128_div", r, "0 0 7fff8000000000000000000000000000 10").is_ok());
        assert!(check("f128_div", r, "0 0 7fff8000000000000000000000000001 10").is_err());
        assert!(check("f128_div", r, "0 0 7fff8000000000000000000000000000 00").is_err());
        assert!(check(
            "f128_add",
            r,
            "7fff8000000000000000000000000011 0 7fff8000000000000000000000000011 00"
        )
        .is_ok());
        assert!(check(
            "f128_add",
            r,
            "7fff8000000000000000000000000011 0 7fff8000000000000000000000000000 00"
        )
        .is_err());
    }
}
