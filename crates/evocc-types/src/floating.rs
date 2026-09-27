//! Host-independent representations and arithmetic. The finite constant API is
//! distinct from the binary32/binary64 target API (RNE, FZ=DN=0), which explicitly
//! applies AArch64 signaling-NaN priority rather than inheriting the engine's
//! generic NaN selection. Neither API changes the process floating environment.

use crate::FloatKind;
use rustc_apfloat::ieee::{Double, Half, Quad, Single};
use rustc_apfloat::{Float, FloatConvert, Round, Status};
use std::cmp::Ordering;

const ROUND: Round = Round::NearestTiesToEven;

/// Representation identity, not C numerical equality (+0 differs from -0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FloatBits {
    kind: FloatKind,
    bits: u128,
}

#[derive(Debug, Clone, Copy)]
pub enum FiniteOp {
    Add,
    Sub,
    Mul,
    Div,
}

macro_rules! with_format {
    ($kind:expr, $format:ident, $expression:expr) => {
        match $kind {
            FloatKind::F16 => {
                type $format = Half;
                $expression
            }
            FloatKind::F32 => {
                type $format = Single;
                $expression
            }
            FloatKind::F64 => {
                type $format = Double;
                $expression
            }
            FloatKind::F128 => {
                type $format = Quad;
                $expression
            }
        }
    };
}

mod environment;
pub use environment::{FpEnvironment, FpResult, FpRounding, FpStatus};

impl FloatBits {
    pub fn from_bits(kind: FloatKind, bits: u128) -> Option<Self> {
        let width = match kind {
            FloatKind::F16 => 16,
            FloatKind::F32 => 32,
            FloatKind::F64 => 64,
            FloatKind::F128 => 128,
        };
        if width < 128 && bits >> width != 0 {
            return None;
        }
        Some(Self { kind, bits })
    }

    /// `text` is already a lexically validated numeric spelling without suffix.
    /// Rounding occurs directly in the source format, never via host double.
    pub fn parse(kind: FloatKind, text: &str) -> Option<Self> {
        let bits = with_format!(kind, F, F::from_str_r(text, ROUND).ok()?.value.to_bits());
        Some(Self { kind, bits })
    }

    pub const fn kind(self) -> FloatKind {
        self.kind
    }
    pub const fn bits(self) -> u128 {
        self.bits
    }

    pub fn is_finite(self) -> bool {
        with_format!(self.kind, F, F::from_bits(self.bits).is_finite())
    }

    pub fn is_zero(self) -> bool {
        with_format!(self.kind, F, F::from_bits(self.bits).is_zero())
    }

    pub fn is_nan(self) -> bool {
        with_format!(self.kind, F, F::from_bits(self.bits).is_nan())
    }

    fn quiet_bit(self) -> u128 {
        1u128
            << match self.kind {
                FloatKind::F16 => 9,
                FloatKind::F32 => 22,
                FloatKind::F64 => 51,
                FloatKind::F128 => 111,
            }
    }

    /// Representation classification only; it neither quiets the NaN nor raises status.
    pub fn is_signaling(self) -> bool {
        self.is_nan() && self.bits & self.quiet_bit() == 0
    }

    fn quieted(self) -> Self {
        Self {
            bits: self.bits | self.quiet_bit(),
            ..self
        }
    }

    /// Operand order is semantic: signaling NaNs first, then the first quiet
    /// NaN. Invalid non-NaN arithmetic produces the positive default NaN.
    pub fn target_binary(self, other: Self, op: FiniteOp) -> Option<Self> {
        self.target_binary_round(other, op, FpRounding::NearestEven)
            .map(|result| result.value)
    }

    /// Numerical format conversion (not object copying). Widening/narrowing a
    /// NaN preserves sign and the most significant payload bits and quiets it.
    pub fn target_convert(self, kind: FloatKind) -> Option<Self> {
        if !matches!(self.kind, FloatKind::F32 | FloatKind::F64 | FloatKind::F128)
            || !matches!(kind, FloatKind::F32 | FloatKind::F64 | FloatKind::F128)
        {
            return None;
        }
        if kind == self.kind || self.is_finite() {
            return self.convert(kind);
        }
        let layout = |kind| match kind {
            FloatKind::F32 => (31, 23, 0x7f800000),
            FloatKind::F64 => (63, 52, 0x7ff0000000000000),
            FloatKind::F128 => (127, 112, 0x7fff0000000000000000000000000000),
            _ => unreachable!("admitted scalar format"),
        };
        let (source_sign, source_fraction, _) = layout(self.kind);
        let (sign, fraction, exponent) = layout(kind);
        let mut payload = self.bits & ((1u128 << source_fraction) - 1);
        if fraction >= source_fraction {
            payload <<= fraction - source_fraction;
        } else {
            payload >>= source_fraction - fraction;
        }
        let bits = ((self.bits >> source_sign) << sign)
            | exponent
            | if self.is_nan() {
                payload | (1u128 << (fraction - 1))
            } else {
                0
            };
        Some(Self { kind, bits })
    }

