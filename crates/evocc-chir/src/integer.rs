//! Target-fixed GNU C integer semantics shared by semantic analysis and execution.
//!
//! Day 5 deliberately supports one ABI: `aarch64-unknown-linux-gnu`.  Keeping
//! widths, ranks, conversions, and checked operations here prevents the parser
//! and interpreter from silently acquiring different host-language behavior.

use evocc_types::{enum_integer_rank, IntRank};

use crate::{BinaryOp, CheckedArithmeticOp, FloatLit, IntBase, IntLit, IntSuffix, UnaryOp};

/// Value operation selected by the GNU bit-counting builtin family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitBuiltinOp {
    ByteSwap,
    FindFirstSet,
    CountLeadingZeros,
    CountTrailingZeros,
    PopulationCount,
}

/// A C integer type in the Day 5 AArch64 GNU ABI.
///
/// The representation is deliberately private: downstream code selects one
/// of the fixed target constants or maps an [`IntRank`], rather than forging a
/// zero-width or over-wide value that could panic during arithmetic.
///
/// ```compile_fail
/// use evocc_chir::integer::CIntegerType;
/// let _forged = CIntegerType { bits: 0, rank: 0, signed: true };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CIntegerType {
    /// Value-representation width. `_Bool` is modeled as one value bit.
    bits: u8,
    /// C conversion rank, with signed and unsigned counterparts sharing rank.
    rank: u8,
    signed: bool,
}

impl CIntegerType {
    pub const BOOL: Self = Self::new(1, 0, false);
    /// Plain `char` is unsigned for the pinned AArch64 GNU target.
    pub const CHAR: Self = Self::new(8, 1, false);
    pub const SCHAR: Self = Self::new(8, 1, true);
    pub const UCHAR: Self = Self::new(8, 1, false);
    pub const SHORT: Self = Self::new(16, 2, true);
    pub const USHORT: Self = Self::new(16, 2, false);
    pub const INT: Self = Self::new(32, 3, true);
    pub const UINT: Self = Self::new(32, 3, false);
    pub const LONG: Self = Self::new(64, 4, true);
    pub const ULONG: Self = Self::new(64, 4, false);
    pub const LONG_LONG: Self = Self::new(64, 5, true);
    pub const ULONG_LONG: Self = Self::new(64, 5, false);
    pub const INT128: Self = Self::new(128, 6, true);
    pub const UINT128: Self = Self::new(128, 6, false);

    const fn new(bits: u8, rank: u8, signed: bool) -> Self {
        Self { bits, rank, signed }
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.bits
    }

    #[must_use]
    pub const fn rank(self) -> u8 {
        self.rank
    }

    #[must_use]
    pub const fn is_signed(self) -> bool {
        self.signed
    }

    #[must_use]
    pub const fn unsigned_version(self) -> Self {
        Self {
            signed: false,
            ..self
        }
    }

    #[must_use]
    pub const fn mask(self) -> u128 {
        if self.bits == 128 {
            u128::MAX
        } else {
            (1_u128 << self.bits) - 1
        }
    }

    #[must_use]
    pub const fn signed_min(self) -> i128 {
        if self.bits == 128 {
            i128::MIN
        } else {
            -(1_i128 << (self.bits - 1))
        }
    }

    #[must_use]
    pub const fn signed_max(self) -> i128 {
        if self.bits == 128 {
            i128::MAX
        } else {
            (1_i128 << (self.bits - 1)) - 1
        }
    }
}

/// A value together with its target C integer type.
///
/// `raw` always contains only the low `ty.bits` bits. Signed values use the
/// target's two's-complement representation; no C arithmetic is delegated to
/// Rust's overflow behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CInteger {
    raw: u128,
    ty: CIntegerType,
}

impl CInteger {
    #[must_use]
    pub const fn new(raw: u64, ty: CIntegerType) -> Self {
        Self::from_raw(raw as u128, ty)
    }

