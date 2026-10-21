//! Scalar FP lowering uses the same typed SSA values, object storage and edges.
use super::*;
use evocc_evoir::{floating_kind, FloatingInst, FloatingOp};
use evocc_types::floating::FloatBits;
use evocc_types::FloatKind;

pub(super) fn ir_type(kind: FloatKind) -> Option<EvoIRType> {
    match kind {
        FloatKind::F16 => Some(EvoIRType::F16),
        FloatKind::BF16 => Some(EvoIRType::BF16),
        FloatKind::F32 => Some(EvoIRType::F32),
        FloatKind::F64 => Some(EvoIRType::F64),
        FloatKind::F128 => Some(EvoIRType::F128),
    }
}

/// The storage format of a half type: `_Float16`, `__fp16` or `__bf16`.
pub(super) fn half_format(types: &TypeInterner, ty: TypeId) -> Option<FloatKind> {
    match types.get(ty)?.kind {
        TypeKind::TargetBuiltin(builtin) => builtin.half_format(),
        _ => None,
    }
}

/// The value format of a scalar floating type: the target's format of a
/// standard floating type, or the storage format of a half type.
pub(super) fn scalar_format(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
) -> Option<FloatKind> {
    match types.get(ty)?.kind {
        TypeKind::Float(kind) => Some(target.float_format(kind)),
        _ => half_format(types, ty),
    }
}

impl<'a> FunctionLowerer<'a> {
    pub(super) fn floating_constant(
        &mut self,
        value: FloatBits,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        let result = self.fresh_value(ir_type(value.kind()).expect("executable constant format"));
        self.emit(
            state,
            Inst::Floating(FloatingInst {
                result: result.clone(),
                op: FloatingOp::Constant(value.bits()),
                arguments: vec![],
            }),
        )?;
        Ok(ExprValue {
            value: result,
            integer: CIntegerType::ULONG,
        })
    }

