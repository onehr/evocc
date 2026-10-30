//! C-HIR interpreter scalar updates.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    /// Step a named scalar object by one, in whichever storage model is active.
    ///
    /// The task machine runs in both modes, so this cannot assume the scalar
    /// lives in an interpreter slot; in memory mode the object is a real
    /// addressable object like any other.
    pub(super) fn apply_scalar_update(
        &mut self,
        operator: UpdateOp,
        symbol: SymbolId,
        target_span: SourceSpan,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let previous = if self.memory_mode {
            let binding = self.require_memory_binding(symbol, target_span)?;
            let qualifiers = self
                .unit
                .types()
                .get(binding.ty)
                .ok_or(InterpreterError::UnsupportedType {
                    ty: binding.ty,
                    span: target_span,
                })?
                .qualifiers;
            self.load_memory_value(binding.address, binding.ty, qualifiers, target_span)?
        } else {
            let slot = *self
                .slots
                .get(&symbol)
                .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?;
            CValue::Integer(
                slot.value
                    .ok_or_else(|| self.uninitialized_read(symbol, target_span))?,
            )
        };
        let updated = self.step_update_value(previous, operator, span)?;
        if self.memory_mode {
            self.store_bound_value(symbol, updated, span)?;
        } else {
            let updated = expect_machine_integer(updated, span)?;
            self.slots
                .get_mut(&symbol)
                .ok_or(InterpreterError::InvalidScalarPlace { symbol, span })?
                .value = Some(updated);
        }
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                previous
            },
        )
    }

    pub(super) fn step_update_value(
        &mut self,
        previous: CValue,
        operator: UpdateOp,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let direction = match operator {
            UpdateOp::PreIncrement | UpdateOp::PostIncrement => OffsetDirection::Add,
            UpdateOp::PreDecrement | UpdateOp::PostDecrement => OffsetDirection::Subtract,
        };
        let strict = self.strict_floating();
        match previous {
            CValue::Floating(value) => floating::binary(
                if direction == OffsetDirection::Add {
                    BinaryOp::Add
                } else {
                    BinaryOp::Sub
                },
                CValue::Floating(value),
                CValue::Integer(CInteger::int(1)),
                span,
                &mut self.world,
                strict,
            ),
            CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
            CValue::Integer(previous) => {
                let binary = if direction == OffsetDirection::Add {
                    BinaryOp::Add
                } else {
                    BinaryOp::Sub
                };
                self.runtime_integer_binary(binary, previous, CInteger::int(1), span)
                    .map(|updated| CValue::Integer(updated.cast(previous.ty())))
            }
            CValue::Pointer { value, ty } => {
                self.offset_pointer_value(value, ty, CInteger::int(1), direction, span)
            }
            CValue::FunctionPointer { value, ty } => {
                self.offset_function_value(value, ty, CInteger::int(1), direction, span)
            }
            CValue::Aggregate { ty, .. }
                if crate::vector::floating(self.unit.types(), ty).is_some() =>
            {
                // One is added to or subtracted from each lane, as GCC does.
                let (kind, _) = crate::vector::floating(self.unit.types(), ty).unwrap();
                let operation = crate::vector::VectorExpr {
                    op: crate::vector::VectorOp::Binary(if direction == OffsetDirection::Add {
                        BinaryOp::Add
                    } else {
                        BinaryOp::Sub
                    }),
                    operands: Vec::new(),
                    facts: ExprFacts::value(ty),
                    span,
                };
                let one = CValue::Floating(evocc_types::floating::FloatBits::from_unsigned(
                    self.target.float_format(kind),
                    1,
                ));
                self.apply_vector(&operation, &[previous, one])
            }
            CValue::Aggregate { ty, .. } if crate::complex::admitted(self.unit.types(), ty) => {
                // GNU C: one is added to or subtracted from the real part.
                let operation = crate::complex::ComplexExpr {
                    op: crate::complex::ComplexOp::Binary(if direction == OffsetDirection::Add {
                        BinaryOp::Add
                    } else {
                        BinaryOp::Sub
                    }),
                    operands: Vec::new(),
                    facts: ExprFacts::value(ty),
                    span,
                };
                self.apply_complex(&operation, &[previous, CValue::Integer(CInteger::int(1))])
            }
            CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
                feature: "aggregate update",
                span,
            }),
        }
    }

    pub(super) fn uninitialized_read(
        &self,
        symbol: SymbolId,
        span: SourceSpan,
    ) -> InterpreterError {
        let spelling = self
            .unit
            .symbols()
            .symbol(symbol)
            .map(|symbol| symbol.spelling().to_owned())
            .unwrap_or_else(|_| format!("symbol#{}", symbol.raw()));
        InterpreterError::ReadOfUninitializedObject {
            symbol,
            spelling,
            span,
        }
    }

    pub(super) fn store_bound_value(
        &mut self,
        symbol: SymbolId,
        value: CValue,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let binding = self.require_memory_binding(symbol, span)?;
        let mut qualifiers = self
            .unit
            .types()
            .get(binding.ty)
            .ok_or(InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?
            .qualifiers;
        // Initialization of an `_Atomic` object is not an atomic access
        // (C11 7.17.2.2); its updates use memory places.
        qualifiers.is_atomic = false;
        self.store_memory_value(binding.address, binding.ty, qualifiers, value, span)?;
        Ok(())
    }

    pub(super) fn validate_memory_scalar_place(
        &self,
        place: ScalarPlace,
        target: &CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Result<SymbolId, InterpreterError> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: target_span,
        } = target
        else {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span,
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            }
        })?;
        if is_global_register(self.unit, *symbol) {
            return Err(InterpreterError::Asm { span: *target_span });
        }
        let binding = self.require_memory_binding(*symbol, *target_span)?;
        if *symbol != place.symbol()
            || !matches!(
                declaration.kind(),
                SymbolKind::Object | SymbolKind::Parameter
            )
            || declaration.spelling() != spelling
            || declaration.ty() != place.ty()
            || binding.ty != place.ty()
            || facts.ty() != place.ty()
            || facts.category() != ExprCategory::LValue
            || !facts.is_modifiable_lvalue()
        {
            return Err(InterpreterError::InvalidScalarPlace {
                symbol: place.symbol(),
                span: *target_span,
            });
        }
        Ok(*symbol)
    }

    pub(super) fn apply_bound_assignment(
        &mut self,
        operator: AssignmentOp,
        symbol: SymbolId,
        right: CValue,
        target_span: SourceSpan,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let binding = self.require_memory_binding(symbol, target_span)?;
        let qualifiers = self
            .unit
            .types()
            .get(binding.ty)
            .ok_or(InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?
            .qualifiers;
        self.apply_memory_assignment(
            operator,
            binding.address,
            binding.ty,
            qualifiers,
            right,
            target_span,
            span,
            self.access_layout(binding.ty, span)?,
        )
    }
}
