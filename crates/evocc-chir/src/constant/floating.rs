//! Fold pure binary32/binary64 expressions with the runtime target semantics.

use super::*;
use crate::{FloatConstant, FloatLit, FloatSuffix};
use evocc_types::floating::{FiniteOp, FloatBits};
use evocc_types::FloatKind;
use std::cmp::Ordering;

#[derive(Clone, Copy)]
enum Number {
    Integer(CInteger),
    Real(FloatBits),
    QuietNaN {
        payload: u128,
        negative: bool,
        kind: FloatKind,
    },
}

impl Number {
    fn truth(self) -> bool {
        match self {
            Self::Integer(value) => value.truth(),
            Self::Real(value) => !value.is_zero(),
            Self::QuietNaN { .. } => true,
        }
    }

    fn real(self, kind: FloatKind) -> Option<FloatBits> {
        if !matches!(kind, FloatKind::F32 | FloatKind::F64) {
            return None;
        }
        let value = match (self, kind) {
            (Self::Integer(value), _) if value.ty().is_signed() => {
                FloatBits::from_signed(kind, value.mathematical_value())
            }
            (Self::Integer(value), _) => FloatBits::from_unsigned(kind, value.raw_bits()),
            (Self::Real(value), _) => {
                // Target headers spell binary64 limits as a cast of an exact
                // binary128 literal. Convert its already-rounded source bits;
                // this does not admit binary128 arithmetic or object reads.
                if matches!(value.kind(), FloatKind::F32 | FloatKind::F64) {
                    value.target_convert(kind)?
                } else {
                    value.convert(kind)?
                }
            }
            (
                Self::QuietNaN {
                    payload,
                    negative,
                    kind: source,
                },
                _,
            ) => {
                let (sign, exponent) = crate::floating::masks(source)?;
                let quiet = if source == FloatKind::F32 {
                    0x400000
                } else {
                    0x8000000000000
                };
                FloatBits::from_bits(
                    source,
                    u128::from(exponent | quiet | if negative { sign } else { 0 })
                        | (payload & u128::from(quiet - 1)),
                )?
                .target_convert(kind)?
            }
        };
        Some(value)
    }
}

#[derive(Clone, Copy)]
enum NumberType {
    Integer(CIntegerType),
    Real(FloatKind),
}

fn common(left: NumberType, right: NumberType) -> Option<NumberType> {
    use NumberType::{Integer, Real};
    match (left, right) {
        (Real(FloatKind::F128 | FloatKind::F16), _)
        | (_, Real(FloatKind::F128 | FloatKind::F16)) => None,
        (Real(FloatKind::F64), _) | (_, Real(FloatKind::F64)) => Some(Real(FloatKind::F64)),
        (Real(FloatKind::F32), _) | (_, Real(FloatKind::F32)) => Some(Real(FloatKind::F32)),
        (Integer(left), Integer(right)) => Some(Integer(usual_arithmetic_type(
            CInteger::new(0, left).promoted().ty(),
            CInteger::new(0, right).promoted().ty(),
        ))),
    }
}

fn convert(value: Number, target: NumberType) -> Option<Number> {
    match (value, target) {
        (Number::Integer(value), NumberType::Integer(target)) => {
            Some(Number::Integer(value.cast(target)))
        }
        (Number::Real(value), NumberType::Integer(target)) => floating_literal_to_integer(
            FloatLit {
                value: FloatConstant::Real(value),
                suffix: FloatSuffix::None,
            },
            target,
        )
        .ok()
        .map(Number::Integer),
        (value @ Number::QuietNaN { kind, .. }, NumberType::Real(target)) if kind == target => {
            Some(value)
        }
        (value @ Number::QuietNaN { .. }, NumberType::Real(kind)) => {
            Some(Number::Real(value.real(kind)?))
        }
        (Number::QuietNaN { .. }, _) => None,
        (value, NumberType::Real(kind)) => Some(Number::Real(value.real(kind)?)),
    }
}

struct Folder<'a> {
    types: &'a TypeInterner,
    symbols: &'a SymbolTable,
    target: &'a TargetDataLayout,
}

