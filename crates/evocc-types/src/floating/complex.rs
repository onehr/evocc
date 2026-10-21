//! Complex multiplication and division of the qualified Linux runtime: the
//! helpers `__mulsc3`, `__muldc3`, `__multc3`, `__divsc3`, `__divdc3` and
//! `__divtc3` of the libgcc.a of GCC 14.2.0-19 (Debian), SHA-256
//! 60dbf748e1c310aa50300a33f30bf4ee070061071aee74a52cf115a7b8765e50.
//!
//! Each model follows its helper's instruction sequence, not only the
//! libgcc2.c source: the operand order of each operation, the fused
//! multiply-adds of the binary32 and binary64 helpers, and the comparisons
//! that execute. So the parts, the NaN signs and payloads and the status
//! flags agree bit for bit. The binary128 helpers call the soft-fp
//! operations, which the scalar binary128 rules model.
//!
//! The interpreters and constant folding use these models. The Darwin
//! helpers of compiler-rt are another sequence, which this module does not
//! model.
use super::*;

/// The complex operation of a qualified helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComplexArithmetic {
    Multiply,
    Divide,
}

/// The result of `(a + bi) op (c + di)` as `__mul?c3` or `__div?c3`
/// computes it for the format of the operands, in `rounding`. `None` for an
/// operand of another format or of a format without a helper.
pub fn libgcc_complex(
    op: ComplexArithmetic,
    [a, b, c, d]: [FloatBits; 4],
    rounding: FpRounding,
) -> Option<FpResult<[FloatBits; 2]>> {
    let kind = a.kind();
    if [b, c, d].iter().any(|value| value.kind() != kind)
        || !matches!(kind, FloatKind::F32 | FloatKind::F64 | FloatKind::F128)
    {
        return None;
    }
    let mut machine = Machine {
        rounding,
        status: FpStatus::NONE,
    };
    let value = match (op, kind) {
        (ComplexArithmetic::Multiply, _) => machine.multiply(a, b, c, d),
        (ComplexArithmetic::Divide, FloatKind::F32) => machine.divide_in_binary64(a, b, c, d),
        (ComplexArithmetic::Divide, _) => machine.divide_smith(a, b, c, d),
    };
    Some(FpResult {
        value,
        status: machine.status,
    })
}

/// One logical execution of a helper: the dynamic rounding direction and
/// the status that its executed operations raise.
struct Machine {
    rounding: FpRounding,
    status: FpStatus,
}

fn constant(kind: FloatKind, bits: u128) -> FloatBits {
    FloatBits::from_bits(kind, bits).expect("helper constant fits its format")
}

fn sign_bit(kind: FloatKind) -> u128 {
    match kind {
        FloatKind::F16 | FloatKind::BF16 => 1 << 15,
        FloatKind::F32 => 1 << 31,
        FloatKind::F64 => 1 << 63,
        FloatKind::F128 => 1 << 127,
    }
}

/// `fabs`: the sign is cleared and nothing else changes, not even a
/// signaling NaN.
fn magnitude(value: FloatBits) -> FloatBits {
    constant(value.kind(), value.bits() & !sign_bit(value.kind()))
}

/// `copysign(magnitude, sign)` as the helpers' bit insertion computes it.
fn with_sign_of(value: FloatBits, sign: FloatBits) -> FloatBits {
    let mask = sign_bit(value.kind());
    constant(value.kind(), (value.bits() & !mask) | (sign.bits() & mask))
}

fn zero(kind: FloatKind) -> FloatBits {
    constant(kind, 0)
}

fn one(kind: FloatKind) -> FloatBits {
    FloatBits::from_unsigned(kind, 1)
}

fn infinity(kind: FloatKind) -> FloatBits {
    constant(
        kind,
        match kind {
            FloatKind::F32 => 0x7f800000,
            FloatKind::F64 => 0x7ff0000000000000,
            FloatKind::F128 => 0x7fff0000000000000000000000000000,
            FloatKind::F16 => 0x7c00,
            FloatKind::BF16 => 0x7f80,
        },
    )
}

fn largest(kind: FloatKind) -> FloatBits {
    constant(
        kind,
        match kind {
            FloatKind::F32 => 0x7f7fffff,
            FloatKind::F64 => 0x7fefffffffffffff,
            FloatKind::F128 => 0x7ffeffffffffffffffffffffffffffff,
            FloatKind::F16 => 0x7bff,
            FloatKind::BF16 => 0x7f7f,
        },
    )
}

