//! Explicit, host-independent floating state and target status adaptation.
//! This API does not by itself enable source strict-FP or binary128 execution.
use super::*;
use rustc_apfloat::ieee::{IeeeFloat, Semantics};

// An internal arithmetic range check, never an interchange format. Preserve
// Quad's 113-bit precision, but admit the exponent of any product or quotient
// of two binary128 values. In particular Quad itself is not wide enough to
// diagnose directed overflow at its own maximum exponent. No value of this
// type is parsed from or serialized to representation bits.
struct ExtendedExponentQuad;
impl Semantics for ExtendedExponentQuad {
    const BITS: usize = 128;
    const EXP_BITS: usize = 15;
    const MIN_EXP: i32 = -65534;
    const MAX_EXP: i32 = 65535;
}
type RangeQuad = IeeeFloat<ExtendedExponentQuad>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FpRounding {
    #[default]
    NearestEven,
    Upward,
    Downward,
    TowardZero,
}

impl FpRounding {
    fn engine(self) -> Round {
        match self {
            Self::NearestEven => Round::NearestTiesToEven,
            Self::Upward => Round::TowardPositive,
            Self::Downward => Round::TowardNegative,
            Self::TowardZero => Round::TowardZero,
        }
    }
}

/// Five logical status bits, deliberately independent of libc's fenv_t.
/// Their serialization uses the low five AArch64 FPSR bits, not TestFloat's
/// reversed exception encoding. IDC/QC are not members of this C environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FpStatus(u8);

impl FpStatus {
    pub const NONE: Self = Self(0);
    pub const INVALID: Self = Self(1);
    pub const DIV_BY_ZERO: Self = Self(2);
    pub const OVERFLOW: Self = Self(4);
    pub const UNDERFLOW: Self = Self(8);
    pub const INEXACT: Self = Self(16);
    pub const ALL: Self = Self(31);

    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !31 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }
    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    fn engine(status: Status) -> Self {
        let mut result = Self::NONE;
        for (input, output) in [
            (Status::INVALID_OP, Self::INVALID),
            (Status::DIV_BY_ZERO, Self::DIV_BY_ZERO),
            (Status::OVERFLOW, Self::OVERFLOW),
            (Status::UNDERFLOW, Self::UNDERFLOW),
            (Status::INEXACT, Self::INEXACT),
        ] {
            if status.contains(input) {
                result = result.union(output);
            }
        }
        result
    }
}

#[must_use = "both the value and status belong to strict execution"]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FpResult<T> {
    pub value: T,
    pub status: FpStatus,
}

/// Value-semantic state owned by a logical thread's world snapshot. There is
/// no ambient Rust state, target register access or process-wide environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FpEnvironment {
    pub rounding: FpRounding,
    pub flags: FpStatus,
}

impl FpEnvironment {
    pub fn raised(self, status: FpStatus) -> Self {
        Self {
            flags: self.flags.union(status),
            ..self
        }
    }
    pub fn cleared(self, mask: FpStatus) -> Self {
        Self {
            flags: self.flags.without(mask),
            ..self
        }
    }
}

impl FloatBits {
    fn executable_scalar(self) -> bool {
        matches!(self.kind, FloatKind::F32 | FloatKind::F64 | FloatKind::F128)
    }

    fn default_nan(self) -> Self {
        Self {
            bits: match self.kind {
                FloatKind::F32 => 0x7fc00000,
                FloatKind::F64 => 0x7ff8000000000000,
                // The qualified AArch64 hosted TFmode runtime uses all-one
                // fraction bits for an invalid operation without NaN inputs.
                FloatKind::F128 => 0x7fffffffffffffffffffffffffffffff,
                _ => unreachable!("admitted scalar format"),
            },
            ..self
        }
    }

    fn target_nan(operands: &[Self]) -> Option<FpResult<Self>> {
        for signaling in [true, false] {
            for &value in operands {
                if value.is_nan() && (!signaling || value.is_signaling()) {
                    return Some(FpResult {
                        value: value.quieted(),
                        status: if signaling {
                            FpStatus::INVALID
                        } else {
                            FpStatus::NONE
                        },
                    });
                }
            }
        }
        None
    }

