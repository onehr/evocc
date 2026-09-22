//! Stream `testfloat_gen <operation>` through the production numeric engine.
//! Baseline FP has no observable exception flags. Only FP-to-int uses INVALID
//! to compare the C conversion domain; every defined result, including NaN
//! payloads and signed zero, is compared by exact bits. No tolerance or skips.
use evocc_types::floating::{FiniteOp, FloatBits};
use evocc_types::FloatKind;
use std::cmp::Ordering;
use std::io::{self, BufRead};

#[derive(Clone, Copy)]
enum Format {
    Float(FloatKind),
    Integer { width: u16, signed: bool },
}

fn format(name: &str) -> Result<Format, String> {
    Ok(match name {
        "f32" => Format::Float(FloatKind::F32),
        "f64" => Format::Float(FloatKind::F64),
        "i32" => Format::Integer {
            width: 32,
            signed: true,
        },
        "i64" => Format::Integer {
            width: 64,
            signed: true,
        },
        "ui32" => Format::Integer {
            width: 32,
            signed: false,
        },
        "ui64" => Format::Integer {
            width: 64,
            signed: false,
        },
        _ => return Err(format!("unsupported TestFloat format {name}")),
    })
}

fn conversion(source: Format, target: Format, bits: u128) -> Result<Option<u128>, String> {
    Ok(match (source, target) {
        (Format::Float(kind), Format::Float(to)) => FloatBits::from_bits(kind, bits)
            .and_then(|v| v.target_convert(to))
            .map(FloatBits::bits),
        (Format::Float(kind), Format::Integer { width, signed }) => {
            FloatBits::from_bits(kind, bits)
                .and_then(|v| v.to_integer(width, signed))
                .map(|v| v & ((1_u128 << width) - 1))
        }
        (Format::Integer { width, signed }, Format::Float(kind)) => {
            if bits >> width != 0 {
                return Err("overwide integer input".into());
            }
            let value = if signed {
                FloatBits::from_signed(kind, ((bits << (128 - width)) as i128) >> (128 - width))
            } else {
                FloatBits::from_unsigned(kind, bits)
            };
            Some(value.bits())
        }
        _ => return Err("not a floating conversion".into()),
    })
}

fn check_line(operation: &str, line: &str) -> Result<bool, String> {
    let values = line
        .split_whitespace()
        .map(|v| u128::from_str_radix(v, 16).map_err(|_| format!("invalid hexadecimal field: {v}")))
        .collect::<Result<Vec<_>, _>>()?;
    let (actual, expected) = if let Some((from, to)) = operation.split_once("_to_") {
        if values.len() != 3 {
            return Err("conversion needs input/result/flags".into());
        }
        let target = format(to)?;
        let invalid = matches!(target, Format::Integer { .. }) && values[2] & 16 != 0;
        (
            conversion(format(from)?, target, values[0])?,
            (!invalid).then_some(values[1]),
        )
    } else {
        if values.len() != 4 {
            return Err("binary operation needs two inputs/result/flags".into());
        }
        let (from, op) = operation.split_once('_').ok_or("missing operation")?;
        let Format::Float(kind) = format(from)? else {
            return Err("nonfloating operand".into());
        };
        let a = FloatBits::from_bits(kind, values[0]).ok_or("overwide first input")?;
        let b = FloatBits::from_bits(kind, values[1]).ok_or("overwide second input")?;
        let result = match op {
            "add" => a.target_binary(b, FiniteOp::Add).unwrap().bits(),
            "sub" => a.target_binary(b, FiniteOp::Sub).unwrap().bits(),
            "mul" => a.target_binary(b, FiniteOp::Mul).unwrap().bits(),
            "div" => a.target_binary(b, FiniteOp::Div).unwrap().bits(),
            "eq" => u128::from(a.compare(b) == Some(Ordering::Equal)),
            "lt" => u128::from(a.compare(b) == Some(Ordering::Less)),
            "le" => u128::from(matches!(
                a.compare(b),
                Some(Ordering::Less | Ordering::Equal)
            )),
            _ => return Err(format!("unsupported operation {operation}")),
        };
        (Some(result), Some(values[2]))
    };
    if values.last().copied().unwrap_or(32) > 31 {
        return Err("invalid exception field".into());
    }
    if actual != expected {
        return Err(format!(
            "{operation}: {line}; observed {actual:x?}, expected {expected:x?}"
        ));
    }
    Ok(actual.is_none())
}

fn main() -> Result<(), String> {
    let operation = std::env::args()
        .nth(1)
        .ok_or("expected TestFloat operation")?;
    let mut count = 0_u64;
    let mut invalid = 0_u64;
    let mut line = String::new();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    loop {
        line.clear();
        if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        invalid += u64::from(check_line(&operation, &line)?);
        count += 1;
    }
    if count == 0 {
        return Err("empty corpus is not acceptance".into());
    }
    println!(
        "{operation}: {count} exact cases PASS ({invalid} invalid integer conversions rejected)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_and_conversion_domain_are_checked_independently() {
        assert_eq!(
            check_line("f32_add", "3f800000 3f800000 40000000 00"),
            Ok(false)
        );
        assert!(check_line("f32_add", "3f800000 3f800000 40000001 00").is_err());
        assert_eq!(check_line("f32_to_i32", "7fc00000 80000000 10"), Ok(true));
        assert!(check_line("f32_to_i32", "7fc00000 80000000 00").is_err());
        assert!(check_line("f32_to_i32", "3f800000 00000001 10").is_err());
        assert_eq!(
            check_line("f64_to_i32", "bff8000000000000 ffffffff 01"),
            Ok(false)
        );
        assert_eq!(
            check_line("i32_to_f64", "ffffffff bff0000000000000 00"),
            Ok(false)
        );
        assert!(check_line("f32_add", "0 0 0").is_err());
        assert!(check_line("f32_to_f64", "NaN 0 0").is_err());
    }
}
