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
        if op == BinaryOp::Div {
            return self.complex_division(a, b, ty, state);
        }
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

    /// `|v|` for the comparison of GCC's wide algorithm: the negation of a
    /// negative signed value, with its overflow obligation. Only `MIN` makes
    /// the unconditional negation overflow, and then `|v|` is undefined too.
    fn complex_magnitude(
        &mut self,
        value: ExprValue,
        ty: CIntegerType,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if !ty.is_signed() {
            return Ok(value);
        }
        let zero = self.constant(state, ty, 0)?;
        let negated =
            self.emit_signed_arithmetic(state, BinaryOp::Sub, zero.clone(), value.clone(), ty)?;
        let negative = self.checked_compare(state, CmpOp::SLt, &value.value, &zero.value)?;
        let magnitude = self.select_bits(state, &negative, &negated.value, &value.value)?;
        Ok(ExprValue {
            value: magnitude,
            integer: ty,
        })
    }

    /// GCC's wide algorithm in the component type (EvoIR 10.1.14). A real
    /// branch on `|c| < |d|` keeps the other quotient from executing: a
    /// select would execute its division and report a false `E3002`.
    fn complex_division(
        &mut self,
        a: [ExprValue; 2],
        b: [ExprValue; 2],
        ty: CIntegerType,
        state: &mut State,
    ) -> Result<[ExprValue; 2], LowerError> {
        let [c, d] = b.clone();
        let c = self.complex_magnitude(c, ty, state)?;
        let d = self.complex_magnitude(d, ty, state)?;
        let compare = if ty.is_signed() {
            CmpOp::SLt
        } else {
            CmpOp::ULt
        };
        let smaller = self.checked_compare(state, compare, &c.value, &d.value)?;
        let mut operands = Vec::new();
        for value in a.into_iter().chain(b) {
            let key = self.retain_temporary(value.value, state);
            state.env.get_mut(&key).unwrap().integer = Some(ty);
            operands.push(key);
        }
        let keys = self.expression_join_keys(state, &[]);
        let mut then_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut else_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let then_arguments = self.edge_values(state, &keys)?;
        let else_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            conditional_branch(
                smaller,
                None,
                then_state.block,
                then_arguments,
                else_state.block,
                else_arguments,
            ),
        )?;
        let results = [
            EnvKey::Temporary(self.next_temporary),
            EnvKey::Temporary(self.next_temporary + 1),
        ];
        self.next_temporary += 2;
        for (branch, smaller) in [(&mut then_state, true), (&mut else_state, false)] {
            let values = operands
                .iter()
                .map(|key| {
                    let bound = branch.env.remove(key).expect("retained quotient operand");
                    ExprValue {
                        value: bound.value,
                        integer: ty,
                    }
                })
                .collect::<Vec<_>>();
            let [a, b, c, d]: [ExprValue; 4] = values.try_into().expect("four operands");
            let pair = self.wide_quotient(smaller, [a, b, c, d], ty, branch)?;
            for (key, value) in results.into_iter().zip(pair) {
                branch.env.insert(
                    key,
                    BoundValue {
                        value: value.value,
                        integer: Some(ty),
                    },
                );
            }
        }
        let mut merge_keys = keys
            .into_iter()
            .filter(|key| !operands.contains(key))
            .collect::<Vec<_>>();
        merge_keys.extend(results);
        let merge = self.new_block_from(&then_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&then_state, merge.block, &merge_keys)?;
        self.branch(&else_state, merge.block, &merge_keys)?;
        *state = merge;
        Ok(results.map(|key| {
            let bound = state.env.remove(&key).expect("joined quotient component");
            ExprValue {
                value: bound.value,
                integer: ty,
            }
        }))
    }

    /// One branch of the wide algorithm, each step in the written order.
    fn wide_quotient(
        &mut self,
        smaller: bool,
        [a, b, c, d]: [ExprValue; 4],
        ty: CIntegerType,
        state: &mut State,
    ) -> Result<[ExprValue; 2], LowerError> {
        // |c| < |d|: ratio = c / d, div = c * ratio + d,
        //   r = (a * ratio + b) / div, i = (b * ratio - a) / div.
        // Otherwise: ratio = d / c, div = d * ratio + c,
        //   r = (b * ratio + a) / div, i = (b - a * ratio) / div.
        let (near, far, first, second) = if smaller {
            (c, d, a.clone(), b.clone())
        } else {
            (d, c, b.clone(), a.clone())
        };
        let ratio = self.emit_div_rem(state, BinaryOp::Div, near.clone(), far.clone(), ty)?;
        let scaled = self.emit_signed_arithmetic(state, BinaryOp::Mul, near, ratio.clone(), ty)?;
        let div = self.emit_signed_arithmetic(state, BinaryOp::Add, scaled, far, ty)?;
        let scaled = self.emit_signed_arithmetic(state, BinaryOp::Mul, first, ratio.clone(), ty)?;
        let real = self.emit_signed_arithmetic(state, BinaryOp::Add, scaled, second, ty)?;
        let real = self.emit_div_rem(state, BinaryOp::Div, real, div.clone(), ty)?;
        let imaginary = if smaller {
            let scaled = self.emit_signed_arithmetic(state, BinaryOp::Mul, b, ratio, ty)?;
            self.emit_signed_arithmetic(state, BinaryOp::Sub, scaled, a, ty)?
        } else {
            let scaled = self.emit_signed_arithmetic(state, BinaryOp::Mul, a, ratio, ty)?;
            self.emit_signed_arithmetic(state, BinaryOp::Sub, b, scaled, ty)?
        };
        let imaginary = self.emit_div_rem(state, BinaryOp::Div, imaginary, div, ty)?;
        Ok([real, imaginary])
    }
}
