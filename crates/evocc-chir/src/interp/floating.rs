//! The task machine shares exact target arithmetic with constant evaluation.
use super::*;
use evocc_types::floating::{FiniteOp, FloatBits, FpResult, FpStatus};
use evocc_types::FloatKind;
use std::cmp::Ordering;

/// The fixed-environment admission of a baseline operation; a strict one
/// needs none.
pub(super) fn admit_environment(
    world: &WorldState,
    strict: bool,
    span: SourceSpan,
) -> Result<(), InterpreterError> {
    admit(world, strict, span)
}

fn admit(world: &WorldState, strict: bool, span: SourceSpan) -> Result<(), InterpreterError> {
    if !strict {
        world
            .check_fixed_fp_environment()
            .map_err(|error| InterpreterError::Memory { span, error })?;
    }
    Ok(())
}

pub(super) fn builtin(
    expression: &crate::floating::FloatingExpr<'_>,
    values: &[CValue],
    world: &mut WorldState,
    strict: bool,
) -> Result<CValue, InterpreterError> {
    let span = expression.span;
    if expression.op.is_comparison() {
        admit(world, strict, span)?;
    }
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
    if strict && expression.op.is_comparison() {
        let result = arguments[0].target_compare(arguments[1], false).ok_or(
            InterpreterError::UnsupportedExpression {
                feature: "strict floating comparison format",
                span,
            },
        )?;
        *world = world.with_fp_environment(world.fp_environment().raised(result.status));
    }
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

pub(super) fn convert_runtime(
    value: CValue,
    kind: FloatKind,
    span: SourceSpan,
    world: &mut WorldState,
    strict: bool,
) -> Result<FloatBits, InterpreterError> {
    if !strict {
        if !matches!(value, CValue::Floating(value) if value.kind() == kind) {
            admit(world, strict, span)?;
        }
        return convert(value, kind, span);
    }
    if !matches!(kind, FloatKind::F32 | FloatKind::F64 | FloatKind::F128) {
        return Err(InterpreterError::UnsupportedExpression {
            feature: "strict floating format",
            span,
        });
    }
    let environment = world.fp_environment();
    let result = match value {
        CValue::Integer(value) => Some(if value.ty().is_signed() {
            FloatBits::from_signed_round(kind, value.mathematical_value(), environment.rounding)
        } else {
            FloatBits::from_unsigned_round(kind, value.raw_bits(), environment.rounding)
        }),
        CValue::Floating(value) => value.target_convert_round(kind, environment.rounding),
        CValue::Uninitialized(_) => return Err(InterpreterError::UninitializedValue { span }),
        _ => None,
    }
    .ok_or(InterpreterError::UnsupportedExpression {
        feature: "strict floating conversion",
        span,
    })?;
    *world = world.with_fp_environment(environment.raised(result.status));
    Ok(result.value)
}

pub(super) fn truth(
    value: FloatBits,
    world: &mut WorldState,
    strict: bool,
    span: SourceSpan,
) -> Result<bool, InterpreterError> {
    admit(world, strict, span)?;
    if strict && value.is_signaling() {
        *world = world.with_fp_environment(world.fp_environment().raised(FpStatus::INVALID));
    }
    Ok(!value.is_zero())
}

impl Interpreter<'_, '_> {
    pub(super) fn strict_floating(&self) -> bool {
        (self.unit.dynamic_rounding() || self.fenv_access) && !self.static_integer_evaluation
    }

    pub(super) fn cast_runtime_value(
        &mut self,
        value: CValue,
        target: ValueType,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let strict = self.strict_floating();
        if matches!(
            (value, target),
            (CValue::Floating(_), ValueType::Integer(_))
        ) {
            admit(&self.world, strict, span)?;
        }
        match (value, target) {
            (value, ValueType::Floating(kind)) => {
                convert_runtime(value, kind, span, &mut self.world, strict).map(CValue::Floating)
            }
            (CValue::Floating(value), ValueType::Integer(target)) if strict => {
                let raw = if target == CIntegerType::BOOL {
                    u128::from(truth(value, &mut self.world, strict, span)?)
                } else {
                    let result = value
                        .to_integer_status(u16::from(target.bits()), target.is_signed())
                        .ok_or(InterpreterError::FloatingToInteger { span })?;
                    let raw = result
                        .value
                        .ok_or(InterpreterError::FloatingToInteger { span })?;
                    self.world = self
                        .world
                        .with_fp_environment(self.world.fp_environment().raised(result.status));
                    raw
                };
                Ok(CValue::Integer(CInteger::from_raw(raw, target)))
            }
            _ => cast_machine_value(value, target, span, self.unit.types(), &mut self.memory),
        }
    }
}

pub(super) fn binary(
    op: BinaryOp,
    left: CValue,
    right: CValue,
    span: SourceSpan,
    world: &mut WorldState,
    strict: bool,
) -> Result<CValue, InterpreterError> {
    admit(world, strict, span)?;
    let kind = if [left, right]
        .iter()
        .any(|v| matches!(v, CValue::Floating(v) if v.kind() == FloatKind::F128))
    {
        FloatKind::F128
    } else if [left, right]
        .iter()
        .any(|v| matches!(v, CValue::Floating(v) if v.kind() == FloatKind::F64))
    {
        FloatKind::F64
    } else {
        FloatKind::F32
    };
    let a = convert_runtime(left, kind, span, world, strict)?;
    let b = convert_runtime(right, kind, span, world, strict)?;
    let arithmetic = match op {
        BinaryOp::Add => Some(FiniteOp::Add),
        BinaryOp::Sub => Some(FiniteOp::Sub),
        BinaryOp::Mul => Some(FiniteOp::Mul),
        BinaryOp::Div => Some(FiniteOp::Div),
        _ => None,
    };
    if let Some(op) = arithmetic {
        if strict {
            let result = a
                .target_binary_round(b, op, world.fp_environment().rounding)
                .expect("binary32/64 operands");
            *world = world.with_fp_environment(world.fp_environment().raised(result.status));
            return Ok(CValue::Floating(result.value));
        }
        return Ok(CValue::Floating(
            a.target_binary(b, op).expect("binary32/64 operands"),
        ));
    }
    let FpResult {
        value: order,
        status,
    } = if strict {
        a.target_compare(b, !matches!(op, BinaryOp::Eq | BinaryOp::Ne))
            .expect("binary32/64 operands")
    } else {
        FpResult {
            value: a.compare(b),
            status: FpStatus::NONE,
        }
    };
    *world = world.with_fp_environment(world.fp_environment().raised(status));
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