    pub(super) fn lower_floating_builtin(
        &mut self,
        expression: &evocc_chir::floating::FloatingExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        use evocc_chir::floating::FloatingBuiltin::*;
        // A later operand may open blocks (an inlined call, conditional or
        // statement expression). Earlier operands remain live through the
        // explicit environment, exactly like ordinary/vector/complex operands;
        // keeping their original Value IDs would violate local SSA.
        let mut keys = Vec::new();
        for operand in &expression.operands {
            let value = self.lower_expr(operand, state)?;
            let key = self.retain_temporary(value.value, state);
            keys.push((key, value.integer));
        }
        let operands = keys
            .into_iter()
            .map(|(key, integer)| ExprValue {
                value: state
                    .env
                    .remove(&key)
                    .expect("retained floating operand")
                    .value,
                integer,
            })
            .collect::<Vec<_>>();
        let fp = operands[0].value.ty.clone();
        let kind = floating_kind(&fp).ok_or(LowerError::FloatingValue {
            span: expression.span,
        })?;
        let (sign, exponent) = evocc_chir::floating::masks(kind).unwrap();
        let integer = if kind == FloatKind::F32 {
            CIntegerType::UINT
        } else if kind == FloatKind::F128 {
            CIntegerType::UINT128
        } else {
            CIntegerType::ULONG
        };
        if expression.op.is_comparison() {
            let arguments = operands
                .iter()
                .map(|operand| operand.value.clone())
                .collect::<Vec<_>>();
            let predicate = match expression.op {
                IsUnordered => {
                    if self.strict_floating() {
                        // Quiet classification still consumes an sNaN. Keep
                        // its status even though the equality value is unused.
                        self.floating_instruction(
                            FloatingOp::Oeq,
                            arguments.clone(),
                            EvoIRType::Bits(1),
                            state,
                        )?;
                    }
                    let mask = self.wide_constant(state, integer, sign - 1)?.value;
                    let exponent = self.wide_constant(state, integer, exponent)?.value;
                    let mut unordered = Vec::new();
                    for argument in arguments {
                        let raw = self
                            .floating_instruction(
                                FloatingOp::ToBits,
                                vec![argument],
                                bits(integer),
                                state,
                            )?
                            .value;
                        let magnitude = self.emit_raw_binary(state, Inst::And, &raw, &mask)?;
                        let nan = self.fresh_value(EvoIRType::Bits(1));
                        self.emit(
                            state,
                            Inst::ICmp(nan.clone(), CmpOp::ULt, exponent.clone(), magnitude),
                        )?;
                        unordered.push(nan);
                    }
                    self.emit_raw_binary(state, Inst::Or, &unordered[0], &unordered[1])?
                }
                IsLessGreater => {
                    let less = self
                        .floating_instruction(
                            if self.strict_floating() {
                                FloatingOp::QuietOlt
                            } else {
                                FloatingOp::Olt
                            },
                            arguments.clone(),
                            EvoIRType::Bits(1),
                            state,
                        )?
                        .value;
                    let greater = self
                        .floating_instruction(
                            if self.strict_floating() {
                                FloatingOp::QuietOgt
                            } else {
                                FloatingOp::Ogt
                            },
                            arguments,
                            EvoIRType::Bits(1),
                            state,
                        )?
                        .value;
                    self.emit_raw_binary(state, Inst::Or, &less, &greater)?
                }
                op => {
                    let op = match op {
                        IsGreater => FloatingOp::Ogt,
                        IsGreaterEqual => FloatingOp::Oge,
                        IsLess => FloatingOp::Olt,
                        IsLessEqual => FloatingOp::Ole,
                        _ => unreachable!(),
                    };
                    let op = if self.strict_floating() {
                        match op {
                            FloatingOp::Olt => FloatingOp::QuietOlt,
                            FloatingOp::Ole => FloatingOp::QuietOle,
                            FloatingOp::Ogt => FloatingOp::QuietOgt,
                            FloatingOp::Oge => FloatingOp::QuietOge,
                            _ => unreachable!(),
                        }
                    } else {
                        op
                    };
                    self.floating_instruction(op, arguments, EvoIRType::Bits(1), state)?
                        .value
                }
            };
            return self.cast(
                ExprValue {
                    value: predicate,
                    integer: CIntegerType::INT,
                },
                CIntegerType::INT,
                state,
            );
        }
        let raw = self
            .floating_instruction(
                FloatingOp::ToBits,
                vec![operands[0].value.clone()],
                bits(integer),
                state,
            )?
            .value;
        let sign_mask = self.wide_constant(state, integer, sign)?.value;
        let magnitude_mask = self.wide_constant(state, integer, sign - 1)?.value;
        let magnitude = self.emit_raw_binary(state, Inst::And, &raw, &magnitude_mask)?;
        if matches!(expression.op, Abs | CopySign) {
            let result = if expression.op == CopySign {
                let b = self
                    .floating_instruction(
                        FloatingOp::ToBits,
                        vec![operands[1].value.clone()],
                        bits(integer),
                        state,
                    )?
                    .value;
                let sign = self.emit_raw_binary(state, Inst::And, &b, &sign_mask)?;
                self.emit_raw_binary(state, Inst::Or, &magnitude, &sign)?
            } else {
                magnitude
            };
            return self.floating_instruction(FloatingOp::FromBits, vec![result], fp, state);
        }
        let exponent = self.wide_constant(state, integer, exponent)?.value;
        let zero = self.constant(state, integer, 0)?.value;
        let compare = |this: &mut Self, state: &State, op, a, b| {
            let result = this.fresh_value(EvoIRType::Bits(1));
            this.emit(state, Inst::ICmp(result.clone(), op, a, b))?;
            Ok::<_, LowerError>(result)
        };
        if let FpClassify(classes) = expression.op {
            // The tests run from the last class to the first, so a NaN or an
            // infinity, whose exponent field is also nonzero, selects last.
            let nan = compare(self, state, CmpOp::ULt, exponent.clone(), magnitude.clone())?;
            let infinite = compare(self, state, CmpOp::Eq, magnitude.clone(), exponent.clone())?;
            let field = self.emit_raw_binary(state, Inst::And, &magnitude, &exponent)?;
            let normal = compare(self, state, CmpOp::Ne, field, zero.clone())?;
            let nonzero = compare(self, state, CmpOp::Ne, magnitude, zero)?;
            let class = |this: &mut Self, state: &State, index: usize| {
                this.constant(state, CIntegerType::INT, u64::from(classes[index] as u32))
                    .map(|constant| constant.value)
            };
            let subnormal = class(self, state, 3)?;
            let zero_class = class(self, state, 4)?;
            let mut result = self.select_bits(state, &nonzero, &subnormal, &zero_class)?;
            for (predicate, index) in [(normal, 2), (infinite, 1), (nan, 0)] {
                let selected = class(self, state, index)?;
                result = self.select_bits(state, &predicate, &selected, &result)?;
            }
            return Ok(ExprValue {
                value: result,
                integer: CIntegerType::INT,
            });
        }
        let predicate = match expression.op {
            IsNan => compare(self, state, CmpOp::ULt, exponent, magnitude)?,
            IsInf | IsInfSign => compare(self, state, CmpOp::Eq, magnitude, exponent)?,
            IsFinite => compare(self, state, CmpOp::ULt, magnitude, exponent)?,
            IsNormal => {
                let field = self.emit_raw_binary(state, Inst::And, &magnitude, &exponent)?;
                let nonzero = compare(self, state, CmpOp::Ne, field, zero.clone())?;
                let finite = compare(self, state, CmpOp::ULt, magnitude, exponent)?;
                self.emit_raw_binary(state, Inst::And, &nonzero, &finite)?
            }
            SignBit => {
                let field = self.emit_raw_binary(state, Inst::And, &raw, &sign_mask)?;
                compare(self, state, CmpOp::Ne, field, zero.clone())?
            }
            Abs | CopySign | IsGreater | IsGreaterEqual | IsLess | IsLessEqual | IsLessGreater
            | IsUnordered | FpClassify(_) => unreachable!(),
        };
        let mut result = self.cast(
            ExprValue {
                value: predicate.clone(),
                integer: CIntegerType::INT,
            },
            CIntegerType::INT,
            state,
        )?;
        if expression.op == IsInfSign {
            let sign = self.emit_raw_binary(state, Inst::And, &raw, &sign_mask)?;
            let negative = compare(self, state, CmpOp::Ne, sign, zero)?;
            let minus = self
                .constant(state, CIntegerType::INT, u64::from(u32::MAX))?
                .value;
            let plus = self.constant(state, CIntegerType::INT, 1)?.value;
            let signed = self.select_bits(state, &negative, &minus, &plus)?;
            let zero = self.constant(state, CIntegerType::INT, 0)?.value;
            result.value = self.select_bits(state, &predicate, &signed, &zero)?;
        }
        Ok(result)
    }