    /// APFloat detects tininess after rounding. The admitted Arm profile
    /// detects it before rounding: a tiny inexact value rounding up to the
    /// minimum normal still raises UFC. Only that boundary needs a second,
    /// directed evaluation; no approximate host arithmetic is involved.
    fn target_result(
        self,
        raw: rustc_apfloat::StatusAnd<u128>,
        toward_zero: impl FnOnce() -> u128,
        beyond_exponent_range: impl FnOnce() -> bool,
    ) -> FpResult<Self> {
        let mut value = Self {
            bits: raw.value,
            ..self
        };
        let mut status = FpStatus::engine(raw.status);
        let (sign, minimum, largest) = match self.kind {
            FloatKind::F32 => (1u128 << 31, 1u128 << 23, 0x7f7fffff),
            FloatKind::F64 => (1u128 << 63, 1u128 << 52, 0x7fefffffffffffff),
            FloatKind::F128 => (
                1u128 << 127,
                1u128 << 112,
                0x7ffeffffffffffffffffffffffffffff,
            ),
            _ => unreachable!("admitted scalar format"),
        };
        if status.contains(FpStatus::INEXACT)
            && !status.contains(FpStatus::UNDERFLOW)
            && value.bits & !sign == minimum
            && toward_zero() & !sign < minimum
        {
            status = status.union(FpStatus::UNDERFLOW);
        }
        // This APFloat pin drops OVERFLOW when overflow_result chooses the
        // largest finite value. A wider toward-zero calculation distinguishes
        // exponent overflow from merely rounding down just above max-finite.
        if status.contains(FpStatus::INEXACT)
            && !status.contains(FpStatus::OVERFLOW)
            && value.bits & !sign == largest
            && beyond_exponent_range()
        {
            status = status.union(FpStatus::OVERFLOW);
        }
        if value.is_nan() {
            value = value.default_nan();
        }
        FpResult { value, status }
    }

    pub fn target_binary_round(
        self,
        other: Self,
        op: FiniteOp,
        rounding: FpRounding,
    ) -> Option<FpResult<Self>> {
        if self.kind != other.kind || !self.executable_scalar() {
            return None;
        }
        if let Some(nan) = Self::target_nan(&[self, other]) {
            return Some(nan);
        }
        let evaluate = |round| {
            with_format!(self.kind, F, {
                let a = F::from_bits(self.bits);
                let b = F::from_bits(other.bits);
                match op {
                    FiniteOp::Add => a.add_r(b, round),
                    FiniteOp::Sub => a.sub_r(b, round),
                    FiniteOp::Mul => a.mul_r(b, round),
                    FiniteOp::Div => a.div_r(b, round),
                }
                .map(Float::to_bits)
            })
        };
        Some(self.target_result(
            evaluate(rounding.engine()),
            || evaluate(Round::TowardZero).value,
            || {
                let a = self.as_range_quad();
                let b = other.as_range_quad();
                let result = match op {
                    FiniteOp::Add => a.add_r(b, Round::TowardZero),
                    FiniteOp::Sub => a.sub_r(b, Round::TowardZero),
                    FiniteOp::Mul => a.mul_r(b, Round::TowardZero),
                    FiniteOp::Div => a.div_r(b, Round::TowardZero),
                };
                self.beyond_range(result.value)
            },
        ))
    }

    /// Explicit a*b+c, one rounding. Arm FMADD examines NaNs in addend, a, b
    /// order. The qualified hosted binary128 fmal has the same signaling
    /// priority, but quiet multiplicand NaNs precede a quiet addend. A quiet
    /// addend does not hide invalid zero*infinity in either implementation.
    pub fn target_fma_round(
        self,
        multiplier: Self,
        addend: Self,
        rounding: FpRounding,
    ) -> Option<FpResult<Self>> {
        if self.kind != multiplier.kind || self.kind != addend.kind || !self.executable_scalar() {
            return None;
        }
        if let Some(mut nan) = Self::target_nan(&[addend, self, multiplier]) {
            if self.kind == FloatKind::F128 && nan.status == FpStatus::NONE {
                nan = Self::target_nan(&[self, multiplier, addend]).unwrap();
            }
            let infinity = |v: Self| !v.is_finite() && !v.is_nan();
            if !nan.status.contains(FpStatus::INVALID)
                && ((self.is_zero() && infinity(multiplier))
                    || (multiplier.is_zero() && infinity(self)))
            {
                return Some(FpResult {
                    value: self.default_nan(),
                    status: FpStatus::INVALID,
                });
            }
            return Some(nan);
        }
        let evaluate = |round| {
            with_format!(
                self.kind,
                F,
                F::from_bits(self.bits)
                    .mul_add_r(
                        F::from_bits(multiplier.bits),
                        F::from_bits(addend.bits),
                        round,
                    )
                    .map(Float::to_bits)
            )
        };
        Some(self.target_result(
            evaluate(rounding.engine()),
            || evaluate(Round::TowardZero).value,
            || {
                self.beyond_range(
                    self.as_range_quad()
                        .mul_add_r(
                            multiplier.as_range_quad(),
                            addend.as_range_quad(),
                            Round::TowardZero,
                        )
                        .value,
                )
            },
        ))
    }

