//! Target intrinsics use ordinary vector/memory instructions after argument
//! evaluation. Unsupported intrinsics never escape as external libc calls.
use super::*;
use evocc_types::neon::{operation, Operation};

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_neon_call(
        &mut self,
        name: &str,
        function_ty: TypeId,
        arguments: &[&'a CHirExpr<'a>],
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        if evocc_types::neon::apple_signature(name).is_some() {
            return self.lower_apple_neon_call(name, function_ty, arguments, span, state);
        }
        let unsupported = || LowerError::TargetBuiltin {
            name: name.into(),
            span,
        };
        let op = operation(name).ok_or_else(unsupported)?;
        let Some(TypeKind::Function {
            return_type,
            parameters:
                FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
        }) = self.unit.types().get(function_ty).map(|ty| &ty.kind)
        else {
            return Err(unsupported());
        };
        let return_ty = *return_type;
        let types = types.clone();
        if types.len() != arguments.len() {
            return Err(unsupported());
        }
        let immediate = |index: usize| {
            let value = evocc_chir::constant::eval_integer_constant(
                arguments[index],
                self.unit.types(),
                self.unit.symbols(),
                self.target,
            )?;
            let parameter =
                canonical_integer(self.unit.types(), self.target, types[index], span).ok()?;
            Some(value.cast(parameter))
        };
        if op == Operation::LaneBounds {
            let values = (0..arguments.len())
                .map(|index| immediate(index).and_then(CInteger::try_mathematical_value))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(unsupported)?;
            if values.len() != 3
                || values[1] <= 0
                || values[2] < 0
                || values[2] >= values[0] / values[1]
            {
                return Err(unsupported());
            }
            return Ok(None);
        }
        let count = if let Some(&index) = evocc_types::neon::immediate_indices(op).first() {
            let (_, element_count) =
                evocc_chir::vector::shape(self.unit.types(), return_ty).ok_or_else(unsupported)?;
            let size = self
                .target
                .layout_of(self.unit.types(), return_ty)
                .map_err(|_| unsupported())?
                .size_bytes;
            let width = size * 8 / u64::from(element_count);
            let n = immediate(index)
                .and_then(CInteger::try_mathematical_value)
                .and_then(|n| u64::try_from(n).ok())
                .filter(|&n| {
                    if matches!(
                        op,
                        Operation::ShiftLeft | Operation::ShiftInsert { right: false }
                    ) {
                        n < width
                    } else {
                        n > 0 && n <= width
                    }
                })
                .ok_or_else(unsupported)?;
            Some((n, width))
        } else {
            None
        };
        let mut keys = Vec::new();
        for (argument, parameter) in arguments.iter().zip(&types) {
            let value = self.lower_expr(argument, state)?;
            let value = self.coerce_to_object_type(
                value,
                Some(argument),
                *parameter,
                argument.span(),
                state,
            )?;
            let key = self.retain_temporary(value.value, state);
            state.env.get_mut(&key).unwrap().integer = Some(value.integer);
            keys.push(key);
        }
        let values = keys
            .into_iter()
            .map(|key| {
                let value = state.env.remove(&key).expect("retained intrinsic argument");
                ExprValue {
                    value: value.value,
                    integer: value.integer.unwrap(),
                }
            })
            .collect::<Vec<_>>();
        if let Operation::ExtractHalf { high } = op {
            let (_, lanes) =
                evocc_chir::vector::shape(self.unit.types(), types[0]).ok_or_else(unsupported)?;
            let half = u64::from(lanes) / 2;
            let offset = if high { half } else { 0 };
            if evocc_chir::vector::shape(self.unit.types(), return_ty).is_none() {
                let mut result =
                    self.vector_lane_constant(&values[0], Some(types[0]), offset, span, state)?;
                result.integer =
                    canonical_integer(self.unit.types(), self.target, return_ty, span)?;
                return Ok(Some(result));
            }
            let mut elements = Vec::new();
            for lane in 0..half {
                elements.push(Some(
                    self.vector_lane_constant(
                        &values[0],
                        Some(types[0]),
                        offset + lane,
                        span,
                        state,
                    )?
                    .value,
                ));
            }
            return self.pack_vector(return_ty, elements, span, state).map(Some);
        }
        if let Operation::PolynomialLong { high } = op {
            let mut operands = Vec::new();
            for (value, ty) in values.iter().zip(&types) {
                operands.push(if high {
                    self.vector_lane_constant(value, Some(*ty), 1, span, state)?
                } else {
                    value.clone()
                });
            }
            let result = self.polynomial_product(
                &operands[0],
                &operands[1],
                CIntegerType::UINT128,
                64,
                state,
            )?;
            return Ok(Some(ExprValue {
                value: result,
                integer: CIntegerType::UINT128,
            }));
        }
        if let Operation::Aes(op) = op {
            let result = self.fresh_value(values[0].value.ty.clone());
            let instruction = evocc_evoir::VectorInst {
                result: result.clone(),
                op: evocc_evoir::VectorOp::Aes(op),
                arguments: values.into_iter().map(|value| value.value).collect(),
            };
            instruction.validate().map_err(|_| unsupported())?;
            self.emit(state, Inst::Vector(instruction))?;
            return Ok(Some(ExprValue {
                value: result,
                integer: CIntegerType::ULONG,
            }));
        }
        if op == Operation::Reinterpret {
            return self
                .lower_vector_values(
                    &evocc_chir::vector::VectorOp::Reinterpret,
                    &types.iter().copied().map(Some).collect::<Vec<_>>(),
                    return_ty,
                    values,
                    span,
                    state,
                )
                .map(Some);
        }
        if let Operation::LoadInterleaved { vectors } = op {
            let vector_ty = evocc_types::neon::tuple_vector(self.unit.types(), return_ty, vectors)
                .ok_or_else(unsupported)?;
            let (element, lanes) =
                evocc_chir::vector::shape(self.unit.types(), vector_ty).ok_or_else(unsupported)?;
            let layout = self.access_layout(element, span)?;
            let vector_layout = self.access_layout(vector_ty, span)?;
            let origin = self.synthetic_origin()?;
            let destination = self.emit_alloca(return_ty, span, origin, state)?;
            for vector in 0..u64::from(vectors) {
                let mut elements = Vec::new();
                for lane in 0..u64::from(lanes) {
                    let address = self.constant_offset_address(
                        values[0].value.clone(),
                        (lane * u64::from(vectors) + vector) * layout.size_bytes,
                        state,
                    )?;
                    let value = self.emit_memory_load(address, element, false, span, state)?;
                    elements.push(Some(value.value));
                }
                let value = self.pack_vector(vector_ty, elements, span, state)?;
                let address = self.constant_offset_address(
                    destination.clone(),
                    vector * vector_layout.size_bytes,
                    state,
                )?;
                self.emit_memory_store(address, value, vector_ty, false, span, state)?;
            }
            return Ok(Some(ExprValue {
                value: destination,
                integer: CIntegerType::ULONG,
            }));
        }
        if matches!(op, Operation::Load | Operation::Store) {
            let ty = if op == Operation::Load {
                return_ty
            } else {
                types[1]
            };
            let TypeKind::Pointer(pointee) = self
                .unit
                .types()
                .get(types[0])
                .ok_or_else(unsupported)?
                .kind
            else {
                return Err(unsupported());
            };
            let vector_layout = self.access_layout(ty, span)?;
            let scalar_layout = self.access_layout(pointee, span)?;
            let layout = AccessLayout::new(vector_layout.size_bytes, scalar_layout.align_bytes);
            if op == Operation::Load {
                return self
                    .emit_memory_load_with_layout(
                        values[0].value.clone(),
                        ty,
                        false,
                        span,
                        state,
                        layout,
                    )
                    .map(Some);
            }
            self.emit_memory_store_with_layout(
                values[0].value.clone(),
                values[1].clone(),
                layout,
                false,
                state,
            )?;
            return Ok(None);
        }
        let (element, lanes) =
            evocc_chir::vector::shape(self.unit.types(), return_ty).ok_or_else(unsupported)?;
        let integer = canonical_integer(self.unit.types(), self.target, element, span)?;
        if op == Operation::Table {
            let (_, table_lanes) =
                evocc_chir::vector::shape(self.unit.types(), types[0]).ok_or_else(unsupported)?;
            let zero = self.constant(state, integer, 0)?;
            let zero_table = self.pack_vector(
                types[0],
                vec![Some(zero.value); usize::from(table_lanes)],
                span,
                state,
            )?;
            let mut indices = Vec::new();
            for lane in 0..u64::from(lanes) {
                let index =
                    self.vector_lane_constant(&values[1], Some(types[1]), lane, span, state)?;
                let limit = self.constant(state, index.integer, u64::from(table_lanes))?;
                let in_range =
                    self.checked_compare(state, CmpOp::ULt, &index.value, &limit.value)?;
                // Pick the first lane of the zero vector for an invalid
                // index. The existing shuffle transports the selected vector
                // before extracting, so it never reads an unrelated lane.
                indices.push(Some(self.select_bits(
                    state,
                    &in_range,
                    &index.value,
                    &limit.value,
                )?));
            }
            let indices = self.pack_vector(types[1], indices, span, state)?;
            return self
                .lower_vector_values(
                    &evocc_chir::vector::VectorOp::Shuffle,
                    &[Some(types[0]), Some(types[0]), Some(types[1])],
                    return_ty,
                    vec![values[0].clone(), zero_table, indices],
                    span,
                    state,
                )
                .map(Some);
        }
        let mut elements = Vec::new();
        for lane in 0..u64::from(lanes) {
            let result = match op {
                Operation::ReverseByteBits => {
                    let source =
                        self.vector_lane_constant(&values[0], Some(types[0]), lane, span, state)?;
                    let mut result = source.value;
                    for (bits, mask) in [(1, 0x55), (2, 0x33), (4, 0x0f)] {
                        let count = self.constant(state, CIntegerType::UINT, bits)?;
                        let mask = self.constant(state, integer, mask)?;
                        let right = self.fresh_value(result.ty.clone());
                        self.emit(
                            state,
                            Inst::LShr(right.clone(), result.clone(), count.value.clone()),
                        )?;
                        let right = self.emit_raw_binary(state, Inst::And, &right, &mask.value)?;
                        let low = self.emit_raw_binary(state, Inst::And, &result, &mask.value)?;
                        let left = self.fresh_value(result.ty.clone());
                        self.emit(state, Inst::Shl(left.clone(), low, count.value))?;
                        result = self.emit_raw_binary(state, Inst::Or, &right, &left)?;
                    }
                    result
                }
                Operation::Combine => {
                    let half = u64::from(lanes) / 2;
                    let source = usize::from(lane >= half);
                    self.vector_lane_constant(
                        &values[source],
                        Some(types[source]),
                        lane % half,
                        span,
                        state,
                    )?
                    .value
                }
                Operation::ShiftRightNarrow => {
                    let (n, _) = count.expect("checked immediate");
                    let value =
                        self.vector_lane_constant(&values[0], Some(types[0]), lane, span, state)?;
                    let amount = self.constant(state, CIntegerType::UINT, n)?;
                    let shifted = self.fresh_value(value.value.ty.clone());
                    self.emit(
                        state,
                        Inst::LShr(shifted.clone(), value.value, amount.value),
                    )?;
                    self.cast(
                        ExprValue {
                            value: shifted,
                            integer: value.integer,
                        },
                        integer,
                        state,
                    )?
                    .value
                }
                Operation::ShiftInsert { right } => {
                    let (n, width) = count.expect("checked immediate");
                    let a =
                        self.vector_lane_constant(&values[0], Some(types[0]), lane, span, state)?;
                    let b =
                        self.vector_lane_constant(&values[1], Some(types[1]), lane, span, state)?;
                    let mask = if right {
                        ((1_u128 << n) - 1) << (width - n)
                    } else {
                        (1_u128 << n) - 1
                    };
                    let mask = self.wide_constant(state, integer, mask)?;
                    let retained = self.emit_raw_binary(state, Inst::And, &a.value, &mask.value)?;
                    let shifted = if right && n == width {
                        let zero = self.constant(state, integer, 0)?;
                        self.emit_raw_binary(state, Inst::And, &b.value, &zero.value)?
                    } else {
                        let amount = self.constant(state, CIntegerType::UINT, n)?;
                        let result = self.fresh_value(b.value.ty.clone());
                        self.emit(
                            state,
                            if right {
                                Inst::LShr(result.clone(), b.value, amount.value)
                            } else {
                                Inst::Shl(result.clone(), b.value, amount.value)
                            },
                        )?;
                        result
                    };
                    self.emit_raw_binary(state, Inst::Or, &retained, &shifted)?
                }
                Operation::PolynomialMultiply => {
                    let a =
                        self.vector_lane_constant(&values[0], Some(types[0]), lane, span, state)?;
                    let b =
                        self.vector_lane_constant(&values[1], Some(types[1]), lane, span, state)?;
                    self.polynomial_product(&a, &b, integer, 8, state)?
                }
                Operation::ShiftLeft => {
                    let (n, _) = count.expect("checked immediate");
                    let value =
                        self.vector_lane_constant(&values[0], Some(types[0]), lane, span, state)?;
                    let amount = self.constant(state, CIntegerType::UINT, n)?;
                    let result = self.fresh_value(value.value.ty.clone());
                    self.emit(state, Inst::Shl(result.clone(), value.value, amount.value))?;
                    result
                }
                Operation::ShiftRight { signed } => {
                    let (n, width) = count.expect("checked immediate");
                    let value =
                        self.vector_lane_constant(&values[0], Some(types[0]), lane, span, state)?;
                    if n == width && !signed {
                        // Still consume the lane: a full-width shift does not
                        // authorize ignoring an uninitialized operand.
                        let zero = self.constant(state, integer, 0)?;
                        self.emit_raw_binary(state, Inst::And, &value.value, &zero.value)?
                    } else {
                        let amount = self.constant(state, CIntegerType::UINT, n.min(width - 1))?;
                        let result = self.fresh_value(value.value.ty.clone());
                        self.emit(
                            state,
                            if signed {
                                Inst::AShr(result.clone(), value.value, amount.value)
                            } else {
                                Inst::LShr(result.clone(), value.value, amount.value)
                            },
                        )?;
                        result
                    }
                }
                _ => return Err(unsupported()),
            };
            elements.push(Some(result));
        }
        self.pack_vector(return_ty, elements, span, state).map(Some)
    }

    fn polynomial_product(
        &mut self,
        a: &ExprValue,
        b: &ExprValue,
        result_type: CIntegerType,
        input_bits: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        // Carryless multiplication: XOR shifted coefficients in the result
        // width. Widen before shifting so the high half is never discarded.
        let a = self.cast(a.clone(), result_type, state)?;
        let b = self.cast(b.clone(), result_type, state)?;
        let zero = self.constant(state, result_type, 0)?;
        let one = self.constant(state, result_type, 1)?;
        let mut result = zero.value.clone();
        for bit in 0..input_bits {
            let count = self.constant(state, CIntegerType::UINT, bit)?;
            let right = self.fresh_value(b.value.ty.clone());
            self.emit(
                state,
                Inst::LShr(right.clone(), b.value.clone(), count.value.clone()),
            )?;
            let low = self.emit_raw_binary(state, Inst::And, &right, &one.value)?;
            let mask = self.emit_raw_binary(state, Inst::Sub, &zero.value, &low)?;
            let left = self.fresh_value(a.value.ty.clone());
            self.emit(state, Inst::Shl(left.clone(), a.value.clone(), count.value))?;
            let product = self.emit_raw_binary(state, Inst::And, &left, &mask)?;
            result = self.emit_raw_binary(state, Inst::Xor, &result, &product)?;
        }
        Ok(result)
    }
}