/// The scaling constants of libgcc2.c's DIVC3: RBIG, RMIN, RMIN2, RMAX2
/// and RMINSCAL.
struct Scaling {
    big: FloatBits,
    min: FloatBits,
    min2: FloatBits,
    max2: FloatBits,
    scale: FloatBits,
}

fn scaling(kind: FloatKind) -> Scaling {
    let value = |bits| constant(kind, bits);
    if kind == FloatKind::F64 {
        Scaling {
            big: value(0x7fdfffffffffffff),
            min: value(0x0010000000000000),
            min2: value(0x3cb0000000000000),
            max2: value(0x7c9fffffffffffff),
            scale: value(0x4330000000000000),
        }
    } else {
        Scaling {
            big: value(0x7ffdffffffffffffffffffffffffffff),
            min: value(0x00010000000000000000000000000000),
            min2: value(0x3f8f0000000000000000000000000000),
            max2: value(0x7f8dffffffffffffffffffffffffffff),
            scale: value(0x406f0000000000000000000000000000),
        }
    }
}

impl Machine {
    fn record<T>(&mut self, result: FpResult<T>) -> T {
        self.status = self.status.union(result.status);
        result.value
    }

    fn binary(&mut self, a: FloatBits, b: FloatBits, op: FiniteOp) -> FloatBits {
        let result = a
            .target_binary_round(b, op, self.rounding)
            .expect("helper operands share an executable format");
        self.record(result)
    }

    fn add(&mut self, a: FloatBits, b: FloatBits) -> FloatBits {
        self.binary(a, b, FiniteOp::Add)
    }

    fn sub(&mut self, a: FloatBits, b: FloatBits) -> FloatBits {
        self.binary(a, b, FiniteOp::Sub)
    }

    fn mul(&mut self, a: FloatBits, b: FloatBits) -> FloatBits {
        self.binary(a, b, FiniteOp::Mul)
    }

    fn div(&mut self, a: FloatBits, b: FloatBits) -> FloatBits {
        self.binary(a, b, FiniteOp::Div)
    }

    /// `n * m + addend` rounded once, as FMADD computes it. FMSUB and
    /// FNMSUB negate one operand first, NaN included, so their callers pass
    /// the negated operand.
    fn fma(&mut self, n: FloatBits, m: FloatBits, addend: FloatBits) -> FloatBits {
        let result = n
            .target_fma_round(m, addend, self.rounding)
            .expect("helper operands share an executable format");
        self.record(result)
    }

    fn convert(&mut self, value: FloatBits, kind: FloatKind) -> FloatBits {
        let result = value
            .target_convert_round(kind, self.rounding)
            .expect("helper formats are executable");
        self.record(result)
    }

    fn compare(&mut self, a: FloatBits, b: FloatBits, signaling: bool) -> Option<Ordering> {
        let result = a
            .target_compare(b, signaling)
            .expect("helper operands share an executable format");
        self.record(result)
    }

    /// FCMPE, or a soft-fp `__lttf2`, `__getf2` or `__gttf2`: invalid for
    /// any NaN.
    fn less(&mut self, a: FloatBits, b: FloatBits) -> bool {
        self.compare(a, b, true) == Some(Ordering::Less)
    }

    fn greater_equal(&mut self, a: FloatBits, b: FloatBits) -> bool {
        matches!(
            self.compare(a, b, true),
            Some(Ordering::Greater | Ordering::Equal)
        )
    }

    fn greater(&mut self, a: FloatBits, b: FloatBits) -> bool {
        self.compare(a, b, true) == Some(Ordering::Greater)
    }

    /// FCMP x, x, or `__unordtf2(x, x)`: invalid only for a signaling NaN.
    fn is_nan(&mut self, value: FloatBits) -> bool {
        self.compare(value, value, false).is_none()
    }

    /// `|x| > max`, a quiet comparison in every helper.
    fn is_infinite(&mut self, value: FloatBits) -> bool {
        self.compare(magnitude(value), largest(value.kind()), false) == Some(Ordering::Greater)
    }