impl Folder<'_> {
    fn declared_type(&self, ty: TypeId) -> Option<NumberType> {
        match self.types.get(ty)?.kind {
            TypeKind::Float(kind) => Some(NumberType::Real(kind)),
            _ => canonical_integer_type(self.types, ty).map(NumberType::Integer),
        }
    }

    fn ty(&self, expression: &CHirExpr<'_>, depth: usize) -> Option<NumberType> {
        if depth == 128 {
            return None;
        }
        match expression {
            CHirExpr::Floating(operation) => self.declared_type(operation.facts.ty()),
            CHirExpr::FloatLit(literal, _) => Some(NumberType::Real(match literal.suffix {
                FloatSuffix::None => FloatKind::F64,
                FloatSuffix::F => FloatKind::F32,
                FloatSuffix::L => FloatKind::F128,
            })),
            CHirExpr::Cast(ty, ..) => self.declared_type(*ty),
            CHirExpr::Unary(UnaryOp::Plus | UnaryOp::Neg, operand, _)
            | CHirExpr::EvaluateOnce(operand, _) => self.ty(operand, depth + 1),
            CHirExpr::Binary(op, left, right, _) => {
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::And
                        | BinaryOp::Or
                ) {
                    Some(NumberType::Integer(CIntegerType::INT))
                } else {
                    common(self.ty(left, depth + 1)?, self.ty(right, depth + 1)?)
                }
            }
            CHirExpr::Ternary(_, left, right, _) | CHirExpr::Elvis(left, right, _) => {
                common(self.ty(left, depth + 1)?, self.ty(right, depth + 1)?)
            }
            _ => integer_expression_type(expression, self.types, self.symbols)
                .map(NumberType::Integer)
                .or_else(|| {
                    self.declared_type(attribute_query_expression_type(expression, self.types)?)
                }),
        }
    }

    fn eval(&mut self, expression: &CHirExpr<'_>, depth: usize) -> Option<Number> {
        if depth == 128 {
            return None;
        }
        match expression {
            CHirExpr::Floating(operation) => {
                let mut arguments = Vec::new();
                for operand in &operation.operands {
                    let NumberType::Real(kind) = self.ty(operand, depth + 1)? else {
                        return None;
                    };
                    arguments.push(self.eval(operand, depth + 1)?.real(kind)?);
                }
                let bits = crate::floating::evaluate(operation.op, &arguments)?;
                Some(
                    if matches!(
                        operation.op,
                        crate::floating::FloatingBuiltin::Abs
                            | crate::floating::FloatingBuiltin::CopySign
                    ) {
                        Number::Real(FloatBits::from_bits(arguments[0].kind(), bits)?)
                    } else {
                        Number::Integer(CInteger::from_raw(bits, CIntegerType::INT))
                    },
                )
            }
            CHirExpr::FloatLit(literal, _) => {
                let ty = self.ty(expression, depth)?;
                match (literal.value, ty) {
                    (FloatConstant::Real(value), _) => Some(Number::Real(value)),
                    (FloatConstant::QuietNaN(payload), NumberType::Real(kind)) => {
                        Some(Number::QuietNaN {
                            payload,
                            negative: false,
                            kind,
                        })
                    }
                    _ => None,
                }
            }
            CHirExpr::Cast(ty, operand, _) => {
                let value = self.eval(operand, depth + 1)?;
                convert(value, self.declared_type(*ty)?)
            }
            CHirExpr::EvaluateOnce(value, _) => self.eval(value, depth + 1),
            CHirExpr::Unary(op, operand, _) => {
                let value = self.eval(operand, depth + 1)?;
                match (op, value) {
                    (_, Number::Integer(value)) => eval_constant_unary_integer(*op, value)
                        .ok()
                        .map(Number::Integer),
                    (UnaryOp::Plus, value) => Some(value),
                    (UnaryOp::Neg, Number::Real(value)) => Some(Number::Real(value.negated())),
                    (
                        UnaryOp::Neg,
                        Number::QuietNaN {
                            payload,
                            negative,
                            kind,
                        },
                    ) => Some(Number::QuietNaN {
                        payload,
                        negative: !negative,
                        kind,
                    }),
                    (UnaryOp::Not, value) => {
                        Some(Number::Integer(CInteger::int(i32::from(!value.truth()))))
                    }
                    _ => None,
                }
            }
            CHirExpr::Ternary(condition, left, right, _) => {
                let condition = self.eval(condition, depth + 1)?.truth();
                let value = self.eval(if condition { left } else { right }, depth + 1)?;
                convert(value, self.ty(expression, depth)?)
            }
            CHirExpr::Elvis(left, right, _) => {
                let value = self.eval(left, depth + 1)?;
                let selected = if value.truth() {
                    value
                } else {
                    self.eval(right, depth + 1)?
                };
                convert(selected, self.ty(expression, depth)?)
            }
            CHirExpr::Binary(op, left, right, _) => {
                if [left, right].iter().any(|operand| {
                    matches!(
                        self.ty(operand, depth + 1),
                        Some(NumberType::Real(FloatKind::F16 | FloatKind::F128))
                    )
                }) {
                    return None;
                }
                let left = self.eval(left, depth + 1)?;
                if (*op == BinaryOp::And && !left.truth()) || (*op == BinaryOp::Or && left.truth())
                {
                    return Some(Number::Integer(CInteger::int(i32::from(left.truth()))));
                }
                let right = self.eval(right, depth + 1)?;
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    return Some(Number::Integer(CInteger::int(i32::from(right.truth()))));
                }
                if let (Number::Integer(left), Number::Integer(right)) = (left, right) {
                    return if matches!(op, BinaryOp::Div | BinaryOp::Mod) {
                        eval_binary_integer(*op, left, right)
                    } else {
                        eval_constant_binary_integer(*op, left, right)
                    }
                    .ok()
                    .map(Number::Integer);
                }
                let kind = if matches!(left, Number::Real(v) if v.kind() == FloatKind::F64)
                    || matches!(right, Number::Real(v) if v.kind() == FloatKind::F64)
                    || matches!(
                        left,
                        Number::QuietNaN {
                            kind: FloatKind::F64,
                            ..
                        }
                    )
                    || matches!(
                        right,
                        Number::QuietNaN {
                            kind: FloatKind::F64,
                            ..
                        }
                    ) {
                    FloatKind::F64
                } else {
                    FloatKind::F32
                };
                let a = left.real(kind)?;
                let b = right.real(kind)?;
                let comparison = match op {
                    BinaryOp::Eq => Some(a.compare(b) == Some(Ordering::Equal)),
                    BinaryOp::Ne => Some(a.compare(b) != Some(Ordering::Equal)),
                    BinaryOp::Lt => Some(a.compare(b) == Some(Ordering::Less)),
                    BinaryOp::Le => Some(matches!(
                        a.compare(b),
                        Some(Ordering::Less | Ordering::Equal)
                    )),
                    BinaryOp::Gt => Some(a.compare(b) == Some(Ordering::Greater)),
                    BinaryOp::Ge => Some(matches!(
                        a.compare(b),
                        Some(Ordering::Greater | Ordering::Equal)
                    )),
                    _ => None,
                };
                if let Some(value) = comparison {
                    return Some(Number::Integer(CInteger::int(i32::from(value))));
                }
                let operation = match op {
                    BinaryOp::Add => FiniteOp::Add,
                    BinaryOp::Sub => FiniteOp::Sub,
                    BinaryOp::Mul => FiniteOp::Mul,
                    BinaryOp::Div => FiniteOp::Div,
                    _ => return None,
                };
                a.target_binary(b, operation).map(Number::Real)
            }
            _ => {
                eval_executable_integer_constant(expression, self.types, self.symbols, self.target)
                    .map(Number::Integer)
            }
        }
    }
}

