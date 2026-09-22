use super::*;
use evocc_types::neon::{operation, Operation};

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn check_neon_immediates(
        &self,
        callee: &CHirExpr<'arena>,
        arguments: &[&CHirExpr<'arena>],
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let CHirExpr::Ident {
            symbol, spelling, ..
        } = callee
        else {
            return Ok(());
        };
        if !self.unit.symbols().is_builtin_function(*symbol) {
            return Ok(());
        }
        let Some(op) = operation(spelling) else {
            return Ok(());
        };
        for &index in evocc_types::neon::immediate_indices(op) {
            if arguments
                .get(index)
                .and_then(|expression| {
                    crate::constant::eval_integer_constant(
                        expression,
                        self.unit.types(),
                        self.unit.symbols(),
                        &self.target,
                    )
                })
                .is_none()
            {
                return Err(InterpreterError::TargetBuiltin {
                    name: spelling.clone(),
                    span,
                });
            }
        }
        Ok(())
    }

    pub(super) fn neon_call(
        &mut self,
        name: &str,
        function_ty: TypeId,
        arguments: &[CValue],
        span: SourceSpan,
    ) -> Result<Option<CValue>, InterpreterError> {
        let unsupported = || InterpreterError::TargetBuiltin {
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
        let ty = *return_type;
        let types = types.clone();
        if arguments.len() != types.len() {
            return Err(unsupported());
        }
        let arguments = arguments
            .iter()
            .zip(&types)
            .map(|(&value, &ty)| {
                cast_machine_value(
                    value,
                    self.value_type(ty, span)?,
                    span,
                    self.unit.types(),
                    &mut self.memory,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if op == Operation::Combine && matches!(arguments[0], CValue::Aggregate { .. }) {
            let destination = self.vector_temporary(ty, span)?;
            for (half, argument) in arguments.iter().enumerate() {
                let CValue::Aggregate {
                    address,
                    qualifiers,
                    ..
                } = *argument
                else {
                    return Err(unsupported());
                };
                let target = destination
                    .checked_offset(half as i64 * 8)
                    .map_err(|error| self.memory_error(error, span))?;
                let (memory, world) = self
                    .memory
                    .copy_with_volatile(
                        &self.world,
                        target,
                        address,
                        AccessLayout::new(8, 8),
                        evocc_memory::CopyVolatility::new(qualifiers.is_volatile, false),
                    )
                    .map_err(|error| self.memory_error(error, span))?;
                self.memory = memory;
                self.world = world;
            }
            return Ok(Some(CValue::Aggregate {
                address: destination,
                ty,
                qualifiers: Qualifiers::default(),
            }));
        }
        if let Operation::ExtractHalf { high } = op {
            let (_, lanes) =
                crate::vector::shape(self.unit.types(), types[0]).ok_or_else(unsupported)?;
            if crate::vector::shape(self.unit.types(), ty).is_none() {
                return self
                    .vector_lane(
                        arguments[0],
                        usize::from(high) * usize::from(lanes) / 2,
                        span,
                    )
                    .map(CValue::Integer)
                    .map(Some);
            }
            let CValue::Aggregate {
                address,
                qualifiers,
                ..
            } = arguments[0]
            else {
                return Err(unsupported());
            };
            let source = address
                .checked_offset(if high { 8 } else { 0 })
                .map_err(|error| self.memory_error(error, span))?;
            let destination = self.vector_temporary(ty, span)?;
            let (memory, world) = self
                .memory
                .copy_with_volatile(
                    &self.world,
                    destination,
                    source,
                    AccessLayout::new(8, 8),
                    if qualifiers.is_volatile {
                        evocc_memory::CopyVolatility::Read
                    } else {
                        evocc_memory::CopyVolatility::None
                    },
                )
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            return Ok(Some(CValue::Aggregate {
                address: destination,
                ty,
                qualifiers: Qualifiers::default(),
            }));
        }
        if let Operation::PolynomialLong { high } = op {
            let mut operand = |value| {
                if high {
                    self.vector_lane(value, 1, span).map(CInteger::raw_bits)
                } else {
                    expect_machine_integer(value, span).map(CInteger::raw_bits)
                }
            };
            let a = operand(arguments[0])?;
            let b = operand(arguments[1])?;
            let result = (0..64).fold(0, |result, bit| {
                result ^ if b & (1 << bit) != 0 { a << bit } else { 0 }
            });
            return Ok(Some(CValue::Integer(CInteger::from_raw(
                result,
                self.integer_type(ty, span)?,
            ))));
        }
        if let Operation::Aes(op) = op {
            let layout = AccessLayout::new(16, 16);
            let mut inputs = Vec::new();
            for value in &arguments {
                let CValue::Aggregate {
                    address,
                    qualifiers,
                    ..
                } = value
                else {
                    return Err(unsupported());
                };
                let (representation, world) = self
                    .memory
                    .load_representation(&self.world, *address, layout, qualifiers.is_volatile)
                    .map_err(|error| self.memory_error(error, span))?;
                self.world = world;
                inputs.push(std::array::from_fn(|i| {
                    representation
                        .read_integer(i, 1, evocc_types::ByteOrder::Little)
                        .map(|b| b as u8)
                }));
            }
            let mut representation = evocc_memory::ObjectRepresentation::uninitialized(16)
                .map_err(|error| self.memory_error(error, span))?;
            for (i, byte) in op.evaluate(&inputs).into_iter().enumerate() {
                representation
                    .write_integer(i, 1, byte.map(u128::from), evocc_types::ByteOrder::Little)
                    .map_err(|error| self.memory_error(error, span))?;
            }
            let address = self.vector_temporary(ty, span)?;
            let (memory, world) = self
                .memory
                .store_representation(&self.world, address, layout, &representation, false)
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = memory;
            self.world = world;
            return Ok(Some(CValue::Aggregate {
                address,
                ty,
                qualifiers: Qualifiers::default(),
            }));
        }
        if op == Operation::Reinterpret {
            return self
                .apply_vector(
                    &crate::vector::VectorExpr {
                        op: crate::vector::VectorOp::Reinterpret,
                        operands: Vec::new(),
                        facts: ExprFacts::value(ty),
                        span,
                    },
                    &arguments,
                )
                .map(Some);
        }
        if op == Operation::LaneBounds {
            let values = arguments
                .iter()
                .map(|&value| {
                    expect_machine_integer(value, span)
                        .ok()
                        .and_then(CInteger::try_mathematical_value)
                })
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
        if let Operation::LoadInterleaved { vectors } = op {
            let vector_ty = evocc_types::neon::tuple_vector(self.unit.types(), ty, vectors)
                .ok_or_else(unsupported)?;
            let (element, lanes) =
                crate::vector::shape(self.unit.types(), vector_ty).ok_or_else(unsupported)?;
            let layout = self.access_layout(element, span)?;
            let vector_layout = self.access_layout(vector_ty, span)?;
            let (source, _) = expect_machine_pointer(arguments[0], span)?;
            let destination = self.vector_temporary(ty, span)?;
            for vector in 0..u64::from(vectors) {
                for lane in 0..u64::from(lanes) {
                    let source = source
                        .checked_offset(
                            ((lane * u64::from(vectors) + vector) * layout.size_bytes) as i64,
                        )
                        .map_err(|error| self.memory_error(error, span))?;
                    let value =
                        self.load_memory_value(source, element, Qualifiers::default(), span)?;
                    let to = destination
                        .checked_offset(
                            (vector * vector_layout.size_bytes + lane * layout.size_bytes) as i64,
                        )
                        .map_err(|error| self.memory_error(error, span))?;
                    self.store_memory_value(to, element, Qualifiers::default(), value, span)?;
                }
            }
            return Ok(Some(CValue::Aggregate {
                address: destination,
                ty,
                qualifiers: Qualifiers::default(),
            }));
        }
        if matches!(op, Operation::Load | Operation::Store) {
            let vector_ty = if op == Operation::Load { ty } else { types[1] };
            let TypeKind::Pointer(pointee) = self
                .unit
                .types()
                .get(types[0])
                .ok_or_else(unsupported)?
                .kind
            else {
                return Err(unsupported());
            };
            let vector_layout = self.access_layout(vector_ty, span)?;
            let scalar_layout = self.access_layout(pointee, span)?;
            let layout = AccessLayout::new(vector_layout.size_bytes, scalar_layout.align_bytes);
            let (address, _) = expect_machine_pointer(arguments[0], span)?;
            if op == Operation::Load {
                return self
                    .load_memory_value_with_layout(
                        address,
                        vector_ty,
                        Qualifiers::default(),
                        span,
                        layout,
                    )
                    .map(Some);
            }
            self.store_memory_value_with_layout(
                address,
                vector_ty,
                Qualifiers::default(),
                arguments[1],
                span,
                layout,
            )?;
            return Ok(None);
        }
        let (element, lanes) =
            crate::vector::shape(self.unit.types(), ty).ok_or_else(unsupported)?;
        let integer = self.integer_type(element, span)?;
        let count = if let Some(&index) = evocc_types::neon::immediate_indices(op).first() {
            Some(
                expect_machine_integer(arguments[index], span)?
                    .try_mathematical_value()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|&n| {
                        if matches!(
                            op,
                            Operation::ShiftLeft | Operation::ShiftInsert { right: false }
                        ) {
                            n < u32::from(integer.bits())
                        } else {
                            n > 0 && n <= u32::from(integer.bits())
                        }
                    })
                    .ok_or_else(unsupported)?,
            )
        } else {
            None
        };
        let destination = self.vector_temporary(ty, span)?;
        let layout = self.access_layout(element, span)?;
        for lane in 0..usize::from(lanes) {
            let raw = match op {
                Operation::ReverseByteBits => u128::from(
                    (self.vector_lane(arguments[0], lane, span)?.raw_bits() as u8).reverse_bits(),
                ),
                Operation::Combine => {
                    let half = usize::from(lanes) / 2;
                    self.vector_lane(arguments[usize::from(lane >= half)], lane % half, span)?
                        .raw_bits()
                }
                Operation::ShiftRightNarrow => {
                    self.vector_lane(arguments[0], lane, span)?.raw_bits()
                        >> count.expect("checked immediate")
                }
                Operation::ShiftInsert { right } => {
                    let a = self.vector_lane(arguments[0], lane, span)?.raw_bits();
                    let b = self.vector_lane(arguments[1], lane, span)?.raw_bits();
                    let n = count.expect("checked immediate");
                    if right {
                        let mask = ((1_u128 << n) - 1) << (u32::from(integer.bits()) - n);
                        (a & mask) | (b >> n)
                    } else {
                        (a & ((1_u128 << n) - 1)) | (b << n)
                    }
                }
                Operation::PolynomialMultiply => {
                    let a = self.vector_lane(arguments[0], lane, span)?.raw_bits();
                    let b = self.vector_lane(arguments[1], lane, span)?.raw_bits();
                    (0..8).fold(0, |result, bit| {
                        result ^ if b & (1 << bit) != 0 { a << bit } else { 0 }
                    })
                }
                Operation::ShiftLeft => {
                    self.vector_lane(arguments[0], lane, span)?.raw_bits()
                        << count.expect("checked shift count")
                }
                Operation::ShiftRight { signed } => {
                    let value = self.vector_lane(arguments[0], lane, span)?;
                    let count = count.expect("checked shift count");
                    if signed {
                        (value.mathematical_value() >> count.min(u32::from(integer.bits()) - 1))
                            as u128
                    } else {
                        value.raw_bits().checked_shr(count).unwrap_or(0)
                    }
                }
                Operation::Table => {
                    let index = self.vector_lane(arguments[1], lane, span)?.raw_bits();
                    let (_, table_lanes) = crate::vector::shape(self.unit.types(), types[0])
                        .ok_or_else(unsupported)?;
                    if index < u128::from(table_lanes) {
                        self.vector_lane(arguments[0], index as usize, span)?
                            .raw_bits()
                    } else {
                        0
                    }
                }
                _ => return Err(unsupported()),
            };
            let at = destination
                .checked_offset((lane as u64 * layout.size_bytes) as i64)
                .map_err(|error| self.memory_error(error, span))?;
            self.store_memory_value(
                at,
                element,
                Qualifiers::default(),
                CValue::Integer(CInteger::from_raw(raw, integer)),
                span,
            )?;
        }
        Ok(Some(CValue::Aggregate {
            address: destination,
            ty,
            qualifiers: Qualifiers::default(),
        }))
    }
}