    /// Exact sign-bit operation; in particular, this does not quiet an sNaN.
    pub fn negated(self) -> Self {
        let sign = match self.kind {
            FloatKind::F16 => 15,
            FloatKind::F32 => 31,
            FloatKind::F64 => 63,
            FloatKind::F128 => 127,
        };
        Self {
            bits: self.bits ^ (1u128 << sign),
            ..self
        }
    }

    pub fn from_signed(kind: FloatKind, value: i128) -> Self {
        let bits = with_format!(kind, F, F::from_i128_r(value, ROUND).value.to_bits());
        Self { kind, bits }
    }

    pub fn from_unsigned(kind: FloatKind, value: u128) -> Self {
        let bits = with_format!(kind, F, F::from_u128_r(value, ROUND).value.to_bits());
        Self { kind, bits }
    }

    /// Finite format conversion only; same-format transport preserves all bits.
    pub fn convert(self, kind: FloatKind) -> Option<Self> {
        if self.kind == kind {
            return Some(self);
        }
        if !self.is_finite() {
            return None;
        }
        fn convert<F>(value: F, kind: FloatKind) -> u128
        where
            F: Float
                + FloatConvert<Half>
                + FloatConvert<Single>
                + FloatConvert<Double>
                + FloatConvert<Quad>,
        {
            with_format!(kind, T, {
                let converted: rustc_apfloat::StatusAnd<T> = value.convert_r(ROUND, &mut false);
                converted.value.to_bits()
            })
        }
        let bits = with_format!(self.kind, F, convert(F::from_bits(self.bits), kind));
        Some(Self { kind, bits })
    }

    /// C truncation domain check, never the engine's invalid-conversion fallback.
    pub fn to_integer(self, width: u16, signed: bool) -> Option<u128> {
        if !(1..=128).contains(&width) || !self.is_finite() {
            return None;
        }
        let result = with_format!(self.kind, F, {
            let value = F::from_bits(self.bits);
            if signed {
                value
                    .to_i128_r(usize::from(width), Round::TowardZero, &mut false)
                    .map(|result| result as u128)
            } else {
                value.to_u128_r(usize::from(width), Round::TowardZero, &mut false)
            }
        });
        (!result.status.contains(Status::INVALID_OP)).then_some(result.value)
    }

    /// Numerical comparison. None is unordered or a mismatched format.
    pub fn compare(self, other: Self) -> Option<Ordering> {
        if self.kind != other.kind {
            return None;
        }
        with_format!(
            self.kind,
            F,
            F::from_bits(self.bits).partial_cmp(&F::from_bits(other.bits))
        )
    }

