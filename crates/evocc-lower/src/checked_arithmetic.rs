//! Lowering checked arithmetic.

use super::*;

impl<'a> FunctionLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_checked(
        &mut self,
        op: evocc_chir::CheckedArithmeticOp,
        lhs: &'a CHirExpr<'a>,
        rhs: &'a CHirExpr<'a>,
        target: &'a CHirExpr<'a>,
        target_ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let left = self.lower_expr(lhs, state)?;
        let left_key = self.retain_temporary(left.value, state);
        let right = self.lower_expr(rhs, state)?;
        let right_key = self.retain_temporary(right.value, state);
        let address = self.lower_expr(target, state)?.value;
        let unsigned_product = op == evocc_chir::CheckedArithmeticOp::Mul
            && !left.integer.is_signed()
            && !right.integer.is_signed();
        let left_width = u16::from(left.integer.bits())
            + u16::from(!unsigned_product && !left.integer.is_signed());
        let right_width = u16::from(right.integer.bits())
            + u16::from(!unsigned_product && !right.integer.is_signed());
        let required = match op {
            evocc_chir::CheckedArithmeticOp::Mul => left_width + right_width,
            _ => left_width.max(right_width) + 1,
        };
        let left_value = state
            .env
            .remove(&left_key)
            .expect("retained checked operand")
            .value;
        let right_value = state
            .env
            .remove(&right_key)
            .expect("retained checked operand")
            .value;
        let destination = canonical_integer(self.unit.types(), self.target, target_ty, span)?;
        let left = ExprValue {
            value: left_value,
            integer: left.integer,
        };
        let right = ExprValue {
            value: right_value,
            integer: right.integer,
        };
        let (stored, result) = if required > 128 {
            self.lower_checked_wide(op, left, right, destination, state)?
        } else {
            let exact_type = match required {
                0..=32 if unsigned_product => CIntegerType::UINT,
                33..=64 if unsigned_product => CIntegerType::ULONG,
                _ if unsigned_product => CIntegerType::UINT128,
                0..=32 => CIntegerType::INT,
                33..=64 => CIntegerType::LONG,
                _ => CIntegerType::INT128,
            };
            let left = self.cast(left, exact_type, state)?;
            let right = self.cast(right, exact_type, state)?;
            let exact = self.fresh_value(bits(exact_type));
            self.emit(
                state,
                match op {
                    evocc_chir::CheckedArithmeticOp::Add => {
                        Inst::Add(exact.clone(), left.value, right.value)
                    }
                    evocc_chir::CheckedArithmeticOp::Sub => {
                        Inst::Sub(exact.clone(), left.value, right.value)
                    }
                    evocc_chir::CheckedArithmeticOp::Mul => {
                        Inst::Mul(exact.clone(), left.value, right.value)
                    }
                },
            )?;
            let stored = self.cast(
                ExprValue {
                    value: exact.clone(),
                    integer: exact_type,
                },
                destination,
                state,
            )?;
            let round_trip = self.cast(stored.clone(), exact_type, state)?;
            let mut result = self.fresh_value(EvoIRType::Bits(1));
            self.emit(
                state,
                Inst::ICmp(result.clone(), CmpOp::Ne, exact.clone(), round_trip.value),
            )?;
            if (exact_type.is_signed() && !destination.is_signed())
                || (!exact_type.is_signed()
                    && destination.is_signed()
                    && destination.bits() <= exact_type.bits())
            {
                let zero = self.constant(state, exact_type, 0)?;
                let negative = self.fresh_value(EvoIRType::Bits(1));
                self.emit(
                    state,
                    Inst::ICmp(negative.clone(), CmpOp::SLt, exact, zero.value),
                )?;
                let overflow = self.fresh_value(EvoIRType::Bits(1));
                self.emit(state, Inst::Or(overflow.clone(), result, negative))?;
                result = overflow;
            }
            (stored, result)
        };
        let volatile = self
            .unit
            .types()
            .get(target_ty)
            .is_some_and(|ty| ty.qualifiers.is_volatile);
        self.emit_memory_store(address, stored, target_ty, volatile, span, state)?;
        Ok(ExprValue {
            value: result,
            integer: CIntegerType::BOOL,
        })
    }

    /// Preserve the exact result's sign, low magnitude and overflow bit, as
    /// the C integer kernel does. No wider IR type or trapping signed operator
    /// is necessary, even when the mathematical product needs 256 bits.
    pub(super) fn lower_checked_wide(
        &mut self,
        op: evocc_chir::CheckedArithmeticOp,
        left: ExprValue,
        right: ExprValue,
        destination: CIntegerType,
        state: &mut State,
    ) -> Result<(ExprValue, Value), LowerError> {
        let (left, left_negative) = self.checked_magnitude(left, state)?;
        let (right, mut right_negative) = self.checked_magnitude(right, state)?;
        let zero = self.constant(state, CIntegerType::UINT128, 0)?.value;
        let truth = self.constant(state, CIntegerType::BOOL, 1)?.value;
        let (magnitude, negative, overflow) = if op == evocc_chir::CheckedArithmeticOp::Mul {
            let mask = self
                .wide_constant(state, CIntegerType::UINT128, u128::from(u64::MAX))?
                .value;
            let shift = self.constant(state, CIntegerType::UINT128, 64)?.value;
            let left_low = self.emit_raw_binary(state, Inst::And, &left, &mask)?;
            let left_high = self.emit_raw_binary(state, Inst::LShr, &left, &shift)?;
            let right_low = self.emit_raw_binary(state, Inst::And, &right, &mask)?;
            let right_high = self.emit_raw_binary(state, Inst::LShr, &right, &shift)?;
            let low = self.emit_raw_binary(state, Inst::Mul, &left_low, &right_low)?;
            let cross_left = self.emit_raw_binary(state, Inst::Mul, &left_low, &right_high)?;
            let cross_right = self.emit_raw_binary(state, Inst::Mul, &left_high, &right_low)?;
            // Detect every contribution above bit 127 in the 64-bit limb
            // product. This needs no division helper in freestanding code.
            // The middle sum has at most 66 bits, so no carry is lost here.
            let carry = self.emit_raw_binary(state, Inst::LShr, &low, &shift)?;
            let cross_left_low = self.emit_raw_binary(state, Inst::And, &cross_left, &mask)?;
            let cross_right_low = self.emit_raw_binary(state, Inst::And, &cross_right, &mask)?;
            let middle = self.emit_raw_binary(state, Inst::Add, &carry, &cross_left_low)?;
            let middle = self.emit_raw_binary(state, Inst::Add, &middle, &cross_right_low)?;
            let middle_high = self.emit_raw_binary(state, Inst::LShr, &middle, &shift)?;
            let cross_left_high = self.emit_raw_binary(state, Inst::LShr, &cross_left, &shift)?;
            let cross_right_high = self.emit_raw_binary(state, Inst::LShr, &cross_right, &shift)?;
            let high = self.emit_raw_binary(state, Inst::Or, &middle_high, &cross_left_high)?;
            let high = self.emit_raw_binary(state, Inst::Or, &high, &cross_right_high)?;
            let overflow = self.checked_compare(state, CmpOp::Ne, &high, &zero)?;
            let left_high_nonzero = self.checked_compare(state, CmpOp::Ne, &left_high, &zero)?;
            let right_high_nonzero = self.checked_compare(state, CmpOp::Ne, &right_high, &zero)?;
            let high_product =
                self.emit_raw_binary(state, Inst::And, &left_high_nonzero, &right_high_nonzero)?;
            let overflow = self.emit_raw_binary(state, Inst::Or, &overflow, &high_product)?;
            let magnitude = self.emit_raw_binary(state, Inst::Mul, &left, &right)?;
            let negative =
                self.emit_raw_binary(state, Inst::Xor, &left_negative, &right_negative)?;
            (magnitude, negative, overflow)
        } else {
            if op == evocc_chir::CheckedArithmeticOp::Sub {
                right_negative = self.emit_raw_binary(state, Inst::Xor, &right_negative, &truth)?;
            }
            let same_sign =
                self.checked_compare(state, CmpOp::Eq, &left_negative, &right_negative)?;
            let sum = self.emit_raw_binary(state, Inst::Add, &left, &right)?;
            let carry = self.checked_compare(state, CmpOp::ULt, &sum, &left)?;
            let left_larger = self.checked_compare(state, CmpOp::ULe, &right, &left)?;
            let difference_left = self.emit_raw_binary(state, Inst::Sub, &left, &right)?;
            let difference_right = self.emit_raw_binary(state, Inst::Sub, &right, &left)?;
            let difference =
                self.select_bits(state, &left_larger, &difference_left, &difference_right)?;
            let difference_negative =
                self.select_bits(state, &left_larger, &left_negative, &right_negative)?;
            let magnitude = self.select_bits(state, &same_sign, &sum, &difference)?;
            let negative =
                self.select_bits(state, &same_sign, &left_negative, &difference_negative)?;
            let overflow = self.emit_raw_binary(state, Inst::And, &same_sign, &carry)?;
            (magnitude, negative, overflow)
        };
        // Equal opposite-signed magnitudes produce zero, never negative zero.
        // If magnitude overflowed, representability is false regardless.
        let nonzero = self.checked_compare(state, CmpOp::Ne, &magnitude, &zero)?;
        let negative = self.emit_raw_binary(state, Inst::And, &negative, &nonzero)?;
        let fits = if destination.is_signed() {
            let boundary = self
                .wide_constant(
                    state,
                    CIntegerType::UINT128,
                    1u128 << (destination.bits() - 1),
                )?
                .value;
            let positive_fits = self.checked_compare(state, CmpOp::ULt, &magnitude, &boundary)?;
            let negative_fits = self.checked_compare(state, CmpOp::ULe, &magnitude, &boundary)?;
            self.select_bits(state, &negative, &negative_fits, &positive_fits)?
        } else {
            let maximum = self
                .wide_constant(state, CIntegerType::UINT128, destination.mask())?
                .value;
            let small_enough = self.checked_compare(state, CmpOp::ULe, &magnitude, &maximum)?;
            let nonnegative = self.emit_raw_binary(state, Inst::Xor, &negative, &truth)?;
            self.emit_raw_binary(state, Inst::And, &nonnegative, &small_enough)?
        };
        let does_not_fit = self.emit_raw_binary(state, Inst::Xor, &fits, &truth)?;
        let result = self.emit_raw_binary(state, Inst::Or, &overflow, &does_not_fit)?;
        let negated = self.emit_raw_binary(state, Inst::Sub, &zero, &magnitude)?;
        let raw = self.select_bits(state, &negative, &negated, &magnitude)?;
        let stored = self.cast(
            ExprValue {
                value: raw,
                integer: CIntegerType::UINT128,
            },
            destination,
            state,
        )?;
        Ok((stored, result))
    }

    pub(super) fn checked_magnitude(
        &mut self,
        input: ExprValue,
        state: &mut State,
    ) -> Result<(Value, Value), LowerError> {
        let signed = input.integer.is_signed();
        let raw = self.cast(input, CIntegerType::UINT128, state)?.value;
        let zero = self.constant(state, CIntegerType::UINT128, 0)?.value;
        let negative = if signed {
            self.checked_compare(state, CmpOp::SLt, &raw, &zero)?
        } else {
            self.constant(state, CIntegerType::BOOL, 0)?.value
        };
        let negated = self.emit_raw_binary(state, Inst::Sub, &zero, &raw)?;
        let magnitude = self.select_bits(state, &negative, &negated, &raw)?;
        Ok((magnitude, negative))
    }

    pub(super) fn emit_raw_binary(
        &mut self,
        state: &State,
        operator: fn(Value, Value, Value) -> Inst,
        left: &Value,
        right: &Value,
    ) -> Result<Value, LowerError> {
        let result = self.fresh_value(left.ty.clone());
        self.emit(state, operator(result.clone(), left.clone(), right.clone()))?;
        Ok(result)
    }

    pub(super) fn checked_compare(
        &mut self,
        state: &State,
        operator: CmpOp,
        left: &Value,
        right: &Value,
    ) -> Result<Value, LowerError> {
        let result = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ICmp(result.clone(), operator, left.clone(), right.clone()),
        )?;
        Ok(result)
    }

    pub(super) fn select_bits(
        &mut self,
        state: &State,
        condition: &Value,
        yes: &Value,
        no: &Value,
    ) -> Result<Value, LowerError> {
        let condition = if condition.ty == yes.ty {
            condition.clone()
        } else {
            let result = self.fresh_value(yes.ty.clone());
            self.emit(
                state,
                Inst::ZExt(result.clone(), condition.clone(), yes.ty.clone()),
            )?;
            result
        };
        let zero = self.fresh_value(yes.ty.clone());
        self.emit(state, Inst::Const(zero.clone(), 0))?;
        let mask = self.emit_raw_binary(state, Inst::Sub, &zero, &condition)?;
        let difference = self.emit_raw_binary(state, Inst::Xor, yes, no)?;
        let selected = self.emit_raw_binary(state, Inst::And, &difference, &mask)?;
        self.emit_raw_binary(state, Inst::Xor, no, &selected)
    }

    pub(super) fn lower_bit(
        &mut self,
        op: evocc_chir::integer::BitBuiltinOp,
        operand: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        use evocc_chir::integer::BitBuiltinOp;
        let mut input = self.lower_expr(operand, state)?;
        let total_op = match op {
            BitBuiltinOp::CountLeadingZeros | BitBuiltinOp::RedundantSignBits => {
                evocc_evoir::BitOp::LeadingZeros
            }
            BitBuiltinOp::CountTrailingZeros | BitBuiltinOp::FindFirstSet => {
                evocc_evoir::BitOp::TrailingZeros
            }
            BitBuiltinOp::PopulationCount | BitBuiltinOp::Parity => evocc_evoir::BitOp::Population,
            BitBuiltinOp::ByteSwap => evocc_evoir::BitOp::ByteSwap,
        };
        if op == BitBuiltinOp::RedundantSignBits {
            let amount =
                self.constant(state, input.integer, u64::from(input.integer.bits() - 1))?;
            let sign = self.fresh_value(input.value.ty.clone());
            self.emit(
                state,
                Inst::AShr(sign.clone(), input.value.clone(), amount.value),
            )?;
            let normalized = self.fresh_value(input.value.ty.clone());
            self.emit(state, Inst::Xor(normalized.clone(), input.value, sign))?;
            input.value = normalized;
        }
        let zero = self.constant(state, input.integer, 0)?;
        let nonzero = self.fresh_value(EvoIRType::Bits(1));
        self.emit(
            state,
            Inst::ICmp(nonzero.clone(), CmpOp::Ne, input.value.clone(), zero.value),
        )?;
        if matches!(
            op,
            BitBuiltinOp::CountLeadingZeros | BitBuiltinOp::CountTrailingZeros
        ) {
            self.emit(
                state,
                Inst::UbAssume(nonzero.clone(), UbReason::BitBuiltinZeroOperand),
            )?;
        }
        let counted = self.fresh_value(input.value.ty.clone());
        self.emit(
            state,
            Inst::BitOperation {
                result: counted.clone(),
                operand: input.value,
                op: total_op,
            },
        )?;
        let mut counted = ExprValue {
            value: counted,
            integer: input.integer,
        };
        if matches!(op, BitBuiltinOp::RedundantSignBits | BitBuiltinOp::Parity) {
            let one = self.constant(state, input.integer, 1)?;
            let result = self.fresh_value(counted.value.ty.clone());
            let instruction = if op == BitBuiltinOp::RedundantSignBits {
                Inst::Sub(result.clone(), counted.value, one.value)
            } else {
                Inst::And(result.clone(), counted.value, one.value)
            };
            self.emit(state, instruction)?;
            counted.value = result;
        }
        if op == BitBuiltinOp::FindFirstSet {
            let one = self.constant(state, input.integer, 1)?;
            let incremented = self.fresh_value(counted.value.ty.clone());
            self.emit(
                state,
                Inst::Add(incremented.clone(), counted.value, one.value),
            )?;
            counted.value = incremented;
        }
        let result_type = if op == BitBuiltinOp::ByteSwap {
            input.integer
        } else {
            CIntegerType::INT
        };
        let counted = self.cast(counted, result_type, state)?;
        if op != BitBuiltinOp::FindFirstSet {
            return Ok(counted);
        }
        let zero = self.constant(state, CIntegerType::INT, 0)?;
        let temporary = self.retain_temporary(counted.value, state);
        let symbols = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) => Some(*symbol),
                _ => None,
            })
            .collect();
        let keys = self.keys_for(state, &symbols, &[]);
        let merge = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        let nonzero_args = self.edge_values(state, &keys)?;
        let mut zero_args = nonzero_args.clone();
        zero_args[keys
            .iter()
            .position(|key| *key == temporary)
            .expect("result edge parameter")] = zero.value;
        self.terminate(
            state,
            Inst::CondBr(nonzero, merge.block, nonzero_args, merge.block, zero_args),
        )?;
        *state = merge;
        Ok(ExprValue {
            value: state
                .env
                .remove(&temporary)
                .expect("merged bit result")
                .value,
            integer: CIntegerType::INT,
        })
    }
}
