use super::*;
use evocc_chir::complex::{ComplexExpr, ComplexOp};
use evocc_evoir::{floating_kind, FloatingOp};
use evocc_types::FloatKind;

/// One operand of floating complex arithmetic: a real operand stays real
/// (C11 6.3.1.8, G.5.1).
enum Operand {
    Real(ExprValue),
    Complex([ExprValue; 2]),
}

fn format_rank(kind: FloatKind) -> u8 {
    match kind {
        FloatKind::F16 => 0,
        FloatKind::F32 => 1,
        FloatKind::F64 => 2,
        FloatKind::F128 => 3,
    }
}

impl<'a> FunctionLowerer<'a> {
    pub(super) fn emit_zero_complex(
        &mut self,
        address: Value,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let element = evocc_chir::complex::element(self.unit.types(), ty).unwrap();
        let width = self.access_layout(element, span)?.size_bytes;
        for offset in [0, width] {
            let at = self.constant_offset_address(address.clone(), offset, state)?;
            let zero = if let Some(kind) =
                evocc_chir::complex::floating_format(self.unit.types(), self.target, ty)
            {
                self.floating_constant(
                    evocc_types::floating::FloatBits::from_bits(kind, 0).expect("zero"),
                    state,
                )?
            } else {
                let integer = canonical_integer(self.unit.types(), self.target, element, span)?;
                self.constant(state, integer, 0)?
            };
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
        } else if let Some(kind) = floating_kind(&value.value.ty) {
            let zero = self.floating_constant(
                evocc_types::floating::FloatBits::from_bits(kind, 0).expect("zero"),
                state,
            )?;
            Ok([value, zero])
        } else {
            let zero = self.constant(state, value.integer, 0)?;
            Ok([value, zero])
        }
    }

    fn is_floating_complex(&self, ty: Option<TypeId>) -> bool {
        ty.is_some_and(|ty| evocc_chir::complex::floating(self.unit.types(), ty).is_some())
    }

    /// The value of a volatile complex object: one volatile read of each
    /// part into a temporary, as the C-HIR interpreter reads it.
    pub(super) fn complex_snapshot(
        &mut self,
        value: ExprValue,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let origin = self.synthetic_origin()?;
        let destination = self.emit_alloca(ty, span, origin, state)?;
        self.emit_aggregate_transfer(
            destination.clone(),
            value.value,
            ty,
            CopyVolatility::Read,
            span,
            state,
        )?;
        Ok(ExprValue {
            value: destination,
            integer: CIntegerType::ULONG,
        })
    }

    /// Read a volatile complex operand once before its parts are used; any
    /// other operand is unchanged.
    pub(super) fn complex_operand(
        &mut self,
        value: ExprValue,
        expression: &CHirExpr<'_>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let Some(ty) = evocc_chir::vector::expression_type(self.unit.types(), expression)
            .filter(|ty| evocc_chir::complex::admitted(self.unit.types(), *ty))
        else {
            return Ok(value);
        };
        if !matches!(
            expression,
            CHirExpr::Ident { .. }
                | CHirExpr::MemoryRead { .. }
                | CHirExpr::Unary(UnaryOp::Deref, ..)
        ) || !self.aggregate_source_is_volatile(expression)
        {
            return Ok(value);
        }
        self.complex_snapshot(value, ty, expression.span(), state)
    }

    pub(super) fn complex_truth(
        &mut self,
        value: ExprValue,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        if self.is_floating_complex(Some(ty)) {
            return self.floating_complex_truth(value, ty, span, state);
        }
        let [a, b] = self.complex_parts(value, Some(ty), span, state)?;
        let a = self.lower_condition_value(a, state)?;
        let b = self.lower_condition_value(b, state)?;
        self.emit_raw_binary(state, Inst::Or, &a, &b)
    }

