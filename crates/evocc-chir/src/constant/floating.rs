//! Fold pure binary32/binary64 expressions with the runtime target semantics.

use super::*;
use crate::complex::ComplexOp;
use crate::{FloatConstant, FloatLit, FloatSuffix};
use evocc_types::floating::{libgcc_complex, ComplexArithmetic, FiniteOp, FloatBits, FpRounding};
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
        let value = match (self, kind) {
            (Self::Integer(value), _) if value.ty().is_signed() => {
                FloatBits::from_signed(kind, value.mathematical_value())
            }
            (Self::Integer(value), _) => FloatBits::from_unsigned(kind, value.raw_bits()),
            (Self::Real(value), _) => value.target_convert(kind)?,
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
                } else if source == FloatKind::F128 {
                    1 << 111
                } else {
                    0x8000000000000
                };
                FloatBits::from_bits(
                    source,
                    (exponent | quiet | if negative { sign } else { 0 }) | (payload & (quiet - 1)),
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

/// The common type of two operands. Both half formats rank below binary32;
/// a mixture of the two has no common type, which the front end reports.
fn common(left: NumberType, right: NumberType) -> Option<NumberType> {
    use NumberType::{Integer, Real};
    match (left, right) {
        (Real(FloatKind::F128), _) | (_, Real(FloatKind::F128)) => Some(Real(FloatKind::F128)),
        (Real(FloatKind::F64), _) | (_, Real(FloatKind::F64)) => Some(Real(FloatKind::F64)),
        (Real(FloatKind::F32), _) | (_, Real(FloatKind::F32)) => Some(Real(FloatKind::F32)),
        (Real(left), Real(right)) => (left == right).then_some(Real(left)),
        (Real(half), Integer(_)) | (Integer(_), Real(half)) => Some(Real(half)),
        (Integer(left), Integer(right)) => Some(Integer(usual_arithmetic_type(
            CInteger::new(0, left).promoted().ty(),
            CInteger::new(0, right).promoted().ty(),
        ))),
    }
}

/// The format in which an operation of operands of `kind` evaluates: an
/// operation on half operands evaluates in binary32 (EvoIR 3.13.6).
fn evaluation_format(kind: FloatKind) -> FloatKind {
    if kind.is_half() {
        FloatKind::F32
    } else {
        kind
    }
}

/// A conditional of half type evaluates in binary32, as in GCC 14: an
/// evaluated arm keeps its binary32 value, and an integer arm converts to
/// binary32 directly. Any other selected value converts to the type of the
/// conditional.
fn convert_selected(value: Number, target: NumberType) -> Option<Number> {
    match (value, target) {
        (Number::Real(value), NumberType::Real(kind))
            if kind.is_half() && value.kind() == FloatKind::F32 =>
        {
            Some(Number::Real(value))
        }
        (value @ Number::Integer(_), NumberType::Real(kind)) if kind.is_half() => {
            convert(value, NumberType::Real(FloatKind::F32))
        }
        _ => convert(value, target),
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

pub(super) fn expression_kind(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<FloatKind> {
    match (Folder {
        types,
        symbols,
        target,
    })
    .ty(expression, 0)?
    {
        NumberType::Real(kind) => Some(kind),
        NumberType::Integer(_) => None,
    }
}

impl Folder<'_> {
    /// A floating type folds in its target format, so a Darwin
    /// `long double` folds as binary64.
    fn declared_type(&self, ty: TypeId) -> Option<NumberType> {
        match self.types.get(ty)?.kind {
            TypeKind::Float(kind) => Some(NumberType::Real(self.target.float_format(kind))),
            TypeKind::TargetBuiltin(builtin) if builtin.half_format().is_some() => {
                builtin.half_format().map(NumberType::Real)
            }
            _ => canonical_integer_type(self.types, ty).map(NumberType::Integer),
        }
    }

    fn ty(&self, expression: &CHirExpr<'_>, depth: usize) -> Option<NumberType> {
        if depth == 128 {
            return None;
        }
        match expression {
            CHirExpr::Floating(operation) => self.declared_type(operation.facts.ty()),
            CHirExpr::Complex(operation) => self.declared_type(operation.facts.ty()),
            CHirExpr::FloatLit(literal, _) => Some(NumberType::Real(
                self.target.float_format(literal.suffix.type_kind()),
            )),
            CHirExpr::Cast(ty, ..) => self.declared_type(*ty),
            CHirExpr::Unary(UnaryOp::Plus | UnaryOp::Neg, operand, _)
            | CHirExpr::EvaluateOnce(operand, _) => self.ty(operand, depth + 1),
            CHirExpr::Comma(_, value, _) | CHirExpr::Binary(BinaryOp::Comma, _, value, _) => {
                self.ty(value, depth + 1)
            }
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
            CHirExpr::Complex(operation) => self.complex_projection(operation, depth),
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
                    // Negation of a half operand evaluates in binary32.
                    (UnaryOp::Neg, Number::Real(value)) => Some(Number::Real(
                        value
                            .target_convert(evaluation_format(value.kind()))?
                            .negated(),
                    )),
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
                convert_selected(value, self.ty(expression, depth)?)
            }
            CHirExpr::Elvis(left, right, _) => {
                let value = self.eval(left, depth + 1)?;
                let selected = if value.truth() {
                    value
                } else {
                    self.eval(right, depth + 1)?
                };
                convert_selected(selected, self.ty(expression, depth)?)
            }
            CHirExpr::Binary(op, left, right, _) => {
                let operand_type = common(self.ty(left, depth + 1)?, self.ty(right, depth + 1)?)?;
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
                let NumberType::Real(kind) = operand_type else {
                    return None;
                };
                let kind = evaluation_format(kind);
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

/// Why a floating complex expression has no translation-time value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplexConstantError {
    /// The expression is not an arithmetic constant expression.
    NotConstant,
    /// A multiplication or division needs a runtime helper that has no
    /// execution model on this target: Darwin's compiler-rt.
    UnmodeledHelper,
}

/// One operand of complex arithmetic: a real value stays real.
#[derive(Clone, Copy)]
enum Operand {
    Real(FloatBits),
    Complex([FloatBits; 2]),
}

impl Folder<'_> {
    /// The part format of the floating complex type of `expression`.
    fn complex_format(&self, expression: &CHirExpr<'_>) -> Option<FloatKind> {
        let ty = crate::vector::expression_type(self.types, expression)?;
        crate::complex::floating_format(self.types, self.target, ty)
    }

    /// An operand of complex arithmetic in `kind`: a floating complex
    /// operand gives its parts, and a real or integer complex operand
    /// converts.
    fn complex_operand(
        &mut self,
        expression: &CHirExpr<'_>,
        kind: FloatKind,
        depth: usize,
    ) -> Result<Operand, ComplexConstantError> {
        use ComplexConstantError::NotConstant;
        if self.complex_format(expression).is_some() {
            let parts = self.complex(expression, depth + 1)?;
            return parts
                .iter()
                .map(|part| part.target_convert(kind))
                .collect::<Option<Vec<_>>>()
                .map(|parts| Operand::Complex([parts[0], parts[1]]))
                .ok_or(NotConstant);
        }
        let ty = crate::vector::expression_type(self.types, expression);
        if ty.is_some_and(|ty| crate::complex::rank(self.types, ty).is_some()) {
            let parts = crate::complex::constant_components(
                expression,
                self.types,
                self.symbols,
                self.target,
            )
            .ok_or(NotConstant)?;
            let parts = parts
                .iter()
                .map(|part| Number::Integer(*part).real(kind))
                .collect::<Option<Vec<_>>>()
                .ok_or(NotConstant)?;
            return Ok(Operand::Complex([parts[0], parts[1]]));
        }
        let value = self.eval(expression, depth + 1).ok_or(NotConstant)?;
        value.real(kind).map(Operand::Real).ok_or(NotConstant)
    }

    /// The parts of a constant floating complex expression, folded at
    /// translation time in round-to-nearest.
    fn complex(
        &mut self,
        expression: &CHirExpr<'_>,
        depth: usize,
    ) -> Result<[FloatBits; 2], ComplexConstantError> {
        use ComplexConstantError::{NotConstant, UnmodeledHelper};
        if depth >= 128 {
            return Err(NotConstant);
        }
        let operation = match expression {
            CHirExpr::Complex(operation) => operation,
            CHirExpr::EvaluateOnce(value, _) => return self.complex(value, depth + 1),
            _ => return Err(NotConstant),
        };
        let kind = self.complex_format(expression).ok_or(NotConstant)?;
        let zero = FloatBits::from_bits(kind, 0).ok_or(NotConstant)?;
        let operand = |this: &mut Self, index: usize| {
            let operand = operation.operands.get(index).ok_or(NotConstant)?;
            this.complex_operand(operand, kind, depth)
        };
        let real = |value: Operand| match value {
            Operand::Real(value) => Ok(value),
            Operand::Complex(_) => Err(NotConstant),
        };
        let parts = |value: Operand| match value {
            Operand::Real(value) => [value, zero],
            Operand::Complex(parts) => parts,
        };
        let arithmetic = |a: FloatBits, b: FloatBits, op| a.target_binary(b, op).ok_or(NotConstant);
        Ok(match operation.op {
            ComplexOp::Imaginary => [zero, real(operand(self, 0)?)?],
            ComplexOp::Construct => [real(operand(self, 0)?)?, real(operand(self, 1)?)?],
            ComplexOp::Convert | ComplexOp::Unary(UnaryOp::Plus) => parts(operand(self, 0)?),
            ComplexOp::Unary(UnaryOp::Neg) => parts(operand(self, 0)?).map(FloatBits::negated),
            ComplexOp::Unary(UnaryOp::BitNot) => {
                let [a, b] = parts(operand(self, 0)?);
                [a, b.negated()]
            }
            ComplexOp::Binary(op @ (BinaryOp::Add | BinaryOp::Sub)) => {
                let finite = if op == BinaryOp::Add {
                    FiniteOp::Add
                } else {
                    FiniteOp::Sub
                };
                match (operand(self, 0)?, operand(self, 1)?) {
                    (Operand::Complex([a, b]), Operand::Complex([c, d])) => {
                        [arithmetic(a, c, finite)?, arithmetic(b, d, finite)?]
                    }
                    (Operand::Complex([a, b]), Operand::Real(r)) => [arithmetic(a, r, finite)?, b],
                    (Operand::Real(r), Operand::Complex([a, b])) => [
                        arithmetic(r, a, finite)?,
                        if op == BinaryOp::Add { b } else { b.negated() },
                    ],
                    (Operand::Real(_), Operand::Real(_)) => return Err(NotConstant),
                }
            }
            ComplexOp::Binary(op @ (BinaryOp::Mul | BinaryOp::Div)) => {
                let finite = if op == BinaryOp::Mul {
                    FiniteOp::Mul
                } else {
                    FiniteOp::Div
                };
                match (operand(self, 0)?, operand(self, 1)?) {
                    (Operand::Complex([a, b]), Operand::Complex([c, d])) => {
                        if self.target.compiler_rt_complex() {
                            return Err(UnmodeledHelper);
                        }
                        let op = if op == BinaryOp::Mul {
                            ComplexArithmetic::Multiply
                        } else {
                            ComplexArithmetic::Divide
                        };
                        libgcc_complex(op, [a, b, c, d], FpRounding::NearestEven)
                            .ok_or(NotConstant)?
                            .value
                    }
                    (Operand::Complex([a, b]), Operand::Real(r)) => {
                        [arithmetic(a, r, finite)?, arithmetic(b, r, finite)?]
                    }
                    (Operand::Real(r), Operand::Complex([a, b])) if op == BinaryOp::Mul => {
                        [arithmetic(r, a, finite)?, arithmetic(r, b, finite)?]
                    }
                    _ => return Err(NotConstant),
                }
            }
            _ => return Err(NotConstant),
        })
    }

    /// A real or integer result of a complex node: a conversion, a part, a
    /// truth value or an equality.
    fn complex_projection(
        &mut self,
        operation: &crate::complex::ComplexExpr<'_>,
        depth: usize,
    ) -> Option<Number> {
        let source = operation.operands.first()?;
        let source_kind = self.complex_format(source);
        let declared = self.declared_type(operation.facts.ty());
        match operation.op {
            ComplexOp::Convert => {
                source_kind?;
                let [real, imaginary] = self.complex(source, depth + 1).ok()?;
                if declared.is_some_and(
                    |ty| matches!(ty, NumberType::Integer(ty) if ty == CIntegerType::BOOL),
                ) {
                    let truth = !real.is_zero() || !imaginary.is_zero();
                    return Some(Number::Integer(CInteger::new(
                        u64::from(truth),
                        CIntegerType::BOOL,
                    )));
                }
                convert(Number::Real(real), declared?)
            }
            ComplexOp::Component { imaginary } => {
                if source_kind.is_some() {
                    let parts = self.complex(source, depth + 1).ok()?;
                    return Some(Number::Real(parts[usize::from(imaginary)]));
                }
                let NumberType::Real(kind) = declared? else {
                    return None;
                };
                let value = self.eval(source, depth + 1)?;
                if imaginary {
                    Some(Number::Real(FloatBits::from_bits(kind, 0)?))
                } else {
                    convert(value, NumberType::Real(kind))
                }
            }
            ComplexOp::Binary(op @ (BinaryOp::Eq | BinaryOp::Ne)) => {
                let left = self.complex(source, depth + 1).ok()?;
                let right = self.complex(operation.operands.get(1)?, depth + 1).ok()?;
                let equal = left[0].compare(right[0]) == Some(Ordering::Equal)
                    && left[1].compare(right[1]) == Some(Ordering::Equal);
                Some(Number::Integer(CInteger::int(i32::from(
                    equal == (op == BinaryOp::Eq),
                ))))
            }
            ComplexOp::Unary(UnaryOp::Not) => {
                let [real, imaginary] = self.complex(source, depth + 1).ok()?;
                Some(Number::Integer(CInteger::int(i32::from(
                    real.is_zero() && imaginary.is_zero(),
                ))))
            }
            _ => None,
        }
    }
}

pub(super) fn complex_value(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Result<[FloatBits; 2], ComplexConstantError> {
    Folder {
        types,
        symbols,
        target,
    }
    .complex(expression, 0)
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
        CHirExpr::Complex(operation) => {
            crate::complex::floating(types, operation.facts.ty()).is_none()
                && operation.operands.iter().any(|operand| {
                    crate::vector::expression_type(types, operand)
                        .is_some_and(|ty| crate::complex::floating(types, ty).is_some())
                })
        }
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
    let kind = match types.get(destination)?.kind {
        TypeKind::Float(kind) => target.float_format(kind),
        TypeKind::TargetBuiltin(builtin) => builtin.half_format()?,
        _ => return None,
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
            // A format conversion quiets the NaN and keeps its sign and its
            // highest payload bits, as at run time.
            let value = value.real(kind)?;
            return Some(CInteger::from_raw(value.bits(), representation_type(kind)));
        }
        let (width, fraction, exponent, ty) = match kind {
            FloatKind::F16 => (16, 10, 0x1f, CIntegerType::USHORT),
            FloatKind::BF16 => (16, 7, 0xff, CIntegerType::USHORT),
            FloatKind::F32 => (32, 23, 0xff, CIntegerType::UINT),
            FloatKind::F64 => (64, 52, 0x7ff, CIntegerType::ULONG),
            FloatKind::F128 => (128, 112, 0x7fff, CIntegerType::UINT128),
        };
        let bits = (u128::from(negative) << (width - 1))
            | ((exponent as u128) << fraction)
            | (1u128 << (fraction - 1))
            | (payload & ((1u128 << fraction) - 1));
        return Some(CInteger::from_raw(bits, ty));
    }
    let value = value.real(kind)?;
    Some(CInteger::from_raw(value.bits(), representation_type(kind)))
}

/// The unsigned integer type of a format's representation bits.
fn representation_type(kind: FloatKind) -> CIntegerType {
    match kind {
        FloatKind::F16 | FloatKind::BF16 => CIntegerType::USHORT,
        FloatKind::F32 => CIntegerType::UINT,
        FloatKind::F64 => CIntegerType::ULONG,
        FloatKind::F128 => CIntegerType::UINT128,
    }
}
