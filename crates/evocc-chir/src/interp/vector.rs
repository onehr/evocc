use super::*;
use crate::vector::{shape, VectorExpr, VectorOp};

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn vector_temporary(
        &mut self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<Pointer, InterpreterError> {
        let layout = self.access_layout(ty, span)?;
        let frame = self
            .memory_frame
            .as_ref()
            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let (memory, address) = self
            .memory
            .allocate_automatic(
                frame.id,
                frame.current_scope,
                self.temporary_origin_site,
                layout,
            )
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        Ok(address)
    }

    pub(super) fn vector_lane(
        &mut self,
        value: CValue,
        index: usize,
        span: SourceSpan,
    ) -> Result<CInteger, InterpreterError> {
        if let CValue::Integer(value) = value {
            return Ok(value);
        }
        let CValue::Aggregate {
            address,
            ty,
            qualifiers,
        } = value
        else {
            return Err(InterpreterError::UninitializedValue { span });
        };
        let (element, lanes) =
            shape(self.unit.types(), ty).ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let layout = self.access_layout(element, span)?;
        if index >= usize::from(lanes) {
            return Err(self.memory_error(
                evocc_memory::MemoryFault::OutOfBounds {
                    address,
                    access: Some(layout),
                    object_size: u64::from(lanes) * layout.size_bytes,
                },
                span,
            ));
        }
        // Use the existing object bounds check to give an invalid index its
        // ordinary memory diagnostic. Never mask an element index.
        let offset = (index as u64)
            .checked_mul(layout.size_bytes)
            .and_then(|offset| i64::try_from(offset).ok())
            .ok_or(InterpreterError::UnsupportedType { ty, span })?;
        let at = address
            .checked_offset(offset)
            .map_err(|error| self.memory_error(error, span))?;
        let value = self.load_memory_value(at, element, qualifiers, span)?;
        expect_machine_integer(value, span)
    }

    pub(super) fn apply_vector(
        &mut self,
        expression: &VectorExpr<'arena>,
        arguments: &[CValue],
    ) -> Result<CValue, InterpreterError> {
        let span = expression.span;
        let invalid = || InterpreterError::UnsupportedExpression {
            feature: "invalid vector operation",
            span,
        };
        if expression.op == VectorOp::Extract {
            if arguments.len() != 2 {
                return Err(invalid());
            }
            let index = expect_machine_integer(arguments[1], span)?
                .try_mathematical_value()
                .and_then(|index| usize::try_from(index).ok())
                .unwrap_or(usize::MAX);
            return self
                .vector_lane(arguments[0], index, span)
                .map(CValue::Integer);
        }
        let ty = expression.facts.ty();
        if expression.op == VectorOp::Reinterpret {
            let [source] = arguments else {
                return Err(invalid());
            };
            let layout = self.access_layout(ty, span)?;
            if shape(self.unit.types(), ty).is_none() {
                let CValue::Aggregate { address, .. } = source else {
                    return Err(invalid());
                };
                let raw = self
                    .memory
                    .load_integer(*address, layout)
                    .map_err(|error| self.memory_error(error, span))?;
                return Ok(CValue::Integer(CInteger::from_raw(
                    raw,
                    self.integer_type(ty, span)?,
                )));
            }
            let destination = self.vector_temporary(ty, span)?;
            self.memory = match source {
                CValue::Aggregate {
                    address,
                    ty: source,
                    ..
                } => {
                    if self.access_layout(*source, span)?.size_bytes != layout.size_bytes {
                        return Err(invalid());
                    }
                    self.memory.copy_bytes(
                        destination,
                        *address,
                        AccessLayout::new(layout.size_bytes, 1),
                    )
                }
                CValue::Integer(value) if u64::from(value.ty().bits()) == layout.size_bytes * 8 => {
                    self.memory
                        .store_integer(destination, layout, value.raw_bits())
                }
                _ => return Err(invalid()),
            }
            .map_err(|error| self.memory_error(error, span))?;
            return Ok(CValue::Aggregate {
                address: destination,
                ty,
                qualifiers: Qualifiers::default(),
            });
        }
        let (element, lanes) = shape(self.unit.types(), ty).ok_or_else(invalid)?;
        let output_integer = self.integer_type(element, span)?;
        let destination = self.vector_temporary(ty, span)?;
        let layout = self.access_layout(element, span)?;
        for lane in 0..usize::from(lanes) {
            let result = match &expression.op {
                VectorOp::Unary(op) => {
                    let [value] = arguments else {
                        return Err(invalid());
                    };
                    let value = self.vector_lane(*value, lane, span)?;
                    if *op == UnaryOp::Not {
                        CInteger::from_raw(
                            if value.truth() { 0 } else { u128::MAX },
                            output_integer,
                        )
                    } else if self.unit.signed_wrapping() && *op == UnaryOp::Neg {
                        CInteger::from_raw(0u128.wrapping_sub(value.raw_bits()), value.ty())
                    } else {
                        crate::integer::eval_unary_at_type(*op, value)
                            .map_err(|error| self.integer_error(error, span))?
                    }
                }
                VectorOp::Binary(op) => {
                    let [left, right] = arguments else {
                        return Err(invalid());
                    };
                    let left = self.vector_lane(*left, lane, span)?;
                    let right = self.vector_lane(*right, lane, span)?;
                    let operation_type = left.ty();
                    let wrapping = self.unit.signed_wrapping()
                        && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul);
                    let value = crate::integer::eval_binary_at_type(
                        *op,
                        left,
                        right,
                        if wrapping {
                            operation_type.unsigned_version()
                        } else {
                            operation_type
                        },
                    )
                    .map_err(|error| self.integer_error(error, span))?;
                    if matches!(
                        op,
                        BinaryOp::Eq
                            | BinaryOp::Ne
                            | BinaryOp::Lt
                            | BinaryOp::Le
                            | BinaryOp::Gt
                            | BinaryOp::Ge
                    ) {
                        CInteger::from_raw(
                            if value.truth() { u128::MAX } else { 0 },
                            output_integer,
                        )
                    } else {
                        value.cast(output_integer)
                    }
                }
                VectorOp::Convert => {
                    let [value] = arguments else {
                        return Err(invalid());
                    };
                    self.vector_lane(*value, lane, span)?.cast(output_integer)
                }
                VectorOp::Shuffle | VectorOp::ShuffleStatic(_) => {
                    let (sources, index) = match &expression.op {
                        VectorOp::Shuffle => {
                            if !matches!(arguments.len(), 2 | 3) {
                                return Err(invalid());
                            }
                            let (mask, sources) = arguments.split_last().ok_or_else(invalid)?;
                            (
                                sources,
                                self.vector_lane(*mask, lane, span)?.raw_bits() as usize,
                            )
                        }
                        VectorOp::ShuffleStatic(indices) => {
                            let index = *indices.get(lane).ok_or_else(invalid)?;
                            if index == -1 {
                                continue;
                            }
                            (arguments, usize::try_from(index).map_err(|_| invalid())?)
                        }
                        _ => unreachable!(),
                    };
                    let CValue::Aggregate {
                        ty: source_type, ..
                    } = sources[0]
                    else {
                        return Err(invalid());
                    };
                    let (_, source_lanes) =
                        shape(self.unit.types(), source_type).ok_or_else(invalid)?;
                    let source_lanes = usize::from(source_lanes);
                    let index = index % (source_lanes * sources.len());
                    self.vector_lane(sources[index / source_lanes], index % source_lanes, span)?
                }
                VectorOp::Extract | VectorOp::Reinterpret => return Err(invalid()),
            };
            let at = destination
                .checked_offset((lane as u64 * layout.size_bytes) as i64)
                .map_err(|error| self.memory_error(error, span))?;
            self.store_memory_value(
                at,
                element,
                Qualifiers::default(),
                CValue::Integer(result.cast(output_integer)),
                span,
            )?;
        }
        Ok(CValue::Aggregate {
            address: destination,
            ty,
            qualifiers: Qualifiers::default(),
        })
    }
}
