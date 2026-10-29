//! Lowering initialization.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn coerce_to_object_type(
        &mut self,
        value: ExprValue,
        source: Option<&CHirExpr<'_>>,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        if matches!(
            self.unit.types().get(ty).map(|ty| &ty.kind),
            Some(TypeKind::TargetBuiltin(
                evocc_types::TargetBuiltinType::ArmMfp8
            ))
        ) {
            return self.cast(value, CIntegerType::UCHAR, state);
        }
        if let Ok(integer) = canonical_integer(self.unit.types(), self.target, ty, span) {
            return self.cast(value, integer, state);
        }
        if let Some(kind) = floating::scalar_format(self.unit.types(), self.target, ty) {
            return self.cast_floating(value, kind, state);
        }
        match self.unit.types().get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Vector { .. }) => {
                if value.value.ty != self.c_value_type(ty, span)?.0 {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "incompatible vector assignment",
                    });
                }
                Ok(value)
            }
            Some(TypeKind::Pointer(_)) => {
                let expected = self.c_value_type(ty, span)?.0;
                if value.value.ty == expected {
                    return Ok(value);
                }
                if source
                    .is_some_and(|source| null_pointer_constant(self.unit, self.target, source))
                {
                    let pointer = self.fresh_value(expected);
                    self.emit(state, Inst::PtrNull(pointer.clone()))?;
                    return Ok(ExprValue {
                        value: pointer,
                        integer: CIntegerType::ULONG,
                    });
                }
                if matches!(
                    value.value.ty,
                    EvoIRType::Bits(_) | EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
                ) {
                    let address = self.cast(value, CIntegerType::ULONG, state)?;
                    let result = self.fresh_value(expected);
                    self.emit(
                        state,
                        Inst::PointerCast {
                            result: result.clone(),
                            operand: address.value,
                        },
                    )?;
                    return Ok(ExprValue {
                        value: result,
                        integer: CIntegerType::ULONG,
                    });
                }
                Err(LowerError::UnsupportedExpression {
                    span,
                    feature: "integer-to-pointer conversion is not a null pointer constant",
                })
            }
            Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)) => {
                let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                self.cast(value, integer, state)
            }
            _ => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "initializer/store destination is not scalar",
            }),
        }
    }

    /// One loop over the leaf objects, preserving their typed pointer zeros
    /// and representation padding without unrolling a runtime array extent.
    pub(super) fn emit_zero_dynamic_array(
        &mut self,
        address: Value,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let mut element = ty;
        while let Some(TypeKind::Array { element: inner, .. }) =
            self.unit.types().get(element).map(|ty| &ty.kind)
        {
            element = *inner;
        }
        let layout = self.access_layout(element, span)?;
        if layout.size_bytes == 0 {
            return Ok(());
        }
        let size = self.pointer_arithmetic_scale(ty, span, state)?;
        let cursor = self.retain_temporary(address, state);
        let remaining = self.retain_temporary(size.value, state);
        let live = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) | EnvKey::Address(symbol) => Some(*symbol),
                _ => None,
            })
            .collect();
        let keys = self.keys_for(state, &live, &[]);
        let header = self.new_block_from(state, &keys, BlockRole::Ordinary)?;
        self.branch(state, header.block, &keys)?;
        let zero = self.constant(&header, CIntegerType::ULONG, 0)?;
        let count = header.env[&remaining].value.clone();
        let predicate = self.checked_compare(&header, CmpOp::Ne, &count, &zero.value)?;
        let mut body = self.new_block_from(&header, &keys, BlockRole::Ordinary)?;
        let mut exit = self.new_block_from(&header, &keys, BlockRole::Ordinary)?;
        let arguments = self.edge_values(&header, &keys)?;
        self.terminate(
            &header,
            Inst::CondBr(
                predicate,
                body.block,
                arguments.clone(),
                exit.block,
                arguments,
            ),
        )?;
        let address = body.env[&cursor].value.clone();
        // Padding is part of the option's promise. Typed stores below attach
        // Null provenance where bytes alone would not represent a pointer.
        for offset in 0..layout.size_bytes {
            let at = self.constant_offset_address(address.clone(), offset, &mut body)?;
            let zero = self.constant(&body, CIntegerType::UCHAR, 0)?;
            self.emit_memory_store_with_layout(
                at,
                zero,
                AccessLayout::new(1, 1),
                false,
                &mut body,
            )?;
        }
        self.emit_zero_initializer_at(
            address.clone(),
            element,
            SubobjectPath::root(),
            &[],
            ZeroInitializer::PointerProvenance,
            span,
            &mut body,
            layout.align_bytes,
        )?;
        let next = self.constant_offset_address(address, layout.size_bytes, &mut body)?;
        body.env.get_mut(&cursor).unwrap().value = next;
        let step = self.constant(&body, CIntegerType::ULONG, layout.size_bytes)?;
        let count = body.env[&remaining].value.clone();
        let next = self.emit_raw_binary(&body, Inst::Sub, &count, &step.value)?;
        body.env.get_mut(&remaining).unwrap().value = next;
        self.branch(&body, header.block, &keys)?;
        exit.env.remove(&cursor);
        exit.env.remove(&remaining);
        *state = exit;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_zero_initializer_at(
        &mut self,
        address: Value,
        ty: TypeId,
        path: SubobjectPath,
        actions: &[InitAction<'a>],
        mode: ZeroInitializer,
        span: SourceSpan,
        state: &mut State,
        alignment: u32,
    ) -> Result<(), LowerError> {
        if actions
            .iter()
            .any(|action| path.steps().starts_with(action.path().steps()))
        {
            return Ok(());
        }
        let kind = self
            .unit
            .types()
            .get(ty)
            .ok_or(LowerError::UnknownType { ty, span })?
            .kind
            .clone();
        match kind {
            TypeKind::Float(
                evocc_types::FloatKind::F32
                | evocc_types::FloatKind::F64
                | evocc_types::FloatKind::F128,
            ) => {
                let mut layout = self.access_layout(ty, span)?;
                if mode == ZeroInitializer::PointerProvenance {
                    return Ok(());
                }
                layout.align_bytes = layout.align_bytes.min(alignment);
                let integer = if layout.size_bytes == 4 {
                    CIntegerType::UINT
                } else {
                    CIntegerType::ULONG
                };
                let zero = self.constant(state, integer, 0)?;
                self.emit_memory_store_with_layout(address, zero, layout, false, state)?;
                Ok(())
            }
            TypeKind::TargetBuiltin(kind) if kind.half_format().is_some() => {
                let mut layout = self.access_layout(ty, span)?;
                if mode == ZeroInitializer::PointerProvenance {
                    return Ok(());
                }
                layout.align_bytes = layout.align_bytes.min(alignment);
                let zero = self.constant(state, CIntegerType::USHORT, 0)?;
                self.emit_memory_store_with_layout(address, zero, layout, false, state)?;
                Ok(())
            }
            TypeKind::TargetBuiltin(evocc_types::TargetBuiltinType::ArmMfp8) => {
                let value = self.constant(state, CIntegerType::UCHAR, 0)?;
                self.emit_memory_store_with_layout(
                    address,
                    value,
                    self.access_layout(ty, span)?,
                    false,
                    state,
                )?;
                Ok(())
            }
            TypeKind::TargetBuiltin(kind) if kind.polynomial_bits().is_some() => {
                let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                let mut layout = self.access_layout(ty, span)?;
                if mode == ZeroInitializer::PointerProvenance {
                    return Ok(());
                }
                let zero = self.constant(state, integer, 0)?;
                layout.align_bytes = layout.align_bytes.min(alignment);
                self.emit_memory_store_with_layout(address, zero, layout, false, state)?;
                Ok(())
            }
            TypeKind::TargetBuiltin(_) => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "target header type has no admitted value execution",
            }),
            TypeKind::Complex(evocc_types::ComplexBase::Integer(_)) => {
                if mode == ZeroInitializer::PointerProvenance {
                    let element = evocc_chir::complex::element(self.unit.types(), ty).unwrap();
                    canonical_integer(self.unit.types(), self.target, element, span)?;
                    self.access_layout(element, span)?;
                    return Ok(());
                }
                self.emit_zero_complex(address, ty, span, state)
            }
            TypeKind::Complex(base) if base.executable_part().is_some() => {
                if mode == ZeroInitializer::PointerProvenance {
                    return Ok(());
                }
                self.emit_zero_complex(address, ty, span, state)
            }
            TypeKind::Vector { element, lanes, .. } => {
                let layout = self.access_layout(element, span)?;
                for index in 0..u64::from(lanes) {
                    let at = self.constant_offset_address(
                        address.clone(),
                        index * layout.size_bytes,
                        state,
                    )?;
                    self.emit_zero_initializer_at(
                        at,
                        element,
                        path.clone().with_index(index),
                        actions,
                        mode,
                        span,
                        state,
                        alignment.min(layout.align_bytes),
                    )?;
                }
                Ok(())
            }
            TypeKind::Int(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_) => {
                let mut layout = self.access_layout(ty, span)?;
                let zero = match kind {
                    TypeKind::Pointer(_) => {
                        let pointer = self.fresh_value(self.c_value_type(ty, span)?.0);
                        self.emit(state, Inst::PtrNull(pointer.clone()))?;
                        ExprValue {
                            value: pointer,
                            integer: CIntegerType::ULONG,
                        }
                    }
                    _ => {
                        let integer = canonical_integer(self.unit.types(), self.target, ty, span)?;
                        // Validate value admission before omitting the store:
                        // prezeroed bytes do not authorize deferred types.
                        if mode == ZeroInitializer::PointerProvenance {
                            return Ok(());
                        }
                        self.constant(state, integer, 0)?
                    }
                };
                layout.align_bytes = layout.align_bytes.min(alignment);
                self.emit_memory_store_with_layout(address, zero, layout, false, state)?;
                Ok(())
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
                            feature: "zero initializer element offset overflow",
                        },
                    )?;
                    let element_address =
                        self.constant_offset_address(address.clone(), offset, state)?;
                    self.emit_zero_initializer_at(
                        element_address,
                        element,
                        path.clone().with_index(index),
                        actions,
                        mode,
                        span,
                        state,
                        alignment.min(element_layout.align_bytes),
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(()),
            TypeKind::Array {
                bound: ArrayBound::Variable | ArrayBound::Captured(_),
                ..
            }
            | TypeKind::Void
            | TypeKind::Function { .. } => Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "object cannot be zero-initialized",
            }),
            TypeKind::BuiltinVaList => Ok(()),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let is_union = matches!(kind, TypeKind::Union(_));
                let definition = self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(LowerError::UnsupportedType {
                        ty,
                        span,
                        feature: "zero initializer record tag is absent",
                    })?;
                let fields = match &definition.state {
                    TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) => fields.clone(),
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "zero initializer record is incomplete",
                        });
                    }
                };
                let record_layout = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|source| LowerError::Layout { ty, span, source })?;
                let selected_union_field = is_union.then(|| {
                    actions.iter().find_map(|action| {
                        let suffix = action.path().steps().strip_prefix(path.steps())?;
                        match suffix.first()? {
                            SubobjectStep::Field { field_index } => Some(*field_index),
                            SubobjectStep::Index { .. } => None,
                        }
                    })
                });
                for (field_index, field) in fields.iter().enumerate() {
                    if selected_union_field
                        .flatten()
                        .is_some_and(|selected| selected != field_index)
                    {
                        continue;
                    }
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    let Some(layout) = record_layout
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                    else {
                        return Err(LowerError::UnsupportedType {
                            ty,
                            span,
                            feature: "zero initializer field has no layout",
                        });
                    };
                    if let Some(bit_field) = layout.bit_field {
                        if mode == ZeroInitializer::PointerProvenance {
                            continue;
                        }
                        // Only bytes intersecting the field are initialized;
                        // a declared allocation unit may extend past a packed record.
                        let count = (bit_field.bit_offset % 8 + bit_field.width_bits).div_ceil(8);
                        for byte in 0..count {
                            let at = self.constant_offset_address(
                                address.clone(),
                                layout.byte_offset + u64::from(byte),
                                state,
                            )?;
                            let zero = self.constant(state, CIntegerType::UCHAR, 0)?.value;
                            self.emit_byte_access(at, Some(zero), None, state)?;
                        }
                        continue;
                    }
                    let field_address =
                        self.constant_offset_address(address.clone(), layout.byte_offset, state)?;
                    self.emit_zero_initializer_at(
                        field_address,
                        field.ty,
                        path.clone().with_field(field_index),
                        actions,
                        mode,
                        span,
                        state,
                        alignment.min(layout.layout.align_bytes),
                    )?;
                    if is_union {
                        break;
                    }
                }
                Ok(())
            }
        }
    }

    pub(super) fn lower_memory_declaration(
        &mut self,
        object: &'a ObjectDecl<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        self.object_declarations.insert(object.symbol, object);
        if !state.env.contains_key(&EnvKey::Address(object.symbol)) {
            self.allocate_object_storage(object, state)?;
        }
        if object.storage.base() == StorageClass::Static {
            return Ok(());
        }
        if object.cleanup.is_some() && !state.cleanups.contains(&object.symbol) {
            state.cleanups.push(object.symbol);
        }
        let Some(initializer) = &object.initializer else {
            if self.unit.zero_initialize_automatic(object) {
                let address = self.token(state, EnvKey::Address(object.symbol))?.value;
                if self
                    .target
                    .array_size_factors(self.unit.types(), object.ty)
                    .is_ok_and(|(_, captures)| !captures.is_empty())
                {
                    return self.emit_zero_dynamic_array(address, object.ty, object.span, state);
                }
                let layout = self.access_layout(object.ty, object.span)?;
                // The implicit initializer includes padding. Typed stores then
                // attach Null provenance to pointer subobjects without changing
                // the target's all-zero representation.
                let mut offset = 0;
                while offset < layout.size_bytes {
                    let size = [8, 4, 2, 1]
                        .into_iter()
                        .find(|size| *size <= layout.size_bytes - offset)
                        .unwrap();
                    let integer = match size {
                        8 => CIntegerType::ULONG,
                        4 => CIntegerType::UINT,
                        2 => CIntegerType::USHORT,
                        _ => CIntegerType::UCHAR,
                    };
                    let at = self.constant_offset_address(address.clone(), offset, state)?;
                    let zero = self.constant(state, integer, 0)?;
                    self.emit_memory_store_with_layout(
                        at,
                        zero,
                        AccessLayout::new(size, layout.align_bytes.min(size as u32)),
                        false,
                        state,
                    )?;
                    offset += size;
                }
                self.emit_zero_initializer_at(
                    address,
                    object.ty,
                    SubobjectPath::root(),
                    &[],
                    ZeroInitializer::PointerProvenance,
                    object.span,
                    state,
                    layout.align_bytes,
                )?;
            }
            return Ok(());
        };
        let address = self.token(state, EnvKey::Address(object.symbol))?.value;
        if self.unit.zero_initialize_automatic_padding(object) {
            self.lower_constructor_padding(address.clone(), initializer, object.span, state)?;
        }
        self.lower_initializer_at(address, object.ty, initializer, object.span, state)
            .map(|_| ())
    }

    /// A scope activation owns storage; reaching the declaration only runs
    /// its initializer. Jump-entry edges therefore allocate without storing.
    pub(super) fn allocate_object_storage(
        &mut self,
        object: &'a ObjectDecl<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let dynamic = self
            .target
            .array_size_factors(self.unit.types(), object.ty)
            .is_ok_and(|(_, captures)| !captures.is_empty());
        let mut layout = if dynamic {
            AccessLayout::new(
                0,
                self.target
                    .alignof_type(self.unit.types(), object.ty)
                    .map_err(|source| LowerError::Layout {
                        ty: object.ty,
                        span: object.span,
                        source,
                    })?,
            )
        } else {
            self.access_layout(object.ty, object.span)?
        };
        layout.align_bytes = layout.align_bytes.max(
            self.unit
                .symbols()
                .symbol(object.symbol)
                .map_err(|_| LowerError::UnknownSymbol {
                    symbol: object.symbol,
                    span: object.span,
                })?
                .declared_alignment(),
        );
        let origin = *self.allocation_origins.get(&object.symbol).ok_or(
            LowerError::InvalidGeneratedSsa {
                message: format!(
                    "automatic object {} has no allocation origin",
                    object.symbol.raw()
                ),
            },
        )?;
        if object.storage.base() == StorageClass::Static {
            let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
            self.emit(
                state,
                Inst::GlobalAddr {
                    result: address.clone(),
                    symbol: format!("__evocc.static.{origin}"),
                },
            )?;
            state.env.insert(
                EnvKey::Address(object.symbol),
                BoundValue {
                    value: address,
                    integer: None,
                },
            );
            state.locals.insert(object.symbol);
            return Ok(());
        }
        let size = if dynamic {
            Some(
                self.pointer_arithmetic_scale(object.ty, object.span, state)?
                    .value,
            )
        } else {
            None
        };
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            if let Some(size) = size {
                Inst::DynamicAlloca {
                    result: address.clone(),
                    mem_in,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    origin,
                    size,
                    align: layout.align_bytes,
                }
            } else {
                Inst::Alloca {
                    result: address.clone(),
                    mem_in,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    origin,
                    layout,
                }
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        state.env.insert(
            EnvKey::Address(object.symbol),
            BoundValue {
                value: address.clone(),
                integer: None,
            },
        );
        state.locals.insert(object.symbol);
        Ok(())
    }

    /// Hold a converted range value in the SSA environment. A later value
    /// can branch or call, so a cached raw SSA name alone is not sufficient.
    pub(super) fn lower_shared_initializer_value(
        &mut self,
        value: &'a CHirExpr<'a>,
        ty: TypeId,
        shared: Option<usize>,
        values: &mut HashMap<usize, (EnvKey, CIntegerType)>,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let Some(shared) = shared else {
            return self.lower_expr(value, state);
        };
        if let Some((key, integer)) = values.get(&shared) {
            return Ok(ExprValue {
                value: self.token(state, *key)?.value,
                integer: *integer,
            });
        }
        let mut lowered = self.lower_expr(value, state)?;
        if self.is_aggregate_type(ty) || evocc_chir::complex::admitted(self.unit.types(), ty) {
            let source = self.retain_temporary(lowered.value, state);
            let origin = self.synthetic_origin()?;
            let snapshot = self.emit_alloca(ty, value.span(), origin, state)?;
            let address = self.token(state, source)?.value;
            self.emit_aggregate_transfer(
                snapshot.clone(),
                address,
                ty,
                CopyVolatility::new(self.aggregate_source_is_volatile(value), false),
                value.span(),
                state,
            )?;
            state.env.remove(&source);
            lowered.value = snapshot;
        } else {
            lowered = self.coerce_to_object_type(lowered, Some(value), ty, value.span(), state)?;
        }
        let key = self.retain_temporary(lowered.value.clone(), state);
        values.insert(shared, (key, lowered.integer));
        Ok(lowered)
    }

    pub(super) fn lower_initializer_at(
        &mut self,
        address: Value,
        object_ty: TypeId,
        initializer: &ObjectInitializer<'a>,
        object_span: SourceSpan,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let address_key = self.retain_temporary(address.clone(), state);
        if initializer.plan().zero_fills_unwritten() {
            self.emit_zero_initializer_at(
                address.clone(),
                object_ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                ZeroInitializer::Values,
                object_span,
                state,
                self.access_layout(object_ty, object_span)?.align_bytes,
            )?;
        }
        let mut shared_values = HashMap::new();
        for (index, action) in initializer.plan().actions().iter().enumerate() {
            let action_span = action.span().unwrap_or(object_span);
            let (offset, actual_ty, bit_field) = subobject_offset(
                self.unit,
                self.target,
                object_ty,
                action.path(),
                action_span,
            )?;
            if actual_ty != action.destination_type() {
                return Err(LowerError::UnsupportedStatement {
                    span: action_span,
                    feature: "initializer destination type does not match subobject path",
                });
            }
            let action_address = self.constant_offset_address(
                self.token(state, address_key)?.value,
                u64::try_from(offset).map_err(|_| LowerError::UnsupportedStatement {
                    span: action_span,
                    feature: "initializer has a negative object offset",
                })?,
                state,
            )?;
            // This is a verified initializer path into a known allocation,
            // so packing can lower the scalar's alignment at this offset.
            let mut action_layout = self.access_layout(actual_ty, action_span)?;
            action_layout.align_bytes = action_layout
                .align_bytes
                .min(self.access_layout(object_ty, object_span)?.align_bytes);
            if offset != 0 {
                action_layout.align_bytes = action_layout
                    .align_bytes
                    .min(1u32 << offset.unsigned_abs().trailing_zeros().min(31));
            }
            match action {
                InitAction::StoreScalar {
                    destination_type,
                    value,
                    ..
                } => {
                    if let Some(bits) = evocc_chir::constant::floating_constant_representation(
                        value,
                        *destination_type,
                        self.unit.types(),
                        self.unit.symbols(),
                        self.target,
                    )
                    .filter(|_| !self.unit.runtime_floating_environment())
                    {
                        let value = self.wide_constant(state, bits.ty(), bits.raw_bits())?;
                        let volatile = self
                            .unit
                            .types()
                            .get(*destination_type)
                            .is_some_and(|ty| ty.qualifiers.is_volatile);
                        self.emit_memory_store_with_layout(
                            action_address,
                            value,
                            action_layout,
                            volatile,
                            state,
                        )?;
                        continue;
                    }
                    let action_key = self.retain_temporary(action_address, state);
                    let lowered = self.lower_shared_initializer_value(
                        value,
                        *destination_type,
                        initializer.plan().shared_value(index),
                        &mut shared_values,
                        state,
                    )?;
                    let action_address = state
                        .env
                        .remove(&action_key)
                        .expect("retained initializer destination")
                        .value;
                    if evocc_chir::complex::admitted(self.unit.types(), *destination_type) {
                        self.emit_representation_copy(
                            action_address,
                            lowered.value,
                            *destination_type,
                            CopyVolatility::new(false, false),
                            action_span,
                            state,
                        )?;
                        continue;
                    }
                    let lowered = self.coerce_to_object_type(
                        lowered,
                        Some(value),
                        *destination_type,
                        action_span,
                        state,
                    )?;
                    let volatile = self
                        .unit
                        .types()
                        .get(*destination_type)
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    if let Some(field) = bit_field {
                        self.emit_bit_field_store(action_address, lowered, field, false, state)?;
                    } else {
                        self.emit_memory_store_with_layout(
                            action_address,
                            lowered,
                            action_layout,
                            volatile,
                            state,
                        )?;
                    }
                }
                InitAction::ZeroScalar {
                    destination_type, ..
                } => {
                    if matches!(
                        self.unit.types().get(*destination_type).map(|ty| &ty.kind),
                        Some(TypeKind::Float(
                            evocc_types::FloatKind::F32
                                | evocc_types::FloatKind::F64
                                | evocc_types::FloatKind::F128
                        ))
                    ) || floating::half_format(self.unit.types(), *destination_type).is_some()
                    {
                        self.emit_zero_initializer_at(
                            action_address,
                            *destination_type,
                            SubobjectPath::root(),
                            &[],
                            ZeroInitializer::Values,
                            action_span,
                            state,
                            action_layout.align_bytes,
                        )?;
                        continue;
                    }
                    if evocc_chir::complex::admitted(self.unit.types(), *destination_type) {
                        self.emit_zero_complex(
                            action_address,
                            *destination_type,
                            action_span,
                            state,
                        )?;
                        continue;
                    }
                    let zero = match self.unit.types().get(*destination_type).map(|ty| &ty.kind) {
                        Some(TypeKind::Pointer(_)) => {
                            let pointer = self
                                .fresh_value(self.c_value_type(*destination_type, action_span)?.0);
                            self.emit(state, Inst::PtrNull(pointer.clone()))?;
                            ExprValue {
                                value: pointer,
                                integer: CIntegerType::ULONG,
                            }
                        }
                        _ => {
                            let integer = canonical_integer(
                                self.unit.types(),
                                self.target,
                                *destination_type,
                                action_span,
                            )?;
                            self.constant(state, integer, 0)?
                        }
                    };
                    if let Some(field) = bit_field {
                        self.emit_bit_field_store(action_address, zero, field, false, state)?;
                    } else {
                        self.emit_memory_store_with_layout(
                            action_address,
                            zero,
                            action_layout,
                            false,
                            state,
                        )?;
                    }
                }
                InitAction::StoreString {
                    destination_type,
                    literal,
                    ..
                } => {
                    let (element, count) =
                        match self.unit.types().get(*destination_type).map(|ty| &ty.kind) {
                            Some(TypeKind::Array {
                                element,
                                bound: ArrayBound::Fixed(count),
                            }) => (*element, *count),
                            _ => {
                                return Err(LowerError::UnsupportedStatement {
                                    span: action_span,
                                    feature: "string initializer destination is not a fixed array",
                                });
                            }
                        };
                    let mut units =
                        literal
                            .decoded_units()
                            .ok_or(LowerError::UnsupportedStatement {
                                span: action_span,
                                feature: "string initializer decoding failed",
                            })?;
                    let count =
                        usize::try_from(count).map_err(|_| LowerError::UnsupportedType {
                            ty: *destination_type,
                            span: action_span,
                            feature: "string destination length exceeds host representation",
                        })?;
                    units.resize(count, 0);
                    let element_layout = self.access_layout(element, action_span)?;
                    for (index, unit) in units.into_iter().take(count).enumerate() {
                        let address = self.constant_offset_address(
                            action_address.clone(),
                            u64::try_from(index)
                                .ok()
                                .and_then(|index| index.checked_mul(element_layout.size_bytes))
                                .ok_or(LowerError::UnsupportedType {
                                    ty: element,
                                    span: action_span,
                                    feature: "string element offset overflow",
                                })?,
                            state,
                        )?;
                        let integer = canonical_integer(
                            self.unit.types(),
                            self.target,
                            element,
                            action_span,
                        )?;
                        let value = self.constant(state, integer, u64::from(unit))?;
                        self.emit_memory_store(address, value, element, false, action_span, state)?;
                    }
                }
                InitAction::CopyAggregate {
                    destination_type,
                    value,
                    span,
                    ..
                } => {
                    let action_key = self.retain_temporary(action_address, state);
                    let source = self.lower_shared_initializer_value(
                        value,
                        *destination_type,
                        initializer.plan().shared_value(index),
                        &mut shared_values,
                        state,
                    )?;
                    let action_address = state
                        .env
                        .remove(&action_key)
                        .expect("retained initializer destination")
                        .value;
                    let volatile_destination = self
                        .unit
                        .types()
                        .get(*destination_type)
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    if !self.is_aggregate_type(*destination_type)
                        && matches!(
                            self.unit.types().get(*destination_type).map(|ty| &ty.kind),
                            Some(TypeKind::Vector { .. })
                        )
                    {
                        let source = self.coerce_to_object_type(
                            source,
                            Some(value),
                            *destination_type,
                            *span,
                            state,
                        )?;
                        self.emit_memory_store(
                            action_address,
                            source,
                            *destination_type,
                            volatile_destination,
                            *span,
                            state,
                        )?;
                        continue;
                    }
                    self.emit_aggregate_transfer(
                        action_address,
                        source.value,
                        *destination_type,
                        CopyVolatility::new(
                            initializer.plan().shared_value(index).is_none()
                                && self.aggregate_source_is_volatile(value),
                            volatile_destination,
                        ),
                        *span,
                        state,
                    )?;
                }
            }
        }
        for (key, _) in shared_values.values() {
            state.env.remove(key);
        }
        Ok(state
            .env
            .remove(&address_key)
            .expect("retained initializer root")
            .value)
    }

    pub(super) fn lower_constructor_padding(
        &mut self,
        address: Value,
        initializer: &ObjectInitializer<'a>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let ranges = initializer
            .plan()
            .padding_ranges(self.unit.types(), *self.target)
            .map_err(|_| LowerError::UnsupportedType {
                ty: initializer.plan().target_type(),
                span,
                feature: "constructor padding has no finite layout",
            })?;
        for range in ranges {
            let mut offset = range.start;
            while offset < range.end {
                let size = [8, 4, 2, 1]
                    .into_iter()
                    .find(|size| *size <= range.end - offset)
                    .unwrap();
                let integer = match size {
                    8 => CIntegerType::ULONG,
                    4 => CIntegerType::UINT,
                    2 => CIntegerType::USHORT,
                    _ => CIntegerType::UCHAR,
                };
                let at = self.constant_offset_address(address.clone(), offset, state)?;
                let zero = self.constant(state, integer, 0)?;
                self.emit_memory_store_with_layout(
                    at,
                    zero,
                    AccessLayout::new(size, 1),
                    false,
                    state,
                )?;
                offset += size;
            }
        }
        Ok(())
    }
}
