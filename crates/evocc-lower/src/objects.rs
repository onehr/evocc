//! Lowering objects.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn synthetic_origin(&mut self) -> Result<u64, LowerError> {
        let local = self.next_synthetic_origin;
        self.next_synthetic_origin =
            self.next_synthetic_origin
                .checked_add(1)
                .ok_or(LowerError::InvalidGeneratedSsa {
                    message: "aggregate temporary origin counter overflow".to_owned(),
                })?;
        Ok((1_u64 << 63) | (u64::from(self.info.ordinal) << 32) | u64::from(local))
    }

    pub(super) fn emit_alloca(
        &mut self,
        ty: TypeId,
        span: SourceSpan,
        origin: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let layout = self.access_layout(ty, span)?;
        self.emit_alloca_with_layout(layout, origin, state)
    }

    pub(super) fn emit_alloca_with_layout(
        &mut self,
        layout: AccessLayout,
        origin: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Alloca {
                result: address.clone(),
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                origin,
                layout,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(address)
    }

    pub(super) fn emit_copy(
        &mut self,
        destination: Value,
        source: Value,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        self.emit_representation_copy(destination, source, ty, CopyVolatility::None, span, state)
    }

    pub(super) fn emit_representation_copy(
        &mut self,
        destination: Value,
        source: Value,
        ty: TypeId,
        volatile: CopyVolatility,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if !matches!(destination.ty, EvoIRType::Ptr(_)) || !matches!(source.ty, EvoIRType::Ptr(_)) {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "aggregate copy operands are not object addresses",
            });
        }
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        let layout = self.access_layout(ty, span)?;
        if !volatile.supports(layout) {
            return Err(LowerError::VolatileAggregate { span });
        }
        self.emit(
            state,
            Inst::Copy {
                destination,
                source,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                layout,
                volatile,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(())
    }

    pub(super) fn aggregate_source_is_volatile(&self, expression: &CHirExpr<'_>) -> bool {
        match expression {
            CHirExpr::Unary(UnaryOp::Deref, pointer, span) => self
                .pointer_pointee_type(pointer, *span)
                .ok()
                .and_then(|ty| self.unit.types().get(ty))
                .is_some_and(|ty| ty.qualifiers.is_volatile),
            CHirExpr::Cast(_, operand, _) => self.aggregate_source_is_volatile(operand),
            CHirExpr::MemoryRead { place, .. } => place.access_qualifiers().is_volatile,
            CHirExpr::Ident { facts, .. } | CHirExpr::MemoryAssign { facts, .. } => self
                .unit
                .types()
                .get(facts.ty())
                .is_some_and(|ty| ty.qualifiers.is_volatile),
            // A statement expression undergoes lvalue conversion. Its value
            // has the unqualified type even when its final expression named a
            // volatile object, so the copy out of its temporary is not
            // volatile.
            CHirExpr::StatementExpr { .. } => false,
            CHirExpr::CompoundLit(ty, _, _) => self
                .unit
                .types()
                .get(*ty)
                .is_some_and(|ty| ty.qualifiers.is_volatile),
            _ => false,
        }
    }

    pub(super) fn emit_aggregate_transfer(
        &mut self,
        destination: Value,
        source: Value,
        ty: TypeId,
        volatile: CopyVolatility,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if volatile == CopyVolatility::None {
            return self.emit_copy(destination, source, ty, span, state);
        }
        let kind = self
            .unit
            .types()
            .get(ty)
            .ok_or(LowerError::UnknownType { ty, span })?
            .kind
            .clone();
        match kind {
            TypeKind::Struct(tag) => {
                let fields = match &self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(LowerError::UnsupportedType {
                        ty,
                        span,
                        feature: "volatile aggregate has an unknown record tag",
                    })?
                    .state
                {
                    TagState::Complete(TagDefinition::Struct(fields)) => fields.clone(),
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile aggregate is not a complete struct",
                        });
                    }
                };
                let layout = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|source| LowerError::Layout { ty, span, source })?;
                for (field_index, field) in fields.into_iter().enumerate() {
                    let Some(field_layout) = layout
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                    else {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile member has no target layout",
                        });
                    };
                    if field.bit_field.is_some() {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile aggregate bit-field copy is not specified",
                        });
                    }
                    let destination = self.constant_offset_address(
                        destination.clone(),
                        field_layout.byte_offset,
                        state,
                    )?;
                    let source = self.constant_offset_address(
                        source.clone(),
                        field_layout.byte_offset,
                        state,
                    )?;
                    self.emit_aggregate_transfer(
                        destination,
                        source,
                        field.ty,
                        volatile,
                        span,
                        state,
                    )?;
                }
                Ok(())
            }
            TypeKind::Union(_) => {
                self.emit_representation_copy(destination, source, ty, volatile, span, state)
            }
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } => {
                let element_layout = self.access_layout(element, span)?;
                for index in 0..count {
                    let offset = index.checked_mul(element_layout.size_bytes).ok_or(
                        LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "volatile array member offset overflow",
                        },
                    )?;
                    let destination =
                        self.constant_offset_address(destination.clone(), offset, state)?;
                    let source = self.constant_offset_address(source.clone(), offset, state)?;
                    self.emit_aggregate_transfer(
                        destination,
                        source,
                        element,
                        volatile,
                        span,
                        state,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(()),
            TypeKind::Pointer(_) | TypeKind::Int(_) | TypeKind::Enum(_) => {
                let value = self.emit_memory_load(source, ty, volatile.reads(), span, state)?;
                self.emit_memory_store(destination, value, ty, volatile.writes(), span, state)?;
                Ok(())
            }
            TypeKind::Float(_) | TypeKind::TargetBuiltin(_)
                if floating::scalar_format(self.unit.types(), self.target, ty).is_some() =>
            {
                // A floating member is transported as its exact bits.
                let value = self.emit_memory_load(source, ty, volatile.reads(), span, state)?;
                self.emit_memory_store(destination, value, ty, volatile.writes(), span, state)?;
                Ok(())
            }
            TypeKind::Vector { element, lanes, .. } => {
                if !is_wide_vector(self.unit.types(), ty) {
                    // A short vector is one access of its size.
                    let value = self.emit_memory_load(source, ty, volatile.reads(), span, state)?;
                    self.emit_memory_store(destination, value, ty, volatile.writes(), span, state)?;
                    return Ok(());
                }
                // A wide vector is an object of lanes: it is accessed lane by
                // lane, as an array is.
                let width = self.access_layout(element, span)?.size_bytes;
                for lane in 0..u64::from(lanes) {
                    let destination =
                        self.constant_offset_address(destination.clone(), lane * width, state)?;
                    let source =
                        self.constant_offset_address(source.clone(), lane * width, state)?;
                    self.emit_aggregate_transfer(
                        destination,
                        source,
                        element,
                        volatile,
                        span,
                        state,
                    )?;
                }
                Ok(())
            }
            TypeKind::Complex(_) if evocc_chir::complex::admitted(self.unit.types(), ty) => {
                // A volatile complex object is accessed part by part.
                let element = evocc_chir::complex::element(self.unit.types(), ty)
                    .ok_or(LowerError::UnknownType { ty, span })?;
                let width = self.access_layout(element, span)?.size_bytes;
                for offset in [0, width] {
                    let destination =
                        self.constant_offset_address(destination.clone(), offset, state)?;
                    let source = self.constant_offset_address(source.clone(), offset, state)?;
                    self.emit_aggregate_transfer(
                        destination,
                        source,
                        element,
                        volatile,
                        span,
                        state,
                    )?;
                }
                Ok(())
            }
            _ => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "volatile aggregate member has no executable scalar representation",
            }),
        }
    }

    pub(super) fn replace_effect_tokens(
        &self,
        state: &mut State,
        memory: Value,
        world: Option<Value>,
    ) {
        state.env.insert(
            EnvKey::Memory,
            BoundValue {
                value: memory,
                integer: None,
            },
        );
        if let Some(world) = world {
            state.env.insert(
                EnvKey::World,
                BoundValue {
                    value: world,
                    integer: None,
                },
            );
        }
    }

    pub(super) fn object_address(
        &mut self,
        symbol: SymbolId,
        span: SourceSpan,
        state: &State,
    ) -> Result<Value, LowerError> {
        if let Some(address) = state.env.get(&EnvKey::Address(symbol)) {
            return Ok(address.value.clone());
        }
        let declaration = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
        if !has_linked_object_storage(self.unit, symbol) {
            return Err(LowerError::InvalidGeneratedSsa {
                message: format!(
                    "automatic memory object '{}' ({}) has no live address binding",
                    declaration.spelling(),
                    symbol.raw()
                ),
            });
        }
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        self.emit(
            state,
            Inst::GlobalAddr {
                result: address.clone(),
                symbol: declaration.spelling().to_owned(),
            },
        )?;
        Ok(address)
    }

    pub(super) fn offset_address(
        &mut self,
        base: Value,
        index: ExprValue,
        direction: OffsetDirection,
        signedness: IndexSignedness,
        scale: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let (index, scale) = if scale == 0 {
            // GNU zero-sized elements have zero displacement, but their index
            // is still evaluated and consumed (including an uninitialized
            // value). Keep ptr.offset's strictly positive scale invariant.
            let zero = self.fresh_value(index.value.ty.clone());
            self.emit(state, Inst::Const(zero.clone(), 0))?;
            let offset = self.emit_raw_binary(state, Inst::Mul, &index.value, &zero)?;
            (offset, 1)
        } else {
            (index.value, scale)
        };
        let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        self.emit(
            state,
            Inst::PtrOffset {
                result: result.clone(),
                base,
                index,
                direction,
                signedness,
                scale,
            },
        )?;
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        self.emit(
            state,
            Inst::PointerValid {
                mem_in,
                pointer: result.clone(),
                mem_out: mem_out.clone(),
            },
        )?;
        self.replace_effect_tokens(state, mem_out, None);
        Ok(result)
    }

    pub(super) fn constant_offset_address(
        &mut self,
        base: Value,
        offset: u64,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        if offset == 0 {
            return Ok(base);
        }
        let raw = self.fresh_value(EvoIRType::Bits(64));
        self.emit(state, Inst::Const(raw.clone(), offset))?;
        self.offset_address(
            base,
            ExprValue {
                value: raw,
                integer: CIntegerType::ULONG,
            },
            OffsetDirection::Add,
            IndexSignedness::Unsigned,
            1,
            state,
        )
    }

    pub(super) fn lower_place_address(
        &mut self,
        place: &MemoryPlace<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let mut address = match place.root() {
            PlaceRoot::Object { symbol } => self.object_address(*symbol, span, state)?,
            PlaceRoot::PointerExpression { pointer, .. } => {
                let lowered = self.lower_expr(pointer, state)?;
                let function_designator = place.projections().is_empty()
                    && matches!(
                        self.unit.types().get(place.value_ty()).map(|ty| &ty.kind),
                        Some(TypeKind::Function { .. })
                    )
                    && matches!(lowered.value.ty, EvoIRType::FnPtr { .. });
                if !matches!(lowered.value.ty, EvoIRType::Ptr(_)) && !function_designator {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "memory place pointer root did not lower to a pointer",
                    });
                }
                lowered.value
            }
            PlaceRoot::CompoundLiteral { expression, .. } => {
                self.lower_compound_literal_address(expression, state)?
            }
        };
        for projection in place.projections() {
            match projection {
                PlaceProjection::Index(projection) => {
                    let address_key = self.retain_temporary(address, state);
                    let index = self.lower_expr(projection.index(), state)?;
                    address = state
                        .env
                        .remove(&address_key)
                        .expect("retained indexed address")
                        .value;
                    let promoted = self.cast(index, projection.promoted_index(), state)?;
                    address = self.offset_for_type(
                        address,
                        promoted,
                        OffsetDirection::Add,
                        projection.element_ty(),
                        span,
                        state,
                    )?;
                }
                PlaceProjection::Field(projection) => {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), projection.tag())
                        .map_err(|source| LowerError::Layout {
                            ty: projection.field_ty(),
                            span,
                            source,
                        })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|field| field.field_index == projection.field_index())
                        .ok_or(LowerError::UnsupportedExpression {
                            span,
                            feature: "record projection has no target field layout",
                        })?;
                    address = self.constant_offset_address(address, field.byte_offset, state)?;
                }
            }
        }
        Ok(address)
    }

    pub(super) fn validate_place(
        &self,
        place: ScalarPlace,
        target: &CHirExpr<'a>,
        span: SourceSpan,
    ) -> Result<(), LowerError> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span: target_span,
        } = target
        else {
            return Err(LowerError::UnsupportedExpression {
                span,
                feature: "assignment target is not a scalar identifier place",
            });
        };
        let declaration =
            self.unit
                .symbols()
                .symbol(*symbol)
                .map_err(|_| LowerError::UnknownSymbol {
                    symbol: *symbol,
                    span: *target_span,
                })?;
        if *symbol != place.symbol()
            || !matches!(
                declaration.kind(),
                SymbolKind::Object | SymbolKind::Parameter
            )
            || declaration.spelling() != spelling
            || declaration.ty() != place.ty()
            || facts.ty() != place.ty()
            || facts.category() != ExprCategory::LValue
            || !facts.is_modifiable_lvalue()
        {
            return Err(LowerError::UnsupportedExpression {
                span: *target_span,
                feature: "scalar place disagrees with its semantic identity",
            });
        }
        Ok(())
    }

    pub(super) fn local(
        &mut self,
        state: &mut State,
        symbol: SymbolId,
        span: SourceSpan,
    ) -> Result<ExprValue, LowerError> {
        if self.global_registers.contains_key(&symbol) {
            return self.register_assembly(symbol, None, span, state);
        }
        if self.memory_symbols.contains(&symbol) {
            let declaration = self
                .unit
                .symbols()
                .symbol(symbol)
                .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
            let ty = declaration.ty();
            let volatile = self
                .unit
                .types()
                .get(ty)
                .is_some_and(|ty| ty.qualifiers.is_volatile);
            let address = self.object_address(symbol, span, state)?;
            return self.emit_memory_load(address, ty, volatile, span, state);
        }
        let Some(bound) = state.env.get(&EnvKey::Local(symbol)) else {
            let declaration = self
                .unit
                .symbols()
                .symbol(symbol)
                .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
            let (ty, integer) = self.c_value_type(declaration.ty(), span)?;
            let value = self.fresh_value(ty);
            self.emit(state, Inst::Uninitialized(value.clone()))?;
            return Ok(ExprValue { value, integer });
        };
        Ok(ExprValue {
            value: bound.value.clone(),
            integer: bound.integer.ok_or(LowerError::UnsupportedExpression {
                span,
                feature: "local does not carry an integer type",
            })?,
        })
    }

    pub(super) fn store_local(
        &mut self,
        symbol: SymbolId,
        value: ExprValue,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if self.global_registers.contains_key(&symbol) {
            self.register_assembly(symbol, Some(value), span, state)?;
        } else if self.memory_symbols.contains(&symbol) {
            let declaration = self
                .unit
                .symbols()
                .symbol(symbol)
                .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
            let ty = declaration.ty();
            let volatile = self
                .unit
                .types()
                .get(ty)
                .is_some_and(|ty| ty.qualifiers.is_volatile);
            let address = self.object_address(symbol, span, state)?;
            self.emit_memory_store(address, value, ty, volatile, span, state)?;
        } else {
            state.env.insert(
                EnvKey::Local(symbol),
                BoundValue {
                    value: value.value,
                    integer: Some(value.integer),
                },
            );
        }
        Ok(())
    }
}
