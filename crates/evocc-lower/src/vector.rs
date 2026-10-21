use super::*;
use evocc_chir::vector::{VectorExpr, VectorOp};
use evocc_evoir::{VectorInst, VectorOp as IrVectorOp};

impl<'a> FunctionLowerer<'a> {
    fn vector_instruction(
        &mut self,
        op: IrVectorOp,
        arguments: Vec<Value>,
        ty: EvoIRType,
        integer: CIntegerType,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        let result = self.fresh_value(ty);
        let instruction = VectorInst {
            result: result.clone(),
            op,
            arguments,
        };
        instruction
            .validate()
            .map_err(|message| LowerError::InvalidGeneratedSsa { message })?;
        self.emit(state, Inst::Vector(instruction))?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    pub(super) fn vector_lane(
        &mut self,
        vector: &ExprValue,
        ty: Option<TypeId>,
        index: ExprValue,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if let Some(ty) = ty.filter(|ty| is_wide_vector(self.unit.types(), *ty)) {
            let (element, _) = evocc_chir::vector::shape(self.unit.types(), ty).unwrap();
            let address = self.offset_for_type(
                vector.value.clone(),
                index,
                OffsetDirection::Add,
                element,
                span,
                state,
            )?;
            return self.emit_memory_load(address, element, false, span, state);
        }
        let EvoIRType::Vec(_, element) = &vector.value.ty else {
            return Ok(vector.clone());
        };
        self.vector_instruction(
            IrVectorOp::Extract,
            vec![vector.value.clone(), index.value],
            element.as_ref().clone(),
            vector.integer,
            state,
        )
    }

    pub(super) fn vector_lane_constant(
        &mut self,
        vector: &ExprValue,
        ty: Option<TypeId>,
        index: u64,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let index = self.constant(state, CIntegerType::UINT, index)?;
        self.vector_lane(vector, ty, index, span, state)
    }

    pub(super) fn pack_vector(
        &mut self,
        ty: TypeId,
        elements: Vec<Option<Value>>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (element, _) = evocc_chir::vector::shape(self.unit.types(), ty).ok_or(
            LowerError::UnsupportedExpression {
                span,
                feature: "invalid vector result",
            },
        )?;
        // A floating lane is an `f32` or `f64` value; its integer field is the
        // placeholder of every floating value.
        let floating = evocc_chir::vector::floating(self.unit.types(), ty).map(|(kind, _)| kind);
        let integer = if floating.is_some() {
            CIntegerType::ULONG
        } else {
            canonical_integer(self.unit.types(), self.target, element, span)?
        };
        if is_wide_vector(self.unit.types(), ty) {
            let origin = self.synthetic_origin()?;
            let destination = self.emit_alloca(ty, span, origin, state)?;
            let layout = self.access_layout(element, span)?;
            for (lane, value) in elements.into_iter().enumerate() {
                if let Some(value) = value {
                    let address = self.constant_offset_address(
                        destination.clone(),
                        lane as u64 * layout.size_bytes,
                        state,
                    )?;
                    self.emit_memory_store(
                        address,
                        ExprValue { value, integer },
                        element,
                        false,
                        span,
                        state,
                    )?;
                }
            }
            Ok(ExprValue {
                value: destination,
                integer,
            })
        } else {
            let mut values = Vec::with_capacity(elements.len());
            let lane =
                floating.map_or_else(|| bits(integer), |kind| floating::ir_type(kind).unwrap());
            for value in elements {
                values.push(match value {
                    Some(value) => value,
                    None => {
                        let value = self.fresh_value(lane.clone());
                        self.emit(state, Inst::Uninitialized(value.clone()))?;
                        value
                    }
                });
            }
            let (output, _) = self.c_value_type(ty, span)?;
            self.vector_instruction(IrVectorOp::Pack, values, output, integer, state)
        }
    }

    pub(super) fn lower_vector(
        &mut self,
        expression: &'a VectorExpr<'a>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let mut keys = Vec::new();
        let mut types = Vec::new();
        for operand in &expression.operands {
            let value = self.lower_expr(operand, state)?;
            let key = self.retain_temporary(value.value.clone(), state);
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
                let value = state.env.remove(&key).expect("retained vector operand");
                ExprValue {
                    value: value.value,
                    integer: value.integer.unwrap(),
                }
            })
            .collect();
        self.lower_vector_values(
            &expression.op,
            &types,
            expression.facts.ty(),
            arguments,
            expression.span,
            state,
        )
    }

