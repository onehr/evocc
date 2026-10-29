//! C-HIR interpreter pointers.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn address_of_expression(
        &mut self,
        expression: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if matches!(expression, CHirExpr::StringLit(..)) {
            return self.string_pointer(expression, false);
        }
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: operand_span,
        } = expression
        else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "address-of outside direct object subset",
                span,
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::UnsupportedExpression {
                feature: "unknown address-of object",
                span: *operand_span,
            }
        })?;
        if declaration.kind() == SymbolKind::Function {
            return self.load_identifier(*symbol, spelling, *facts, *operand_span);
        }
        if declaration.spelling() != spelling
            || declaration.ty() != facts.ty()
            || facts.category() != ExprCategory::LValue
            || !facts.is_addressable()
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent address-of object",
                span: *operand_span,
            });
        }
        let place = MemoryPlace::derive(
            self.unit.symbols(),
            self.unit.types(),
            PlaceRoot::Object { symbol: *symbol },
            Vec::new(),
            facts.ty(),
        )
        .map_err(|_| InterpreterError::UnsupportedExpression {
            feature: "invalid address-of place",
            span: *operand_span,
        })?;
        let binding = self.require_memory_binding(*symbol, *operand_span)?;
        Ok(CValue::Pointer {
            value: binding.address,
            ty: PointerTypeDesc::from_addressed_place(&place),
        })
    }

    pub(super) fn string_pointer(
        &self,
        expression: &CHirExpr<'arena>,
        decay: bool,
    ) -> Result<CValue, InterpreterError> {
        let unsupported = || InterpreterError::UnsupportedExpression {
            feature: "string has no allocation site or complete array type",
            span: expression.span(),
        };
        let CHirExpr::StringLit(literal, _) = expression else {
            return Err(unsupported());
        };
        let value = *self
            .string_bindings
            .get(&(expression as *const CHirExpr<'_> as usize))
            .ok_or_else(unsupported)?;
        let ty = if decay {
            PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), expression)
                .map_err(|_| unsupported())?
        } else {
            let array = literal
                .array_type(self.unit.types())
                .ok_or_else(unsupported)?;
            PointerTypeDesc::to_object(self.unit.types(), array).ok_or_else(unsupported)?
        };
        Ok(CValue::Pointer { value, ty })
    }

    pub(super) fn pointer_type_of_expression(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Result<PointerTypeDesc, InterpreterError> {
        let span = expression.span();
        PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), expression)
            .map_err(|_| InterpreterError::UnsupportedExpression {
                feature: "invalid pointer expression type",
                span,
            })
    }

    /// The value type of a conditional. A half conditional with an arm that
    /// has a binary32 value, or with an integer arm, evaluates in binary32,
    /// as lowering does (EvoIR 3.13.6).
    pub(super) fn conditional_value_type(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<ValueType, InterpreterError> {
        let result = self.expression_value_type(expression)?;
        if matches!(result, ValueType::Floating(kind) if kind.is_half()) {
            let mut is_half = |arm: &'arena CHirExpr<'arena>| {
                matches!(
                    self.expression_value_type(arm),
                    Ok(ValueType::Floating(kind)) if kind.is_half()
                )
            };
            if crate::floating::half_carrier(expression, &mut is_half) {
                return Ok(ValueType::Floating(evocc_types::FloatKind::F32));
            }
        }
        Ok(result)
    }

    pub(super) fn expression_value_type(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<ValueType, InterpreterError> {
        if let Some(ty) =
            crate::vector::expression_type(self.unit.types(), expression).filter(|ty| {
                crate::vector::shape(self.unit.types(), *ty).is_some()
                    || crate::complex::admitted(self.unit.types(), *ty)
            })
        {
            return self.value_type(ty, expression.span());
        }
        if self.static_integer_evaluation {
            return self.infer_integer_type(expression).map(ValueType::Integer);
        }
        if let Some(kind) = crate::constant::floating_expression_kind(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(ValueType::Floating(kind));
        }
        if let Ok(ty) =
            crate::place::record_expression_type(self.unit.symbols(), self.unit.types(), expression)
        {
            return self.value_type(ty, expression.span());
        }
        if let Ok(pointer) =
            PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), expression)
        {
            return Ok(ValueType::Pointer(pointer));
        }
        self.infer_integer_type(expression).map(ValueType::Integer)
    }

    pub(super) fn offset_function_value(
        &self,
        pointer: PointerValue,
        ty: PointerTypeDesc,
        index: CInteger,
        direction: OffsetDirection,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let address = self
            .memory
            .pointer_address(pointer)
            .map_err(|error| self.memory_error(error, span))?;
        let index = index.cast(CIntegerType::ULONG).raw_bits() as u64;
        let address = match direction {
            OffsetDirection::Add => address.wrapping_add(index),
            OffsetDirection::Subtract => address.wrapping_sub(index),
        };
        Ok(CValue::FunctionPointer {
            value: if address == 0 {
                PointerValue::Null
            } else {
                PointerValue::Exposed(address)
            },
            ty,
        })
    }

    pub(super) fn offset_pointer_value(
        &self,
        pointer: Pointer,
        pointer_type: PointerTypeDesc,
        index: CInteger,
        direction: OffsetDirection,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let index = index.promoted();
        let scale_bytes = self.pointer_arithmetic_scale(pointer_type.pointee(), pointer, span)?;
        let signedness = if index.ty().is_signed() {
            IndexSignedness::Signed
        } else {
            IndexSignedness::Unsigned
        };
        let value = pointer
            .checked_scaled_offset(
                if scale_bytes == 0 {
                    0
                } else {
                    index.raw_bits()
                },
                u16::from(index.ty().bits()),
                signedness,
                direction,
                scale_bytes.max(1),
            )
            .map_err(|error| self.memory_error(error, span))?;
        // Exposed integer/code addresses have no object until an access
        // resolves one. Arithmetic transports those bits; it must not try to
        // dereference a label address as an object. Genuine object pointers
        // retain the ordinary lifetime/one-past checks.
        if !pointer.object().is_null() {
            self.memory
                .check_pointer(value)
                .map_err(|error| self.memory_error(error, span))?;
        }
        Ok(CValue::Pointer {
            value,
            ty: pointer_type,
        })
    }

    pub(super) fn pointer_arithmetic_scale(
        &self,
        pointee: TypeId,
        pointer: Pointer,
        span: SourceSpan,
    ) -> Result<u64, InterpreterError> {
        let (mut size, captures) = self
            .target
            .array_size_factors(self.unit.types(), pointee)
            .map_err(|_| InterpreterError::UnsupportedType { ty: pointee, span })?;
        let captured = !captures.is_empty();
        for raw in captures {
            let symbol = self
                .unit
                .symbols()
                .symbols_in_source_order()
                .get(raw as usize)
                .ok_or(InterpreterError::UnsupportedExpression {
                    span,
                    feature: "array size capture has no symbol",
                })?;
            if symbol.kind() != SymbolKind::Object
                || self.integer_type(symbol.ty(), span)? != CIntegerType::ULONG
                || self
                    .unit
                    .types()
                    .get(symbol.ty())
                    .is_none_or(|ty| ty.qualifiers.is_volatile || ty.qualifiers.is_atomic)
            {
                return Err(InterpreterError::UnsupportedExpression {
                    span,
                    feature: "array size capture is not size_t",
                });
            }
            let count = if self.memory_mode {
                let binding = self.require_memory_binding(symbol.id(), span)?;
                self.memory
                    .load_integer(binding.address, self.access_layout(symbol.ty(), span)?)
                    .map_err(|error| self.memory_error(error, span))? as u64
            } else {
                self.slots
                    .get(&symbol.id())
                    .filter(|slot| slot.active)
                    .and_then(|slot| slot.value)
                    .ok_or(InterpreterError::UnsupportedExpression {
                        span,
                        feature: "array size capture has no active value",
                    })?
                    .raw_bits() as u64
            };
            size = size
                .checked_mul(count)
                .filter(|size| *size <= i64::MAX as u64)
                .unwrap_or(0);
        }
        if captured && size == 0 {
            return Err(self.memory_error(
                MemoryFault::PointerOffsetOverflow {
                    pointer,
                    operand: OffsetOperand::Scaled {
                        raw_bits: 0,
                        width_bits: 64,
                        signedness: IndexSignedness::Unsigned,
                        direction: OffsetDirection::Add,
                        scale_bytes: size,
                    },
                },
                span,
            ));
        }
        Ok(size)
    }

    pub(super) fn subtract_pointer_values(
        &self,
        left: Pointer,
        right: Pointer,
        pointer_type: PointerTypeDesc,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let byte_delta = self.pointer_byte_difference(left, right, span)?;
        let scale =
            i64::try_from(self.pointer_arithmetic_scale(pointer_type.pointee(), left, span)?)
                .map_err(|_| InterpreterError::UnsupportedExpression {
                    feature: "pointer subtraction scale is not representable",
                    span,
                })?;
        // GNU empty records and zero-length arrays have a zero byte stride.
        // Pointer subtraction still divides by that stride; report its C
        // obligation before evaluating host remainder or division.
        if scale == 0 {
            return Err(InterpreterError::Integer {
                error: IntegerError::DivisionByZero,
                span,
            });
        }
        if byte_delta % scale != 0 {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "pointer subtraction is not an exact element difference",
                span,
            });
        }
        Ok(CValue::Integer(CInteger::new(
            (byte_delta / scale) as u64,
            CIntegerType::LONG,
        )))
    }

    pub(super) fn pointer_byte_difference(
        &self,
        left: Pointer,
        right: Pointer,
        span: SourceSpan,
    ) -> Result<i64, InterpreterError> {
        let address = |pointer| {
            self.memory
                .pointer_address(PointerValue::object(pointer))
                .map_err(|error| self.memory_error(error, span))
        };
        let left_address = address(left)?;
        let right_address = address(right)?;
        let owner = |address| {
            self.symbol_order()
                .labels
                .values()
                .find_map(|(owner, _, identity)| {
                    (self
                        .memory
                        .pointer_address(PointerValue::Function(*identity))
                        == Ok(address))
                    .then_some(*owner)
                })
        };
        if owner(left_address).is_some_and(|left| Some(left) == owner(right_address)) {
            return i64::try_from(i128::from(left_address) - i128::from(right_address))
                .map_err(|_| self.integer_error(IntegerError::SignedOverflow, span));
        }
        self.memory
            .check_pointer(left)
            .map_err(|error| self.memory_error(error, span))?;
        self.memory
            .check_pointer(right)
            .map_err(|error| self.memory_error(error, span))?;
        if left.object() != right.object() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "pointer subtraction across distinct objects",
                span,
            });
        }
        left.byte_offset().checked_sub(right.byte_offset()).ok_or(
            InterpreterError::UnsupportedExpression {
                feature: "pointer subtraction overflow",
                span,
            },
        )
    }
}