    /// One complex part instruction (EvoIR 3.13.5), strict in a strict region.
    pub(super) fn floating_part(
        &mut self,
        op: FloatingOp,
        arguments: Vec<Value>,
        ty: EvoIRType,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        self.floating_instruction(op, arguments, ty, state)
    }

    fn floating_instruction(
        &mut self,
        op: FloatingOp,
        arguments: Vec<Value>,
        ty: EvoIRType,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let result = self.fresh_value(ty);
        let operation = FloatingInst {
            result: result.clone(),
            op,
            arguments,
        };
        operation
            .validate()
            .map_err(|message| LowerError::InvalidGeneratedSsa { message })?;
        if self.strict_floating()
            && !matches!(
                op,
                FloatingOp::Constant(_)
                    | FloatingOp::Neg
                    | FloatingOp::ToBits
                    | FloatingOp::FromBits
                    | FloatingOp::ToIntDefined { .. }
            )
        {
            let world_in = self.token(state, EnvKey::World)?.value;
            let world_out = self.fresh_value(EvoIRType::World);
            self.emit(
                state,
                Inst::StrictFloating(evocc_evoir::StrictFloatingInst {
                    instruction: operation,
                    world_in,
                    world_out: world_out.clone(),
                }),
            )?;
            state.env.insert(
                EnvKey::World,
                BoundValue {
                    value: world_out,
                    integer: None,
                },
            );
        } else {
            self.emit(state, Inst::Floating(operation))?;
        }
        Ok(ExprValue {
            value: result,
            integer: CIntegerType::ULONG,
        })
    }

    pub(super) fn floating_literal(
        &mut self,
        literal: &evocc_chir::FloatLit,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        // A half constant's value is binary32 (C23 H.3).
        let kind = self.target.float_format(literal.suffix.value_kind());
        let raw = match literal.value {
            evocc_chir::FloatConstant::Real(value) => value
                .target_convert(kind)
                .ok_or(LowerError::FloatingValue { span })?
                .bits(),
            evocc_chir::FloatConstant::QuietNaN(payload) => {
                if kind == FloatKind::F32 {
                    0x7fc00000 | (payload & 0x3fffff)
                } else if kind == FloatKind::F128 {
                    (0x7fff8u128 << 108) | (payload & ((1 << 111) - 1))
                } else {
                    0x7ff8000000000000 | (payload & 0x7ffffffffffff)
                }
            }
        };
        self.floating_instruction(
            FloatingOp::Constant(raw),
            vec![],
            ir_type(kind).unwrap(),
            state,
        )
    }

