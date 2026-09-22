use super::*;
use evocc_chir::complex::{ComplexExpr, ComplexOp};

impl<'a> FunctionLowerer<'a> {
    pub(super) fn emit_zero_complex(
        &mut self,
        address: Value,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let element = evocc_chir::complex::element(self.unit.types(), ty).unwrap();
        let integer = canonical_integer(self.unit.types(), self.target, element, span)?;
        let width = self.access_layout(element, span)?.size_bytes;
        for offset in [0, width] {
            let at = self.constant_offset_address(address.clone(), offset, state)?;
            let zero = self.constant(state, integer, 0)?;
            self.emit_memory_store(at, zero, element, false, span, state)?;
        }
        Ok(())
    }
    fn complex_parts(
        &mut self,
        value: ExprValue,
        ty: Option<TypeId>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<[ExprValue; 2], LowerError> {
        if let Some(element) = ty.and_then(|ty| evocc_chir::complex::element(self.unit.types(), ty))
        {
            let layout = self.access_layout(element, span)?;
            let next =
                self.constant_offset_address(value.value.clone(), layout.size_bytes, state)?;
            let a = self.emit_memory_load(value.value, element, false, span, state)?;
            let b = self.emit_memory_load(next, element, false, span, state)?;
            Ok([a, b])
        } else {
            let zero = self.constant(state, value.integer, 0)?;
            Ok([value, zero])
        }
    }

    pub(super) fn complex_truth(
        &mut self,
        value: ExprValue,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let [a, b] = self.complex_parts(value, Some(ty), span, state)?;
        let a = self.lower_condition_value(a, state)?;
        let b = self.lower_condition_value(b, state)?;
        self.emit_raw_binary(state, Inst::Or, &a, &b)
    }

    fn pack_complex(
        &mut self,
        pair: [ExprValue; 2],
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let element = evocc_chir::complex::element(self.unit.types(), ty).ok_or(
            LowerError::UnsupportedType {
                ty,
                span,
                feature: "invalid complex component",
            },
        )?;
        let integer = canonical_integer(self.unit.types(), self.target, element, span)?;
        let origin = self.synthetic_origin()?;
        let destination = self.emit_alloca(ty, span, origin, state)?;
        let width = self.access_layout(element, span)?.size_bytes;
        for (index, value) in pair.into_iter().enumerate() {
            let address =
                self.constant_offset_address(destination.clone(), index as u64 * width, state)?;
            let value = self.cast(value, integer, state)?;
            self.emit_memory_store(address, value, element, false, span, state)?;
        }
        Ok(ExprValue {
            value: destination,
            integer: CIntegerType::ULONG,
        })
    }

    pub(super) fn lower_complex(
        &mut self,
        expression: &'a ComplexExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let mut keys = Vec::new();
        let mut types = Vec::new();
        for operand in &expression.operands {
            let value = self.lower_expr(operand, state)?;
            let key = self.retain_temporary(value.value, state);
            state.env.get_mut(&key).unwrap().integer = Some(value.integer);
            keys.push(key);
            types.push(evocc_chir::vector::expression_type(
                self.unit.types(),
                operand,
            ));
        }
        let arguments = keys
            .into_iter()
            .map(|key| {
                let value = state.env.remove(&key).unwrap();
                ExprValue {
                    value: value.value,
                    integer: value.integer.unwrap(),
                }
            })
            .collect();
        self.lower_complex_values(
            expression.op,
            &types,
            expression.facts.ty(),
            arguments,
            expression.span,
            state,
        )
    }

    pub(super) fn lower_complex_values(
        &mut self,
        op: ComplexOp,
        types: &[Option<TypeId>],
        ty: TypeId,
        arguments: Vec<ExprValue>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if let ComplexOp::Component { imaginary } = op {
            if let Some(element) =
                types[0].and_then(|ty| evocc_chir::complex::element(self.unit.types(), ty))
            {
                let width = self.access_layout(element, span)?.size_bytes;
                let address = self.constant_offset_address(
                    arguments[0].value.clone(),
                    u64::from(imaginary) * width,
                    state,
                )?;
                return self.emit_memory_load(address, element, false, span, state);
            }
            return if imaginary {
                self.constant(state, arguments[0].integer, 0)
            } else {
                Ok(arguments[0].clone())
            };
        }
        let mut pairs = Vec::new();
        for (value, source) in arguments.into_iter().zip(types) {
            pairs.push(self.complex_parts(value, *source, span, state)?);
        }
        let component = evocc_chir::complex::element(self.unit.types(), ty);
        if component.is_none() {
            let result = match op {
                ComplexOp::Convert
                    if canonical_integer(self.unit.types(), self.target, ty, span)?
                        != CIntegerType::BOOL =>
                {
                    let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                    return self.cast(pairs[0][0].clone(), integer, state);
                }
                ComplexOp::Convert | ComplexOp::Unary(UnaryOp::Not) => {
                    let a = self.lower_condition_value(pairs[0][0].clone(), state)?;
                    let b = self.lower_condition_value(pairs[0][1].clone(), state)?;
                    let truth = self.emit_raw_binary(state, Inst::Or, &a, &b)?;
                    if op == ComplexOp::Unary(UnaryOp::Not) {
                        let zero = self.constant(state, CIntegerType::BOOL, 0)?;
                        self.checked_compare(state, CmpOp::Eq, &truth, &zero.value)?
                    } else {
                        truth
                    }
                }
                ComplexOp::Binary(BinaryOp::Eq | BinaryOp::Ne) => {
                    let a = self.checked_compare(
                        state,
                        CmpOp::Eq,
                        &pairs[0][0].value,
                        &pairs[1][0].value,
                    )?;
                    let b = self.checked_compare(
                        state,
                        CmpOp::Eq,
                        &pairs[0][1].value,
                        &pairs[1][1].value,
                    )?;
                    let equal = self.emit_raw_binary(state, Inst::And, &a, &b)?;
                    if op == ComplexOp::Binary(BinaryOp::Ne) {
                        let zero = self.constant(state, CIntegerType::BOOL, 0)?;
                        self.checked_compare(state, CmpOp::Eq, &equal, &zero.value)?
                    } else {
                        equal
                    }
                }
                _ => {
                    return Err(LowerError::UnsupportedType {
                        ty,
                        span,
                        feature: "invalid complex result",
                    })
                }
            };
            return self.cast(
                ExprValue {
                    value: result,
                    integer: CIntegerType::BOOL,
                },
                canonical_integer(self.unit.types(), self.target, ty, span)?,
                state,
            );
        }
        let integer = canonical_integer(self.unit.types(), self.target, component.unwrap(), span)?;
        // Conversion is per component; never reinterpret a complex value as
        // one wide integer or put it in the short-vector ABI class.
        let pair = match op {
            ComplexOp::Imaginary => [self.constant(state, integer, 0)?, pairs[0][0].clone()],
            ComplexOp::Convert | ComplexOp::Unary(UnaryOp::Plus) => pairs[0].clone(),
            ComplexOp::Unary(UnaryOp::Neg | UnaryOp::BitNot) => {
                let [a, b] = pairs[0].clone();
                let zero = self.constant(state, integer, 0)?;
                let a = if op == ComplexOp::Unary(UnaryOp::Neg) {
                    self.emit_signed_arithmetic(state, BinaryOp::Sub, zero.clone(), a, integer)?
                } else {
                    a
                };
                let b = self.emit_signed_arithmetic(state, BinaryOp::Sub, zero, b, integer)?;
                [a, b]
            }
            ComplexOp::Binary(op) => {
                let common = usual_arithmetic_type(pairs[0][0].integer, pairs[1][0].integer);
                let a = [
                    self.cast(pairs[0][0].clone(), common, state)?,
                    self.cast(pairs[0][1].clone(), common, state)?,
                ];
                let b = [
                    self.cast(pairs[1][0].clone(), common, state)?,
                    self.cast(pairs[1][1].clone(), common, state)?,
                ];
                self.complex_arithmetic(op, a, b, common, span, state)?
            }
            _ => {
                return Err(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "invalid complex operation",
                })
            }
        };
        self.pack_complex(pair, ty, span, state)
    }