    /// Construct from the full target representation. This is used by the
    /// front-end constant evaluator for GNU 128-bit integers; execution paths
    /// retain their explicit E3033 boundary.
    #[must_use]
    pub const fn from_raw(raw: u128, ty: CIntegerType) -> Self {
        Self {
            raw: raw & ty.mask(),
            ty,
        }
    }

    #[must_use]
    pub const fn int(value: i32) -> Self {
        Self::new(value as u64, CIntegerType::INT)
    }

    #[must_use]
    pub const fn raw_bits(self) -> u128 {
        self.raw
    }

    #[must_use]
    pub const fn ty(self) -> CIntegerType {
        self.ty
    }

    #[must_use]
    pub fn mathematical_value(self) -> i128 {
        self.try_mathematical_value()
            .expect("unsigned integer value does not fit i128")
    }

    /// Return the mathematical value when it is representable by Rust's
    /// signed 128-bit carrier. Unsigned values above `i128::MAX` remain fully
    /// usable for target-width operations and comparisons through `raw_bits`.
    #[must_use]
    pub fn try_mathematical_value(self) -> Option<i128> {
        if !self.ty.signed {
            return i128::try_from(self.raw).ok();
        }
        if self.ty.bits == 128 {
            return Some(self.raw as i128);
        }
        let sign_bit = 1_u128 << (self.ty.bits - 1);
        if self.raw & sign_bit == 0 {
            i128::try_from(self.raw).ok()
        } else {
            Some(i128::try_from(self.raw).ok()? - (1_i128 << self.ty.bits))
        }
    }

    #[must_use]
    pub const fn truth(self) -> bool {
        self.raw != 0
    }

    /// Apply C integer conversion, including `_Bool` truth conversion and
    /// target-width truncation/sign extension.
    #[must_use]
    pub fn cast(self, target: CIntegerType) -> Self {
        if target == CIntegerType::BOOL {
            return Self::new(u64::from(self.truth()), target);
        }
        if self.ty.signed && target.bits > self.ty.bits && self.mathematical_value() < 0 {
            return Self::from_raw(self.mathematical_value() as u128, target);
        }
        Self::from_raw(self.raw, target)
    }

    /// Apply the integer promotions for the pinned ABI.
    #[must_use]
    pub fn promoted(self) -> Self {
        if self.ty.rank < CIntegerType::INT.rank {
            // Every AArch64 integer type below `int` fits in signed `int`.
            self.cast(CIntegerType::INT)
        } else {
            self
        }
    }
}

/// Evaluate one GNU bit builtin after its source-level parameter conversion.
///
/// `None` is the one typed runtime failure in this family: `clz(0)` or
/// `ctz(0)`. Counting returns `int`; byte swap retains the operand type.
#[must_use]
pub fn eval_bit_builtin(op: BitBuiltinOp, operand: CInteger) -> Option<CInteger> {
    let raw = operand.raw_bits();
    let result = match op {
        BitBuiltinOp::ByteSwap => {
            if !matches!(operand.ty().bits(), 16 | 32 | 64) {
                return None;
            }
            return Some(CInteger::from_raw(
                raw.swap_bytes() >> (128 - operand.ty().bits()),
                operand.ty(),
            ));
        }
        BitBuiltinOp::FindFirstSet => {
            if raw == 0 {
                0
            } else {
                raw.trailing_zeros() + 1
            }
        }
        BitBuiltinOp::CountLeadingZeros => {
            if raw == 0 {
                return None;
            }
            raw.leading_zeros() - u32::from(128 - operand.ty().bits())
        }
        BitBuiltinOp::CountTrailingZeros => {
            if raw == 0 {
                return None;
            }
            raw.trailing_zeros()
        }
        BitBuiltinOp::PopulationCount => raw.count_ones(),
    };
    Some(CInteger::new(u64::from(result), CIntegerType::INT))
}