    pub(super) fn cast_floating(
        &mut self,
        value: ExprValue,
        kind: FloatKind,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let ty = ir_type(kind).ok_or(LowerError::FloatingValue {
            span: self.info.span,
        })?;
        if value.value.ty == ty {
            return Ok(value);
        }
        if kind.is_half() || evocc_evoir::is_half_type(&value.value.ty) {
            return self.cast_half(value, kind, state);
        }
        let op = if floating_kind(&value.value.ty).is_some() {
            FloatingOp::Convert
        } else if value.integer.is_signed() && value.value.ty != EvoIRType::Bits(1) {
            FloatingOp::FromSigned
        } else {
            FloatingOp::FromUnsigned
        };
        self.floating_instruction(op, vec![value.value], ty, state)
    }

    /// A conversion with a half format on one side rounds once (EvoIR
    /// 3.13.6). A half value reaches the other half format and binary128
    /// through its exact binary32 value. An integer of more than 32 bits
    /// reaches binary16 saturated to 32 bits: every value beyond that range
    /// overflows binary16 in every rounding direction, as GCC 14 converts
    /// it. It reaches bfloat16 through its round-to-odd value of 24
    /// significant bits, which binary32 holds exactly, so the final rounding
    /// is the only one.
    fn cast_half(
        &mut self,
        value: ExprValue,
        kind: FloatKind,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let ty = ir_type(kind).expect("a format has an IR type");
        if let Some(source) = floating_kind(&value.value.ty) {
            if source.is_half() && (kind.is_half() || kind == FloatKind::F128) {
                let wide = self.floating_instruction(
                    FloatingOp::Convert,
                    vec![value.value],
                    EvoIRType::F32,
                    state,
                )?;
                return self.cast_floating(wide, kind, state);
            }
            return self.floating_instruction(FloatingOp::Convert, vec![value.value], ty, state);
        }
        let EvoIRType::Bits(width) = value.value.ty else {
            return Err(LowerError::FloatingValue {
                span: self.info.span,
            });
        };
        let signed = value.integer.is_signed() && width != 1;
        let op = if signed {
            FloatingOp::FromSigned
        } else {
            FloatingOp::FromUnsigned
        };
        if width <= 32 {
            return self.floating_instruction(op, vec![value.value], ty, state);
        }
        let wide = EvoIRType::Bits(width);
        let constant = |this: &mut Self, state: &State, raw: u128| {
            let result = this.fresh_value(wide.clone());
            this.emit(state, Inst::Const(result.clone(), raw as u64))?;
            if raw >> 64 == 0 {
                return Ok::<_, LowerError>(result);
            }
            let high = this.fresh_value(wide.clone());
            this.emit(state, Inst::Const(high.clone(), (raw >> 64) as u64))?;
            let amount = this.fresh_value(wide.clone());
            this.emit(state, Inst::Const(amount.clone(), 64))?;
            let shifted = this.emit_raw_binary(state, Inst::Shl, &high, &amount)?;
            this.emit_raw_binary(state, Inst::Or, &result, &shifted)
        };
        let all = u128::MAX >> (128 - width);
        let x = value.value;
        if kind == FloatKind::F16 {
            let y = if signed {
                let max = constant(self, state, 0x7fff_ffff)?;
                let min = constant(self, state, all & !0x7fff_ffff)?;
                let over = self.checked_compare(state, CmpOp::SLt, &max, &x)?;
                let under = self.checked_compare(state, CmpOp::SLt, &x, &min)?;
                let low = self.select_bits(state, &under, &min, &x)?;
                self.select_bits(state, &over, &max, &low)?
            } else {
                let max = constant(self, state, 0xffff_ffff)?;
                let over = self.checked_compare(state, CmpOp::ULt, &max, &x)?;
                self.select_bits(state, &over, &max, &x)?
            };
            let narrow = self.fresh_value(EvoIRType::Bits(32));
            self.emit(state, Inst::Trunc(narrow.clone(), y, EvoIRType::Bits(32)))?;
            return self.floating_instruction(op, vec![narrow], ty, state);
        }
        // The magnitude, as an unsigned value of the same width.
        let zero = constant(self, state, 0)?;
        let negative = if signed {
            Some(self.checked_compare(state, CmpOp::SLt, &x, &zero)?)
        } else {
            None
        };
        let magnitude = match &negative {
            Some(negative) => {
                let negated = self.emit_raw_binary(state, Inst::Sub, &zero, &x)?;
                self.select_bits(state, negative, &negated, &x)?
            }
            None => x,
        };
        // The 24 leading bits with the discarded bits ORed into the last
        // of them, as an integer below 2^24 and its scale 2^shift.
        let leading = self.leading_zeros(&magnitude, state)?;
        let limit = constant(self, state, u128::from(width) - 24)?;
        let long = self.checked_compare(state, CmpOp::ULt, &leading, &limit)?;
        let excess = self.emit_raw_binary(state, Inst::Sub, &limit, &leading)?;
        let shift = self.select_bits(state, &long, &excess, &zero)?;
        let one = constant(self, state, 1)?;
        let unit = self.emit_raw_binary(state, Inst::Shl, &one, &shift)?;
        let mask = self.emit_raw_binary(state, Inst::Sub, &unit, &one)?;
        let low = self.emit_raw_binary(state, Inst::And, &magnitude, &mask)?;
        let sticky = self.checked_compare(state, CmpOp::Ne, &low, &zero)?;
        let sticky_value = self.fresh_value(wide.clone());
        self.emit(
            state,
            Inst::ZExt(sticky_value.clone(), sticky, wide.clone()),
        )?;
        let kept = self.emit_raw_binary(state, Inst::LShr, &magnitude, &shift)?;
        let odd = self.emit_raw_binary(state, Inst::Or, &kept, &sticky_value)?;
        let narrow = self.fresh_value(EvoIRType::Bits(32));
        self.emit(state, Inst::Trunc(narrow.clone(), odd, EvoIRType::Bits(32)))?;
        let digits = self.floating_instruction(
            FloatingOp::FromUnsigned,
            vec![narrow],
            EvoIRType::F32,
            state,
        )?;
        // 2^shift is a binary32 normal number, as shift is at most 104, and
        // the product is exact: it stays below 2^128.
        let shift32 = self.fresh_value(EvoIRType::Bits(32));
        self.emit(
            state,
            Inst::Trunc(shift32.clone(), shift, EvoIRType::Bits(32)),
        )?;
        let bias = self.fresh_value(EvoIRType::Bits(32));
        self.emit(state, Inst::Const(bias.clone(), 127))?;
        let exponent = self.emit_raw_binary(state, Inst::Add, &shift32, &bias)?;
        let position = self.fresh_value(EvoIRType::Bits(32));
        self.emit(state, Inst::Const(position.clone(), 23))?;
        let scale_bits = self.emit_raw_binary(state, Inst::Shl, &exponent, &position)?;
        let scale = self.floating_instruction(
            FloatingOp::FromBits,
            vec![scale_bits],
            EvoIRType::F32,
            state,
        )?;
        let mut single = self.floating_instruction(
            FloatingOp::Mul,
            vec![digits.value, scale.value],
            EvoIRType::F32,
            state,
        )?;
        if let Some(negative) = negative {
            // Set the sign bit by its representation: an exact operation.
            let raw = self
                .floating_instruction(
                    FloatingOp::ToBits,
                    vec![single.value],
                    EvoIRType::Bits(32),
                    state,
                )?
                .value;
            let sign = self.fresh_value(EvoIRType::Bits(32));
            self.emit(
                state,
                Inst::ZExt(sign.clone(), negative, EvoIRType::Bits(32)),
            )?;
            let amount = self.fresh_value(EvoIRType::Bits(32));
            self.emit(state, Inst::Const(amount.clone(), 31))?;
            let sign = self.emit_raw_binary(state, Inst::Shl, &sign, &amount)?;
            let signed_raw = self.emit_raw_binary(state, Inst::Or, &raw, &sign)?;
            single = self.floating_instruction(
                FloatingOp::FromBits,
                vec![signed_raw],
                EvoIRType::F32,
                state,
            )?;
        }
        self.floating_instruction(FloatingOp::Convert, vec![single.value], ty, state)
    }