    fn complex_arithmetic(
        &mut self,
        op: BinaryOp,
        a: [ExprValue; 2],
        b: [ExprValue; 2],
        ty: CIntegerType,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<[ExprValue; 2], LowerError> {
        if !matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "unsupported integer-complex arithmetic",
            });
        }
        let [a, b0] = a;
        let [c, d] = b;
        if matches!(op, BinaryOp::Add | BinaryOp::Sub) {
            return Ok([
                self.emit_signed_arithmetic(state, op, a, c, ty)?,
                self.emit_signed_arithmetic(state, op, b0, d, ty)?,
            ]);
        }
        let ac = self.emit_signed_arithmetic(state, BinaryOp::Mul, a.clone(), c.clone(), ty)?;
        let bd = self.emit_signed_arithmetic(state, BinaryOp::Mul, b0.clone(), d.clone(), ty)?;
        let ad = self.emit_signed_arithmetic(state, BinaryOp::Mul, a, d.clone(), ty)?;
        let bc = self.emit_signed_arithmetic(state, BinaryOp::Mul, b0, c.clone(), ty)?;
        match op {
            BinaryOp::Mul => Ok([
                self.emit_signed_arithmetic(state, BinaryOp::Sub, ac, bd, ty)?,
                self.emit_signed_arithmetic(state, BinaryOp::Add, ad, bc, ty)?,
            ]),
            _ => Err(LowerError::UnsupportedExpression {
                span,
                feature: "invalid complex arithmetic",
            }),
        }
    }
}