    pub(super) fn lower_vector_values(
        &mut self,
        op: &VectorOp,
        types: &[Option<TypeId>],
        ty: TypeId,
        arguments: Vec<ExprValue>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if *op == VectorOp::Extract {
            return self.vector_lane(&arguments[0], types[0], arguments[1].clone(), span, state);
        }
        if *op == VectorOp::Reinterpret {
            if is_wide_vector(self.unit.types(), ty) {
                return Ok(arguments[0].clone());
            }
            let (output, integer) = self.c_value_type(ty, span)?;
            return self.vector_instruction(
                IrVectorOp::Bitcast,
                vec![arguments[0].value.clone()],
                output,
                integer,
                state,
            );
        }
        if matches!(op, VectorOp::Shuffle | VectorOp::ShuffleStatic(_)) {
            return self.lower_vector_shuffle(op, types, ty, arguments, span, state);
        }
        let floating = |ty: &Option<TypeId>| {
            ty.is_some_and(|ty| evocc_chir::vector::floating(self.unit.types(), ty).is_some())
        };
        if floating(&Some(ty)) || types.iter().any(floating) {
            return self.lower_floating_vector_values(op, types, ty, arguments, span, state);
        }
        let (element, lanes) = evocc_chir::vector::shape(self.unit.types(), ty).ok_or(
            LowerError::UnsupportedExpression {
                span,
                feature: "invalid vector result",
            },
        )?;
        let integer = canonical_integer(self.unit.types(), self.target, element, span)?;
        let mut elements = Vec::new();
        for lane in 0..u64::from(lanes) {
            let left = self.vector_lane_constant(&arguments[0], types[0], lane, span, state)?;
            let value = match op {
                VectorOp::Binary(op) => {
                    let right =
                        self.vector_lane_constant(&arguments[1], types[1], lane, span, state)?;
                    self.vector_binary(*op, left, right, integer, span, state)?
                }
                VectorOp::Unary(UnaryOp::Plus) => left,
                VectorOp::Unary(UnaryOp::Neg) => {
                    let zero = self.constant(state, left.integer, 0)?;
                    self.emit_signed_arithmetic(
                        state,
                        BinaryOp::Sub,
                        zero,
                        left.clone(),
                        left.integer,
                    )?
                }
                VectorOp::Unary(UnaryOp::BitNot) => {
                    let ones = self.wide_constant(state, left.integer, left.integer.mask())?;
                    ExprValue {
                        value: self.emit_raw_binary(state, Inst::Xor, &left.value, &ones.value)?,
                        integer: left.integer,
                    }
                }
                VectorOp::Unary(UnaryOp::Not) => {
                    let zero = self.constant(state, left.integer, 0)?;
                    let condition =
                        self.checked_compare(state, CmpOp::Eq, &left.value, &zero.value)?;
                    self.vector_mask(condition, integer, state)?
                }
                VectorOp::Convert => self.cast(left, integer, state)?,
                _ => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "invalid vector operation",
                    })
                }
            };
            elements.push(Some(value.value));
        }
        self.pack_vector(ty, elements, span, state)
    }

    /// L05: `++` and `--` on a floating vector add or subtract one in each
    /// lane, as GCC does. The value is the vector before or after the update.
    pub(super) fn lower_floating_vector_update(
        &mut self,
        operator: UpdateOp,
        ty: TypeId,
        address: Value,
        volatile: bool,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (kind, _) = evocc_chir::vector::floating(self.unit.types(), ty).ok_or(
            LowerError::UnsupportedExpression {
                span,
                feature: "invalid floating vector update",
            },
        )?;
        let wide = is_wide_vector(self.unit.types(), ty);
        let original = if wide {
            // A wide vector is an object: the old value is a copy of it.
            let origin = self.synthetic_origin()?;
            let copy = self.emit_alloca(ty, span, origin, state)?;
            self.emit_aggregate_transfer(
                copy.clone(),
                address.clone(),
                ty,
                CopyVolatility::new(volatile, false),
                span,
                state,
            )?;
            ExprValue {
                value: copy,
                integer: CIntegerType::ULONG,
            }
        } else {
            self.emit_memory_load(address.clone(), ty, volatile, span, state)?
        };
        let address = self.retain_temporary(address, state);
        let kept = self.retain_temporary(original.value.clone(), state);
        let one = self.floating_constant(
            evocc_types::floating::FloatBits::from_unsigned(kind, 1),
            state,
        )?;
        let op = if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PostIncrement) {
            BinaryOp::Add
        } else {
            BinaryOp::Sub
        };
        let updated = self.lower_vector_values(
            &VectorOp::Binary(op),
            &[Some(ty), None],
            ty,
            vec![original, one],
            span,
            state,
        )?;
        let address = state.env.remove(&address).expect("retained address").value;
        let original = ExprValue {
            value: state.env.remove(&kept).expect("retained vector").value,
            integer: CIntegerType::ULONG,
        };
        if wide {
            self.emit_aggregate_transfer(
                address,
                updated.value.clone(),
                ty,
                CopyVolatility::new(false, volatile),
                span,
                state,
            )?;
        } else {
            self.emit_memory_store(address, updated.clone(), ty, volatile, span, state)?;
        }
        Ok(
            if matches!(operator, UpdateOp::PreIncrement | UpdateOp::PreDecrement) {
                updated
            } else {
                original
            },
        )
    }

    /// L05: each lane of a floating vector operation is the scalar operation
    /// of EvoIR 3.13 on that lane, in ascending lane order, so a strict region
    /// threads its world through the lanes. A comparison gives a mask lane of
    /// all ones or zero, and a conversion is the C cast of each lane, with
    /// its `E3063` obligation.
    fn lower_floating_vector_values(
        &mut self,
        op: &VectorOp,
        types: &[Option<TypeId>],
        ty: TypeId,
        arguments: Vec<ExprValue>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (element, lanes) = evocc_chir::vector::shape(self.unit.types(), ty).ok_or(
            LowerError::UnsupportedExpression {
                span,
                feature: "invalid vector result",
            },
        )?;
        let result = evocc_chir::vector::floating(self.unit.types(), ty).map(|(kind, _)| kind);
        let integer = match result {
            Some(_) => CIntegerType::ULONG,
            None => canonical_integer(self.unit.types(), self.target, element, span)?,
        };
        let mut keys = Vec::new();
        for argument in arguments {
            let key = self.retain_temporary(argument.value, state);
            state.env.get_mut(&key).unwrap().integer = Some(argument.integer);
            keys.push(key);
        }
        let at = |key: EnvKey, state: &State| {
            let v = &state.env[&key];
            ExprValue {
                value: v.value.clone(),
                integer: v.integer.unwrap(),
            }
        };
        let mut result_keys = Vec::new();
        for lane in 0..u64::from(lanes) {
            let left =
                self.vector_lane_constant(&at(keys[0], state), types[0], lane, span, state)?;
            let value = match op {
                VectorOp::Binary(op) => {
                    let left = self.retain_temporary(left.value, state);
                    let right = self.vector_lane_constant(
                        &at(keys[1], state),
                        types[1],
                        lane,
                        span,
                        state,
                    )?;
                    let left = state.env.remove(&left).unwrap().value;
                    let left = ExprValue {
                        value: left,
                        integer: CIntegerType::ULONG,
                    };
                    let value = self.floating_binary(*op, left, right, state)?;
                    if matches!(
                        op,
                        BinaryOp::Eq
                            | BinaryOp::Ne
                            | BinaryOp::Lt
                            | BinaryOp::Le
                            | BinaryOp::Gt
                            | BinaryOp::Ge
                    ) {
                        self.vector_mask(value.value, integer, state)?
                    } else {
                        value
                    }
                }
                VectorOp::Unary(UnaryOp::Plus) => left,
                VectorOp::Unary(UnaryOp::Neg) => self.floating_unary(UnaryOp::Neg, left, state)?,
                VectorOp::Convert => match result {
                    Some(kind) => self.cast_floating(left, kind, state)?,
                    None => self.cast(left, integer, state)?,
                },
                _ => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "invalid floating vector operation",
                    })
                }
            };
            result_keys.push(self.retain_temporary(value.value, state));
        }
        for key in keys {
            state.env.remove(&key);
        }
        let values = result_keys
            .into_iter()
            .map(|key| Some(state.env.remove(&key).unwrap().value))
            .collect();
        self.pack_vector(ty, values, span, state)
    }

    /// Select a representation before reading its lane; unchosen sources and
    /// static -1 lanes do not consume indeterminate values.
    fn lower_vector_shuffle(
        &mut self,
        op: &VectorOp,
        types: &[Option<TypeId>],
        ty: TypeId,
        arguments: Vec<ExprValue>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (_, lanes) = evocc_chir::vector::shape(self.unit.types(), ty).unwrap();
        let (_, source_lanes) =
            evocc_chir::vector::shape(self.unit.types(), types[0].unwrap()).unwrap();
        let source_count = arguments.len() - usize::from(*op == VectorOp::Shuffle);
        let mut keys = Vec::new();
        for value in arguments {
            let key = self.retain_temporary(value.value.clone(), state);
            state.env.get_mut(&key).unwrap().integer = Some(value.integer);
            keys.push(key);
        }
        let at = |key: EnvKey, state: &State| {
            let v = &state.env[&key];
            ExprValue {
                value: v.value.clone(),
                integer: v.integer.unwrap(),
            }
        };
        let mut result_keys = Vec::new();
        for lane in 0..lanes {
            let value = if let VectorOp::ShuffleStatic(indices) = op {
                let index = indices[usize::from(lane)];
                if index < 0 {
                    result_keys.push(None);
                    continue;
                }
                let source = index as usize / usize::from(source_lanes);
                self.vector_lane_constant(
                    &at(keys[source], state),
                    types[source],
                    index as u64 % u64::from(source_lanes),
                    span,
                    state,
                )?
            } else {
                let mask = self.vector_lane_constant(
                    &at(*keys.last().unwrap(), state),
                    *types.last().unwrap(),
                    u64::from(lane),
                    span,
                    state,
                )?;
                let mask = self.cast(mask, CIntegerType::ULONG, state)?;
                let count = self.constant(state, CIntegerType::ULONG, u64::from(source_lanes))?;
                let index = self.emit_raw_binary(state, Inst::URem, &mask.value, &count.value)?;
                let index_key = self.retain_temporary(index, state);
                let mut source = at(keys[0], state);
                if source_count == 2 {
                    let bit = self.emit_raw_binary(state, Inst::And, &mask.value, &count.value)?;
                    let zero = self.constant(state, CIntegerType::ULONG, 0)?;
                    let condition = self.checked_compare(state, CmpOp::Eq, &bit, &zero.value)?;
                    let old_keys: Vec<_> = state.env.keys().copied().collect();
                    let mut yes = self.new_block_from(state, &old_keys, BlockRole::Ordinary)?;
                    let mut no = self.new_block_from(state, &old_keys, BlockRole::Ordinary)?;
                    let yes_args = self.edge_values(state, &old_keys)?;
                    let no_args = self.edge_values(state, &old_keys)?;
                    self.terminate(
                        state,
                        Inst::CondBr(condition, yes.block, yes_args, no.block, no_args),
                    )?;
                    let selected = EnvKey::Temporary(self.next_temporary);
                    self.next_temporary += 1;
                    yes.env.insert(selected, yes.env[&keys[0]].clone());
                    no.env.insert(selected, no.env[&keys[1]].clone());
                    let mut merged = old_keys;
                    merged.push(selected);
                    let merge = self.new_block_from(&yes, &merged, BlockRole::Ordinary)?;
                    self.branch(&yes, merge.block, &merged)?;
                    self.branch(&no, merge.block, &merged)?;
                    *state = merge;
                    source = at(selected, state);
                    state.env.remove(&selected);
                }
                let index = state.env.remove(&index_key).unwrap().value;
                self.vector_lane(
                    &source,
                    types[0],
                    ExprValue {
                        value: index,
                        integer: CIntegerType::ULONG,
                    },
                    span,
                    state,
                )?
            };
            let key = self.retain_temporary(value.value, state);
            result_keys.push(Some(key));
        }
        let values = result_keys
            .into_iter()
            .map(|key| key.map(|key| state.env.remove(&key).unwrap().value))
            .collect();
        for key in keys {
            state.env.remove(&key);
        }
        self.pack_vector(ty, values, span, state)
    }

    fn vector_mask(
        &mut self,
        condition: Value,
        integer: CIntegerType,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        let result = self.fresh_value(bits(integer));
        self.emit(state, Inst::SExt(result.clone(), condition, bits(integer)))?;
        Ok(ExprValue {
            value: result,
            integer,
        })
    }

    fn vector_binary(
        &mut self,
        op: BinaryOp,
        left: ExprValue,
        right: ExprValue,
        result_type: CIntegerType,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let ty = left.integer;
        match op {
            BinaryOp::Shl | BinaryOp::Shr => self.emit_shift(state, op, left, right, ty),
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                self.emit_signed_arithmetic(state, op, left, right, ty)
            }
            BinaryOp::Div | BinaryOp::Mod => self.emit_div_rem(state, op, left, right, ty),
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                let instruction = match op {
                    BinaryOp::BitAnd => Inst::And,
                    BinaryOp::BitOr => Inst::Or,
                    _ => Inst::Xor,
                };
                let value = self.emit_raw_binary(state, instruction, &left.value, &right.value)?;
                Ok(ExprValue { value, integer: ty })
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let (cmp, a, b) = match op {
                    BinaryOp::Eq => (CmpOp::Eq, left.value, right.value),
                    BinaryOp::Ne => (CmpOp::Ne, left.value, right.value),
                    BinaryOp::Lt => (
                        if ty.is_signed() {
                            CmpOp::SLt
                        } else {
                            CmpOp::ULt
                        },
                        left.value,
                        right.value,
                    ),
                    BinaryOp::Le => (
                        if ty.is_signed() {
                            CmpOp::SLe
                        } else {
                            CmpOp::ULe
                        },
                        left.value,
                        right.value,
                    ),
                    BinaryOp::Gt => (
                        if ty.is_signed() {
                            CmpOp::SLt
                        } else {
                            CmpOp::ULt
                        },
                        right.value,
                        left.value,
                    ),
                    _ => (
                        if ty.is_signed() {
                            CmpOp::SLe
                        } else {
                            CmpOp::ULe
                        },
                        right.value,
                        left.value,
                    ),
                };
                let condition = self.checked_compare(state, cmp, &a, &b)?;
                self.vector_mask(condition, result_type, state)
            }
            _ => Err(LowerError::UnsupportedExpression {
                span,
                feature: "invalid vector binary operation",
            }),
        }
    }
}