    /// The leading zero count of an unsigned value of 64 or 128 bits, in
    /// its own width. A 128-bit value counts its two halves, as EvoMIR has
    /// no 128-bit `ctlz`.
    fn leading_zeros(&mut self, value: &Value, state: &mut State) -> Result<Value, LowerError> {
        let EvoIRType::Bits(width) = value.ty else {
            unreachable!("an integer magnitude");
        };
        let count = |this: &mut Self, state: &State, operand: Value| {
            let result = this.fresh_value(operand.ty.clone());
            this.emit(
                state,
                Inst::BitOperation {
                    result: result.clone(),
                    operand,
                    op: evocc_evoir::BitOp::LeadingZeros,
                },
            )?;
            Ok::<_, LowerError>(result)
        };
        if width <= 64 {
            return count(self, state, value.clone());
        }
        let half = EvoIRType::Bits(64);
        let wide = EvoIRType::Bits(width);
        let amount = self.fresh_value(wide.clone());
        self.emit(state, Inst::Const(amount.clone(), 64))?;
        let shifted = self.emit_raw_binary(state, Inst::LShr, value, &amount)?;
        let high = self.fresh_value(half.clone());
        self.emit(state, Inst::Trunc(high.clone(), shifted, half.clone()))?;
        let low = self.fresh_value(half.clone());
        self.emit(state, Inst::Trunc(low.clone(), value.clone(), half.clone()))?;
        let high_count = count(self, state, high.clone())?;
        let low_count = count(self, state, low)?;
        let sixty_four = self.fresh_value(half.clone());
        self.emit(state, Inst::Const(sixty_four.clone(), 64))?;
        let low_total = self.emit_raw_binary(state, Inst::Add, &low_count, &sixty_four)?;
        let zero = self.fresh_value(half.clone());
        self.emit(state, Inst::Const(zero.clone(), 0))?;
        let empty = self.checked_compare(state, CmpOp::Eq, &high, &zero)?;
        let selected = self.select_bits(state, &empty, &low_total, &high_count)?;
        let result = self.fresh_value(wide.clone());
        self.emit(state, Inst::ZExt(result.clone(), selected, wide))?;
        Ok(result)
    }