    /// `|x| <= max`, a quiet comparison in every helper.
    fn is_finite(&mut self, value: FloatBits) -> bool {
        matches!(
            self.compare(magnitude(value), largest(value.kind()), false),
            Some(Ordering::Less | Ordering::Equal)
        )
    }

    /// FCMP with zero, or `__eqtf2`.
    fn is_zero(&mut self, value: FloatBits) -> bool {
        self.compare(value, zero(value.kind()), false) == Some(Ordering::Equal)
    }

    /// `copysign(isinf(x) ? 1 : 0, x)`.
    fn boxed(&mut self, value: FloatBits) -> FloatBits {
        let kind = value.kind();
        let unit = if self.is_infinite(value) {
            one(kind)
        } else {
            zero(kind)
        };
        with_sign_of(unit, value)
    }

    /// `if (isnan(x)) x = copysign(0, x)`.
    fn nan_to_zero(&mut self, value: FloatBits) -> FloatBits {
        if self.is_nan(value) {
            with_sign_of(zero(value.kind()), value)
        } else {
            value
        }
    }

    /// MULC3. The binary32 and binary64 helpers fuse the recomputation of
    /// their recovery path; the binary128 one calls soft-fp operations.
    fn multiply(
        &mut self,
        a: FloatBits,
        b: FloatBits,
        c: FloatBits,
        d: FloatBits,
    ) -> [FloatBits; 2] {
        let kind = a.kind();
        let ac = self.mul(a, c);
        let bd = self.mul(b, d);
        let ad = self.mul(a, d);
        // The helpers compute b * c as c * b, so a NaN c wins over a NaN b.
        let bc = self.mul(c, b);
        let x = self.sub(ac, bd);
        let y = self.add(ad, bc);
        if !(self.is_nan(x) && self.is_nan(y)) {
            return [x, y];
        }
        let (mut a, mut b, mut c, mut d) = (a, b, c, d);
        let mut recalculate = false;
        if self.is_infinite(a) || self.is_infinite(b) {
            a = self.boxed(a);
            b = self.boxed(b);
            c = self.nan_to_zero(c);
            d = self.nan_to_zero(d);
            recalculate = true;
        }
        if self.is_infinite(c) || self.is_infinite(d) {
            c = self.boxed(c);
            d = self.boxed(d);
            a = self.nan_to_zero(a);
            b = self.nan_to_zero(b);
            recalculate = true;
        }
        if !recalculate
            && (self.is_infinite(ac)
                || self.is_infinite(bd)
                || self.is_infinite(ad)
                || self.is_infinite(bc))
        {
            a = self.nan_to_zero(a);
            b = self.nan_to_zero(b);
            c = self.nan_to_zero(c);
            d = self.nan_to_zero(d);
            recalculate = true;
        }
        if !recalculate {
            return [x, y];
        }
        let infinity = infinity(kind);
        let (x, y) = if kind == FloatKind::F128 {
            let ac = self.mul(a, c);
            let bd = self.mul(b, d);
            let x = self.sub(ac, bd);
            let ad = self.mul(a, d);
            let bc = self.mul(b, c);
            (x, self.add(ad, bc))
        } else {
            let bd = self.mul(b, d);
            let bc = self.mul(b, c);
            // FNMSUB a, c, bd and FMADD a, d, bc.
            let x = self.fma(a, c, bd.negated());
            (x, self.fma(a, d, bc))
        };
        [self.mul(x, infinity), self.mul(y, infinity)]
    }

    /// `__divsc3`: with hardware binary64, the plain formula in binary64
    /// with fused sums, one conversion per part, then the binary32 recovery.
    fn divide_in_binary64(
        &mut self,
        a: FloatBits,
        b: FloatBits,
        c: FloatBits,
        d: FloatBits,
    ) -> [FloatBits; 2] {
        let wide = FloatKind::F64;
        let bb = self.convert(b, wide);
        let dd = self.convert(d, wide);
        let aa = self.convert(a, wide);
        let cc = self.convert(c, wide);
        let bd = self.mul(bb, dd);
        let dd2 = self.mul(dd, dd);
        let denominator = self.fma(cc, cc, dd2);
        let real = self.fma(aa, cc, bd);
        let ad = self.mul(aa, dd);
        let imaginary = self.fma(bb, cc, ad.negated());
        let real = self.div(real, denominator);
        let imaginary = self.div(imaginary, denominator);
        let x = self.convert(real, FloatKind::F32);
        let y = self.convert(imaginary, FloatKind::F32);
        self.recover(a, b, c, d, x, y)
    }

