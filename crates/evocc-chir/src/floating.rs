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