impl<'a> FunctionLowerer<'a> {
    /// The Clang generic forms carry vectors as sixteen bytes and specify
    /// their lane type with a constant selector. Preserve each operand once
    /// before another operand can change the block or memory state.
    fn lower_apple_neon_call(
        &mut self,
        name: &str,
        function_ty: TypeId,
        arguments: &[&'a CHirExpr<'a>],
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        let unsupported = || LowerError::UnknownBuiltin {
            name: name.into(),
            span,
        };
        if !matches!(
            name,
            "__builtin_neon_vld1q_v"
                | "__builtin_neon_vst1q_v"
                | "__builtin_neon_vzipq_v"
                | "__builtin_neon_vuzpq_v"
        ) {
            return Err(unsupported());
        }
        let Some(TypeKind::Function {
            return_type,
            parameters:
                FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
        }) = self.unit.types().get(function_ty).map(|ty| &ty.kind)
        else {
            return Err(unsupported());
        };
        let return_ty = *return_type;
        let types = types.clone();
        if types.len() != arguments.len() {
            return Err(unsupported());
        }
        let selector = evocc_chir::constant::eval_integer_constant(
            arguments.last().ok_or_else(unsupported)?,
            self.unit.types(),
            self.unit.symbols(),
            self.target,
        )
        .map(|value| value.cast(CIntegerType::INT))
        .and_then(CInteger::try_mathematical_value)
        .ok_or_else(unsupported)?;
        let width = match selector {
            32 => 1,
            33 => 2,
            34 => 4,
            _ => return Err(unsupported()),
        };
        let mut keys = Vec::new();
        for (argument, parameter) in arguments[..arguments.len() - 1].iter().zip(&types) {
            let value = self.lower_expr(argument, state)?;
            let value = self.coerce_to_object_type(
                value,
                Some(argument),
                *parameter,
                argument.span(),
                state,
            )?;
            let key = self.retain_temporary(value.value, state);
            state.env.get_mut(&key).unwrap().integer = Some(value.integer);
            keys.push(key);
        }
        let values = keys
            .into_iter()
            .map(|key| {
                let value = state.env.remove(&key).expect("retained intrinsic operand");
                ExprValue {
                    value: value.value,
                    integer: value.integer.unwrap(),
                }
            })
            .collect::<Vec<_>>();
        let layout = AccessLayout::new(16, 1);
        if name == "__builtin_neon_vld1q_v" {
            return self
                .emit_memory_load_with_layout(
                    values[0].value.clone(),
                    return_ty,
                    false,
                    span,
                    state,
                    layout,
                )
                .map(Some);
        }
        if name == "__builtin_neon_vst1q_v" {
            self.emit_memory_store_with_layout(
                values[0].value.clone(),
                values[1].clone(),
                layout,
                false,
                state,
            )?;
            return Ok(None);
        }
        let lanes = 16 / width;
        for half in 0..2 {
            let indices = (0..16)
                .map(|byte| {
                    let lane = byte / width;
                    let sub = byte % width;
                    let source_lane = if name == "__builtin_neon_vzipq_v" {
                        (lane % 2) * lanes + half * (lanes / 2) + lane / 2
                    } else {
                        (lane / (lanes / 2)) * lanes + 2 * (lane % (lanes / 2)) + half
                    };
                    source_lane * width + sub
                })
                .collect();
            let vector = self.lower_vector_values(
                &evocc_chir::vector::VectorOp::ShuffleStatic(indices),
                &[Some(types[1]), Some(types[2])],
                types[1],
                vec![values[1].clone(), values[2].clone()],
                span,
                state,
            )?;
            let address =
                self.constant_offset_address(values[0].value.clone(), half as u64 * 16, state)?;
            self.emit_memory_store_with_layout(address, vector, layout, false, state)?;
        }
        Ok(None)
    }
}