    /// `__divdc3` and `__divtc3`: Smith's method with libgcc's scaling. The
    /// binary64 helper fuses each product-plus-term; the binary128 one calls
    /// soft-fp operations in the source order.
    fn divide_smith(
        &mut self,
        a: FloatBits,
        b: FloatBits,
        c: FloatBits,
        d: FloatBits,
    ) -> [FloatBits; 2] {
        let kind = a.kind();
        let fused = kind == FloatKind::F64;
        let limits = scaling(kind);
        let half = FloatBits::parse(kind, "0.5").expect("one half");
        let (mut a, mut b, mut c, mut d) = (a, b, c, d);
        let smaller = self.less(magnitude(c), magnitude(d));
        // The larger of |c| and |d| decides the scaling, and the ratio is the
        // smaller over the larger.
        let large = if smaller { d } else { c };
        let large = if self.greater_equal(magnitude(large), limits.big) {
            a = self.mul(a, half);
            b = self.mul(b, half);
            c = self.mul(c, half);
            d = self.mul(d, half);
            if smaller {
                d
            } else {
                c
            }
        } else {
            large
        };
        let scale_up = if self.less(magnitude(large), limits.min2) {
            true
        } else if self.less(magnitude(a), limits.min) {
            self.less(magnitude(b), limits.max2) && self.less(magnitude(large), limits.max2)
        } else if self.less(magnitude(b), limits.min) {
            self.less(magnitude(a), limits.max2) && self.less(magnitude(large), limits.max2)
        } else {
            false
        };
        if scale_up {
            a = self.mul(a, limits.scale);
            b = self.mul(b, limits.scale);
            c = self.mul(c, limits.scale);
            d = self.mul(d, limits.scale);
        }
        let (x, y) = if smaller {
            let ratio = self.div(c, d);
            let denominator = if fused {
                self.fma(c, ratio, d)
            } else {
                let product = self.mul(c, ratio);
                self.add(product, d)
            };
            let (x, y) = if self.greater(magnitude(ratio), limits.min) {
                if fused {
                    let x = self.fma(a, ratio, b);
                    (x, self.fma(b, ratio, a.negated()))
                } else {
                    let product = self.mul(a, ratio);
                    let x = self.add(product, b);
                    let product = self.mul(b, ratio);
                    (x, self.sub(product, a))
                }
            } else if fused {
                let first = self.div(a, d);
                let second = self.div(b, d);
                let x = self.fma(first, c, b);
                (x, self.fma(second, c, a.negated()))
            } else {
                let quotient = self.div(a, d);
                let product = self.mul(quotient, c);
                let x = self.add(product, b);
                let quotient = self.div(b, d);
                let product = self.mul(quotient, c);
                (x, self.sub(product, a))
            };
            (self.div(x, denominator), self.div(y, denominator))
        } else {
            let ratio = self.div(d, c);
            let denominator = if fused {
                self.fma(d, ratio, c)
            } else {
                let product = self.mul(d, ratio);
                self.add(product, c)
            };
            let (x, y) = if self.greater(magnitude(ratio), limits.min) {
                if fused {
                    let x = self.fma(b, ratio, a);
                    // FMSUB a, ratio, b: b - a * ratio.
                    (x, self.fma(a.negated(), ratio, b))
                } else {
                    let product = self.mul(b, ratio);
                    let x = self.add(product, a);
                    let product = self.mul(a, ratio);
                    (x, self.sub(b, product))
                }
            } else if fused {
                let first = self.div(b, c);
                let second = self.div(a, c);
                let x = self.fma(first, d, a);
                (x, self.fma(second.negated(), d, b))
            } else {
                let quotient = self.div(b, c);
                let product = self.mul(quotient, d);
                let x = self.add(product, a);
                let quotient = self.div(a, c);
                let product = self.mul(quotient, d);
                (x, self.sub(b, product))
            };
            (self.div(x, denominator), self.div(y, denominator))
        };
        self.recover(a, b, c, d, x, y)
    }

