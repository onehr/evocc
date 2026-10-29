//! Lowering value types.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn c_value_type(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<(EvoIRType, CIntegerType), LowerError> {
        if matches!(
            self.unit.types().get(ty).map(|ty| &ty.kind),
            Some(TypeKind::TargetBuiltin(
                evocc_types::TargetBuiltinType::ArmMfp8
            ))
        ) {
            return Ok((EvoIRType::Bits(8), CIntegerType::UCHAR));
        }
        if let Ok(integer) = canonical_integer(self.unit.types(), self.target, ty, span) {
            return Ok((bits(integer), integer));
        }
        if let Some(kind) = floating::scalar_format(self.unit.types(), self.target, ty) {
            return Ok((
                floating::ir_type(kind).ok_or(LowerError::FloatingValue { span })?,
                CIntegerType::ULONG,
            ));
        }
        match self.unit.types().get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Vector { .. }) => {
                let value = function_value_type(self.unit.types(), self.target, ty, span)?;
                Ok((value.ir, value.integer))
            }
            Some(TypeKind::Pointer(pointee))
                if matches!(
                    self.unit.types().get(*pointee).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) =>
            {
                Ok((self.fnptr_type(*pointee, span)?, CIntegerType::ULONG))
            }
            Some(TypeKind::Pointer(_)) => {
                Ok((EvoIRType::Ptr(AddressSpace::Generic), CIntegerType::ULONG))
            }
            Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)) => {
                let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                Ok((bits(integer), integer))
            }
            _ => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "object subvalue is neither an integer nor pointer",
            }),
        }
    }

    pub(super) fn assembly_register_value_type(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<(EvoIRType, CIntegerType), LowerError> {
        if !self.is_aggregate_type(ty) {
            return self.c_value_type(ty, span);
        }
        let layout = self.access_layout(ty, span)?;
        let integer = match layout.size_bytes {
            4 => CIntegerType::UINT,
            8 => CIntegerType::ULONG,
            size => {
                return Err(LowerError::AsmOperand {
                    span,
                    reason: format!("r aggregate size {size}"),
                })
            }
        };
        Ok((bits(integer), integer))
    }

    pub(super) fn fnptr_type(
        &self,
        function_ty: TypeId,
        span: SourceSpan,
    ) -> Result<EvoIRType, LowerError> {
        function_pointer_ir_type(self.unit.types(), self.target, function_ty, span)
    }

    pub(super) fn function_address(
        &mut self,
        symbol: SymbolId,
        span: SourceSpan,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        let function = self
            .functions
            .get(&symbol)
            .ok_or(LowerError::UnknownSymbol { symbol, span })?;
        // An identifier-list definition can supply a complete call interface
        // even when the expression's source type still has no prototype.
        let mut params = function
            .parameter_types
            .iter()
            .map(|ty| ty.ir.clone())
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        if let Some(returned) = &function.return_type {
            if self.is_aggregate_type(returned.source) {
                params.push(EvoIRType::Ptr(AddressSpace::Generic));
            } else {
                results.push(returned.ir.clone());
            }
        }
        if function.participant {
            params.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
            results.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
        }
        let param_marks = record_signature_marks(
            self.unit.types(),
            self.target,
            &function
                .parameter_types
                .iter()
                .map(|ty| ty.source)
                .collect::<Vec<_>>(),
            function.return_type.as_ref().map(|ty| ty.source),
            params.len(),
            span,
        )?;
        let name = function.name.clone();
        let result = self.fresh_value(EvoIRType::FnPtr {
            params,
            results,
            variadic: function.variadic,
            marks: param_marks,
        });
        self.emit(
            state,
            Inst::FuncAddr {
                result: result.clone(),
                symbol: name,
            },
        )?;
        Ok(ExprValue {
            value: result,
            integer: CIntegerType::ULONG,
        })
    }

    pub(super) fn call_designator_function_type(&self, callee: &CHirExpr<'_>) -> Option<TypeId> {
        let candidate = evocc_chir::PointerTypeDesc::from_expression(
            self.unit.symbols(),
            self.unit.types(),
            callee,
        )
        .ok()?
        .pointee();
        match self.unit.types().get(candidate).map(|ty| &ty.kind) {
            Some(TypeKind::Function { .. }) => Some(candidate),
            _ => None,
        }
    }

    pub(super) fn scalar_call_signature(
        &self,
        function_ty: TypeId,
        arguments: &[&CHirExpr<'_>],
        span: SourceSpan,
    ) -> Result<(Vec<FunctionValueType>, Option<FunctionValueType>), LowerError> {
        let TypeKind::Function {
            return_type,
            parameters,
        } = &self
            .unit
            .types()
            .get(function_ty)
            .ok_or(LowerError::UnknownType {
                ty: function_ty,
                span,
            })?
            .kind
        else {
            return Err(LowerError::UnsupportedType {
                ty: function_ty,
                span,
                feature: "call designator does not name a function type",
            });
        };
        let parameter_types = match parameters {
            FunctionParameters::Prototype { types, .. } => types
                .iter()
                .map(|ty| function_value_type(self.unit.types(), self.target, *ty, span))
                .collect::<Result<Vec<_>, _>>()?,
            FunctionParameters::Unspecified => arguments
                .iter()
                .map(|argument| {
                    let CHirExpr::Cast(ty, _, _) = argument else {
                        return Err(LowerError::UnsupportedExpression {
                            span: argument.span(),
                            feature: "unprototyped argument lacks default promotion",
                        });
                    };
                    function_value_type(self.unit.types(), self.target, *ty, argument.span())
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let return_type = match self.unit.types().get(*return_type).map(|ty| &ty.kind) {
            Some(TypeKind::Void) => None,
            Some(_) => Some(function_result_type(
                self.unit.types(),
                self.target,
                *return_type,
                span,
            )?),
            None => {
                return Err(LowerError::UnknownType {
                    ty: *return_type,
                    span,
                });
            }
        };
        Ok((parameter_types, return_type))
    }

    pub(super) fn access_layout(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<AccessLayout, LowerError> {
        let layout = self
            .target
            .layout_of(self.unit.types(), ty)
            .map_err(|source| LowerError::Layout { ty, span, source })?;
        Ok(AccessLayout::new(layout.size_bytes, layout.align_bytes))
    }

    pub(super) fn place_access_layout(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<AccessLayout, LowerError> {
        place
            .access_layout(self.unit.types(), self.target)
            .map(AccessLayout::from_type_layout)
            .ok_or(LowerError::UnsupportedType {
                ty: place.value_ty(),
                span,
                feature: "place has no target access layout",
            })
    }

    pub(super) fn pointer_arithmetic_scale(
        &mut self,
        pointee: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let (fixed, captures) = self
            .target
            .array_size_factors(self.unit.types(), pointee)
            .map_err(|source| LowerError::Layout {
                ty: pointee,
                span,
                source,
            })?;
        let mut size = self.constant(state, CIntegerType::ULONG, fixed)?;
        for raw in captures {
            let symbol = self
                .unit
                .symbols()
                .symbols_in_source_order()
                .get(raw as usize)
                .ok_or(LowerError::UnsupportedExpression {
                    span,
                    feature: "array size capture has no symbol",
                })?;
            if symbol.kind() != SymbolKind::Object
                || canonical_integer(self.unit.types(), self.target, symbol.ty(), span)?
                    != CIntegerType::ULONG
                || self
                    .unit
                    .types()
                    .get(symbol.ty())
                    .is_none_or(|ty| ty.qualifiers.is_volatile || ty.qualifiers.is_atomic)
            {
                return Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "array size capture is not size_t",
                });
            }
            let count = if self.memory_symbols.contains(&symbol.id()) {
                let address = self.object_address(symbol.id(), span, state)?;
                self.emit_memory_load(address, symbol.ty(), false, span, state)?
            } else {
                self.local(state, symbol.id(), span)?
            };
            let wide_size = self.cast(size, CIntegerType::UINT128, state)?;
            let wide_count = self.cast(
                ExprValue {
                    value: count.value,
                    integer: CIntegerType::ULONG,
                },
                CIntegerType::UINT128,
                state,
            )?;
            let product =
                self.emit_raw_binary(state, Inst::Mul, &wide_size.value, &wide_count.value)?;
            let maximum = self.constant(state, CIntegerType::UINT128, i64::MAX as u64)?;
            let fits = self.checked_compare(state, CmpOp::ULe, &product, &maximum.value)?;
            let zero = self.constant(state, CIntegerType::UINT128, 0)?;
            let checked = self.select_bits(state, &fits, &product, &zero.value)?;
            size = self.cast(
                ExprValue {
                    value: checked,
                    integer: CIntegerType::UINT128,
                },
                CIntegerType::ULONG,
                state,
            )?;
        }
        Ok(size)
    }

    pub(super) fn offset_for_type(
        &mut self,
        base: Value,
        index: ExprValue,
        direction: OffsetDirection,
        pointee: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        if matches!(base.ty, EvoIRType::FnPtr { .. })
            || matches!(
                self.unit.types().get(pointee).map(|ty| &ty.kind),
                Some(TypeKind::Void)
            )
        {
            // GNU function/void addresses use byte arithmetic. A void pointer
            // may hold a label, not an object; expose and reconstruct instead
            // of attaching a nonzero offset to exact Function provenance.
            let ty = base.ty.clone();
            let base = self.cast(
                ExprValue {
                    value: base,
                    integer: CIntegerType::ULONG,
                },
                CIntegerType::ULONG,
                state,
            )?;
            let index = self.cast(index, CIntegerType::ULONG, state)?;
            let address = self.fresh_value(EvoIRType::Bits(64));
            self.emit(
                state,
                match direction {
                    OffsetDirection::Add => Inst::Add(address.clone(), base.value, index.value),
                    OffsetDirection::Subtract => {
                        Inst::Sub(address.clone(), base.value, index.value)
                    }
                },
            )?;
            let result = self.fresh_value(ty);
            self.emit(
                state,
                Inst::PointerCast {
                    result: result.clone(),
                    operand: address,
                },
            )?;
            return Ok(result);
        }
        let (fixed, captures) = self
            .target
            .array_size_factors(self.unit.types(), pointee)
            .map_err(|source| LowerError::Layout {
                ty: pointee,
                span,
                source,
            })?;
        let signedness = if index.integer.is_signed() {
            IndexSignedness::Signed
        } else {
            IndexSignedness::Unsigned
        };
        if captures.is_empty() {
            return self.offset_address(base, index, direction, signedness, fixed, state);
        }
        let scale = self.pointer_arithmetic_scale(pointee, span, state)?;
        // Retain the full product so checked pointer arithmetic sees negative
        // indexes and oversized deltas, not a wrapped 64-bit byte offset.
        let wide = if index.integer.is_signed() {
            CIntegerType::INT128
        } else {
            CIntegerType::UINT128
        };
        let index = self.cast(index, wide, state)?;
        let narrow = self.cast(
            index.clone(),
            if wide.is_signed() {
                CIntegerType::LONG
            } else {
                CIntegerType::ULONG
            },
            state,
        )?;
        let widened = self.cast(narrow, wide, state)?;
        let representable = self.checked_compare(state, CmpOp::Eq, &index.value, &widened.value)?;
        let zero = self.constant(state, CIntegerType::ULONG, 0)?;
        let positive_scale = self.checked_compare(state, CmpOp::Ne, &scale.value, &zero.value)?;
        let representable =
            self.emit_raw_binary(state, Inst::And, &representable, &positive_scale)?;
        let scale = self.cast(scale, wide, state)?;
        let product = self.fresh_value(EvoIRType::Bits(128));
        self.emit(
            state,
            Inst::Mul(product.clone(), widened.value, scale.value),
        )?;
        // A source index wider than the address model must not wrap during
        // multiplication and become an in-bounds index. Preserve a checked
        // invalid delta instead; ptr.offset reports the existing E3011.
        let invalid = self.wide_constant(state, wide, 1_u128 << 64)?;
        let product = self.select_bits(state, &representable, &product, &invalid.value)?;
        self.offset_address(
            base,
            ExprValue {
                value: product,
                integer: wide,
            },
            direction,
            signedness,
            1,
            state,
        )
    }

    pub(super) fn is_aggregate_type(&self, ty: TypeId) -> bool {
        is_aggregate_type(self.unit.types(), ty)
    }

    pub(super) fn record_value_type(&self, expression: &CHirExpr<'_>) -> Option<TypeId> {
        let ty = match expression {
            CHirExpr::Complex(operation) => operation.facts.ty(),
            CHirExpr::Ident { facts, .. }
            | CHirExpr::StatementExpr { facts, .. }
            | CHirExpr::BuiltinVarArgs { facts, .. }
            | CHirExpr::MemoryAssign { facts, .. }
            | CHirExpr::Update { facts, .. }
            | CHirExpr::MemoryUpdate { facts, .. } => facts.ty(),
            CHirExpr::MemoryRead { place, .. } => place.value_ty(),
            CHirExpr::CompoundLit(ty, ..) | CHirExpr::Cast(ty, ..) => *ty,
            CHirExpr::Call(callee, ..) => {
                let ty = self.call_designator_function_type(callee)?;
                let TypeKind::Function { return_type, .. } = &self.unit.types().get(ty)?.kind
                else {
                    return None;
                };
                *return_type
            }
            CHirExpr::Member(base, name, _) => {
                let base = self.record_value_type(base)?;
                evocc_chir::record_member_path(self.unit.types(), base, name)?
                    .last()?
                    .field_ty()
            }
            CHirExpr::Comma(_, right, _) | CHirExpr::EvaluateOnce(right, _) => {
                self.record_value_type(right)?
            }
            CHirExpr::Ternary(_, left, right, _) => {
                let ty = self.record_value_type(left)?;
                (self.record_value_type(right)? == ty).then_some(ty)?
            }
            // `*p` names the record that `p` points to, as a variadic
            // argument does when it passes `*p`.
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                let pointer = evocc_chir::vector::expression_type(self.unit.types(), pointer)?;
                match self.unit.types().get(pointer)?.kind {
                    TypeKind::Pointer(pointee) => pointee,
                    _ => return None,
                }
            }
            _ => return None,
        };
        self.is_aggregate_type(ty).then_some(ty)
    }

    pub(super) fn cast(
        &mut self,
        value: ExprValue,
        target: CIntegerType,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if target == CIntegerType::BOOL {
            let condition = self.lower_condition_value(value, state)?;
            return Ok(ExprValue {
                value: condition,
                integer: target,
            });
        }
        if evocc_evoir::floating_kind(&value.value.ty).is_some() {
            return self.floating_to_integer(value, target, state);
        }
        if matches!(value.value.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
            let result = self.fresh_value(EvoIRType::Bits(64));
            self.emit(
                state,
                Inst::PointerCast {
                    result: result.clone(),
                    operand: value.value,
                },
            )?;
            return self.cast(
                ExprValue {
                    value: result,
                    integer: CIntegerType::ULONG,
                },
                target,
                state,
            );
        }
        let EvoIRType::Bits(source_width) = value.value.ty else {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "integer expression has a non-bit-vector EvoIR value",
            });
        };
        let target_width = u16::from(target.bits());
        if source_width == target_width {
            return Ok(ExprValue {
                value: value.value,
                integer: target,
            });
        }
        let result = self.fresh_value(bits(target));
        let instruction = if source_width < target_width {
            // C comparison, logical, and `!` expressions have type `int`, but
            // EvoIR keeps their canonical truth value in `bits<1>`.  That
            // representation is always zero-extended; treating it as a
            // one-bit signed integer would turn true into -1.
            if value.integer.is_signed()
                && !(source_width == 1 && value.integer == CIntegerType::INT)
            {
                Inst::SExt(result.clone(), value.value, bits(target))
            } else {
                Inst::ZExt(result.clone(), value.value, bits(target))
            }
        } else {
            Inst::Trunc(result.clone(), value.value, bits(target))
        };
        self.emit(state, instruction)?;
        Ok(ExprValue {
            value: result,
            integer: target,
        })
    }

    pub(super) fn constant(
        &mut self,
        state: &State,
        integer: CIntegerType,
        raw: u64,
    ) -> Result<ExprValue, LowerError> {
        self.wide_constant(state, integer, u128::from(raw))
    }

    /// Keep the instruction's existing 64-bit immediate representation. A wide
    /// constant is composed from its two halves using ordinary bit operations.
    pub(super) fn wide_constant(
        &mut self,
        state: &State,
        integer: CIntegerType,
        raw: u128,
    ) -> Result<ExprValue, LowerError> {
        let raw = raw & integer.mask();
        let result = self.fresh_value(bits(integer));
        self.emit(state, Inst::Const(result.clone(), raw as u64))?;
        let result = if raw >> 64 != 0 {
            let high = self.fresh_value(bits(integer));
            let amount = self.fresh_value(EvoIRType::Bits(32));
            let shifted = self.fresh_value(bits(integer));
            let joined = self.fresh_value(bits(integer));
            self.emit(state, Inst::Const(high.clone(), (raw >> 64) as u64))?;
            self.emit(state, Inst::Const(amount.clone(), 64))?;
            self.emit(state, Inst::Shl(shifted.clone(), high, amount))?;
            self.emit(state, Inst::Or(joined.clone(), result, shifted))?;
            joined
        } else {
            result
        };
        Ok(ExprValue {
            value: result,
            integer,
        })
    }
}