    pub fn finite_binary(self, other: Self, op: FiniteOp) -> Option<Self> {
        if self.kind != other.kind || !self.is_finite() || !other.is_finite() {
            return None;
        }
        let bits = with_format!(self.kind, F, {
            let a = F::from_bits(self.bits);
            let b = F::from_bits(other.bits);
            match op {
                FiniteOp::Add => a.add_r(b, ROUND),
                FiniteOp::Sub => a.sub_r(b, ROUND),
                FiniteOp::Mul => a.mul_r(b, ROUND),
                FiniteOp::Div => a.div_r(b, ROUND),
            }
            .value
            .to_bits()
        });
        let result = Self { bits, ..self };
        result.is_finite().then_some(result)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sealed_aarch64_arithmetic_vectors_match_exactly() {
        use super::{FiniteOp, FloatBits};
        use crate::FloatKind;
        let source = include_str!("../../../tests/fixtures/floating-engine-vectors.out");
        let mut count = 0;
        for line in source.lines() {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            assert_eq!(parts.len(), 5);
            let kind = match parts[0] {
                "32" => FloatKind::F32,
                "64" => FloatKind::F64,
                _ => panic!("unknown format"),
            };
            let op = match parts[1] {
                "0" => FiniteOp::Add,
                "1" => FiniteOp::Sub,
                "2" => FiniteOp::Mul,
                "3" => FiniteOp::Div,
                _ => panic!("unknown operation"),
            };
            let bits = |s| u128::from_str_radix(s, 16).unwrap();
            let a = FloatBits::from_bits(kind, bits(parts[2])).unwrap();
            let b = FloatBits::from_bits(kind, bits(parts[3])).unwrap();
            assert_eq!(
                a.target_binary(b, op).unwrap().bits(),
                bits(parts[4]),
                "{line}"
            );
            count += 1;
        }
        assert_eq!(count, 3072);
    }
    use super::*;

    #[test]
    fn direct_rounding_and_subnormal_boundaries() {
        for (text, bits) in [
            ("1.0000000596046447753906250000000000000001", 0x3f800001),
            ("1.000000059604644775390625", 0x3f800000),
            ("1.0000001788139343261718749999999999999999", 0x3f800001),
            ("1.000000178813934326171875", 0x3f800002),
            ("0x1.00000100000000000001p0", 0x3f800001),
            ("0x1p-149", 1),
            ("0x1p-150", 0),
            ("0x1.fffffep127", 0x7f7fffff),
        ] {
            assert_eq!(
                FloatBits::parse(FloatKind::F32, text).unwrap().bits(),
                bits,
                "{text}"
            );
        }
        assert_eq!(
            FloatBits::parse(FloatKind::F64, "0x1p-1074")
                .unwrap()
                .bits(),
            1
        );
    }

    #[test]
    fn bit_identity_is_not_numerical_equality() {
        let zero = FloatBits::parse(FloatKind::F64, "0").unwrap();
        assert_ne!(zero, zero.negated());
        assert_eq!(zero.compare(zero.negated()), Some(Ordering::Equal));
        let snan = FloatBits::from_bits(FloatKind::F64, 0x7ff0000000000011).unwrap();
        assert_eq!(snan.negated().bits(), 0xfff0000000000011);
        assert_eq!(snan.compare(snan), None);
        assert_eq!(snan.finite_binary(zero, FiniteOp::Add), None);
        assert_eq!(zero.finite_binary(zero, FiniteOp::Div), None);
    }

    #[test]
    fn conversions_use_full_integer_width_and_never_saturate() {
        let high = FloatBits::from_unsigned(FloatKind::F64, 1u128 << 127);
        assert_eq!(high.bits(), 0x47e0000000000000);
        assert_eq!(high.to_integer(128, false), Some(1u128 << 127));
        assert_eq!(high.to_integer(128, true), None);
        assert_eq!(high.to_integer(64, false), None);
        let low = FloatBits::from_signed(FloatKind::F64, i128::MIN);
        assert_eq!(low.to_integer(128, true), Some(i128::MIN as u128));
        let negative_fraction = FloatBits::parse(FloatKind::F64, "-0.75").unwrap();
        assert_eq!(negative_fraction.to_integer(32, false), Some(0));
        let outside = FloatBits::parse(FloatKind::F64, "4294967296").unwrap();
        assert_eq!(outside.to_integer(32, false), None);
        assert_eq!(
            FloatBits::parse(FloatKind::F64, "4294967295.75")
                .unwrap()
                .to_integer(32, false),
            Some(4294967295)
        );
    }

    #[test]
    fn operations_round_in_their_own_format_without_contraction() {
        let a = FloatBits::parse(FloatKind::F32, "0x1.000002p0").unwrap();
        let b = FloatBits::parse(FloatKind::F32, "0x1.fffffcp-1").unwrap();
        let one = FloatBits::parse(FloatKind::F32, "1").unwrap();
        assert_eq!(
            a.finite_binary(b, FiniteOp::Mul)
                .unwrap()
                .finite_binary(one, FiniteOp::Sub)
                .unwrap()
                .bits(),
            0
        );
        let small = FloatBits::from_bits(FloatKind::F64, 1).unwrap();
        let two = FloatBits::parse(FloatKind::F64, "2").unwrap();
        assert_eq!(small.finite_binary(two, FiniteOp::Mul).unwrap().bits(), 2);
        assert_eq!(small.finite_binary(two, FiniteOp::Div).unwrap().bits(), 0);
    }

    #[test]
    fn target_nan_priority_and_exceptional_results_are_explicit() {
        let value = |bits| FloatBits::from_bits(FloatKind::F64, bits).unwrap();
        let q = value(0x7ff8000000000011);
        let s = value(0xfff0000000000022);
        for op in [FiniteOp::Add, FiniteOp::Sub, FiniteOp::Mul, FiniteOp::Div] {
            // A signaling operand takes priority even when it is second.
            assert_eq!(q.target_binary(s, op).unwrap().bits(), 0xfff8000000000022);
            assert_eq!(s.target_binary(q, op).unwrap().bits(), 0xfff8000000000022);
            assert_eq!(q.target_binary(q.negated(), op).unwrap(), q);
        }
        let zero = value(0);
        let one = value(0x3ff0000000000000);
        assert_eq!(
            zero.target_binary(zero, FiniteOp::Div).unwrap().bits(),
            0x7ff8000000000000
        );
        assert_eq!(
            one.target_binary(zero, FiniteOp::Div).unwrap().bits(),
            0x7ff0000000000000
        );
        assert_eq!(s.target_convert(FloatKind::F32).unwrap().bits(), 0xffc00000);
        assert_eq!(s.target_convert(FloatKind::F64).unwrap(), s);
    }
}