    fn as_range_quad(self) -> RangeQuad {
        fn convert<F: FloatConvert<RangeQuad>>(value: F) -> RangeQuad {
            value.convert_r(Round::TowardZero, &mut false).value
        }
        with_format!(self.kind, F, convert(F::from_bits(self.bits)))
    }

    fn beyond_range(self, value: RangeQuad) -> bool {
        let threshold = match self.kind {
            FloatKind::F32 => 128,
            FloatKind::F64 => 1024,
            FloatKind::F128 => 16384,
            _ => unreachable!(),
        };
        value.abs() >= RangeQuad::from_u128(1).value.scalbn(threshold)
    }

    /// `signaling` distinguishes FCMP-like quiet predicates from FCMPE-like
    /// ordered C comparisons. Unordered is a numeric outcome, not poison.
    pub fn target_compare(
        self,
        other: Self,
        signaling: bool,
    ) -> Option<FpResult<Option<Ordering>>> {
        if self.kind != other.kind || !self.executable_scalar() {
            return None;
        }
        Some(FpResult {
            value: self.compare(other),
            status: if self.is_signaling()
                || other.is_signaling()
                || (signaling && (self.is_nan() || other.is_nan()))
            {
                FpStatus::INVALID
            } else {
                FpStatus::NONE
            },
        })
    }

    pub fn target_convert_round(
        self,
        kind: FloatKind,
        rounding: FpRounding,
    ) -> Option<FpResult<Self>> {
        if !self.executable_scalar()
            || !matches!(kind, FloatKind::F32 | FloatKind::F64 | FloatKind::F128)
        {
            return None;
        }
        if self.kind == kind {
            return Some(FpResult {
                value: self,
                status: FpStatus::NONE,
            });
        }
        if !self.is_finite() {
            return Some(FpResult {
                value: self.target_convert(kind)?,
                status: if self.is_signaling() {
                    FpStatus::INVALID
                } else {
                    FpStatus::NONE
                },
            });
        }
        fn convert<F: Float + FloatConvert<Single> + FloatConvert<Double> + FloatConvert<Quad>>(
            value: F,
            kind: FloatKind,
            round: Round,
        ) -> rustc_apfloat::StatusAnd<u128> {
            if kind == FloatKind::F32 {
                let result: rustc_apfloat::StatusAnd<Single> = value.convert_r(round, &mut false);
                result.map(Float::to_bits)
            } else if kind == FloatKind::F64 {
                let result: rustc_apfloat::StatusAnd<Double> = value.convert_r(round, &mut false);
                result.map(Float::to_bits)
            } else {
                let result: rustc_apfloat::StatusAnd<Quad> = value.convert_r(round, &mut false);
                result.map(Float::to_bits)
            }
        }
        let evaluate =
            |round| with_format!(self.kind, F, convert(F::from_bits(self.bits), kind, round));
        let target = Self { kind, bits: 0 };
        Some(target.target_result(
            evaluate(rounding.engine()),
            || evaluate(Round::TowardZero).value,
            || target.beyond_range(self.as_range_quad()),
        ))
    }

    pub fn from_signed_round(kind: FloatKind, value: i128, rounding: FpRounding) -> FpResult<Self> {
        let result = with_format!(
            kind,
            F,
            F::from_i128_r(value, rounding.engine()).map(Float::to_bits)
        );
        FpResult {
            value: Self {
                kind,
                bits: result.value,
            },
            status: FpStatus::engine(result.status),
        }
    }

    pub fn from_unsigned_round(
        kind: FloatKind,
        value: u128,
        rounding: FpRounding,
    ) -> FpResult<Self> {
        let result = with_format!(
            kind,
            F,
            F::from_u128_r(value, rounding.engine()).map(Float::to_bits)
        );
        FpResult {
            value: Self {
                kind,
                bits: result.value,
            },
            status: FpStatus::engine(result.status),
        }
    }

    /// C always truncates toward zero. Out-of-domain input has no C value;
    /// status never turns APFloat's invalid fallback into a defined conversion.
    pub fn to_integer_status(self, width: u16, signed: bool) -> Option<FpResult<Option<u128>>> {
        if !(1..=128).contains(&width) {
            return None;
        }
        let result = with_format!(self.kind, F, {
            let value = F::from_bits(self.bits);
            if signed {
                value
                    .to_i128_r(usize::from(width), Round::TowardZero, &mut false)
                    .map(|v| v as u128)
            } else {
                value.to_u128_r(usize::from(width), Round::TowardZero, &mut false)
            }
        });
        let status = FpStatus::engine(result.status);
        Some(FpResult {
            value: (!status.contains(FpStatus::INVALID)).then_some(result.value),
            status,
        })
    }
}
