//! Lowering assignments.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_assignment(
        &mut self,
        operator: AssignmentOp,
        place: ScalarPlace,
        target: &'a CHirExpr<'a>,
        assigned: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.validate_place(place, target, span)?;
        if self
            .unit
            .types()
            .get(place.ty())
            .is_some_and(|ty| ty.qualifiers.is_atomic)
        {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "atomic update through a direct place",
            });
        }
        if self.memory_symbols.contains(&place.symbol()) {
            let address = self.object_address(place.symbol(), span, state)?;
            let address_key = self.retain_temporary(address.clone(), state);
            let volatile = self
                .unit
                .types()
                .get(place.ty())
                .is_some_and(|ty| ty.qualifiers.is_volatile);
            let right = self.lower_expr(assigned, state)?;
            let address = state
                .env
                .remove(&address_key)
                .expect("retained assignment address")
                .value;
            let result = if operator == AssignmentOp::Simple {
                self.coerce_to_object_type(right, Some(assigned), place.ty(), span, state)?
            } else {
                let original =
                    self.emit_memory_load(address.clone(), place.ty(), volatile, span, state)?;
                if evocc_evoir::floating_kind(&original.value.ty).is_some()
                    || evocc_evoir::floating_kind(&right.value.ty).is_some()
                {
                    let op = evocc_chir::vector::assignment_binary(operator).ok_or(
                        LowerError::UnsupportedExpression {
                            span,
                            feature: "invalid floating assignment",
                        },
                    )?;
                    let result = self.floating_binary(op, original, right, state)?;
                    self.coerce_to_object_type(result, None, place.ty(), span, state)?
                } else if matches!(
                    original.value.ty,
                    EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
                ) {
                    self.compute_pointer_assignment(
                        operator,
                        original,
                        right,
                        place.ty(),
                        span,
                        state,
                    )?
                } else {
                    let destination =
                        canonical_integer(self.unit.types(), self.target, place.ty(), span)?;
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
            return self.emit_memory_store(address, result, place.ty(), volatile, span, state);
        }
        if matches!(
            self.unit.types().get(place.ty()).map(|ty| &ty.kind),
            Some(TypeKind::Pointer(_) | TypeKind::Float(_))
        ) || floating::half_format(self.unit.types(), place.ty()).is_some()
        {
            let right = self.lower_expr(assigned, state)?;
            let result = if operator == AssignmentOp::Simple {
                self.coerce_to_object_type(right, Some(assigned), place.ty(), span, state)?
            } else {
                let original = self.local(state, place.symbol(), span)?;
                if evocc_evoir::floating_kind(&original.value.ty).is_some() {
                    let op = evocc_chir::vector::assignment_binary(operator).ok_or(
                        LowerError::UnsupportedExpression {
                            span,
                            feature: "invalid floating assignment",
                        },
                    )?;
                    let result = self.floating_binary(op, original, right, state)?;
                    self.coerce_to_object_type(result, None, place.ty(), span, state)?
                } else {
                    self.compute_pointer_assignment(
                        operator,
                        original,
                        right,
                        place.ty(),
                        span,
                        state,
                    )?
                }
            };
            self.store_local(place.symbol(), result.clone(), span, state)?;
            return Ok(result);
        }
        let destination = canonical_integer(self.unit.types(), self.target, place.ty(), span)?;
        let right = self.lower_expr(assigned, state)?;
        let result = if operator == AssignmentOp::Simple {
            self.cast(right, destination, state)?
        } else if evocc_evoir::floating_kind(&right.value.ty).is_some() {
            let left = self.local(state, place.symbol(), span)?;
            let op = evocc_chir::vector::assignment_binary(operator).ok_or(
                LowerError::UnsupportedExpression {
                    span,
                    feature: "invalid floating assignment",
                },
            )?;
            let value = self.floating_binary(op, left, right, state)?;
            self.cast(value, destination, state)?
        } else {
            let left = self.local(state, place.symbol(), span)?;
            let computed = if matches!(operator, AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight)
            {
                let shifted_type = integer_promotion(left.integer);
                let amount_type = integer_promotion(right.integer);
                let left = self.cast(left, shifted_type, state)?;
                let right = self.cast(right, amount_type, state)?;
                self.emit_shift(
                    state,
                    if operator == AssignmentOp::ShiftLeft {
                        BinaryOp::Shl
                    } else {
                        BinaryOp::Shr
                    },
                    left,
                    right,
                    shifted_type,
                )?
            } else {
                let common = arithmetic_common_type(left.integer, right.integer);
                let left = self.cast(left, common, state)?;
                let right = self.cast(right, common, state)?;
                match operator {
                    AssignmentOp::Add => {
                        self.emit_signed_arithmetic(state, BinaryOp::Add, left, right, common)?
                    }
                    AssignmentOp::Subtract => {
                        self.emit_signed_arithmetic(state, BinaryOp::Sub, left, right, common)?
                    }
                    AssignmentOp::Multiply => {
                        self.emit_signed_arithmetic(state, BinaryOp::Mul, left, right, common)?
                    }
                    AssignmentOp::Divide => {
                        self.emit_div_rem(state, BinaryOp::Div, left, right, common)?
                    }
                    AssignmentOp::Remainder => {
                        self.emit_div_rem(state, BinaryOp::Mod, left, right, common)?
                    }
                    AssignmentOp::BitAnd | AssignmentOp::BitOr | AssignmentOp::BitXor => {
                        let result = self.fresh_value(bits(common));
                        let instruction = match operator {
                            AssignmentOp::BitAnd => {
                                Inst::And(result.clone(), left.value, right.value)
                            }
                            AssignmentOp::BitOr => {
                                Inst::Or(result.clone(), left.value, right.value)
                            }
                            AssignmentOp::BitXor => {
                                Inst::Xor(result.clone(), left.value, right.value)
                            }
                            _ => {
                                return Err(LowerError::InvalidControlFlow {
                                    span,
                                    reason:
                                        "non-bitwise compound assignment reached bitwise emission",
                                });
                            }
                        };
                        self.emit(state, instruction)?;
                        ExprValue {
                            value: result,
                            integer: common,
                        }
                    }
                    AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "shift assignment bypassed shift emission",
                        });
                    }
                    AssignmentOp::Simple => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "simple assignment reached compound-assignment emission",
                        });
                    }
                }
            };
            self.cast(computed, destination, state)?
        };
        self.store_local(place.symbol(), result.clone(), span, state)?;
        Ok(result)
    }

    pub(super) fn lower_update(
        &mut self,
        operator: UpdateOp,
        place: ScalarPlace,
        target: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.validate_place(place, target, span)?;
        if self
            .unit
            .types()
            .get(place.ty())
            .is_some_and(|ty| ty.qualifiers.is_atomic)
        {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "atomic update through a direct place",
            });
        }
        let original = self.local(state, place.symbol(), span)?;
        if evocc_evoir::floating_kind(&original.value.ty).is_some() {
            let updated = self.floating_increment(original.clone(), operator, state)?;
            self.store_local(place.symbol(), updated.clone(), span, state)?;
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
                place.ty(),
                span,
                state,
            )?;
            self.store_local(place.symbol(), updated.clone(), span, state)?;
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
        let updated = self.cast(computed, original.integer, state)?;
        self.store_local(place.symbol(), updated.clone(), span, state)?;
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                original
            },
        )
    }

    pub(super) fn compute_compound_assignment(
        &mut self,
        operator: AssignmentOp,
        left: ExprValue,
        right: ExprValue,
        destination: CIntegerType,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let computed = if matches!(operator, AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight) {
            let shifted_type = integer_promotion(left.integer);
            let amount_type = integer_promotion(right.integer);
            let left = self.cast(left, shifted_type, state)?;
            let right = self.cast(right, amount_type, state)?;
            self.emit_shift(
                state,
                if operator == AssignmentOp::ShiftLeft {
                    BinaryOp::Shl
                } else {
                    BinaryOp::Shr
                },
                left,
                right,
                shifted_type,
            )?
        } else {
            let common = arithmetic_common_type(left.integer, right.integer);
            let left = self.cast(left, common, state)?;
            let right = self.cast(right, common, state)?;
            match operator {
                AssignmentOp::Add => {
                    self.emit_signed_arithmetic(state, BinaryOp::Add, left, right, common)?
                }
                AssignmentOp::Subtract => {
                    self.emit_signed_arithmetic(state, BinaryOp::Sub, left, right, common)?
                }
                AssignmentOp::Multiply => {
                    self.emit_signed_arithmetic(state, BinaryOp::Mul, left, right, common)?
                }
                AssignmentOp::Divide => {
                    self.emit_div_rem(state, BinaryOp::Div, left, right, common)?
                }
                AssignmentOp::Remainder => {
                    self.emit_div_rem(state, BinaryOp::Mod, left, right, common)?
                }
                AssignmentOp::BitAnd | AssignmentOp::BitOr | AssignmentOp::BitXor => {
                    let result = self.fresh_value(bits(common));
                    let instruction = match operator {
                        AssignmentOp::BitAnd => Inst::And(result.clone(), left.value, right.value),
                        AssignmentOp::BitOr => Inst::Or(result.clone(), left.value, right.value),
                        AssignmentOp::BitXor => Inst::Xor(result.clone(), left.value, right.value),
                        _ => unreachable!(),
                    };
                    self.emit(state, instruction)?;
                    ExprValue {
                        value: result,
                        integer: common,
                    }
                }
                AssignmentOp::Simple | AssignmentOp::ShiftLeft | AssignmentOp::ShiftRight => {
                    return Err(LowerError::InvalidControlFlow {
                        span,
                        reason: "invalid assignment operator reached compound computation",
                    });
                }
            }
        };
        self.cast(computed, destination, state)
    }

    pub(super) fn compute_pointer_assignment(
        &mut self,
        operator: AssignmentOp,
        pointer: ExprValue,
        index: ExprValue,
        pointer_ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if !matches!(
            pointer.value.ty,
            EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
        ) || !matches!(index.value.ty, EvoIRType::Bits(_))
        {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "pointer assignment operands disagree with C-HIR types",
            });
        }
        let pointee = match self.unit.types().get(pointer_ty).map(|ty| &ty.kind) {
            Some(TypeKind::Pointer(pointee)) => *pointee,
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "pointer assignment target is not a canonical pointer",
                });
            }
        };
        let direction = match operator {
            AssignmentOp::Add => OffsetDirection::Add,
            AssignmentOp::Subtract => OffsetDirection::Subtract,
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "invalid compound pointer assignment operator",
                });
            }
        };
        let promoted_type = integer_promotion(index.integer);
        let index = self.cast(index, promoted_type, state)?;
        let value = self.offset_for_type(pointer.value, index, direction, pointee, span, state)?;
        Ok(ExprValue {
            value,
            integer: CIntegerType::ULONG,
        })
    }
}
