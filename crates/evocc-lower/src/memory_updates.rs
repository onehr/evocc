//! Lowering memory updates.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_memory_assignment(
        &mut self,
        operator: AssignmentOp,
        place: &'a MemoryPlace<'a>,
        target_span: SourceSpan,
        assigned: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if place.access_qualifiers().is_atomic {
            self.require_admitted_atomic_object(place.value_ty(), span)?;
        }
        let bit_field = self.place_bit_field(place, target_span)?;
        let address = self.lower_place_address(place, target_span, state)?;
        let address_key = self.retain_temporary(address.clone(), state);
        let volatile = place.access_qualifiers().is_volatile;
        let access_layout = self.place_access_layout(place, target_span)?;
        if operator != AssignmentOp::Simple
            && evocc_chir::complex::admitted(self.unit.types(), place.value_ty())
        {
            let right = self.lower_expr(assigned, state)?;
            let address = state.env[&address_key].value.clone();
            let mut left = ExprValue {
                value: address,
                integer: CIntegerType::ULONG,
            };
            if volatile {
                // Read each part of the volatile object once.
                left = self.complex_snapshot(left, place.value_ty(), target_span, state)?;
            }
            let op = evocc_chir::vector::assignment_binary(operator)
                .expect("compound complex operation");
            let result = self.lower_complex_values(
                evocc_chir::complex::ComplexOp::Binary(op),
                &[
                    Some(place.value_ty()),
                    evocc_chir::vector::expression_type(self.unit.types(), assigned),
                ],
                place.value_ty(),
                vec![left, right],
                span,
                state,
            )?;
            let address = state.env.remove(&address_key).unwrap().value;
            self.emit_aggregate_transfer(
                address.clone(),
                result.value,
                place.value_ty(),
                CopyVolatility::new(false, volatile),
                span,
                state,
            )?;
            return Ok(ExprValue {
                value: address,
                integer: CIntegerType::ULONG,
            });
        }
        if operator != AssignmentOp::Simple
            && evocc_chir::vector::shape(self.unit.types(), place.value_ty()).is_some()
        {
            let right = self.lower_expr(assigned, state)?;
            let address = state.env[&address_key].value.clone();
            let left = if is_wide_vector(self.unit.types(), place.value_ty()) {
                ExprValue {
                    value: address.clone(),
                    integer: CIntegerType::ULONG,
                }
            } else {
                self.emit_memory_load_with_layout(
                    address,
                    place.value_ty(),
                    volatile,
                    target_span,
                    state,
                    access_layout,
                )?
            };
            let op =
                evocc_chir::vector::assignment_binary(operator).expect("compound vector operation");
            let result = self.lower_vector_values(
                &evocc_chir::vector::VectorOp::Binary(op),
                &[
                    Some(place.value_ty()),
                    evocc_chir::vector::expression_type(self.unit.types(), assigned),
                ],
                place.value_ty(),
                vec![left, right],
                span,
                state,
            )?;
            let address = state.env.remove(&address_key).unwrap().value;
            if is_wide_vector(self.unit.types(), place.value_ty()) {
                self.emit_aggregate_transfer(
                    address.clone(),
                    result.value,
                    place.value_ty(),
                    CopyVolatility::new(false, volatile),
                    span,
                    state,
                )?;
                return Ok(ExprValue {
                    value: address,
                    integer: CIntegerType::ULONG,
                });
            }
            return self.emit_memory_store_with_layout(
                address,
                result,
                access_layout,
                volatile,
                state,
            );
        }
        if self.is_aggregate_type(place.value_ty()) {
            if operator != AssignmentOp::Simple {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "aggregate compound assignment reached lowering",
                });
            }
            let source = self.lower_expr(assigned, state)?;
            let address = state
                .env
                .remove(&address_key)
                .expect("retained aggregate assignment address")
                .value;
            self.emit_aggregate_transfer(
                address.clone(),
                source.value,
                place.value_ty(),
                CopyVolatility::new(self.aggregate_source_is_volatile(assigned), volatile),
                span,
                state,
            )?;
            return Ok(ExprValue {
                value: address,
                integer: CIntegerType::ULONG,
            });
        }
        // Match C-HIR: evaluate the destination once, finish the right
        // operand (including calls), then read the value to be updated.
        // A call can change that object; caching its old value loses effects.
        let right = self.lower_expr(assigned, state)?;
        let mut address = state
            .env
            .remove(&address_key)
            .expect("retained assignment address")
            .value;
        if place.access_qualifiers().is_atomic {
            return self.lower_atomic_assignment(
                operator,
                place.value_ty(),
                volatile,
                address,
                right,
                assigned,
                span,
                state,
            );
        }
        let original = if operator == AssignmentOp::Simple {
            None
        } else if let Some(field) = bit_field {
            Some(self.emit_bit_field_load(
                address.clone(),
                place.value_ty(),
                field,
                volatile,
                target_span,
                state,
            )?)
        } else {
            Some(self.emit_memory_load_with_layout(
                address.clone(),
                place.value_ty(),
                volatile,
                target_span,
                state,
                access_layout,
            )?)
        };
        let result = if operator == AssignmentOp::Simple {
            self.coerce_to_object_type(right, Some(assigned), place.value_ty(), span, state)?
        } else {
            let original = original.expect("memory compound assignment loaded its left value");
            let complex = evocc_chir::vector::expression_type(self.unit.types(), assigned)
                .filter(|ty| evocc_chir::complex::admitted(self.unit.types(), *ty));
            if let Some(complex) = complex {
                // A real target with a complex value: the front end converted
                // the value to the common complex type. Compute there, then
                // convert the result to the target type (EvoIR 10.1.14).
                let op = evocc_chir::vector::assignment_binary(operator).ok_or(
                    LowerError::UnsupportedExpression {
                        span,
                        feature: "invalid complex assignment",
                    },
                )?;
                // Division branches; keep the destination across the join.
                let address_key = self.retain_temporary(address, state);
                let result = self.lower_complex_values(
                    evocc_chir::complex::ComplexOp::Binary(op),
                    &[None, Some(complex)],
                    complex,
                    vec![original, right],
                    span,
                    state,
                )?;
                let result = self.lower_complex_values(
                    evocc_chir::complex::ComplexOp::Convert,
                    &[Some(complex)],
                    place.value_ty(),
                    vec![result],
                    span,
                    state,
                )?;
                address = state
                    .env
                    .remove(&address_key)
                    .expect("retained complex assignment address")
                    .value;
                result
            } else if evocc_evoir::floating_kind(&original.value.ty).is_some()
                || evocc_evoir::floating_kind(&right.value.ty).is_some()
            {
                let op = evocc_chir::vector::assignment_binary(operator).ok_or(
                    LowerError::UnsupportedExpression {
                        span,
                        feature: "invalid floating assignment",
                    },
                )?;
                let result = self.floating_binary(op, original, right, state)?;
                self.coerce_to_object_type(result, None, place.value_ty(), span, state)?
            } else if matches!(
                original.value.ty,
                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
            ) {
                self.compute_pointer_assignment(
                    operator,
                    original,
                    right,
                    place.value_ty(),
                    span,
                    state,
                )?
            } else {
                let destination =
                    canonical_integer(self.unit.types(), self.target, place.value_ty(), span)?;
                self.compute_compound_assignment(
                    operator,
                    original,
                    right,
                    destination,
                    span,
                    state,
                )?
            }
        };
        if let Some(field) = bit_field {
            return self.emit_bit_field_store(address, result, field, volatile, state);
        }
        self.emit_memory_store_with_layout(address, result, access_layout, volatile, state)
    }

    pub(super) fn lower_memory_update(
        &mut self,
        operator: UpdateOp,
        place: &'a MemoryPlace<'a>,
        target_span: SourceSpan,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let address = self.lower_place_address(place, target_span, state)?;
        if place.access_qualifiers().is_atomic {
            return self.lower_atomic_update(
                operator,
                place.value_ty(),
                place.access_qualifiers().is_volatile,
                address,
                span,
                state,
            );
        }
        if evocc_chir::complex::admitted(self.unit.types(), place.value_ty()) {
            return self.lower_complex_update(
                operator,
                place.value_ty(),
                address,
                place.access_qualifiers().is_volatile,
                span,
                state,
            );
        }
        if evocc_chir::vector::floating(self.unit.types(), place.value_ty()).is_some() {
            return self.lower_floating_vector_update(
                operator,
                place.value_ty(),
                address,
                place.access_qualifiers().is_volatile,
                span,
                state,
            );
        }
        let bit_field = self.place_bit_field(place, target_span)?;
        let access_layout = self.place_access_layout(place, target_span)?;
        let original = if let Some(field) = bit_field {
            self.emit_bit_field_load(
                address.clone(),
                place.value_ty(),
                field,
                place.access_qualifiers().is_volatile,
                target_span,
                state,
            )?
        } else {
            self.emit_memory_load_with_layout(
                address.clone(),
                place.value_ty(),
                place.access_qualifiers().is_volatile,
                target_span,
                state,
                access_layout,
            )?
        };
        if evocc_evoir::floating_kind(&original.value.ty).is_some() {
            let updated = self.floating_increment(original.clone(), operator, state)?;
            self.emit_memory_store_with_layout(
                address,
                updated.clone(),
                access_layout,
                place.access_qualifiers().is_volatile,
                state,
            )?;
            return Ok(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                    updated
                } else {
                    original
                },
            );
        }
        if matches!(
            original.value.ty,
            EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
        ) {
            let one = self.constant(state, CIntegerType::INT, 1)?;
            let updated = self.compute_pointer_assignment(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                    AssignmentOp::Add
                } else {
                    AssignmentOp::Subtract
                },
                original.clone(),
                one,
                place.value_ty(),
                span,
                state,
            )?;
            self.emit_memory_store_with_layout(
                address,
                updated.clone(),
                access_layout,
                place.access_qualifiers().is_volatile,
                state,
            )?;
            return Ok(
                if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                    updated
                } else {
                    original
                },
            );
        }
        let promoted_type = integer_promotion(original.integer);
        let promoted = self.cast(original.clone(), promoted_type, state)?;
        let one = self.constant(state, promoted_type, 1)?;
        let computed = self.emit_signed_arithmetic(
            state,
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            },
            promoted,
            one,
            promoted_type,
        )?;
        let destination =
            canonical_integer(self.unit.types(), self.target, place.value_ty(), span)?;
        let updated = self.cast(computed, destination, state)?;
        let updated = if let Some(field) = bit_field {
            self.emit_bit_field_store(
                address,
                updated,
                field,
                place.access_qualifiers().is_volatile,
                state,
            )?
        } else {
            self.emit_memory_store_with_layout(
                address,
                updated.clone(),
                access_layout,
                place.access_qualifiers().is_volatile,
                state,
            )?
        };
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                original
            },
        )
    }
}