/// Evaluate a checked-arithmetic builtin without inheriting host overflow.
///
/// Supported C operands are at most 64 bits. Their exact signed magnitude,
/// including the product of two `unsigned long` values, fits in `u128` even
/// when its mathematical value does not fit in Rust's `i128`.
#[must_use]
pub fn eval_checked_arithmetic(
    op: CheckedArithmeticOp,
    lhs: CInteger,
    rhs: CInteger,
    target: CIntegerType,
) -> (CInteger, bool) {
    let lhs = SignedMagnitude::from_integer(lhs);
    let rhs = SignedMagnitude::from_integer(rhs);
    let exact = match op {
        CheckedArithmeticOp::Add => lhs.add(rhs),
        CheckedArithmeticOp::Sub => lhs.add(rhs.negated()),
        CheckedArithmeticOp::Mul => lhs.multiply(rhs),
    };
    let representable = exact.is_representable(target);
    (
        CInteger::new(exact.wrapped_bits(target), target),
        !representable,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SignedMagnitude {
    negative: bool,
    magnitude: u128,
}

impl SignedMagnitude {
    fn from_integer(value: CInteger) -> Self {
        let mathematical = value.mathematical_value();
        Self {
            negative: mathematical < 0,
            magnitude: mathematical.unsigned_abs(),
        }
        .normalized()
    }

    fn normalized(self) -> Self {
        Self {
            negative: self.negative && self.magnitude != 0,
            magnitude: self.magnitude,
        }
    }

    fn negated(self) -> Self {
        Self {
            negative: !self.negative,
            magnitude: self.magnitude,
        }
        .normalized()
    }

    fn add(self, other: Self) -> Self {
        if self.negative == other.negative {
            return Self {
                negative: self.negative,
                magnitude: self
                    .magnitude
                    .checked_add(other.magnitude)
                    .expect("the sum of two 64-bit magnitudes fits u128"),
            }
            .normalized();
        }
        if self.magnitude >= other.magnitude {
            Self {
                negative: self.negative,
                magnitude: self.magnitude - other.magnitude,
            }
            .normalized()
        } else {
            Self {
                negative: other.negative,
                magnitude: other.magnitude - self.magnitude,
            }
            .normalized()
        }
    }

    fn multiply(self, other: Self) -> Self {
        Self {
            negative: self.negative != other.negative,
            magnitude: self
                .magnitude
                .checked_mul(other.magnitude)
                .expect("the product of two 64-bit magnitudes fits u128"),
        }
        .normalized()
    }

    fn is_representable(self, target: CIntegerType) -> bool {
        if target.is_signed() {
            let negative_limit = 1_u128 << (target.bits() - 1);
            if self.negative {
                self.magnitude <= negative_limit
            } else {
                self.magnitude < negative_limit
            }
        } else {
            !self.negative && self.magnitude <= target.mask()
        }
    }

    fn wrapped_bits(self, target: CIntegerType) -> u64 {
        let modulus_mask = if target.bits() == 64 {
            u128::from(u64::MAX)
        } else {
            (1_u128 << target.bits()) - 1
        };
        let low = self.magnitude & modulus_mask;
        let wrapped = if self.negative {
            ((!low).wrapping_add(1)) & modulus_mask
        } else {
            low
        };
        u64::try_from(wrapped).expect("target width is at most 64 bits")
    }
}

/// A defined failure from target integer evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegerError {
    LiteralOutOfRange,
    ConversionOutOfRange,
    SignedOverflow,
    DivisionByZero,
    RemainderByZero,
    InvalidShiftCount,
    InvalidSignedLeftShift,
    UnsupportedOperator,
}

/// Convert a canonical C integer rank into the pinned target representation.
#[must_use]
pub const fn integer_type_for_rank(rank: IntRank) -> CIntegerType {
    match rank {
        IntRank::Bool => CIntegerType::BOOL,
        IntRank::Char => CIntegerType::CHAR,
        IntRank::SChar => CIntegerType::SCHAR,
        IntRank::UChar => CIntegerType::UCHAR,
        IntRank::Short => CIntegerType::SHORT,
        IntRank::UShort => CIntegerType::USHORT,
        IntRank::Int => CIntegerType::INT,
        IntRank::UInt => CIntegerType::UINT,
        IntRank::Long => CIntegerType::LONG,
        IntRank::ULong => CIntegerType::ULONG,
        IntRank::LongLong => CIntegerType::LONG_LONG,
        IntRank::ULongLong => CIntegerType::ULONG_LONG,
    }
}

/// Recover a canonical rank. Plain `char` aliases `unsigned char` in the
/// value model, so the canonical reverse mapping is `UChar`.
#[must_use]
pub const fn rank_for_integer_type(ty: CIntegerType) -> Option<IntRank> {
    match (ty.bits, ty.rank, ty.signed) {
        (1, 0, false) => Some(IntRank::Bool),
        (8, 1, true) => Some(IntRank::SChar),
        (8, 1, false) => Some(IntRank::UChar),
        (16, 2, true) => Some(IntRank::Short),
        (16, 2, false) => Some(IntRank::UShort),
        (32, 3, true) => Some(IntRank::Int),
        (32, 3, false) => Some(IntRank::UInt),
        (64, 4, true) => Some(IntRank::Long),
        (64, 4, false) => Some(IntRank::ULong),
        (64, 5, true) => Some(IntRank::LongLong),
        (64, 5, false) => Some(IntRank::ULongLong),
        _ => None,
    }
}

/// GNU AArch64 enum mode selection for the parser's supported value range.
#[must_use]
pub fn enum_integer_type(minimum: i128, maximum: i128) -> CIntegerType {
    integer_type_for_rank(enum_integer_rank(minimum, maximum, false))
}

/// Convert an already-rounded floating literal to a target integer type.
pub fn floating_literal_to_integer(
    literal: FloatLit,
    target: CIntegerType,
) -> Result<CInteger, IntegerError> {
    let value = literal.value;
    if !value.is_finite() {
        return Err(IntegerError::ConversionOutOfRange);
    }
    if target == CIntegerType::BOOL {
        return Ok(CInteger::new(u64::from(value != 0.0), target));
    }
    let truncated = value.trunc();
    if target.signed {
        let bound = 2_f64.powi(i32::from(target.bits) - 1);
        if truncated < -bound || truncated >= bound {
            return Err(IntegerError::ConversionOutOfRange);
        }
        Ok(CInteger::from_raw((truncated as i128) as u128, target))
    } else {
        let upper_bound = 2_f64.powi(i32::from(target.bits));
        if truncated < 0.0 || truncated >= upper_bound {
            return Err(IntegerError::ConversionOutOfRange);
        }
        Ok(CInteger::from_raw(truncated as u128, target))
    }
}

/// Select a literal type using the C11 candidate lists and the pinned ABI.
pub fn integer_literal_value(literal: IntLit) -> Result<CInteger, IntegerError> {
    let decimal = literal.base == IntBase::Dec;
    let candidates: &[CIntegerType] = match (literal.suffix, decimal) {
        (IntSuffix::None, true) => &[
            CIntegerType::INT,
            CIntegerType::LONG,
            CIntegerType::LONG_LONG,
        ],
        (IntSuffix::None, false) => &[
            CIntegerType::INT,
            CIntegerType::UINT,
            CIntegerType::LONG,
            CIntegerType::ULONG,
            CIntegerType::LONG_LONG,
            CIntegerType::ULONG_LONG,
        ],
        (IntSuffix::U, _) => &[
            CIntegerType::UINT,
            CIntegerType::ULONG,
            CIntegerType::ULONG_LONG,
        ],
        (IntSuffix::L, true) => &[CIntegerType::LONG, CIntegerType::LONG_LONG],
        (IntSuffix::L, false) => &[
            CIntegerType::LONG,
            CIntegerType::ULONG,
            CIntegerType::LONG_LONG,
            CIntegerType::ULONG_LONG,
        ],
        (IntSuffix::UL, _) => &[CIntegerType::ULONG, CIntegerType::ULONG_LONG],
        (IntSuffix::LL, true) => &[CIntegerType::LONG_LONG],
        (IntSuffix::LL, false) => &[CIntegerType::LONG_LONG, CIntegerType::ULONG_LONG],
        (IntSuffix::ULL, _) => &[CIntegerType::ULONG_LONG],
    };
    candidates
        .iter()
        .copied()
        .find(|candidate| {
            if candidate.signed {
                i128::from(literal.value) <= candidate.signed_max()
            } else {
                u128::from(literal.value) <= candidate.mask()
            }
        })
        .map(|ty| CInteger::new(literal.value, ty))
        .ok_or(IntegerError::LiteralOutOfRange)
}

/// The usual arithmetic conversion result after integer promotion.
#[must_use]
pub fn usual_arithmetic_type(left: CIntegerType, right: CIntegerType) -> CIntegerType {
    if left == right {
        return left;
    }
    if left.signed == right.signed {
        return if left.rank >= right.rank { left } else { right };
    }
    let (signed, unsigned) = if left.signed {
        (left, right)
    } else {
        (right, left)
    };
    if unsigned.rank >= signed.rank {
        return unsigned;
    }
    if signed.bits > unsigned.bits {
        return signed;
    }
    signed.unsigned_version()
}

/// Promote and convert two operands to their common arithmetic type.
#[must_use]
pub fn converted_pair(left: CInteger, right: CInteger) -> (CInteger, CInteger, CIntegerType) {
    let left = left.promoted();
    let right = right.promoted();
    let ty = usual_arithmetic_type(left.ty, right.ty);
    (left.cast(ty), right.cast(ty), ty)
}

/// Construct a mathematical value only when it is representable in `ty`.
pub fn integer_from_mathematical(value: i128, ty: CIntegerType) -> Result<CInteger, IntegerError> {
    if ty.signed && !(ty.signed_min()..=ty.signed_max()).contains(&value) {
        return Err(IntegerError::SignedOverflow);
    }
    if !ty.signed && (value < 0 || u128::try_from(value).map_or(true, |value| value > ty.mask())) {
        return Err(IntegerError::ConversionOutOfRange);
    }
    Ok(CInteger::from_raw(value as u128, ty))
}

/// Evaluate a C integer unary operator after integer promotion.
pub fn eval_unary_integer(op: UnaryOp, operand: CInteger) -> Result<CInteger, IntegerError> {
    let operand = operand.promoted();
    match op {
        UnaryOp::Plus => Ok(operand),
        UnaryOp::Neg if operand.ty.signed => {
            let value = operand
                .mathematical_value()
                .checked_neg()
                .ok_or(IntegerError::SignedOverflow)?;
            integer_from_mathematical(value, operand.ty)
        }
        UnaryOp::Neg => Ok(CInteger::from_raw(
            0_u128.wrapping_sub(operand.raw),
            operand.ty,
        )),
        UnaryOp::Not => Ok(CInteger::int(i32::from(!operand.truth()))),
        UnaryOp::BitNot => Ok(CInteger::from_raw(!operand.raw, operand.ty)),
        UnaryOp::AddrOf | UnaryOp::Deref => Err(IntegerError::UnsupportedOperator),
    }
}

/// Evaluate one unary operator in an integer constant expression.
///
/// GNU constant folding uses target-width two's-complement wrap for signed
/// arithmetic. Runtime evaluation deliberately continues to use
/// [`eval_unary_integer`], where signed overflow is a typed failure.
pub fn eval_constant_unary_integer(
    op: UnaryOp,
    operand: CInteger,
) -> Result<CInteger, IntegerError> {
    let operand = operand.promoted();
    if op == UnaryOp::Neg && operand.ty.is_signed() {
        return Ok(CInteger::from_raw(
            0_u128.wrapping_sub(operand.raw_bits()),
            operand.ty,
        ));
    }
    eval_unary_integer(op, operand)
}

/// Evaluate a C integer binary operator. Callers retain responsibility for
/// expression-level short-circuiting before invoking this value operation.
pub fn eval_binary_integer(
    op: BinaryOp,
    left: CInteger,
    right: CInteger,
) -> Result<CInteger, IntegerError> {
    if matches!(op, BinaryOp::Shl | BinaryOp::Shr) {
        return eval_integer_shift(op, left.promoted(), right.promoted());
    }
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
        return Ok(CInteger::int(i32::from(match op {
            BinaryOp::And => left.truth() && right.truth(),
            BinaryOp::Or => left.truth() || right.truth(),
            _ => unreachable!(),
        })));
    }
    let (left, right, ty) = converted_pair(left, right);
    if matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    ) {
        let ordering = if ty.signed {
            left.mathematical_value().cmp(&right.mathematical_value())
        } else {
            left.raw.cmp(&right.raw)
        };
        return Ok(CInteger::int(i32::from(match op {
            BinaryOp::Lt => ordering.is_lt(),
            BinaryOp::Le => !ordering.is_gt(),
            BinaryOp::Gt => ordering.is_gt(),
            BinaryOp::Ge => !ordering.is_lt(),
            BinaryOp::Eq => ordering.is_eq(),
            BinaryOp::Ne => !ordering.is_eq(),
            _ => unreachable!(),
        })));
    }
    match op {
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul if ty.signed => {
            let left = left.mathematical_value();
            let right = right.mathematical_value();
            let value = match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                _ => unreachable!(),
            }
            .ok_or(IntegerError::SignedOverflow)?;
            integer_from_mathematical(value, ty)
        }
        BinaryOp::Add => Ok(CInteger::from_raw(left.raw.wrapping_add(right.raw), ty)),
        BinaryOp::Sub => Ok(CInteger::from_raw(left.raw.wrapping_sub(right.raw), ty)),
        BinaryOp::Mul => Ok(CInteger::from_raw(left.raw.wrapping_mul(right.raw), ty)),
        BinaryOp::Div | BinaryOp::Mod => {
            if right.raw == 0 {
                return Err(if op == BinaryOp::Div {
                    IntegerError::DivisionByZero
                } else {
                    IntegerError::RemainderByZero
                });
            }
            if ty.signed {
                let left = left.mathematical_value();
                let right = right.mathematical_value();
                // C defines both `/` and `%` in terms of a representable
                // quotient.  MIN / -1 is not representable in the result
                // type, so the paired remainder expression is undefined too
                // even though its mathematical remainder would be zero.
                if left == ty.signed_min() && right == -1 {
                    return Err(IntegerError::SignedOverflow);
                }
                let value = if op == BinaryOp::Div {
                    left.checked_div(right)
                } else {
                    left.checked_rem(right)
                }
                .ok_or(IntegerError::SignedOverflow)?;
                integer_from_mathematical(value, ty)
            } else {
                Ok(CInteger::from_raw(
                    if op == BinaryOp::Div {
                        left.raw / right.raw
                    } else {
                        left.raw % right.raw
                    },
                    ty,
                ))
            }
        }
        BinaryOp::BitAnd => Ok(CInteger::from_raw(left.raw & right.raw, ty)),
        BinaryOp::BitOr => Ok(CInteger::from_raw(left.raw | right.raw, ty)),
        BinaryOp::BitXor => Ok(CInteger::from_raw(left.raw ^ right.raw, ty)),
        BinaryOp::Shl
        | BinaryOp::Shr
        | BinaryOp::Lt
        | BinaryOp::Le
        | BinaryOp::Gt
        | BinaryOp::Ge
        | BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::And
        | BinaryOp::Or => unreachable!("handled before arithmetic dispatch"),
        BinaryOp::Comma
        | BinaryOp::Assign
        | BinaryOp::AddAssign
        | BinaryOp::SubAssign
        | BinaryOp::MulAssign
        | BinaryOp::DivAssign
        | BinaryOp::ModAssign
        | BinaryOp::ShlAssign
        | BinaryOp::ShrAssign
        | BinaryOp::BitAndAssign
        | BinaryOp::BitOrAssign
        | BinaryOp::BitXorAssign => Err(IntegerError::UnsupportedOperator),
    }
}

