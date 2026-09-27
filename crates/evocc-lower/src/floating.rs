//! Scalar FP lowering uses the same typed SSA values, object storage and edges.
use super::*;
use evocc_evoir::{floating_kind, FloatingInst, FloatingOp};
use evocc_types::floating::FloatBits;
use evocc_types::FloatKind;

pub(super) fn ir_type(kind: FloatKind) -> Option<EvoIRType> {
    match kind {
        FloatKind::F32 => Some(EvoIRType::F32),
        FloatKind::F64 => Some(EvoIRType::F64),
        _ => None,
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
                op: FloatingOp::Constant(value.bits() as u64),
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
                    let mask = self.constant(state, integer, sign - 1)?.value;
                    let exponent = self.constant(state, integer, exponent)?.value;
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
        let sign_mask = self.constant(state, integer, sign)?.value;
        let magnitude_mask = self.constant(state, integer, sign - 1)?.value;
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
        let exponent = self.constant(state, integer, exponent)?.value;
        let zero = self.constant(state, integer, 0)?.value;
        let compare = |this: &mut Self, state: &State, op, a, b| {
            let result = this.fresh_value(EvoIRType::Bits(1));
            this.emit(state, Inst::ICmp(result.clone(), op, a, b))?;
            Ok::<_, LowerError>(result)
        };
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
            | IsUnordered => unreachable!(),
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
        let kind = match literal.suffix {
            evocc_chir::FloatSuffix::F => FloatKind::F32,
            evocc_chir::FloatSuffix::None => FloatKind::F64,
            _ => return Err(LowerError::FloatingValue { span }),
        };
        let raw = match literal.value {
            evocc_chir::FloatConstant::Real(value) => value
                .target_convert(kind)
                .ok_or(LowerError::FloatingValue { span })?
                .bits(),
            evocc_chir::FloatConstant::QuietNaN(payload) => {
                if kind == FloatKind::F32 {
                    0x7fc00000 | (payload & 0x3fffff)
                } else {
                    0x7ff8000000000000 | (payload & 0x7ffffffffffff)
                }
            }
        };
        self.floating_instruction(
            FloatingOp::Constant(raw as u64),
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
        let op = if floating_kind(&value.value.ty).is_some() {
            FloatingOp::Convert
        } else if value.integer.is_signed() && value.value.ty != EvoIRType::Bits(1) {
            FloatingOp::FromSigned
        } else {
            FloatingOp::FromUnsigned
        };
        self.floating_instruction(op, vec![value.value], ty, state)
    }

    pub(super) fn floating_to_integer(
        &mut self,
        value: ExprValue,
        target: CIntegerType,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
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
        let kind = if left.value.ty == EvoIRType::F64 || right.value.ty == EvoIRType::F64 {
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
            1u64 << 31
        } else {
            1u64 << 63
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
            .any(|v| matches!(definition(v.id), Some(Inst::Const(_, mask)) if mask & sign == 0))
    }

    pub(super) fn floating_truth(
        &mut self,
        value: ExprValue,
        state: &mut State,
    ) -> Result<Value, LowerError> {
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
        let kind = floating_kind(&value.value.ty).unwrap();
        let one = self.floating_instruction(
            FloatingOp::Constant(FloatBits::from_unsigned(kind, 1).bits() as u64),
            vec![],
            value.value.ty.clone(),
            state,
        )?;
        self.floating_binary(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            },
            value,
            one,
            state,
        )
    }
}
