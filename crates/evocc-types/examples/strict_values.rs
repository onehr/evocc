//! Check the fixed-target stream from floating-strict-target.c, including flags.
use evocc_types::{
    floating::{FiniteOp, FloatBits, FpRounding},
    FloatKind,
};
use std::{
    cmp::Ordering,
    io::{self, BufRead},
};

#[path = "strict_values/testfloat.rs"]
mod testfloat;

fn check_wide(line: &str) -> Result<(), String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 7 {
        return Err("expected format/mode/op/input/result/status/control".into());
    }
    let kind = match fields[0] {
        "32" => FloatKind::F32,
        "64" => FloatKind::F64,
        _ => return Err("unknown format".into()),
    };
    let (mode, rounding) = match fields[1] {
        "0" => (0, FpRounding::NearestEven),
        "1" => (1, FpRounding::Upward),
        "2" => (2, FpRounding::Downward),
        "3" => (3, FpRounding::TowardZero),
        _ => return Err("unknown rounding".into()),
    };
    let values = fields[3..]
        .iter()
        .map(|v| u128::from_str_radix(v, 16).map_err(|_| "invalid hex".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let (bits, status) = match fields[2] {
        "from_s" | "from_u" => {
            let r = if fields[2] == "from_s" {
                FloatBits::from_signed_round(kind, values[0] as i128, rounding)
            } else {
                FloatBits::from_unsigned_round(kind, values[0], rounding)
            };
            (r.value.bits(), r.status.bits())
        }
        "to_s" | "to_u" => {
            let r = FloatBits::from_bits(kind, values[0])
                .ok_or("overwide input")?
                .to_integer_status(128, fields[2] == "to_s")
                .ok_or("unsupported conversion")?;
            (r.value.ok_or("out-of-domain C probe")?, r.status.bits())
        }
        _ => return Err("unknown wide operation".into()),
    };
    if (bits, u128::from(status), values[3]) != (values[1], values[2], mode << 22) {
        return Err(format!("{line}: adapter={bits:032x}/{status:02x}"));
    }
    Ok(())
}

fn check(line: &str) -> Result<(), String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 8 {
        return Err("expected format/mode/op/a/b/c/result/status".into());
    }
    let kind = match fields[0] {
        "32" => FloatKind::F32,
        "64" => FloatKind::F64,
        _ => return Err("unknown format".into()),
    };
    let rounding = match fields[1] {
        "0" => FpRounding::NearestEven,
        "1" => FpRounding::Upward,
        "2" => FpRounding::Downward,
        "3" => FpRounding::TowardZero,
        _ => return Err("unknown rounding mode".into()),
    };
    let numbers = fields[3..]
        .iter()
        .map(|s| u128::from_str_radix(s, 16).map_err(|_| "invalid hex".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let a = FloatBits::from_bits(kind, numbers[0]).ok_or("overwide a")?;
    let b = FloatBits::from_bits(kind, numbers[1]).ok_or("overwide b")?;
    let c = FloatBits::from_bits(kind, numbers[2]).ok_or("overwide c")?;
    let (bits, flags) = match fields[2] {
        "0" | "1" | "2" | "3" => {
            let op = match fields[2] {
                "0" => FiniteOp::Add,
                "1" => FiniteOp::Sub,
                "2" => FiniteOp::Mul,
                _ => FiniteOp::Div,
            };
            let result = a.target_binary_round(b, op, rounding).unwrap();
            (result.value.bits(), result.status.bits())
        }
        "4" => {
            let result = a.target_fma_round(b, c, rounding).unwrap();
            (result.value.bits(), result.status.bits())
        }
        "5" | "6" => {
            let result = a.target_compare(b, fields[2] == "6").unwrap();
            let expected = if fields[2] == "5" {
                Ordering::Equal
            } else {
                Ordering::Less
            };
            (
                u128::from(result.value == Some(expected)),
                result.status.bits(),
            )
        }
        _ => return Err("unknown operation".into()),
    };
    if (bits, u128::from(flags)) != (numbers[3], numbers[4]) {
        return Err(format!("{line}: adapter={bits:x}/{flags:02x}"));
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let batch = args.as_slice() == ["--batch"];
    let wide = args.as_slice() == ["--wide"];
    let reference = match args.as_slice() {
        [] => None,
        [_] if batch || wide => None,
        [op, mode] => Some((
            op.as_str(),
            match mode.as_str() {
                "rne" => FpRounding::NearestEven,
                "up" => FpRounding::Upward,
                "down" => FpRounding::Downward,
                "zero" => FpRounding::TowardZero,
                _ => return Err("unknown reference rounding mode".into()),
            },
        )),
        _ => return Err("expected no arguments or TestFloat operation and rounding mode".into()),
    };
    let mut count = 0;
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if wide {
            check_wide(&line)?;
        } else if batch {
            let mut fields = line.splitn(3, ' ');
            let op = fields.next().ok_or("missing batch operation")?;
            let mode = match fields.next() {
                Some("rne") => FpRounding::NearestEven,
                Some("up") => FpRounding::Upward,
                Some("down") => FpRounding::Downward,
                Some("zero") => FpRounding::TowardZero,
                _ => return Err("unknown batch rounding mode".into()),
            };
            testfloat::check(op, mode, fields.next().ok_or("missing batch values")?)?;
        } else if let Some((op, mode)) = reference {
            testfloat::check(op, mode, &line)?;
        } else {
            check(&line)?;
        }
        count += 1;
    }
    if count == 0 {
        return Err("empty corpus is not acceptance".into());
    }
    println!("{count} exact values and exception statuses PASS");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_independent_result_status_and_mode_mutations() {
        assert!(check("32 0 2 00800000 3f7fffff 00000000 00800000 18").is_ok());
        assert!(check("32 0 2 00800000 3f7fffff 00000000 00800000 10").is_err());
        assert!(check("32 0 2 00800000 3f7fffff 00000000 007fffff 18").is_err());
        assert!(check("32 3 2 00800000 3f7fffff 00000000 00800000 18").is_err());
        assert!(check("32 0 2 00800000 3f7fffff 00000000 00800000 98").is_err());
        assert!(check("32 0 2 00800000 3f7fffff 00000000 00800000").is_err());
    }
}