    /// The common recovery of DIVC3: infinities and zeros that computed as
    /// NaN + iNaN. `a`, `b`, `c` and `d` are the scaled operands.
    fn recover(
        &mut self,
        a: FloatBits,
        b: FloatBits,
        c: FloatBits,
        d: FloatBits,
        x: FloatBits,
        y: FloatBits,
    ) -> [FloatBits; 2] {
        if !(self.is_nan(x) && self.is_nan(y)) {
            return [x, y];
        }
        let kind = a.kind();
        let infinity = infinity(kind);
        if self.is_zero(d) && self.is_zero(c) {
            if self.is_nan(a) && self.is_nan(b) {
                return [x, y];
            }
            let signed = with_sign_of(infinity, c);
            return [self.mul(signed, a), self.mul(signed, b)];
        }
        if (self.is_infinite(a) || self.is_infinite(b)) && self.is_finite(c) && self.is_finite(d) {
            let a = self.boxed(a);
            let b = self.boxed(b);
            let (x, y) = if kind == FloatKind::F128 {
                let first = self.mul(c, a);
                let second = self.mul(d, b);
                let x = self.add(first, second);
                let first = self.mul(c, b);
                let second = self.mul(d, a);
                (x, self.sub(first, second))
            } else {
                let da = self.mul(d, a);
                let db = self.mul(d, b);
                // FNMSUB c, b, da and FMADD c, a, db.
                let y = self.fma(c, b, da.negated());
                (self.fma(c, a, db), y)
            };
            return [self.mul(x, infinity), self.mul(y, infinity)];
        }
        if (self.is_infinite(c) || self.is_infinite(d)) && self.is_finite(a) && self.is_finite(b) {
            let c = self.boxed(c);
            let d = self.boxed(d);
            let (x, y) = if kind == FloatKind::F128 {
                let first = self.mul(a, c);
                let second = self.mul(b, d);
                let x = self.add(first, second);
                let first = self.mul(b, c);
                let second = self.mul(a, d);
                (x, self.sub(first, second))
            } else {
                let bd = self.mul(b, d);
                let ad = self.mul(a, d);
                // FMADD a, c, bd and FNMSUB b, c, ad.
                let x = self.fma(a, c, bd);
                (x, self.fma(b, c, ad.negated()))
            };
            let zero = zero(kind);
            return [self.mul(x, zero), self.mul(y, zero)];
        }
        [x, y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grid of tests/fixtures/floating-complex-grid.c, hashed as that
    /// program hashes it.
    fn grid_values(kind: FloatKind) -> Vec<FloatBits> {
        let (subnormal, quiet, payload_positive, payload_negative) = match kind {
            FloatKind::F32 => (1u128, 1u128 << 22, 0x123, 0x456),
            FloatKind::F64 => (1, 1 << 51, 0x123, 0x456),
            _ => (1, 1 << 111, 0x123, 0x456),
        };
        let nan = |payload| constant(kind, infinity(kind).bits() | quiet | payload);
        let three = FloatBits::from_unsigned(kind, 3);
        [
            zero(kind),
            zero(kind).negated(),
            constant(kind, subnormal),
            constant(kind, subnormal).negated(),
            one(kind),
            one(kind).negated(),
            three,
            three.negated(),
            largest(kind),
            largest(kind).negated(),
            infinity(kind),
            infinity(kind).negated(),
            nan(payload_positive),
            nan(payload_negative).negated(),
        ]
        .to_vec()
    }

    fn mix(hash: u64, word: u64) -> u64 {
        (hash ^ word).wrapping_mul(0x100000001b3)
    }

    fn part(mut hash: u64, value: FloatBits) -> u64 {
        hash = mix(hash, value.bits() as u64);
        if value.kind() == FloatKind::F128 {
            hash = mix(hash, (value.bits() >> 64) as u64);
        }
        hash
    }

    /// The operand sets of the grid, or of its sample: the `F(i, j, k, l)`
    /// entries of the fixture's `SAMPLE_CELLS`.
    fn grid_cells(sample: bool) -> Vec<[usize; 4]> {
        if !sample {
            return (0..14usize.pow(4))
                .map(|index| [index / 2744, index / 196 % 14, index / 14 % 14, index % 14])
                .collect();
        }
        let source = include_str!("../../../../tests/fixtures/floating-complex-grid.c");
        let (_, cells) = source.split_once("#define SAMPLE_CELLS(F)").unwrap();
        let (cells, _) = cells.split_once("\n\n").unwrap();
        cells
            .split("F(")
            .skip(1)
            .map(|cell| {
                let (cell, _) = cell.split_once(')').unwrap();
                let parts = cell
                    .split(',')
                    .map(|part| part.trim().parse().unwrap())
                    .collect::<Vec<usize>>();
                [parts[0], parts[1], parts[2], parts[3]]
            })
            .collect()
    }

    fn grid_hashes(
        kind: FloatKind,
        op: ComplexArithmetic,
        rounding: FpRounding,
        sample: bool,
    ) -> (u64, u64) {
        let values = grid_values(kind);
        let (mut bits, mut flags) = (0xcbf29ce484222325, 0xcbf29ce484222325);
        for [i, j, k, l] in grid_cells(sample) {
            let operands = [values[i], values[j], values[k], values[l]];
            let result = libgcc_complex(op, operands, rounding).unwrap();
            bits = part(bits, result.value[0]);
            bits = part(bits, result.value[1]);
            flags = mix(flags, u64::from(result.status.bits()));
        }
        (bits, flags)
    }

    /// The models give the recorded hashes of the libgcc helpers on the
    /// special-value grid and on its sample, in all four rounding modes.
    #[test]
    fn grid_hashes_match_the_recorded_libgcc_helpers() {
        let source = include_str!("../../../../tests/fixtures/floating-complex-grid.out");
        let (_, records) = source.split_once("\nrecords:\n").unwrap();
        let mut checked = 0;
        for line in records.lines() {
            let words = line.split(' ').collect::<Vec<_>>();
            let (selection, kind) = words[0].split_once('=').unwrap();
            let sample = selection == "S";
            let kind = match kind {
                "32" => FloatKind::F32,
                "64" => FloatKind::F64,
                _ => FloatKind::F128,
            };
            let rounding = match words[2] {
                "0" => FpRounding::NearestEven,
                "1" => FpRounding::Upward,
                "2" => FpRounding::Downward,
                _ => FpRounding::TowardZero,
            };
            let hex = |text: &str| u64::from_str_radix(text, 16).unwrap();
            for (op, bits, flags) in [
                (ComplexArithmetic::Multiply, words[4], words[5]),
                (ComplexArithmetic::Divide, words[7], words[8]),
            ] {
                assert_eq!(
                    grid_hashes(kind, op, rounding, sample),
                    (hex(bits), hex(flags)),
                    "{line} {op:?}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 48);
        assert_eq!(grid_cells(true).len(), 50);
    }

    #[test]
    fn exact_quotients_and_the_zero_divisor_flags() {
        let value = |kind, text| FloatBits::parse(kind, text).unwrap();
        for kind in [FloatKind::F32, FloatKind::F64, FloatKind::F128] {
            let operands = ["9", "7", "2", "1"].map(|text| value(kind, text));
            let result =
                libgcc_complex(ComplexArithmetic::Divide, operands, FpRounding::NearestEven)
                    .unwrap();
            assert_eq!(result.value, [value(kind, "5"), value(kind, "1")]);
            assert_eq!(result.status, FpStatus::NONE);
            let operands = ["1", "1", "0", "0"].map(|text| value(kind, text));
            let result =
                libgcc_complex(ComplexArithmetic::Divide, operands, FpRounding::NearestEven)
                    .unwrap();
            assert_eq!(result.value, [infinity(kind), infinity(kind)]);
            let expected = if kind == FloatKind::F32 {
                FpStatus::INVALID
            } else {
                FpStatus::INVALID.union(FpStatus::DIV_BY_ZERO)
            };
            assert_eq!(result.status, expected, "{kind:?}");
        }
        let mixed = [
            FloatBits::parse(FloatKind::F32, "1").unwrap(),
            FloatBits::parse(FloatKind::F64, "1").unwrap(),
            FloatBits::parse(FloatKind::F64, "1").unwrap(),
            FloatBits::parse(FloatKind::F64, "1").unwrap(),
        ];
        assert!(
            libgcc_complex(ComplexArithmetic::Multiply, mixed, FpRounding::NearestEven).is_none()
        );
    }
}