/// Evaluate one binary operator in an integer constant expression.
///
/// Signed add, subtract, multiply and left shift wrap in the converted
/// operand width. Division by zero and invalid shift counts remain failures;
/// `MIN / -1` wraps to `MIN` and its paired remainder is zero. Runtime
/// evaluation must continue to call [`eval_binary_integer`] so its UB
/// diagnostics are unchanged.
pub fn eval_constant_binary_integer(
    op: BinaryOp,
    left: CInteger,
    right: CInteger,
) -> Result<CInteger, IntegerError> {
    if op == BinaryOp::Shl {
        let left = left.promoted();
        let right = right.promoted();
        let shift = u32::try_from(
            right
                .try_mathematical_value()
                .ok_or(IntegerError::InvalidShiftCount)?,
        )
        .map_err(|_| IntegerError::InvalidShiftCount)?;
        if shift >= u32::from(left.ty.bits()) {
            return Err(IntegerError::InvalidShiftCount);
        }
        return Ok(CInteger::from_raw(left.raw_bits() << shift, left.ty));
    }

    if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
        let (left, right, ty) = converted_pair(left, right);
        let raw = match op {
            BinaryOp::Add => left.raw_bits().wrapping_add(right.raw_bits()),
            BinaryOp::Sub => left.raw_bits().wrapping_sub(right.raw_bits()),
            BinaryOp::Mul => left.raw_bits().wrapping_mul(right.raw_bits()),
            _ => unreachable!(),
        };
        return Ok(CInteger::from_raw(raw, ty));
    }

    if matches!(op, BinaryOp::Div | BinaryOp::Mod) {
        let (left, right, ty) = converted_pair(left, right);
        if right.raw_bits() == 0 {
            return Err(if op == BinaryOp::Div {
                IntegerError::DivisionByZero
            } else {
                IntegerError::RemainderByZero
            });
        }
        if ty.is_signed()
            && left.mathematical_value() == ty.signed_min()
            && right.mathematical_value() == -1
        {
            return Ok(if op == BinaryOp::Div {
                CInteger::from_raw(left.raw_bits(), ty)
            } else {
                CInteger::new(0, ty)
            });
        }
    }

    eval_binary_integer(op, left, right)
}

