//! Lowering binary.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn pointer_pointee_type(
        &self,
        expression: &CHirExpr<'_>,
        span: SourceSpan,
    ) -> Result<TypeId, LowerError> {
        self.pointer_descriptor(expression)
            .map(|pointer| pointer.pointee())
            .map_err(|_| LowerError::UnsupportedExpression {
                span,
                feature: "pointer expression type is unavailable",
            })
    }

    pub(super) fn lower_binary(
        &mut self,
        operator: BinaryOp,
        left: &'a CHirExpr<'a>,
        right: &'a CHirExpr<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if matches!(operator, BinaryOp::And | BinaryOp::Or) {
            return self.lower_logical(operator, left, right, state);
        }
        if operator == BinaryOp::Comma {
            self.lower_discarded_expr(left, state)?;
            return self.lower_expr(right, state);
        }
        if matches!(
            operator,
            BinaryOp::Assign
                | BinaryOp::AddAssign
                | BinaryOp::SubAssign
                | BinaryOp::MulAssign
                | BinaryOp::DivAssign
                | BinaryOp::ModAssign
                | BinaryOp::ShlAssign
                | BinaryOp::ShrAssign
                | BinaryOp::BitAndAssign
                | BinaryOp::BitOrAssign
                | BinaryOp::BitXorAssign
        ) {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "assignment must be represented by CHirExpr::Assign",
            });
        }
        let value = self.lower_expr(left, state)?;
        self.lower_binary_after_left(operator, (left, right), value, span, state)
    }

    pub(super) fn lower_binary_after_left(
        &mut self,
        operator: BinaryOp,
        expressions: (&'a CHirExpr<'a>, &'a CHirExpr<'a>),
        left: ExprValue,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (left_expression, right_expression) = expressions;
        let left_key = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        state.env.insert(
            left_key,
            BoundValue {
                value: left.value,
                integer: Some(left.integer),
            },
        );
        let right = self.lower_expr(right_expression, state)?;
        let left = state
            .env
            .remove(&left_key)
            .ok_or(LowerError::InvalidControlFlow {
                span,
                reason: "right operand control flow lost the evaluated left operand",
            })?;
        let left = ExprValue {
            value: left.value,
            integer: left.integer.ok_or(LowerError::InvalidControlFlow {
                span,
                reason: "threaded left operand lost its integer type",
            })?,
        };
        if matches!(left.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
            || matches!(right.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
        {
            return self.lower_pointer_binary(
                operator,
                left_expression,
                right_expression,
                left,
                right,
                span,
                state,
            );
        }
        if matches!(operator, BinaryOp::Shl | BinaryOp::Shr) {
            let shifted_type = integer_promotion(left.integer);
            let amount_type = integer_promotion(right.integer);
            let left = self.cast(left, shifted_type, state)?;
            let right = self.cast(right, amount_type, state)?;
            return self.emit_shift(state, operator, left, right, shifted_type);
        }
        if evocc_evoir::floating_kind(&left.value.ty).is_some()
            || evocc_evoir::floating_kind(&right.value.ty).is_some()
        {
            return self.floating_binary(operator, left, right, state);
        }
        let common = arithmetic_common_type(left.integer, right.integer);
        let left = self.cast(left, common, state)?;
        let right = self.cast(right, common, state)?;
        match operator {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                self.emit_signed_arithmetic(state, operator, left, right, common)
            }
            BinaryOp::Div | BinaryOp::Mod => {
                self.emit_div_rem(state, operator, left, right, common)
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                let result = self.fresh_value(bits(common));
                let instruction = match operator {
                    BinaryOp::BitAnd => Inst::And(result.clone(), left.value, right.value),
                    BinaryOp::BitOr => Inst::Or(result.clone(), left.value, right.value),
                    BinaryOp::BitXor => Inst::Xor(result.clone(), left.value, right.value),
                    _ => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "non-bitwise operator reached bitwise emission",
                        });
                    }
                };
                self.emit(state, instruction)?;
                Ok(ExprValue {
                    value: result,
                    integer: common,
                })
            }
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => {
                let result = self.fresh_value(EvoIRType::Bits(1));
                let (comparison, lhs, rhs) = match operator {
                    BinaryOp::Eq => (CmpOp::Eq, left.value, right.value),
                    BinaryOp::Ne => (CmpOp::Ne, left.value, right.value),
                    BinaryOp::Lt if common.is_signed() => (CmpOp::SLt, left.value, right.value),
                    BinaryOp::Lt => (CmpOp::ULt, left.value, right.value),
                    BinaryOp::Le if common.is_signed() => (CmpOp::SLe, left.value, right.value),
                    BinaryOp::Le => (CmpOp::ULe, left.value, right.value),
                    BinaryOp::Gt if common.is_signed() => (CmpOp::SLt, right.value, left.value),
                    BinaryOp::Gt => (CmpOp::ULt, right.value, left.value),
                    BinaryOp::Ge if common.is_signed() => (CmpOp::SLe, right.value, left.value),
                    BinaryOp::Ge => (CmpOp::ULe, right.value, left.value),
                    _ => {
                        return Err(LowerError::InvalidControlFlow {
                            span,
                            reason: "non-comparison operator reached comparison emission",
                        });
                    }
                };
                self.emit(state, Inst::ICmp(result.clone(), comparison, lhs, rhs))?;
                Ok(ExprValue {
                    value: result,
                    integer: CIntegerType::INT,
                })
            }
            _ => Err(LowerError::UnsupportedExpression {
                span,
                feature: "unsupported binary operator",
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_pointer_binary(
        &mut self,
        operator: BinaryOp,
        left_expression: &'a CHirExpr<'a>,
        right_expression: &'a CHirExpr<'a>,
        left: ExprValue,
        right: ExprValue,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let left_pointer = matches!(left.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. });
        let right_pointer = matches!(right.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. });
        if matches!(operator, BinaryOp::Add | BinaryOp::Sub) && left_pointer && !right_pointer {
            let pointee = self.pointer_pointee_type(left_expression, span)?;
            let promoted_type = integer_promotion(right.integer);
            let index = self.cast(right, promoted_type, state)?;
            let value = self.offset_for_type(
                left.value,
                index,
                if operator == BinaryOp::Add {
                    OffsetDirection::Add
                } else {
                    OffsetDirection::Subtract
                },
                pointee,
                span,
                state,
            )?;
            return Ok(ExprValue {
                value,
                integer: CIntegerType::ULONG,
            });
        }
        if operator == BinaryOp::Add && !left_pointer && right_pointer {
            let pointee = self.pointer_pointee_type(right_expression, span)?;
            let promoted_type = integer_promotion(left.integer);
            let index = self.cast(left, promoted_type, state)?;
            let value = self.offset_for_type(
                right.value,
                index,
                OffsetDirection::Add,
                pointee,
                span,
                state,
            )?;
            return Ok(ExprValue {
                value,
                integer: CIntegerType::ULONG,
            });
        }
        if operator == BinaryOp::Sub && left_pointer && right_pointer {
            if matches!(left.value.ty, EvoIRType::FnPtr { .. })
                || matches!(right.value.ty, EvoIRType::FnPtr { .. })
            {
                let left = self.cast(left, CIntegerType::UINT128, state)?;
                let right = self.cast(right, CIntegerType::UINT128, state)?;
                let difference =
                    self.emit_raw_binary(state, Inst::Sub, &left.value, &right.value)?;
                let result = self.cast(
                    ExprValue {
                        value: difference.clone(),
                        integer: CIntegerType::INT128,
                    },
                    CIntegerType::LONG,
                    state,
                )?;
                let round_trip = self.cast(result.clone(), CIntegerType::INT128, state)?;
                let fits =
                    self.checked_compare(state, CmpOp::Eq, &difference, &round_trip.value)?;
                self.emit(state, Inst::UbAssume(fits, UbReason::SignedOverflow))?;
                return Ok(result);
            }
            let pointee = self.pointer_pointee_type(left_expression, span)?;
            let scale = self.pointer_arithmetic_scale(pointee, span, state)?;
            let mut left = left;
            if self
                .target
                .array_size_factors(self.unit.types(), pointee)
                .is_ok_and(|(_, captures)| !captures.is_empty())
            {
                let zero = self.constant(state, CIntegerType::LONG, 0)?;
                left.value = self.offset_for_type(
                    left.value,
                    zero,
                    OffsetDirection::Add,
                    pointee,
                    span,
                    state,
                )?;
            }
            let left = self.cast(left, CIntegerType::ULONG, state)?;
            let right = self.cast(right, CIntegerType::ULONG, state)?;
            let bytes = self.fresh_value(EvoIRType::Bits(64));
            self.emit(state, Inst::Sub(bytes.clone(), left.value, right.value))?;
            // A zero-sized GNU element type has stride zero. Preserve the
            // division obligation rather than relying on raw total sdiv.
            let nonzero = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::DivisorNonZero(nonzero.clone(), scale.value.clone()),
            )?;
            self.emit(state, Inst::UbAssume(nonzero, UbReason::DivisionByZero))?;
            let result = self.fresh_value(EvoIRType::Bits(64));
            self.emit(state, Inst::SDiv(result.clone(), bytes, scale.value))?;
            return Ok(ExprValue {
                value: result,
                integer: CIntegerType::LONG,
            });
        }
        if matches!(
            operator,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) && left_pointer
            && right_pointer
        {
            let left = self.cast(left, CIntegerType::ULONG, state)?;
            let right = self.cast(right, CIntegerType::ULONG, state)?;
            let (left, right) = if matches!(operator, BinaryOp::Gt | BinaryOp::Ge) {
                (right.value, left.value)
            } else {
                (left.value, right.value)
            };
            let result = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::ICmp(
                    result.clone(),
                    if matches!(operator, BinaryOp::Lt | BinaryOp::Gt) {
                        CmpOp::ULt
                    } else {
                        CmpOp::ULe
                    },
                    left,
                    right,
                ),
            )?;
            return Ok(ExprValue {
                value: result,
                integer: CIntegerType::INT,
            });
        }
        if matches!(operator, BinaryOp::Eq | BinaryOp::Ne) {
            let (left, right) = match (left_pointer, right_pointer) {
                (true, true) if left.value.ty == right.value.ty => (left.value, right.value),
                (true, true) if null_pointer_constant(self.unit, self.target, left_expression) => {
                    let null = self.fresh_value(right.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (null, right.value)
                }
                (true, true) if null_pointer_constant(self.unit, self.target, right_expression) => {
                    let null = self.fresh_value(left.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (left.value, null)
                }
                (true, false)
                    if null_pointer_constant(self.unit, self.target, right_expression) =>
                {
                    let null = self.fresh_value(left.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (left.value, null)
                }
                (false, true) if null_pointer_constant(self.unit, self.target, left_expression) => {
                    let null = self.fresh_value(right.value.ty.clone());
                    self.emit(state, Inst::PtrNull(null.clone()))?;
                    (null, right.value)
                }
                (true, true)
                    if self
                        .pointer_descriptor(left_expression)
                        .ok()
                        .zip(self.pointer_descriptor(right_expression).ok())
                        .is_some_and(|(left, right)| {
                            evocc_chir::PointerTypeDesc::equality_compatible(
                                self.unit.types(),
                                left,
                                right,
                            )
                        }) =>
                {
                    // The source comparison is valid even when its operands
                    // use different object/function pointer representations.
                    let left = self.cast(left, CIntegerType::ULONG, state)?;
                    let right = self.cast(right, CIntegerType::ULONG, state)?;
                    (left.value, right.value)
                }
                _ => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature:
                            "pointer comparison requires a compatible pointer or null constant",
                    });
                }
            };
            let result = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::ICmp(
                    result.clone(),
                    if operator == BinaryOp::Eq {
                        CmpOp::Eq
                    } else {
                        CmpOp::Ne
                    },
                    left,
                    right,
                ),
            )?;
            return Ok(ExprValue {
                value: result,
                integer: CIntegerType::INT,
            });
        }
        Err(LowerError::UnsupportedExpression {
            span,
            feature: "unsupported pointer binary operation",
        })
    }

    pub(super) fn emit_signed_arithmetic(
        &mut self,
        state: &State,
        operator: BinaryOp,
        left: ExprValue,
        right: ExprValue,
        integer: CIntegerType,
    ) -> Result<ExprValue, LowerError> {
        let result = self.fresh_value(bits(integer));
        if integer.is_signed() && !self.unit.signed_wrapping() {
            let defined = self.fresh_value(EvoIRType::Bits(1));
            let predicate = match operator {
                BinaryOp::Add => {
                    Inst::SignedAddDefined(defined.clone(), left.value.clone(), right.value.clone())
                }
                BinaryOp::Sub => {
                    Inst::SignedSubDefined(defined.clone(), left.value.clone(), right.value.clone())
                }
                BinaryOp::Mul => {
                    Inst::SignedMulDefined(defined.clone(), left.value.clone(), right.value.clone())
                }
                _ => {
                    return Err(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "non-arithmetic operator reached signed arithmetic emission",
                    });
                }
            };
            self.emit(state, predicate)?;
            self.emit(state, Inst::UbAssume(defined, UbReason::SignedOverflow))?;
        }
        let instruction = match operator {
            BinaryOp::Add => Inst::Add(result.clone(), left.value, right.value),
            BinaryOp::Sub => Inst::Sub(result.clone(), left.value, right.value),
            BinaryOp::Mul => Inst::Mul(result.clone(), left.value, right.value),
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "unsupported arithmetic emission",
                });
            }
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    pub(super) fn emit_div_rem(
        &mut self,
        state: &State,
        operator: BinaryOp,
        left: ExprValue,
        right: ExprValue,
        integer: CIntegerType,
    ) -> Result<ExprValue, LowerError> {
        let nonzero = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::DivisorNonZero(nonzero.clone(), right.value.clone()),
        )?;
        self.emit(
            state,
            Inst::UbAssume(
                nonzero,
                if operator == BinaryOp::Div {
                    UbReason::DivisionByZero
                } else {
                    UbReason::RemainderByZero
                },
            ),
        )?;
        if integer.is_signed() {
            let defined = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::SignedDivRemNoOverflow(
                    defined.clone(),
                    left.value.clone(),
                    right.value.clone(),
                ),
            )?;
            self.emit(state, Inst::UbAssume(defined, UbReason::SignedOverflow))?;
        }
        let result = self.fresh_value(bits(integer));
        let instruction = match (operator, integer.is_signed()) {
            (BinaryOp::Div, true) => Inst::SDiv(result.clone(), left.value, right.value),
            (BinaryOp::Div, false) => Inst::UDiv(result.clone(), left.value, right.value),
            (BinaryOp::Mod, true) => Inst::SRem(result.clone(), left.value, right.value),
            (BinaryOp::Mod, false) => Inst::URem(result.clone(), left.value, right.value),
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "unsupported division/remainder emission",
                });
            }
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    pub(super) fn emit_shift(
        &mut self,
        state: &State,
        operator: BinaryOp,
        shifted: ExprValue,
        amount: ExprValue,
        integer: CIntegerType,
    ) -> Result<ExprValue, LowerError> {
        let count_defined = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ShiftCountDefined {
                result: count_defined.clone(),
                shifted: shifted.value.clone(),
                amount: amount.value.clone(),
                amount_signed: amount.integer.is_signed(),
            },
        )?;
        self.emit(
            state,
            Inst::UbAssume(count_defined, UbReason::InvalidShiftCount),
        )?;
        let result = self.fresh_value(bits(integer));
        let instruction = match (operator, integer.is_signed()) {
            (BinaryOp::Shl, _) => Inst::Shl(result.clone(), shifted.value, amount.value),
            (BinaryOp::Shr, true) => Inst::AShr(result.clone(), shifted.value, amount.value),
            (BinaryOp::Shr, false) => Inst::LShr(result.clone(), shifted.value, amount.value),
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "unsupported shift emission",
                });
            }
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    pub(super) fn lower_logical(
        &mut self,
        operator: BinaryOp,
        left: &'a CHirExpr<'a>,
        right: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let known_truth = self.known_expression_truth(left, &[right]);
        let skips_right = matches!(
            (operator, known_truth),
            (BinaryOp::And, Some(false)) | (BinaryOp::Or, Some(true))
        ) && !self.expression_has_external_entry(right);
        let left = self.lower_condition(left, state)?;
        if skips_right {
            // Evaluate the left operand, including its calls and obligations,
            // but do not demand execution support from a skipped operand.
            // Its own labels/gotos are harmless; incoming edges are not.
            return Ok(ExprValue {
                value: left,
                integer: CIntegerType::INT,
            });
        }
        let keys = self.expression_join_keys(state, &[right]);
        let mut right_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let skip_state = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let (true_target, true_args, false_target, false_args) = if operator == BinaryOp::And {
            (
                right_state.block,
                self.edge_values(state, &keys)?,
                skip_state.block,
                self.edge_values(state, &keys)?,
            )
        } else {
            (
                skip_state.block,
                self.edge_values(state, &keys)?,
                right_state.block,
                self.edge_values(state, &keys)?,
            )
        };
        self.terminate(
            state,
            conditional_branch(
                left,
                known_truth,
                true_target,
                true_args,
                false_target,
                false_args,
            ),
        )?;
        let right_result = self.lower_condition(right, &mut right_state)?;
        let mut skip_state = skip_state;
        let skip_result = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            &skip_state,
            Inst::Const(skip_result.clone(), u64::from(operator == BinaryOp::Or)),
        )?;
        let temporary = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        right_state.env.insert(
            temporary,
            BoundValue {
                value: right_result,
                integer: Some(CIntegerType::INT),
            },
        );
        skip_state.env.insert(
            temporary,
            BoundValue {
                value: skip_result,
                integer: Some(CIntegerType::INT),
            },
        );
        let mut merge_keys = keys;
        merge_keys.push(temporary);
        let merge = self.new_block_from(&right_state, &merge_keys, BlockRole::Ordinary)?;
        self.branch(&right_state, merge.block, &merge_keys)?;
        self.branch(&skip_state, merge.block, &merge_keys)?;
        *state = merge;
        let result = state
            .env
            .remove(&temporary)
            .ok_or(LowerError::InvalidControlFlow {
                span: right.span(),
                reason: "logical short-circuit merge lost its result",
            })?;
        Ok(ExprValue {
            value: result.value,
            integer: CIntegerType::INT,
        })
    }
}