    /// A quiet `une` of the real part against zero, and of the imaginary part
    /// only when the real part is zero, as GCC builds the test. Strict code
    /// branches, so the second comparison does not execute otherwise; pure
    /// comparisons have no observable status and both execute.
    fn floating_complex_truth(
        &mut self,
        value: ExprValue,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let element = evocc_chir::complex::element(self.unit.types(), ty).unwrap();
        let width = self.access_layout(element, span)?.size_bytes;
        let address = value.value;
        let real = self.emit_memory_load(address.clone(), element, false, span, state)?;
        let real = self.floating_truth(real, state)?;
        if !self.strict_floating() {
            let next = self.constant_offset_address(address, width, state)?;
            let imaginary = self.emit_memory_load(next, element, false, span, state)?;
            let imaginary = self.floating_truth(imaginary, state)?;
            return self.emit_raw_binary(state, Inst::Or, &real, &imaginary);
        }
        let address_key = self.retain_temporary(address, state);
        let keys = self.expression_join_keys(state, &[]);
        let mut then_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let mut else_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let then_arguments = self.edge_values(state, &keys)?;
        let else_arguments = self.edge_values(state, &keys)?;
        self.terminate(
            state,
            conditional_branch(
                real,
                None,
                then_state.block,
                then_arguments,
                else_state.block,
                else_arguments,
            ),
        )?;
        let result = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        let one = self.fresh_value(EvoIRType::Bits(1));
        self.emit(&then_state, Inst::Const(one.clone(), 1))?;
        then_state.env.remove(&address_key);
        then_state.env.insert(
            result,
            BoundValue {
                value: one,
                integer: None,
            },
        );
        let address = else_state
            .env
            .remove(&address_key)
            .expect("retained complex truth operand")
            .value;
        let next = self.constant_offset_address(address, width, &mut else_state)?;
        let imaginary = self.emit_memory_load(next, element, false, span, &mut else_state)?;
        let imaginary = self.floating_truth(imaginary, &mut else_state)?;
        else_state.env.insert(
            result,
            BoundValue {
                value: imaginary,
                integer: None,
            },
        );
        let mut merge_keys = keys
            .into_iter()
            .filter(|key| *key != address_key)
            .collect::<Vec<_>>();
        merge_keys.push(result);
        let merge = self.new_block_from(&then_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&then_state, merge.block, &merge_keys)?;
        self.branch(&else_state, merge.block, &merge_keys)?;
        *state = merge;
        Ok(state
            .env
            .remove(&result)
            .expect("joined complex truth")
            .value)
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
        let floating = self.is_floating_complex(Some(ty));
        let origin = self.synthetic_origin()?;
        let destination = self.emit_alloca(ty, span, origin, state)?;
        let width = self.access_layout(element, span)?.size_bytes;
        for (index, value) in pair.into_iter().enumerate() {
            let address =
                self.constant_offset_address(destination.clone(), index as u64 * width, state)?;
            let value = if floating {
                self.coerce_to_object_type(value, None, element, span, state)?
            } else {
                let integer = canonical_integer(self.unit.types(), self.target, element, span)?;
                self.cast(value, integer, state)?
            };
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
            let value = self.complex_operand(value, operand, state)?;
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

    /// Whether an operation computes with floating complex values, rather
    /// than with integer complex values only.
    fn floating_complex_operation(
        &self,
        types: &[Option<TypeId>],
        ty: TypeId,
        arguments: &[ExprValue],
    ) -> bool {
        self.is_floating_complex(Some(ty))
            || types.iter().any(|ty| self.is_floating_complex(*ty))
            || arguments
                .iter()
                .any(|value| floating_kind(&value.value.ty).is_some())
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
        if self.floating_complex_operation(types, ty, &arguments) {
            return self.lower_floating_complex(op, types, ty, arguments, span, state);
        }
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

    fn floating_operand(
        &mut self,
        value: ExprValue,
        ty: Option<TypeId>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Operand, LowerError> {
        if ty.is_some_and(|ty| evocc_chir::complex::admitted(self.unit.types(), ty)) {
            Ok(Operand::Complex(
                self.complex_parts(value, ty, span, state)?,
            ))
        } else {
            Ok(Operand::Real(value))
        }
    }

    fn floating_zero(&mut self, kind: FloatKind, state: &State) -> Result<ExprValue, LowerError> {
        self.floating_constant(
            evocc_types::floating::FloatBits::from_bits(kind, 0).expect("zero"),
            state,
        )
    }

    /// Floating complex operations; each part follows the scalar rules.
    fn lower_floating_complex(
        &mut self,
        op: ComplexOp,
        types: &[Option<TypeId>],
        ty: TypeId,
        mut arguments: Vec<ExprValue>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        match op {
            ComplexOp::Component { imaginary } => {
                if types[0].is_some_and(|ty| evocc_chir::complex::admitted(self.unit.types(), ty)) {
                    let element =
                        evocc_chir::complex::element(self.unit.types(), types[0].unwrap())
                            .expect("complex part type");
                    let width = self.access_layout(element, span)?.size_bytes;
                    let address = self.constant_offset_address(
                        arguments[0].value.clone(),
                        u64::from(imaginary) * width,
                        state,
                    )?;
                    return self.emit_memory_load(address, element, false, span, state);
                }
                let value = arguments.remove(0);
                if !imaginary {
                    return Ok(value);
                }
                let kind =
                    floating_kind(&value.value.ty).ok_or(LowerError::FloatingValue { span })?;
                self.floating_zero(kind, state)
            }
            ComplexOp::Imaginary => {
                let kind = evocc_chir::complex::floating_format(self.unit.types(), self.target, ty)
                    .ok_or(LowerError::FloatingValue { span })?;
                let zero = self.floating_zero(kind, state)?;
                let value = arguments.remove(0);
                self.pack_complex([zero, value], ty, span, state)
            }
            ComplexOp::Construct => {
                let imaginary = arguments.remove(1);
                let real = arguments.remove(0);
                self.pack_complex([real, imaginary], ty, span, state)
            }
            ComplexOp::Convert => {
                let source = arguments.remove(0);
                if evocc_chir::complex::admitted(self.unit.types(), ty) {
                    let parts = match self.floating_operand(source, types[0], span, state)? {
                        Operand::Complex(parts) => parts,
                        Operand::Real(value) => {
                            let element = evocc_chir::complex::element(self.unit.types(), ty)
                                .expect("complex part type");
                            let zero = if let Some(kind) = evocc_chir::complex::floating_format(
                                self.unit.types(),
                                self.target,
                                ty,
                            ) {
                                self.floating_zero(kind, state)?
                            } else {
                                let integer = canonical_integer(
                                    self.unit.types(),
                                    self.target,
                                    element,
                                    span,
                                )?;
                                self.constant(state, integer, 0)?
                            };
                            [value, zero]
                        }
                    };
                    return self.pack_complex(parts, ty, span, state);
                }
                let integer = canonical_integer(self.unit.types(), self.target, ty, span).ok();
                if integer == Some(CIntegerType::BOOL) {
                    let source_ty = types[0].expect("complex conversion source type");
                    let truth = self.complex_truth(source, source_ty, span, state)?;
                    return Ok(ExprValue {
                        value: truth,
                        integer: CIntegerType::BOOL,
                    });
                }
                // A conversion to a real type reads only the real part.
                let element = types[0]
                    .and_then(|ty| evocc_chir::complex::element(self.unit.types(), ty))
                    .ok_or(LowerError::FloatingValue { span })?;
                let real = self.emit_memory_load(source.value, element, false, span, state)?;
                self.coerce_to_object_type(real, None, ty, span, state)
            }
            ComplexOp::Unary(UnaryOp::Plus) => {
                let source = arguments.remove(0);
                let parts = self.complex_parts(source, types[0], span, state)?;
                self.pack_complex(parts, ty, span, state)
            }
            ComplexOp::Unary(unary @ (UnaryOp::Neg | UnaryOp::BitNot)) => {
                // Sign flips: exact, with no status, NaN included.
                let source = arguments.remove(0);
                let [a, b] = self.complex_parts(source, types[0], span, state)?;
                let a = if unary == UnaryOp::Neg {
                    self.floating_unary(UnaryOp::Neg, a, state)?
                } else {
                    a
                };
                let b = self.floating_unary(UnaryOp::Neg, b, state)?;
                self.pack_complex([a, b], ty, span, state)
            }
            ComplexOp::Unary(UnaryOp::Not) => {
                let source = arguments.remove(0);
                let source_ty = types[0].expect("complex truth operand type");
                let truth = self.complex_truth(source, source_ty, span, state)?;
                let zero = self.constant(state, CIntegerType::BOOL, 0)?;
                let value = self.checked_compare(state, CmpOp::Eq, &truth, &zero.value)?;
                self.cast(
                    ExprValue {
                        value,
                        integer: CIntegerType::BOOL,
                    },
                    CIntegerType::INT,
                    state,
                )
            }
            ComplexOp::Binary(binary @ (BinaryOp::Eq | BinaryOp::Ne)) => {
                // Both quiet comparisons execute, real part first.
                let right = arguments.remove(1);
                let left = arguments.remove(0);
                let [a, b] = self.complex_parts(left, types[0], span, state)?;
                let [c, d] = self.complex_parts(right, types[1], span, state)?;
                let real = self.floating_binary(BinaryOp::Eq, a, c, state)?.value;
                let imaginary = self.floating_binary(BinaryOp::Eq, b, d, state)?.value;
                let mut equal = self.emit_raw_binary(state, Inst::And, &real, &imaginary)?;
                if binary == BinaryOp::Ne {
                    let zero = self.constant(state, CIntegerType::BOOL, 0)?;
                    equal = self.checked_compare(state, CmpOp::Eq, &equal, &zero.value)?;
                }
                self.cast(
                    ExprValue {
                        value: equal,
                        integer: CIntegerType::BOOL,
                    },
                    CIntegerType::INT,
                    state,
                )
            }
            ComplexOp::Binary(binary) => {
                let right = arguments.remove(1);
                let left = arguments.remove(0);
                self.floating_complex_arithmetic(binary, types, ty, left, right, span, state)
            }
            ComplexOp::Unary(_) => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "invalid floating complex operation",
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn floating_complex_arithmetic(
        &mut self,
        op: BinaryOp,
        types: &[Option<TypeId>],
        ty: TypeId,
        left: ExprValue,
        right: ExprValue,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let left = self.floating_operand(left, types[0], span, state)?;
        let right = self.floating_operand(right, types[1], span, state)?;
        // The computation format is the widest format of the operands; a
        // compound assignment can pair a narrower target with its value.
        let mut kind = evocc_chir::complex::floating_format(self.unit.types(), self.target, ty);
        for operand in [&left, &right] {
            let values: Vec<&ExprValue> = match operand {
                Operand::Real(value) => vec![value],
                Operand::Complex(parts) => parts.iter().collect(),
            };
            for value in values {
                if let Some(value) = floating_kind(&value.value.ty) {
                    kind = Some(match kind {
                        Some(previous) if format_rank(previous) >= format_rank(value) => previous,
                        _ => value,
                    });
                }
            }
        }
        let kind = kind.ok_or(LowerError::FloatingValue { span })?;
        let mut convert = |this: &mut Self, operand: Operand| -> Result<_, LowerError> {
            Ok(match operand {
                Operand::Real(value) => (this.cast_floating(value, kind, state)?, None),
                Operand::Complex([a, b]) => (
                    this.cast_floating(a, kind, state)?,
                    Some(this.cast_floating(b, kind, state)?),
                ),
            })
        };
        let (a, b) = convert(self, left)?;
        let (c, d) = convert(self, right)?;
        let pair = match (op, b, d) {
            (BinaryOp::Add | BinaryOp::Sub, Some(b), Some(d)) => [
                self.floating_binary(op, a, c, state)?,
                self.floating_binary(op, b, d, state)?,
            ],
            (BinaryOp::Add | BinaryOp::Sub, Some(b), None) => {
                [self.floating_binary(op, a, c, state)?, b]
            }
            (BinaryOp::Add | BinaryOp::Sub, None, Some(d)) => {
                let d = if op == BinaryOp::Sub {
                    self.floating_unary(UnaryOp::Neg, d, state)?
                } else {
                    d
                };
                [self.floating_binary(op, a, c, state)?, d]
            }
            (BinaryOp::Mul, Some(b), Some(d)) => {
                self.complex_helper(false, [a, b, c, d], kind, state)?
            }
            (BinaryOp::Div, Some(b), Some(d)) => {
                self.complex_helper(true, [a, b, c, d], kind, state)?
            }
            // A real divided by a complex value is complex division of (r, +0).
            (BinaryOp::Div, None, Some(d)) => {
                let zero = self.floating_zero(kind, state)?;
                self.complex_helper(true, [a, zero, c, d], kind, state)?
            }
            (BinaryOp::Mul | BinaryOp::Div, Some(b), None) => [
                self.floating_binary(op, a, c.clone(), state)?,
                self.floating_binary(op, b, c, state)?,
            ],
            (BinaryOp::Mul, None, Some(d)) => [
                self.floating_binary(op, a.clone(), c, state)?,
                self.floating_binary(op, a, d, state)?,
            ],
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "invalid floating complex arithmetic",
                })
            }
        };
        self.pack_complex(pair, ty, span, state)
    }

    /// The two parts of a complex product or quotient: two instructions with
    /// the same operands, which the backend joins into one helper call.
    fn complex_helper(
        &mut self,
        division: bool,
        operands: [ExprValue; 4],
        kind: FloatKind,
        state: &mut State,
    ) -> Result<[ExprValue; 2], LowerError> {
        let ty = floating::ir_type(kind).ok_or(LowerError::FloatingValue {
            span: self.info.span,
        })?;
        let arguments = operands.map(|operand| operand.value).to_vec();
        let mut parts = Vec::with_capacity(2);
        for imaginary in [false, true] {
            let op = if division {
                FloatingOp::ComplexDiv { imaginary }
            } else {
                FloatingOp::ComplexMul { imaginary }
            };
            parts.push(self.floating_part(op, arguments.clone(), ty.clone(), state)?);
        }
        let imaginary = parts.pop().unwrap();
        Ok([parts.pop().unwrap(), imaginary])
    }

    /// GNU C `++` and `--` on a floating complex object: one is added to or
    /// subtracted from the real part, and both parts are stored again.
    pub(super) fn lower_complex_update(
        &mut self,
        operator: UpdateOp,
        ty: TypeId,
        address: Value,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let element = evocc_chir::complex::element(self.unit.types(), ty)
            .ok_or(LowerError::UnknownType { ty, span })?;
        let width = self.access_layout(element, span)?.size_bytes;
        let next = self.constant_offset_address(address.clone(), width, state)?;
        let real = self.emit_memory_load(address.clone(), element, volatile, span, state)?;
        let imaginary = self.emit_memory_load(next.clone(), element, volatile, span, state)?;
        let original = self.pack_complex([real.clone(), imaginary.clone()], ty, span, state)?;
        let updated = self.floating_increment(real, operator, state)?;
        self.emit_memory_store(address, updated.clone(), element, volatile, span, state)?;
        self.emit_memory_store(next, imaginary.clone(), element, volatile, span, state)?;
        if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
            self.pack_complex([updated, imaginary], ty, span, state)
        } else {
            Ok(original)
        }
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