fn eval_integer_shift(
    op: BinaryOp,
    left: CInteger,
    right: CInteger,
) -> Result<CInteger, IntegerError> {
    let shift = u32::try_from(
        right
            .try_mathematical_value()
            .ok_or(IntegerError::InvalidShiftCount)?,
    )
    .map_err(|_| IntegerError::InvalidShiftCount)?;
    if shift >= u32::from(left.ty.bits) {
        return Err(IntegerError::InvalidShiftCount);
    }
    match op {
        BinaryOp::Shl if left.ty.signed => {
            let left_value = left.mathematical_value();
            if left_value < 0 || left_value > (left.ty.signed_max() >> shift) {
                return Err(IntegerError::InvalidSignedLeftShift);
            }
            Ok(CInteger::from_raw(left.raw << shift, left.ty))
        }
        BinaryOp::Shl => Ok(CInteger::from_raw(left.raw << shift, left.ty)),
        BinaryOp::Shr if left.ty.signed => integer_from_mathematical(
            left.mathematical_value()
                .checked_shr(shift)
                .ok_or(IntegerError::InvalidShiftCount)?,
            left.ty,
        ),
        BinaryOp::Shr => Ok(CInteger::from_raw(left.raw >> shift, left.ty)),
        _ => unreachable!("shift helper called only for shifts"),
    }
}
