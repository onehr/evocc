//! GNU integer-complex values retain both component types, independently of vectors.
use crate::integer::{CInteger, CIntegerType, IntegerError};
use crate::{BinaryOp, CHirExpr, ExprFacts, UnaryOp};
use evocc_common::SourceSpan;
use evocc_types::{ComplexBase, IntRank, TypeId, TypeInterner, TypeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplexOp {
    Imaginary,
    Component { imaginary: bool },
    Unary(UnaryOp),
    Binary(BinaryOp),
    Convert,
}

#[derive(Debug, Clone)]
pub struct ComplexExpr<'arena> {
    pub op: ComplexOp,
    pub operands: Vec<&'arena CHirExpr<'arena>>,
    pub facts: ExprFacts,
    pub span: SourceSpan,
}

pub fn rank(types: &TypeInterner, ty: TypeId) -> Option<IntRank> {
    match types.get(ty)?.kind {
        TypeKind::Complex(ComplexBase::Integer(rank)) => Some(rank),
        _ => None,
    }
}

pub fn element(types: &TypeInterner, ty: TypeId) -> Option<TypeId> {
    types.find(&evocc_types::CanonicalType {
        kind: TypeKind::Int(rank(types, ty)?),
        qualifiers: Default::default(),
    })
}

/// Component arithmetic at the selected complex width (no scalar promotion).
pub fn binary(
    op: BinaryOp,
    a: [CInteger; 2],
    b: [CInteger; 2],
    wrapping: bool,
) -> Result<[CInteger; 2], IntegerError> {
    let calc = |op, x: CInteger, y: CInteger| {
        if wrapping && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
            let raw = match op {
                BinaryOp::Add => x.raw_bits().wrapping_add(y.raw_bits()),
                BinaryOp::Sub => x.raw_bits().wrapping_sub(y.raw_bits()),
                _ => x.raw_bits().wrapping_mul(y.raw_bits()),
            };
            Ok(CInteger::from_raw(raw, x.ty()))
        } else {
            crate::integer::eval_binary_at_type(op, x, y, x.ty())
        }
    };
    let [a, b0] = a;
    let [c, d] = b;
    match op {
        BinaryOp::Div => {
            // GCC's wide algorithm (EvoIR 10.1.14). Each step is the C
            // operation in the component type; only one branch executes.
            let magnitude = |v: CInteger| {
                if !v.ty().is_signed() || v.mathematical_value() >= 0 {
                    Ok(v)
                } else if wrapping {
                    Ok(CInteger::from_raw(0u128.wrapping_sub(v.raw_bits()), v.ty()))
                } else {
                    crate::integer::eval_unary_at_type(UnaryOp::Neg, v)
                }
            };
            let (abs_c, abs_d) = (magnitude(c)?, magnitude(d)?);
            let smaller =
                crate::integer::eval_binary_at_type(BinaryOp::Lt, abs_c, abs_d, c.ty())?.truth();
            if smaller {
                let ratio = calc(BinaryOp::Div, c, d)?;
                let div = calc(BinaryOp::Add, calc(BinaryOp::Mul, c, ratio)?, d)?;
                let real = calc(BinaryOp::Add, calc(BinaryOp::Mul, a, ratio)?, b0)?;
                let real = calc(BinaryOp::Div, real, div)?;
                let imaginary = calc(BinaryOp::Sub, calc(BinaryOp::Mul, b0, ratio)?, a)?;
                Ok([real, calc(BinaryOp::Div, imaginary, div)?])
            } else {
                let ratio = calc(BinaryOp::Div, d, c)?;
                let div = calc(BinaryOp::Add, calc(BinaryOp::Mul, d, ratio)?, c)?;
                let real = calc(BinaryOp::Add, calc(BinaryOp::Mul, b0, ratio)?, a)?;
                let real = calc(BinaryOp::Div, real, div)?;
                let imaginary = calc(BinaryOp::Sub, b0, calc(BinaryOp::Mul, a, ratio)?)?;
                Ok([real, calc(BinaryOp::Div, imaginary, div)?])
            }
        }
        BinaryOp::Add | BinaryOp::Sub => Ok([calc(op, a, c)?, calc(op, b0, d)?]),
        BinaryOp::Mul => Ok([
            calc(
                BinaryOp::Sub,
                calc(BinaryOp::Mul, a, c)?,
                calc(BinaryOp::Mul, b0, d)?,
            )?,
            calc(
                BinaryOp::Add,
                calc(BinaryOp::Mul, a, d)?,
                calc(BinaryOp::Mul, b0, c)?,
            )?,
        ]),
        _ => Err(IntegerError::UnsupportedOperator),
    }
}

pub fn constant_components(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &crate::SymbolTable,
    target: &evocc_types::TargetDataLayout,
) -> Option<[CInteger; 2]> {
    let zero = |ty| CInteger::new(0, ty);
    if let CHirExpr::Complex(value) = expression {
        let args = value
            .operands
            .iter()
            .map(|operand| constant_components(operand, types, symbols, target))
            .collect::<Option<Vec<_>>>()?;
        let component = rank(types, value.facts.ty()).map(crate::integer::integer_type_for_rank);
        return match value.op {
            ComplexOp::Imaginary => Some([zero(component?), args[0][0].cast(component?)]),
            ComplexOp::Convert => {
                if let Some(ty) = component {
                    Some(args[0].map(|v| v.cast(ty)))
                } else {
                    let ty = crate::constant::canonical_integer_type(types, value.facts.ty())?;
                    let result = if ty == CIntegerType::BOOL {
                        CInteger::new(u64::from(args[0].iter().any(|v| v.truth())), ty)
                    } else {
                        args[0][0].cast(ty)
                    };
                    Some([result, zero(ty)])
                }
            }
            ComplexOp::Component { imaginary } => {
                Some([args[0][usize::from(imaginary)], zero(args[0][0].ty())])
            }
            ComplexOp::Binary(BinaryOp::Eq | BinaryOp::Ne) => {
                let equal = args[0] == args[1];
                Some([
                    CInteger::int(i32::from(
                        equal == (value.op == ComplexOp::Binary(BinaryOp::Eq)),
                    )),
                    zero(CIntegerType::INT),
                ])
            }
            ComplexOp::Binary(op) => binary(op, args[0], args[1], true).ok(),
            ComplexOp::Unary(UnaryOp::Plus) => Some(args[0]),
            ComplexOp::Unary(UnaryOp::Not) => Some([
                CInteger::int(i32::from(!args[0].iter().any(|v| v.truth()))),
                zero(CIntegerType::INT),
            ]),
            ComplexOp::Unary(UnaryOp::Neg) => {
                Some(args[0].map(|v| CInteger::from_raw(0u128.wrapping_sub(v.raw_bits()), v.ty())))
            }
            ComplexOp::Unary(UnaryOp::BitNot) => Some([
                args[0][0],
                CInteger::from_raw(0u128.wrapping_sub(args[0][1].raw_bits()), args[0][1].ty()),
            ]),
            _ => None,
        };
    }
    let value = crate::constant::eval_integer_constant(expression, types, symbols, target)?;
    Some([value, zero(value.ty())])
}
