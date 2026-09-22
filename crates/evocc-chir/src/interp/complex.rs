use super::*;
use crate::complex::{ComplexExpr, ComplexOp};

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
        expect_machine_integer(
            self.load_memory_value(address, element, qualifiers, span)?,
            span,
        )
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
        } else {
            machine_truth(value, span)
        }
    }

    pub(super) fn apply_complex(
        &mut self,
        expression: &ComplexExpr<'arena>,
        arguments: &[CValue],
    ) -> Result<CValue, InterpreterError> {
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
}