    /// A half operand of an operation converts to binary32 first. The
    /// conversion is exact, except that it quiets a signaling NaN.
    pub(super) fn widen_half(
        &mut self,
        value: ExprValue,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if evocc_evoir::is_half_type(&value.value.ty) {
            self.cast_floating(value, FloatKind::F32, state)
        } else {
            Ok(value)
        }
    }

    pub(super) fn floating_to_integer(
        &mut self,
        value: ExprValue,
        target: CIntegerType,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        // A half value truncates through its exact binary32 value, as GCC
        // 14 converts it (FCVT, then FCVTZS or FCVTZU).
        let value = self.widen_half(value, state)?;
        let guard = self.floating_instruction(
            FloatingOp::ToIntDefined {
                width: u16::from(target.bits()),
                signed: target.is_signed(),
            },
            vec![value.value.clone()],
            EvoIRType::Bits(1),
            state,
        )?;
        self.emit(
            state,
            Inst::UbAssume(guard.value, UbReason::FloatingToInteger),
        )?;
        let mut result = self.floating_instruction(
            if target.is_signed() {
                FloatingOp::ToSigned
            } else {
                FloatingOp::ToUnsigned
            },
            vec![value.value],
            bits(target),
            state,
        )?;
        result.integer = target;
        Ok(result)
    }

    pub(super) fn floating_unary(
        &mut self,
        operator: UnaryOp,
        operand: ExprValue,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if operator == UnaryOp::Plus {
            return Ok(operand);
        }
        let operand = self.widen_half(operand, state)?;
        self.floating_instruction(
            FloatingOp::Neg,
            vec![operand.value.clone()],
            operand.value.ty,
            state,
        )
    }

