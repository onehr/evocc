//! The task machine shares exact target arithmetic with constant evaluation.
use super::*;
use evocc_types::floating::{FiniteOp, FloatBits};
use evocc_types::FloatKind;
use std::cmp::Ordering;

pub(super) fn builtin(
    expression: &crate::floating::FloatingExpr<'_>,
    values: &[CValue],
) -> Result<CValue, InterpreterError> {
    let span = expression.span;
    let arguments = values
        .iter()
        .map(|v| match v {
            CValue::Floating(v) => Ok(*v),
            CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
            _ => Err(InterpreterError::UnsupportedExpression {
                feature: "floating builtin operand",
                span,
            }),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let raw = crate::floating::evaluate(expression.op, &arguments).ok_or(
        InterpreterError::UnsupportedExpression {
            feature: "deferred floating builtin format",
            span,
        },
    )?;
    Ok(
        if matches!(
            expression.op,
            crate::floating::FloatingBuiltin::Abs | crate::floating::FloatingBuiltin::CopySign
        ) {
            CValue::Floating(FloatBits::from_bits(arguments[0].kind(), raw).unwrap())
        } else {
            CValue::Integer(CInteger::from_raw(raw, CIntegerType::INT))
        },
    )
}

pub(super) fn convert(
    value: CValue,
    kind: FloatKind,
    span: SourceSpan,
) -> Result<FloatBits, InterpreterError> {
    match value {
        CValue::Integer(v) => Ok(if v.ty().is_signed() {
            FloatBits::from_signed(kind, v.mathematical_value())
        } else {
            FloatBits::from_unsigned(kind, v.raw_bits())
        }),
        CValue::Floating(v) => {
            v.target_convert(kind)
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "deferred floating format",
                    span,
                })
        }
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
        _ => Err(InterpreterError::UnsupportedExpression {
            feature: "non-arithmetic floating operand",
            span,
        }),
    }
}

pub(super) fn binary(
    op: BinaryOp,
    left: CValue,
    right: CValue,
    span: SourceSpan,
) -> Result<CValue, InterpreterError> {
    let kind = if [left, right]
        .iter()
        .any(|v| matches!(v, CValue::Floating(v) if v.kind() == FloatKind::F64))
    {
        FloatKind::F64
    } else {
        FloatKind::F32
    };
    let a = convert(left, kind, span)?;
    let b = convert(right, kind, span)?;
    let arithmetic = match op {
        BinaryOp::Add => Some(FiniteOp::Add),
        BinaryOp::Sub => Some(FiniteOp::Sub),
        BinaryOp::Mul => Some(FiniteOp::Mul),
        BinaryOp::Div => Some(FiniteOp::Div),
        _ => None,
    };
    if let Some(op) = arithmetic {
        return Ok(CValue::Floating(
            a.target_binary(b, op).expect("binary32/64 operands"),
        ));
    }
    let order = a.compare(b);
    let result = match op {
        BinaryOp::Eq => order == Some(Ordering::Equal),
        BinaryOp::Ne => order != Some(Ordering::Equal),
        BinaryOp::Lt => order == Some(Ordering::Less),
        BinaryOp::Le => matches!(order, Some(Ordering::Less | Ordering::Equal)),
        BinaryOp::Gt => order == Some(Ordering::Greater),
        BinaryOp::Ge => matches!(order, Some(Ordering::Greater | Ordering::Equal)),
        _ => {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "invalid floating operator",
                span,
            })
        }
    };
    Ok(CValue::Integer(CInteger::int(i32::from(result))))
}
