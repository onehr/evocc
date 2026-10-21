use super::*;
use crate::complex::{ComplexExpr, ComplexOp};
use evocc_types::floating::{libgcc_complex, ComplexArithmetic, FloatBits, FpRounding};
use evocc_types::FloatKind;

/// One operand of floating complex arithmetic: a real operand stays real
/// (C11 6.3.1.8, G.5.1).
#[derive(Clone, Copy)]
enum Operand {
    Real(CValue),
    Complex([CValue; 2]),
}

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    fn complex_component_value(
        &mut self,
        value: CValue,
        imaginary: bool,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        if let CValue::Integer(value) = value {
            return Ok(if imaginary {
                CInteger::new(0, value.ty())
            } else {
                value
            });
        }
        expect_machine_integer(self.complex_part(value, imaginary, span)?, span)
    }

    /// One part of a complex aggregate, read from its adjacent-element
    /// representation. The other part is not read.
    fn complex_part(
        &mut self,
        value: CValue,
        imaginary: bool,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let CValue::Aggregate {
            address,
            ty,
            qualifiers,
        } = value
        else {
            return Err(InterpreterError::UninitializedValue { span });
        };
        let element = crate::complex::element(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let layout = self.access_layout(element, span)?;
        let address = address
            .checked_offset(if imaginary {
                layout.size_bytes as i64
            } else {
                0
            })
            .map_err(|error| self.memory_error(error, span))?;
        self.load_memory_value(address, element, qualifiers, span)
    }

    fn is_complex_value(&self, value: CValue) -> bool {
        matches!(value, CValue::Aggregate { ty, .. } if crate::complex::admitted(self.unit.types(), ty))
    }

    fn is_floating_complex(&self, ty: TypeId) -> bool {
        crate::complex::floating(self.unit.types(), ty).is_some()
    }

    pub(super) fn complex_truth(
        &mut self,
        value: CValue,
        span: SourceSpan,
    ) -> Result<bool, InterpreterError> {
        if matches!(value, CValue::Aggregate { ty, .. } if crate::complex::rank(self.unit.types(), ty).is_some())
        {
            let a = self.complex_component_value(value, false, span)?;
            let b = self.complex_component_value(value, true, span)?;
            Ok(a.truth() || b.truth())
        } else if matches!(value, CValue::Aggregate { ty, .. } if self.is_floating_complex(ty)) {
            self.floating_complex_truth(value, span)
        } else {
            if let CValue::Floating(value) = value {
                let strict = self.strict_floating();
                return floating::truth(value, &mut self.world, strict, span);
            }
            machine_truth(value, span)
        }
    }

    /// A quiet `une` of the real part against zero, then of the imaginary
    /// part only when the real part is zero, as GCC builds the test.
    fn floating_complex_truth(
        &mut self,
        value: CValue,
        span: SourceSpan,
    ) -> Result<bool, InterpreterError> {
        let strict = self.strict_floating();
        for imaginary in [false, true] {
            let part = expect_floating(self.complex_part(value, imaginary, span)?, span)?;
            if floating::truth(part, &mut self.world, strict, span)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Store `parts` as a new temporary value of the complex type `ty`.
    fn pack_complex(
        &mut self,
        ty: TypeId,
        parts: [CValue; 2],
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let element = crate::complex::element(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let destination = self.vector_temporary(ty, span)?;
        let layout = self.access_layout(element, span)?;
        for (index, value) in parts.into_iter().enumerate() {
            let address = destination
                .checked_offset(index as i64 * layout.size_bytes as i64)
                .map_err(|e| self.memory_error(e, span))?;
            self.store_memory_value(address, element, Qualifiers::default(), value, span)?;
        }
        Ok(CValue::Aggregate {
            address: destination,
            ty,
            qualifiers: Qualifiers::default(),
        })
    }

    fn floating_operand(
        &mut self,
        value: CValue,
        span: SourceSpan,
    ) -> Result<Operand, InterpreterError> {
        if self.is_complex_value(value) {
            Ok(Operand::Complex([
                self.complex_part(value, false, span)?,
                self.complex_part(value, true, span)?,
            ]))
        } else {
            Ok(Operand::Real(value))
        }
    }

    fn floating_parts(
        &mut self,
        value: CValue,
        span: SourceSpan,
    ) -> Result<[CValue; 2], InterpreterError> {
        match self.floating_operand(value, span)? {
            Operand::Complex(parts) => Ok(parts),
            Operand::Real(_) => Err(InterpreterError::UnsupportedExpression {
                feature: "real operand of a complex part operation",
                span,
            }),
        }
    }

    /// Convert one value to the floating format `kind`, raising its status
    /// in a strict region.
    fn convert_part(
        &mut self,
        value: CValue,
        kind: FloatKind,
        span: SourceSpan,
    ) -> Result<FloatBits, InterpreterError> {
        let strict = self.strict_floating();
        floating::convert_runtime(value, kind, span, &mut self.world, strict)
    }

    /// The result of a floating complex operation in `ty`: the parts
    /// convert to its part type, which may be integer complex after a
    /// compound assignment.
    fn finish_complex(
        &mut self,
        ty: TypeId,
        parts: [FloatBits; 2],
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let element = crate::complex::element(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let target = self.value_type(element, span)?;
        let mut converted = [CValue::Uninitialized(target); 2];
        for (index, part) in parts.into_iter().enumerate() {
            converted[index] = self.cast_runtime_value(CValue::Floating(part), target, span)?;
        }
        self.pack_complex(ty, converted, span)
    }

    fn part_binary(
        &mut self,
        op: BinaryOp,
        left: FloatBits,
        right: FloatBits,
        span: SourceSpan,
    ) -> Result<FloatBits, InterpreterError> {
        let strict = self.strict_floating();
        expect_floating(
            floating::binary(
                op,
                CValue::Floating(left),
                CValue::Floating(right),
                span,
                &mut self.world,
                strict,
            )?,
            span,
        )
    }

    /// Complex multiplication or division of two complex operands: the
    /// shared model of the qualified Linux helper, in the current rounding
    /// direction. Darwin's compiler-rt sequence has no model.
    fn helper_arithmetic(
        &mut self,
        op: ComplexArithmetic,
        operands: [FloatBits; 4],
        span: SourceSpan,
    ) -> Result<[FloatBits; 2], InterpreterError> {
        if self.target.compiler_rt_complex() {
            return Err(InterpreterError::ComplexHelper { span });
        }
        let strict = self.strict_floating();
        floating::admit_environment(&self.world, strict, span)?;
        let rounding = if strict {
            self.world.fp_environment().rounding
        } else {
            FpRounding::NearestEven
        };
        let result = libgcc_complex(op, operands, rounding).ok_or(
            InterpreterError::UnsupportedExpression {
                feature: "complex helper format",
                span,
            },
        )?;
        if strict {
            self.world = self
                .world
                .with_fp_environment(self.world.fp_environment().raised(result.status));
        }
        Ok(result.value)
    }

    fn floating_binary_complex(
        &mut self,
        op: BinaryOp,
        ty: TypeId,
        arguments: &[CValue],
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let left = self.floating_operand(arguments[0], span)?;
        let right = self.floating_operand(arguments[1], span)?;
        // The computation format is the widest format of the operands; a
        // compound assignment can pair a narrower target with its value.
        let mut kind = crate::complex::floating_format(self.unit.types(), &self.target, ty);
        for operand in [left, right] {
            let values = match operand {
                Operand::Real(value) => vec![value],
                Operand::Complex(parts) => parts.to_vec(),
            };
            for value in values {
                if let CValue::Floating(value) = value {
                    kind = Some(match kind {
                        Some(previous) if format_rank(previous) >= format_rank(value.kind()) => {
                            previous
                        }
                        _ => value.kind(),
                    });
                }
            }
        }
        let kind = kind.ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let convert = |this: &mut Self, operand: Operand| -> Result<_, InterpreterError> {
            Ok(match operand {
                Operand::Real(value) => (this.convert_part(value, kind, span)?, None),
                Operand::Complex([a, b]) => (
                    this.convert_part(a, kind, span)?,
                    Some(this.convert_part(b, kind, span)?),
                ),
            })
        };
        let (a, b) = convert(self, left)?;
        let (c, d) = convert(self, right)?;
        let parts = match (op, b, d) {
            (BinaryOp::Add | BinaryOp::Sub, Some(b), Some(d)) => [
                self.part_binary(op, a, c, span)?,
                self.part_binary(op, b, d, span)?,
            ],
            (BinaryOp::Add | BinaryOp::Sub, Some(b), None) => {
                [self.part_binary(op, a, c, span)?, b]
            }
            (BinaryOp::Add | BinaryOp::Sub, None, Some(d)) => [
                self.part_binary(op, a, c, span)?,
                if op == BinaryOp::Add { d } else { d.negated() },
            ],
            (BinaryOp::Mul, Some(b), Some(d)) => {
                self.helper_arithmetic(ComplexArithmetic::Multiply, [a, b, c, d], span)?
            }
            (BinaryOp::Div, Some(b), Some(d)) => {
                self.helper_arithmetic(ComplexArithmetic::Divide, [a, b, c, d], span)?
            }
            // A real divided by a complex value is complex division of (r, +0).
            (BinaryOp::Div, None, Some(d)) => {
                let zero = FloatBits::from_bits(kind, 0).expect("zero fits every format");
                self.helper_arithmetic(ComplexArithmetic::Divide, [a, zero, c, d], span)?
            }
            (BinaryOp::Mul | BinaryOp::Div, Some(b), None) => [
                self.part_binary(op, a, c, span)?,
                self.part_binary(op, b, c, span)?,
            ],
            (BinaryOp::Mul, None, Some(d)) => [
                self.part_binary(op, a, c, span)?,
                self.part_binary(op, a, d, span)?,
            ],
            _ => {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "invalid floating complex arithmetic",
                    span,
                })
            }
        };
        self.finish_complex(ty, parts, span)
    }

    /// Floating complex operations; each part follows the scalar rules.
    fn apply_floating_complex(
        &mut self,
        expression: &ComplexExpr<'arena>,
        arguments: &[CValue],
    ) -> Result<CValue, InterpreterError> {
        let span = expression.span;
        let ty = expression.facts.ty();
        let strict = self.strict_floating();
        match expression.op {
            ComplexOp::Component { imaginary } => {
                if self.is_complex_value(arguments[0]) {
                    return self.complex_part(arguments[0], imaginary, span);
                }
                let value = expect_floating(arguments[0], span)?;
                Ok(CValue::Floating(if imaginary {
                    FloatBits::from_bits(value.kind(), 0).expect("zero fits every format")
                } else {
                    value
                }))
            }
            ComplexOp::Imaginary => {
                let kind = crate::complex::floating_format(self.unit.types(), &self.target, ty)
                    .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                let zero = FloatBits::from_bits(kind, 0).expect("zero fits every format");
                self.pack_complex(ty, [CValue::Floating(zero), arguments[0]], span)
            }
            // The operands have the part type already; no conversion and no
            // status are involved.
            ComplexOp::Construct => self.pack_complex(ty, [arguments[0], arguments[1]], span),
            ComplexOp::Convert => {
                let source = arguments[0];
                if crate::complex::admitted(self.unit.types(), ty) {
                    let parts = match self.floating_operand(source, span)? {
                        Operand::Complex(parts) => parts,
                        Operand::Real(value) => {
                            let element = crate::complex::element(self.unit.types(), ty)
                                .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                            let zero = self.zero_value(element, span)?;
                            [value, zero]
                        }
                    };
                    let element = crate::complex::element(self.unit.types(), ty)
                        .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                    let target = self.value_type(element, span)?;
                    let real = self.cast_runtime_value(parts[0], target, span)?;
                    let imaginary = self.cast_runtime_value(parts[1], target, span)?;
                    return self.pack_complex(ty, [real, imaginary], span);
                }
                let target = self.value_type(ty, span)?;
                if matches!(target, ValueType::Integer(integer) if integer == CIntegerType::BOOL) {
                    let truth = self.complex_truth(source, span)?;
                    return Ok(CValue::Integer(CInteger::new(
                        u64::from(truth),
                        CIntegerType::BOOL,
                    )));
                }
                // A conversion to a real type reads only the real part.
                let real = self.complex_part(source, false, span)?;
                self.cast_runtime_value(real, target, span)
            }
            ComplexOp::Unary(UnaryOp::Plus) => {
                let parts = self.floating_parts(arguments[0], span)?;
                self.pack_complex(ty, parts, span)
            }
            ComplexOp::Unary(op @ (UnaryOp::Neg | UnaryOp::BitNot)) => {
                // Sign flips: exact, with no status, NaN included.
                let [a, b] = self.floating_parts(arguments[0], span)?;
                let flip = |value: CValue| -> Result<CValue, InterpreterError> {
                    Ok(CValue::Floating(expect_floating(value, span)?.negated()))
                };
                let a = if op == UnaryOp::Neg { flip(a)? } else { a };
                self.pack_complex(ty, [a, flip(b)?], span)
            }
            ComplexOp::Unary(UnaryOp::Not) => {
                let truth = self.complex_truth(arguments[0], span)?;
                Ok(CValue::Integer(CInteger::int(i32::from(!truth))))
            }
            ComplexOp::Binary(op @ (BinaryOp::Eq | BinaryOp::Ne)) => {
                // Both quiet comparisons execute, real part first.
                let [a, b] = self.floating_parts(arguments[0], span)?;
                let [c, d] = self.floating_parts(arguments[1], span)?;
                let mut equal = true;
                for (left, right) in [(a, c), (b, d)] {
                    let result =
                        floating::binary(BinaryOp::Eq, left, right, span, &mut self.world, strict)?;
                    equal &= machine_truth(result, span)?;
                }
                Ok(CValue::Integer(CInteger::int(i32::from(
                    equal == (op == BinaryOp::Eq),
                ))))
            }
            ComplexOp::Binary(op) => self.floating_binary_complex(op, ty, arguments, span),
            ComplexOp::Unary(_) => Err(InterpreterError::UnsupportedType { ty, span }),
        }
    }

    /// Whether `expression` computes with floating complex values, and not
    /// with integer complex values only.
    fn is_floating_complex_operation(
        &self,
        expression: &ComplexExpr<'arena>,
        arguments: &[CValue],
    ) -> bool {
        self.is_floating_complex(expression.facts.ty()) || arguments.iter().any(|value| {
            matches!(value, CValue::Floating(_))
                || matches!(value, CValue::Aggregate { ty, .. } if self.is_floating_complex(*ty))
        })
    }

    pub(super) fn apply_complex(
        &mut self,
        expression: &ComplexExpr<'arena>,
        arguments: &[CValue],
    ) -> Result<CValue, InterpreterError> {
        if self.is_floating_complex_operation(expression, arguments) {
            return self.apply_floating_complex(expression, arguments);
        }
        let span = expression.span;
        if let ComplexOp::Component { imaginary } = expression.op {
            return self
                .complex_component_value(arguments[0], imaginary, span)
                .map(CValue::Integer);
        }
        let mut pairs = Vec::new();
        for &value in arguments {
            pairs.push([
                self.complex_component_value(value, false, span)?,
                self.complex_component_value(value, true, span)?,
            ]);
        }
        let ty = expression.facts.ty();
        let component = crate::complex::element(self.unit.types(), ty);
        if component.is_none() {
            let result = match expression.op {
                ComplexOp::Unary(UnaryOp::Not) => {
                    CInteger::int(i32::from(!pairs[0].iter().any(|v| v.truth())))
                }
                ComplexOp::Binary(BinaryOp::Eq | BinaryOp::Ne) => CInteger::int(i32::from(
                    (pairs[0] == pairs[1]) == (expression.op == ComplexOp::Binary(BinaryOp::Eq)),
                )),
                ComplexOp::Convert => {
                    let ty = self.integer_type(ty, span)?;
                    if ty == CIntegerType::BOOL {
                        CInteger::new(u64::from(pairs[0].iter().any(|v| v.truth())), ty)
                    } else {
                        pairs[0][0].cast(ty)
                    }
                }
                _ => return Err(InterpreterError::UnsupportedType { ty, span }),
            };
            return Ok(CValue::Integer(result));
        }
        let element = component.unwrap();
        let integer = self.integer_type(element, span)?;
        let negate = |value: CInteger| {
            if self.unit.signed_wrapping() {
                Ok(CInteger::from_raw(
                    0u128.wrapping_sub(value.raw_bits()),
                    value.ty(),
                ))
            } else {
                crate::integer::eval_unary_at_type(UnaryOp::Neg, value)
            }
        };
        let pair = match expression.op {
            ComplexOp::Imaginary => [CInteger::new(0, integer), pairs[0][0].cast(integer)],
            ComplexOp::Convert | ComplexOp::Unary(UnaryOp::Plus) => {
                pairs[0].map(|v| v.cast(integer))
            }
            ComplexOp::Unary(UnaryOp::Neg) => [
                negate(pairs[0][0]).map_err(|e| self.integer_error(e, span))?,
                negate(pairs[0][1]).map_err(|e| self.integer_error(e, span))?,
            ],
            ComplexOp::Unary(UnaryOp::BitNot) => [
                pairs[0][0],
                negate(pairs[0][1]).map_err(|e| self.integer_error(e, span))?,
            ],
            ComplexOp::Binary(op) => {
                let common =
                    crate::integer::usual_arithmetic_type(pairs[0][0].ty(), pairs[1][0].ty());
                crate::complex::binary(
                    op,
                    pairs[0].map(|v| v.cast(common)),
                    pairs[1].map(|v| v.cast(common)),
                    self.unit.signed_wrapping(),
                )
                .map_err(|e| self.integer_error(e, span))?
                .map(|v| v.cast(integer))
            }
            _ => return Err(InterpreterError::UnsupportedType { ty, span }),
        };
        let destination = self.vector_temporary(ty, span)?;
        let layout = self.access_layout(element, span)?;
        for (index, value) in pair.into_iter().enumerate() {
            let address = destination
                .checked_offset(index as i64 * layout.size_bytes as i64)
                .map_err(|e| self.memory_error(e, span))?;
            self.store_memory_value(
                address,
                element,
                Qualifiers::default(),
                CValue::Integer(value),
                span,
            )?;
        }
        Ok(CValue::Aggregate {
            address: destination,
            ty,
            qualifiers: Qualifiers::default(),
        })
    }

    /// A static floating complex object: both parts are the translation-time
    /// values of its constant initializer.
    pub(super) fn store_static_complex(
        &mut self,
        address: Pointer,
        ty: TypeId,
        value: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let parts = crate::constant::floating_complex_constant(
            value,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        )
        .map_err(|error| match error {
            crate::constant::ComplexConstantError::UnmodeledHelper => {
                InterpreterError::ComplexHelper { span }
            }
            crate::constant::ComplexConstantError::NotConstant => {
                InterpreterError::UnsupportedType { ty, span }
            }
        })?;
        let element = crate::complex::element(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let layout = self.access_layout(element, span)?;
        for (index, part) in parts.into_iter().enumerate() {
            let at = address
                .checked_offset(index as i64 * layout.size_bytes as i64)
                .map_err(|e| self.memory_error(e, span))?;
            let width = match part.kind() {
                FloatKind::F32 => CIntegerType::UINT,
                FloatKind::F64 => CIntegerType::ULONG,
                _ => CIntegerType::UINT128,
            };
            self.store_constant_representation(
                at,
                element,
                CInteger::from_raw(part.bits(), width),
                false,
                span,
            )?;
        }
        Ok(())
    }
}

fn format_rank(kind: FloatKind) -> u8 {
    match kind {
        FloatKind::F16 | FloatKind::BF16 => 0,
        FloatKind::F32 => 1,
        FloatKind::F64 => 2,
        FloatKind::F128 => 3,
    }
}

fn expect_floating(value: CValue, span: SourceSpan) -> Result<FloatBits, InterpreterError> {
    match value {
        CValue::Floating(value) => Ok(value),
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
        _ => Err(InterpreterError::UnsupportedExpression {
            feature: "non-floating complex part",
            span,
        }),
    }
}