    pub(super) fn floating_binary(
        &mut self,
        op: BinaryOp,
        left: ExprValue,
        right: ExprValue,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let kind = if left.value.ty == EvoIRType::F128 || right.value.ty == EvoIRType::F128 {
            FloatKind::F128
        } else if left.value.ty == EvoIRType::F64 || right.value.ty == EvoIRType::F64 {
            FloatKind::F64
        } else {
            FloatKind::F32
        };
        let left = self.cast_floating(left, kind, state)?;
        let right = self.cast_floating(right, kind, state)?;
        let operation = match op {
            BinaryOp::Add => FloatingOp::Add,
            BinaryOp::Sub => FloatingOp::Sub,
            BinaryOp::Mul => FloatingOp::Mul,
            BinaryOp::Div => FloatingOp::Div,
            BinaryOp::Eq => FloatingOp::Oeq,
            BinaryOp::Ne => FloatingOp::Une,
            BinaryOp::Lt => FloatingOp::Olt,
            BinaryOp::Le => FloatingOp::Ole,
            BinaryOp::Gt => FloatingOp::Ogt,
            BinaryOp::Ge => FloatingOp::Oge,
            _ => {
                return Err(LowerError::UnsupportedExpression {
                    span: self.info.span,
                    feature: "invalid floating binary operation",
                })
            }
        };
        let compare = matches!(
            op,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        );
        let nonnegative = self.nonnegative_comparison(op, &left.value, &right.value, state);
        let mut value = self.floating_instruction(
            operation,
            vec![left.value, right.value],
            if compare {
                EvoIRType::Bits(1)
            } else {
                ir_type(kind).unwrap()
            },
            state,
        )?;
        if nonnegative {
            // The original comparison still consumes both operands and its
            // native admission check remains. Only its completed result is
            // known; >= is deliberately excluded because NaN is unordered.
            let result = self.fresh_value(EvoIRType::Bits(1));
            self.emit(state, Inst::Const(result.clone(), 0))?;
            value.value = result;
        }
        if compare {
            value.integer = CIntegerType::INT;
        }
        Ok(value)
    }

    fn nonnegative_comparison(&self, op: BinaryOp, a: &Value, b: &Value, state: &State) -> bool {
        let (value, zero) = match op {
            BinaryOp::Lt => (a, b),
            BinaryOp::Gt => (b, a),
            _ => return false,
        };
        let Some(kind) = floating_kind(&value.ty) else {
            return false;
        };
        let Some(block) = self.blocks.iter().find(|b| b.id == state.block) else {
            return false;
        };
        let definition = |id| {
            block.insts.iter().rev().take(256).find(|inst| {
                evocc_evoir::inst_results(inst)
                    .iter()
                    .any(|result| result.id == id)
            })
        };
        let sign = if kind == FloatKind::F32 {
            1u128 << 31
        } else if kind == FloatKind::F128 {
            1u128 << 127
        } else {
            1u128 << 63
        };
        let mut zero_id = zero.id;
        for _ in 0..8 {
            let Some(Inst::Floating(FloatingInst {
                op: FloatingOp::Neg,
                arguments,
                ..
            })) = definition(zero_id)
            else {
                break;
            };
            let Some(value) = arguments.first() else {
                return false;
            };
            zero_id = value.id;
        }
        if !matches!(definition(zero_id), Some(Inst::Floating(FloatingInst { op: FloatingOp::Constant(raw), .. })) if raw & !sign == 0)
        {
            return false;
        }
        let Some(Inst::Floating(FloatingInst {
            op: FloatingOp::FromBits,
            arguments,
            ..
        })) = definition(value.id)
        else {
            return false;
        };
        let Some(Inst::And(_, left, right)) = arguments.first().and_then(|v| definition(v.id))
        else {
            return false;
        };
        [left, right]
            .into_iter()
            .any(|v| matches!(definition(v.id), Some(Inst::Const(_, mask)) if u128::from(*mask) & sign == 0))
    }

    pub(super) fn floating_truth(
        &mut self,
        value: ExprValue,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let value = self.widen_half(value, state)?;
        let zero = self.floating_instruction(
            FloatingOp::Constant(0),
            vec![],
            value.value.ty.clone(),
            state,
        )?;
        Ok(self
            .floating_binary(BinaryOp::Ne, value, zero, state)?
            .value)
    }

    pub(super) fn floating_increment(
        &mut self,
        value: ExprValue,
        operator: UpdateOp,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        // A half object updates in binary32 and rounds once to its format.
        let stored = floating_kind(&value.value.ty).unwrap();
        let value = self.widen_half(value, state)?;
        let kind = floating_kind(&value.value.ty).unwrap();
        let one = self.floating_instruction(
            FloatingOp::Constant(FloatBits::from_unsigned(kind, 1).bits()),
            vec![],
            value.value.ty.clone(),
            state,
        )?;
        let updated = self.floating_binary(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            },
            value,
            one,
            state,
        )?;
        self.cast_floating(updated, stored, state)
    }
}