pub(super) fn value(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<FloatBits> {
    match (Folder {
        types,
        symbols,
        target,
    })
    .eval(expression, 0)?
    {
        Number::Real(value) => Some(value),
        _ => None,
    }
}

pub(super) fn truth(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<bool> {
    Folder {
        types,
        symbols,
        target,
    }
    .eval(expression, 0)
    .map(Number::truth)
}

pub(super) fn fold(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<CInteger> {
    let mut folder = Folder {
        types,
        symbols,
        target,
    };
    // Avoid reevaluating every integer subtree at each ordinary lowering node.
    let relevant = match expression {
        CHirExpr::Floating(_) => true,
        CHirExpr::Cast(ty, operand, _) if canonical_integer_type(types, *ty).is_some() => {
            matches!(folder.ty(operand, 0), Some(NumberType::Real(_)))
        }
        CHirExpr::Unary(UnaryOp::Not, operand, _) => {
            matches!(folder.ty(operand, 0), Some(NumberType::Real(_)))
        }
        CHirExpr::Binary(
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::And
            | BinaryOp::Or,
            left,
            right,
            _,
        ) => {
            matches!(folder.ty(left, 0), Some(NumberType::Real(_)))
                || matches!(folder.ty(right, 0), Some(NumberType::Real(_)))
        }
        _ => false,
    };
    if !relevant {
        return None;
    }
    let Number::Integer(value) = folder.eval(expression, 0)? else {
        return None;
    };
    Some(value)
}

pub(super) fn representation(
    expression: &CHirExpr<'_>,
    destination: TypeId,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<CInteger> {
    let TypeKind::Float(kind) = types.get(destination)?.kind else {
        return None;
    };
    let value = Folder {
        types,
        symbols,
        target,
    }
    .eval(expression, 0)?;
    if let Number::QuietNaN {
        payload,
        negative,
        kind: source_kind,
    } = value
    {
        if source_kind != kind {
            return None;
        }
        let (width, fraction, exponent, ty) = match kind {
            FloatKind::F32 => (32, 23, 0xff, CIntegerType::UINT),
            FloatKind::F64 => (64, 52, 0x7ff, CIntegerType::ULONG),
            FloatKind::F128 => (128, 112, 0x7fff, CIntegerType::UINT128),
            FloatKind::F16 => return None,
        };
        let bits = (u128::from(negative) << (width - 1))
            | ((exponent as u128) << fraction)
            | (1u128 << (fraction - 1))
            | (payload & ((1u128 << fraction) - 1));
        return Some(CInteger::from_raw(bits, ty));
    }
    let value = value.real(kind)?;
    Some(match kind {
        FloatKind::F32 => CInteger::from_raw(value.bits(), CIntegerType::UINT),
        FloatKind::F64 => CInteger::from_raw(value.bits(), CIntegerType::ULONG),
        _ => unreachable!("representation kind was checked"),
    })
}
