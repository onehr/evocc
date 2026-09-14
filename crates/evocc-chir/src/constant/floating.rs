//! Fold pure binary32/binary64 arithmetic only when its result is an integer.
//! This does not introduce floating values into either execution model.

use super::*;
use crate::{FloatLit, FloatSuffix};
use evocc_types::FloatKind;

#[derive(Clone, Copy)]
enum Number {
    Integer(CInteger),
    Real(f64, FloatKind),
}

impl Number {
    fn truth(self) -> bool {
        match self {
            Self::Integer(value) => value.truth(),
            Self::Real(value, _) => value != 0.0,
        }
    }

    fn real(self, kind: FloatKind) -> Option<f64> {
        let value = match (self, kind) {
            (Self::Integer(value), FloatKind::F32) if value.ty().is_signed() => {
                (value.mathematical_value() as f32) as f64
            }
            (Self::Integer(value), FloatKind::F32) => (value.raw_bits() as f32) as f64,
            (Self::Integer(value), FloatKind::F64) if value.ty().is_signed() => {
                value.mathematical_value() as f64
            }
            (Self::Integer(value), FloatKind::F64) => value.raw_bits() as f64,
            (Self::Real(value, _), FloatKind::F32) => (value as f32) as f64,
            (Self::Real(value, _), FloatKind::F64) => value,
            _ => return None,
        };
        value.is_finite().then_some(value)
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
        (Number::Real(value, _), NumberType::Integer(target)) => floating_literal_to_integer(
            FloatLit {
                value,
                suffix: FloatSuffix::None,
            },
            target,
        )
        .ok()
        .map(Number::Integer),
        (value, NumberType::Real(kind)) => Some(Number::Real(value.real(kind)?, kind)),
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
            CHirExpr::FloatLit(literal, _) => convert(
                Number::Real(literal.value, FloatKind::F64),
                self.ty(expression, depth)?,
            ),
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
                    (UnaryOp::Neg, Number::Real(value, kind)) => Some(Number::Real(-value, kind)),
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
            CHirExpr::Binary(op, left, right, _) => {
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
                let kind = match (left, right) {
                    (Number::Real(_, FloatKind::F64), _) | (_, Number::Real(_, FloatKind::F64)) => {
                        FloatKind::F64
                    }
                    _ => FloatKind::F32,
                };
                let a = left.real(kind)?;
                let b = right.real(kind)?;
                let comparison = match op {
                    BinaryOp::Eq => Some(a == b),
                    BinaryOp::Ne => Some(a != b),
                    BinaryOp::Lt => Some(a < b),
                    BinaryOp::Le => Some(a <= b),
                    BinaryOp::Gt => Some(a > b),
                    BinaryOp::Ge => Some(a >= b),
                    _ => None,
                };
                if let Some(value) = comparison {
                    return Some(Number::Integer(CInteger::int(i32::from(value))));
                }
                let value = if kind == FloatKind::F32 {
                    let (a, b) = (a as f32, b as f32);
                    (match op {
                        BinaryOp::Add => a + b,
                        BinaryOp::Sub => a - b,
                        BinaryOp::Mul => a * b,
                        BinaryOp::Div if b != 0.0 => a / b,
                        _ => return None,
                    }) as f64
                } else {
                    match op {
                        BinaryOp::Add => a + b,
                        BinaryOp::Sub => a - b,
                        BinaryOp::Mul => a * b,
                        BinaryOp::Div if b != 0.0 => a / b,
                        _ => return None,
                    }
                };
                value.is_finite().then_some(Number::Real(value, kind))
            }
            _ => {
                eval_executable_integer_constant(expression, self.types, self.symbols, self.target)
                    .map(Number::Integer)
            }
        }
    }
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
