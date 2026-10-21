//! Scalar classification and representation builtins, not library calls.
use crate::{CHirExpr, ExprFacts};
use evocc_common::SourceSpan;
use evocc_types::floating::FloatBits;
use evocc_types::FloatKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatingBuiltin {
    IsNan,
    IsInf,
    IsInfSign,
    IsFinite,
    IsNormal,
    SignBit,
    Abs,
    CopySign,
    IsGreater,
    IsGreaterEqual,
    IsLess,
    IsLessEqual,
    IsLessGreater,
    IsUnordered,
    /// `__builtin_fpclassify`: the class value for NaN, infinity, normal,
    /// subnormal and zero, in that order. They were integer constant
    /// expressions in the source, converted to `int`.
    FpClassify([i32; 5]),
}

impl FloatingBuiltin {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            Self::IsGreater
                | Self::IsGreaterEqual
                | Self::IsLess
                | Self::IsLessEqual
                | Self::IsLessGreater
                | Self::IsUnordered
        )
    }
}

#[derive(Debug, Clone)]
pub struct FloatingExpr<'arena> {
    pub op: FloatingBuiltin,
    pub operands: Vec<&'arena CHirExpr<'arena>>,
    pub facts: ExprFacts,
    pub span: SourceSpan,
}

/// Whether an expression of a half type has the binary32 value of an
/// evaluation rather than a half value (EvoIR 3.13.6): an arithmetic
/// operator, negation or a half constant, or a conditional or comma
/// expression that can select one. A conditional of a half type evaluates
/// in binary32, as in GCC 14, so an arm that is not a half value, an integer
/// arm, converts to binary32 directly. `is_half` tells whether an arm has a
/// half type. Lowering reaches the same placement from the value types; the
/// interpreter applies this predicate.
#[must_use]
pub fn half_carrier<'a>(
    expression: &'a CHirExpr<'a>,
    is_half: &mut dyn FnMut(&'a CHirExpr<'a>) -> bool,
) -> bool {
    use crate::{BinaryOp, FloatSuffix, UnaryOp};
    match expression {
        CHirExpr::Binary(BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div, ..)
        | CHirExpr::Unary(UnaryOp::Neg, ..) => true,
        CHirExpr::FloatLit(literal, _) => {
            matches!(literal.suffix, FloatSuffix::F16 | FloatSuffix::BF16)
        }
        CHirExpr::Unary(UnaryOp::Plus, operand, _) | CHirExpr::EvaluateOnce(operand, _) => {
            half_carrier(operand, is_half)
        }
        CHirExpr::Ternary(_, left, right, _) | CHirExpr::Elvis(left, right, _) => [left, right]
            .into_iter()
            .any(|arm| !is_half(arm) || half_carrier(arm, is_half)),
        CHirExpr::Comma(_, right, _) | CHirExpr::Binary(BinaryOp::Comma, _, right, _) => {
            half_carrier(right, is_half)
        }
        _ => false,
    }
}

/// Representation masks are shared by constant, interpreted and IR lowering.
pub fn masks(kind: FloatKind) -> Option<(u128, u128)> {
    match kind {
        FloatKind::F32 => Some((0x80000000, 0x7f800000)),
        FloatKind::F64 => Some((0x8000000000000000, 0x7ff0000000000000)),
        FloatKind::F128 => Some((1 << 127, 0x7fff << 112)),
        _ => None,
    }
}

pub fn evaluate(op: FloatingBuiltin, arguments: &[FloatBits]) -> Option<u128> {
    use FloatingBuiltin::*;
    let a = *arguments.first()?;
    let (sign, exponent) = masks(a.kind())?;
    if op.is_comparison() {
        use std::cmp::Ordering::{Equal, Greater, Less};
        let b = *arguments.get(1)?;
        if a.kind() != b.kind() {
            return None;
        }
        let order = a.compare(b);
        return Some(u128::from(match op {
            IsGreater => order == Some(Greater),
            IsGreaterEqual => matches!(order, Some(Greater | Equal)),
            IsLess => order == Some(Less),
            IsLessEqual => matches!(order, Some(Less | Equal)),
            IsLessGreater => matches!(order, Some(Less | Greater)),
            IsUnordered => order.is_none(),
            _ => unreachable!(),
        }));
    }
    let bits = a.bits();
    let magnitude = bits & !sign;
    Some(match op {
        IsNan => u128::from(magnitude > exponent),
        IsInf => u128::from(magnitude == exponent),
        IsInfSign => {
            if magnitude == exponent {
                if bits & sign == 0 {
                    1
                } else {
                    u128::from(u32::MAX)
                }
            } else {
                0
            }
        }
        IsFinite => u128::from(magnitude < exponent),
        IsNormal => u128::from(magnitude & exponent != 0 && magnitude < exponent),
        FpClassify(classes) => {
            let class = if magnitude > exponent {
                classes[0]
            } else if magnitude == exponent {
                classes[1]
            } else if magnitude & exponent != 0 {
                classes[2]
            } else if magnitude != 0 {
                classes[3]
            } else {
                classes[4]
            };
            u128::from(class as u32)
        }
        SignBit => u128::from(bits & sign != 0),
        Abs => magnitude,
        CopySign => {
            let b = *arguments.get(1)?;
            if a.kind() != b.kind() {
                return None;
            }
            magnitude | (b.bits() & sign)
        }
        IsGreater | IsGreaterEqual | IsLess | IsLessEqual | IsLessGreater | IsUnordered => {
            unreachable!()
        }
    })
}
