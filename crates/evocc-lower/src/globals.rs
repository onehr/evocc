//! Lowering globals.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_globals<'a>(
    unit: &CHirUnit<'a>,
    target: &TargetDataLayout,
    function_statics: &[&'a ObjectDecl<'a>],
    memory_symbols: &BTreeSet<SymbolId>,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
    label_addresses: &BTreeMap<usize, String>,
) -> Result<Vec<GlobalObject>, LowerError> {
    let mut declarations = function_statics
        .iter()
        .map(|object| (object.symbol, *object))
        .collect::<BTreeMap<SymbolId, &ObjectDecl<'_>>>();
    for declaration in unit.iter() {
        match declaration {
            CHirDecl::Object(object)
                if memory_symbols.contains(&object.symbol)
                    && origins.contains_key(&object.symbol) =>
            {
                let canonical_type = unit
                    .symbols()
                    .symbol(object.symbol)
                    .ok()
                    .map(|symbol| symbol.ty());
                declarations
                    .entry(object.symbol)
                    .and_modify(|selected| {
                        if object.initializer.is_some()
                            || (selected.initializer.is_none()
                                && canonical_type == Some(object.ty)
                                && canonical_type != Some(selected.ty))
                        {
                            *selected = object;
                        }
                    })
                    .or_insert(object);
            }
            _ => {}
        }
    }
    declarations
        .into_values()
        .map(|object| lower_global_object(unit, target, object, origins, strings, label_addresses))
        .collect()
}

pub(super) fn lower_global_object(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    object: &ObjectDecl<'_>,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
    label_addresses: &BTreeMap<usize, String>,
) -> Result<GlobalObject, LowerError> {
    let linkage = unit.symbol_linkage().map_err(LowerError::Linkage)?;
    let symbol = unit
        .symbols()
        .symbol(object.symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: object.symbol,
            span: object.span,
        })?;
    let layout = target
        .layout_of(unit.types(), object.ty)
        .map_err(|source| LowerError::Layout {
            ty: object.ty,
            span: object.span,
            source,
        })?;
    let object_size = match &object.initializer {
        Some(initializer) => {
            initializer
                .plan()
                .storage_layout(unit.types(), target)
                .map_err(|source| LowerError::Layout {
                    ty: object.ty,
                    span: object.span,
                    source,
                })?
                .size_bytes
        }
        None => layout.size_bytes,
    };
    let size = usize::try_from(object_size).map_err(|_| LowerError::UnsupportedType {
        ty: object.ty,
        span: object.span,
        feature: "global object exceeds host representation",
    })?;
    let mut bytes = vec![0_u8; size];
    let mut relocs = Vec::new();
    if let Some(initializer) = &object.initializer {
        for action in initializer.plan().actions() {
            apply_static_initializer(
                unit,
                target,
                object.ty,
                action,
                &mut bytes,
                &mut relocs,
                origins,
                strings,
                label_addresses,
            )?;
        }
    }
    relocs.sort_by_key(|relocation: &evocc_evoir::Relocation| relocation.offset);
    let origin = *origins
        .get(&object.symbol)
        .ok_or(LowerError::InvalidGeneratedSsa {
            message: format!("global {} has no allocation origin", object.symbol.raw()),
        })?;
    let file_scope = unit
        .symbols()
        .is_bound_in_scope(object.symbol, unit.symbols().file_scope())
        .unwrap_or(false);
    let section = linkage.object_sections.get(&object.symbol).cloned();
    if section
        .as_deref()
        .is_some_and(|name| name == ".bss" || name.starts_with(".bss."))
        && (!relocs.is_empty() || bytes.iter().any(|byte| *byte != 0))
    {
        return Err(LowerError::Linkage(evocc_chir::linkage::LinkageError {
            code: "E2943",
            message: "invalid-or-conflicting-symbol-attribute",
            span: object.span,
        }));
    }
    Ok(GlobalObject {
        read_only: unit.types().is_const_qualified_object(object.ty),
        section,
        relocs,
        symbol: if file_scope {
            symbol.spelling().to_owned()
        } else {
            format!("__evocc.static.{origin}")
        },
        origin,
        kind: if file_scope {
            GlobalKind::File
        } else {
            GlobalKind::FunctionStatic
        },
        linkage: evocc_evoir::Linkage::Strong,
        visibility: if file_scope && object.storage.base() != StorageClass::Static {
            GlobalVisibility::External
        } else {
            GlobalVisibility::Internal
        },
        layout: AccessLayout::new(
            object_size,
            layout.align_bytes.max(symbol.declared_alignment()),
        ),
        bytes,
        initialized: vec![true; size],
    })
}

/// Resolve an admitted static address without inventing a runtime object.
pub(super) fn symbolic_address(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
    address_of: bool,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
    label_addresses: &BTreeMap<usize, String>,
) -> Result<(String, i64, bool), LowerError> {
    let pointer_type = |value| pointer_descriptor(unit, target, value);
    let span = expression.span();
    let error = || LowerError::StaticRelocation { span };
    let symbol_address = |id: SymbolId| -> Result<(String, i64, bool), LowerError> {
        let symbol = unit.symbols().symbol(id).map_err(|_| error())?;
        if symbol.kind() == SymbolKind::Function {
            Ok((
                unit.symbol_linkage()
                    .map_err(LowerError::Linkage)?
                    .function_name(id, symbol.spelling())
                    .to_owned(),
                0,
                false,
            ))
        } else if has_linked_object_storage(unit, id) {
            Ok((symbol.spelling().to_owned(), 0, false))
        } else if symbol.storage_class().map(StorageClass::base) == Some(StorageClass::Static) {
            Ok((
                format!("__evocc.static.{}", origins.get(&id).ok_or_else(error)?),
                0,
                false,
            ))
        } else {
            Err(error())
        }
    };
    match expression {
        CHirExpr::LabelAddress { .. } => label_addresses
            .get(&(expression as *const CHirExpr<'_> as usize))
            .cloned()
            .map(|symbol| (symbol, 0, false))
            .ok_or_else(error),
        CHirExpr::Cast(ty, operand, _) => {
            let canonical = unit.types().get(*ty).ok_or_else(error)?;
            if !matches!(canonical.kind, TypeKind::Pointer(_))
                && target
                    .layout_of(unit.types(), *ty)
                    .map_err(|_| error())?
                    .size_bytes
                    < 8
            {
                return Err(error());
            }
            let (symbol, offset, exposed) = symbolic_address(
                unit,
                target,
                operand,
                false,
                origins,
                strings,
                label_addresses,
            )?;
            Ok((
                symbol,
                offset,
                exposed || !matches!(canonical.kind, TypeKind::Pointer(_)),
            ))
        }
        CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => symbolic_address(
            unit,
            target,
            operand,
            true,
            origins,
            strings,
            label_addresses,
        ),
        CHirExpr::Unary(UnaryOp::Deref, operand, _) if address_of => symbolic_address(
            unit,
            target,
            operand,
            false,
            origins,
            strings,
            label_addresses,
        ),
        CHirExpr::Ident { symbol, facts, .. }
            if address_of
                || unit.types().get(facts.ty()).is_some_and(|ty| {
                    matches!(ty.kind, TypeKind::Array { .. } | TypeKind::Function { .. })
                }) =>
        {
            symbol_address(*symbol)
        }
        CHirExpr::StringLit(..) => {
            let origin = strings
                .get(&(expression as *const CHirExpr<'_> as usize))
                .ok_or_else(error)?;
            Ok((format!("__evocc.string.{origin}"), 0, false))
        }
        CHirExpr::CompoundLit(..) => {
            let origin = strings
                .get(&(expression as *const CHirExpr<'_> as usize))
                .ok_or_else(error)?;
            Ok((format!("__evocc.compound.{origin}"), 0, false))
        }
        CHirExpr::Member(base, name, _) | CHirExpr::PtrMember(base, name, _) => {
            let direct = matches!(expression, CHirExpr::Member(..));
            let addressed = CHirExpr::Unary(UnaryOp::AddrOf, base, span);
            let record = pointer_type(if direct { &addressed } else { base })
                .map_err(|_| error())?
                .pointee();
            let path =
                evocc_chir::record_member_path(unit.types(), record, name).ok_or_else(error)?;
            let member = path.last().ok_or_else(error)?.field_ty();
            if !address_of
                && !unit
                    .types()
                    .get(member)
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Array { .. }))
            {
                return Err(error());
            }
            let (symbol, mut offset, exposed) = symbolic_address(
                unit,
                target,
                base,
                direct,
                origins,
                strings,
                label_addresses,
            )?;
            for field in path {
                let layout = target
                    .record_layout(unit.types(), field.tag())
                    .map_err(|_| error())?;
                let delta = layout
                    .fields
                    .iter()
                    .find(|entry| entry.field_index == field.field_index())
                    .ok_or_else(error)?
                    .byte_offset;
                offset = offset
                    .checked_add(i64::try_from(delta).map_err(|_| error())?)
                    .ok_or_else(error)?;
            }
            Ok((symbol, offset, exposed))
        }
        CHirExpr::MemoryRead { place, .. }
            if address_of
                || unit
                    .types()
                    .get(place.value_ty())
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Array { .. })) =>
        {
            let (symbol, mut offset, exposed) = match place.root() {
                PlaceRoot::Object { symbol } => symbol_address(*symbol)?,
                PlaceRoot::PointerExpression { pointer, .. } => symbolic_address(
                    unit,
                    target,
                    pointer,
                    false,
                    origins,
                    strings,
                    label_addresses,
                )?,
                PlaceRoot::CompoundLiteral { expression, .. } => symbolic_address(
                    unit,
                    target,
                    expression,
                    true,
                    origins,
                    strings,
                    label_addresses,
                )?,
            };
            for projection in place.projections() {
                let delta = match projection {
                    PlaceProjection::Field(field) => {
                        let layout = target
                            .record_layout(unit.types(), field.tag())
                            .map_err(|_| error())?;
                        i64::try_from(
                            layout
                                .fields
                                .iter()
                                .find(|entry| entry.field_index == field.field_index())
                                .ok_or_else(error)?
                                .byte_offset,
                        )
                        .map_err(|_| error())?
                    }
                    PlaceProjection::Index(index) => {
                        let value = evocc_chir::interp::interpret_static_integer_expression(
                            unit,
                            index.index(),
                            target,
                        )
                        .map_err(|_| error())?;
                        let signed = value.cast(CIntegerType::LONG).raw_bits() as i64;
                        let size = target
                            .layout_of(unit.types(), index.element_ty())
                            .map_err(|_| error())?
                            .size_bytes;
                        signed
                            .checked_mul(i64::try_from(size).map_err(|_| error())?)
                            .ok_or_else(error)?
                    }
                };
                offset = offset.checked_add(delta).ok_or_else(error)?;
            }
            Ok((symbol, offset, exposed))
        }
        CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), pointer, index, _) => {
            // Addition permits the constant addend on either side. Only
            // commute a proven integer constant; two symbols still cannot be
            // represented by one relocation, and subtraction is not symmetric.
            let (pointer, index) = if *op == BinaryOp::Add
                && evocc_chir::interp::interpret_static_integer_expression(unit, pointer, target)
                    .is_ok()
            {
                (*index, *pointer)
            } else {
                (*pointer, *index)
            };
            let (symbol, offset, exposed) = symbolic_address(
                unit,
                target,
                pointer,
                false,
                origins,
                strings,
                label_addresses,
            )?;
            let value =
                evocc_chir::interp::interpret_static_integer_expression(unit, index, target)
                    .map_err(|_| error())?;
            let scale = pointer_type(pointer)
                .ok()
                .and_then(|ty| target.layout_of(unit.types(), ty.pointee()).ok())
                .map_or(1, |layout| layout.size_bytes);
            let delta = (value.cast(CIntegerType::LONG).raw_bits() as i64)
                .checked_mul(i64::try_from(scale).map_err(|_| error())?)
                .ok_or_else(error)?;
            let offset = if *op == BinaryOp::Add {
                offset.checked_add(delta)
            } else {
                offset.checked_sub(delta)
            }
            .ok_or_else(error)?;
            Ok((symbol, offset, exposed))
        }
        CHirExpr::Ternary(condition, left, right, _) => {
            let condition = if let Ok(value) =
                evocc_chir::interp::interpret_static_integer_expression(unit, condition, target)
            {
                value.raw_bits() != 0
            } else if let CHirExpr::Binary(op @ (BinaryOp::Eq | BinaryOp::Ne), a, b, _) = condition
            {
                let a = if static_null_pointer(unit, target, a) {
                    None
                } else {
                    Some(symbolic_address(
                        unit,
                        target,
                        a,
                        false,
                        origins,
                        strings,
                        label_addresses,
                    )?)
                };
                let b = if static_null_pointer(unit, target, b) {
                    None
                } else {
                    Some(symbolic_address(
                        unit,
                        target,
                        b,
                        false,
                        origins,
                        strings,
                        label_addresses,
                    )?)
                };
                // Exposure records a conversion, not part of address identity.
                (a.as_ref().map(|(symbol, offset, _)| (symbol, offset))
                    == b.as_ref().map(|(symbol, offset, _)| (symbol, offset)))
                    == (*op == BinaryOp::Eq)
            } else {
                symbolic_address(
                    unit,
                    target,
                    condition,
                    false,
                    origins,
                    strings,
                    label_addresses,
                )?;
                true
            };
            symbolic_address(
                unit,
                target,
                if condition { left } else { right },
                address_of,
                origins,
                strings,
                label_addresses,
            )
        }
        _ => Err(error()),
    }
}

pub(super) fn initializer_destination_layout(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    action: &InitAction<'_>,
    span: SourceSpan,
) -> Result<TypeLayout, LowerError> {
    let ty = action.destination_type();
    if let (
        InitAction::StoreString { literal, .. },
        Some(TypeKind::Array {
            element,
            bound: ArrayBound::Unbounded,
        }),
    ) = (
        action,
        unit.types().get(ty).map(|canonical| &canonical.kind),
    ) {
        let element_layout =
            target
                .layout_of(unit.types(), *element)
                .map_err(|source| LowerError::Layout {
                    ty: *element,
                    span,
                    source,
                })?;
        let size_bytes = literal
            .initializer_element_count(&ArrayBound::Unbounded)
            .and_then(|count| count.checked_mul(element_layout.size_bytes))
            .ok_or(LowerError::UnsupportedStatement {
                span,
                feature: "invalid string initializer extent",
            })?;
        return Ok(TypeLayout::new(size_bytes, element_layout.align_bytes));
    }
    target
        .layout_of(unit.types(), ty)
        .map_err(|source| LowerError::Layout { ty, span, source })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_static_initializer(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    object_ty: TypeId,
    action: &InitAction<'_>,
    bytes: &mut [u8],
    relocs: &mut Vec<evocc_evoir::Relocation>,
    origins: &BTreeMap<SymbolId, u64>,
    strings: &BTreeMap<usize, u64>,
    label_addresses: &BTreeMap<usize, String>,
) -> Result<(), LowerError> {
    let span = action
        .span()
        .unwrap_or_else(|| SourceSpan::new(evocc_common::FileId::new(0), 0, 0, 1, 1));
    let (offset, actual_ty, bit_field) =
        subobject_offset(unit, target, object_ty, action.path(), span)?;
    if actual_ty != action.destination_type() {
        return Err(LowerError::UnsupportedStatement {
            span,
            feature: "static initializer path disagrees with its object",
        });
    }
    let offset = usize::try_from(offset).map_err(|_| LowerError::UnsupportedStatement {
        span,
        feature: "negative static initializer offset",
    })?;
    let destination = action.destination_type();
    if let Some(field) = bit_field {
        let raw = match action {
            InitAction::ZeroScalar { .. } => 0,
            InitAction::StoreScalar { value, .. } => {
                evocc_chir::interp::interpret_static_integer_expression(unit, value, target)
                    .map_err(|_| LowerError::UnsupportedExpression {
                        span,
                        feature: "bit-field initializer is not constant",
                    })?
                    .raw_bits()
            }
            _ => {
                return Err(LowerError::UnsupportedStatement {
                    span,
                    feature: "non-scalar bit-field initializer",
                })
            }
        };
        for bit in 0..field.width_bits {
            let position = usize::from(field.bit_offset % 8 + bit);
            let byte =
                bytes
                    .get_mut(offset + position / 8)
                    .ok_or(LowerError::UnsupportedStatement {
                        span,
                        feature: "bit-field initializer outside object",
                    })?;
            let mask = 1u8 << (position % 8);
            *byte = (*byte & !mask) | (u8::from(raw & (1u128 << bit) != 0) * mask);
        }
        return Ok(());
    }
    let layout = initializer_destination_layout(unit, target, action, span)?;
    let end = offset
        .checked_add(usize::try_from(layout.size_bytes).map_err(|_| {
            LowerError::UnsupportedType {
                ty: destination,
                span,
                feature: "initializer destination exceeds host representation",
            }
        })?)
        .ok_or(LowerError::UnsupportedStatement {
            span,
            feature: "initializer range overflow",
        })?;
    let target_bytes = bytes
        .get_mut(offset..end)
        .ok_or(LowerError::UnsupportedStatement {
            span,
            feature: "initializer range lies outside global object",
        })?;
    relocs
        .retain(|relocation| relocation.offset < offset as u64 || relocation.offset >= end as u64);
    match action {
        InitAction::ZeroScalar { .. } => target_bytes.fill(0),
        InitAction::StoreString { literal, .. } => {
            let TypeKind::Array { element, ref bound } = unit
                .types()
                .get(destination)
                .ok_or(LowerError::UnknownType {
                    ty: destination,
                    span,
                })?
                .kind
            else {
                return Err(LowerError::UnsupportedType {
                    ty: destination,
                    span,
                    feature: "string initializer destination is not a character array",
                });
            };
            let count = literal.initializer_element_count(bound).ok_or(
                LowerError::UnsupportedStatement {
                    span,
                    feature: "invalid string initializer extent",
                },
            )?;
            let width = usize::try_from(
                target
                    .layout_of(unit.types(), element)
                    .map_err(|source| LowerError::Layout {
                        ty: element,
                        span,
                        source,
                    })?
                    .size_bytes,
            )
            .map_err(|_| LowerError::UnsupportedType {
                ty: element,
                span,
                feature: "string code unit exceeds host representation",
            })?;
            let count = usize::try_from(count).map_err(|_| LowerError::UnsupportedType {
                ty: destination,
                span,
                feature: "string destination length exceeds host representation",
            })?;
            let mut decoded = literal
                .decoded_units()
                .ok_or(LowerError::UnsupportedStatement {
                    span,
                    feature: "invalid string initializer",
                })?;
            decoded.resize(count, 0);
            if width == 0 || width > 4 || target_bytes.len() != count.saturating_mul(width) {
                return Err(LowerError::UnsupportedType {
                    ty: destination,
                    span,
                    feature: "string code unit layout is unsupported",
                });
            }
            for (unit, bytes) in decoded
                .into_iter()
                .zip(target_bytes.chunks_exact_mut(width))
            {
                let encoded = match target.byte_order() {
                    evocc_types::ByteOrder::Little => unit.to_le_bytes(),
                    evocc_types::ByteOrder::Big => unit.to_be_bytes(),
                };
                let source = match target.byte_order() {
                    evocc_types::ByteOrder::Little => &encoded[..width],
                    evocc_types::ByteOrder::Big => &encoded[4 - width..],
                };
                bytes.copy_from_slice(source);
            }
        }
        InitAction::StoreScalar { value, .. } => {
            if let Some(bits) = evocc_chir::constant::floating_constant_representation(
                value,
                destination,
                unit.types(),
                unit.symbols(),
                target,
            ) {
                write_static_integer_bytes(bits.raw_bits(), target, target_bytes);
                return Ok(());
            }
            if evocc_chir::complex::floating(unit.types(), destination).is_some() {
                let parts = evocc_chir::constant::floating_complex_constant(
                    value,
                    unit.types(),
                    unit.symbols(),
                    target,
                )
                .map_err(|_| LowerError::StaticRelocation { span })?;
                let width = target_bytes.len() / 2;
                for (bytes, part) in target_bytes.chunks_mut(width).zip(parts) {
                    write_static_integer_bytes(part.bits(), target, bytes);
                }
                return Ok(());
            }
            if let Some(element) = evocc_chir::complex::element(unit.types(), destination) {
                let pair = evocc_chir::complex::constant_components(
                    value,
                    unit.types(),
                    unit.symbols(),
                    target,
                )
                .ok_or(LowerError::StaticRelocation { span })?;
                let integer = canonical_integer(unit.types(), target, element, span)?;
                let width = target_bytes.len() / 2;
                for (bytes, value) in target_bytes.chunks_mut(width).zip(pair) {
                    write_static_integer_bytes(value.cast(integer).raw_bits(), target, bytes);
                }
                return Ok(());
            }
            if let Some((left, right)) =
                evocc_chir::constant::label_address_difference(value, unit.types())
            {
                if !matches!(target_bytes.len(), 1 | 2 | 4 | 8) {
                    return Err(LowerError::StaticRelocation { span });
                }
                // A difference relocation truncates only at its destination.
                // A narrower intermediate conversion followed by extension
                // cannot be replaced by a wider raw label difference.
                let mut converted = *value;
                while let CHirExpr::Cast(ty, operand, _) = converted {
                    let integer = canonical_integer(unit.types(), target, *ty, span)?;
                    if usize::from(integer.bits()) < target_bytes.len() * 8 {
                        return Err(LowerError::StaticRelocation { span });
                    }
                    converted = operand;
                }
                let label = |expression: &CHirExpr<'_>| {
                    label_addresses
                        .get(&(expression as *const CHirExpr<'_> as usize))
                        .cloned()
                        .ok_or(LowerError::StaticRelocation { span })
                };
                let symbol = label(left)?;
                let base = label(right)?;
                if symbol.split_once('^').map(|(owner, _)| owner)
                    != base.split_once('^').map(|(owner, _)| owner)
                {
                    return Err(LowerError::StaticRelocation { span });
                }
                target_bytes.fill(0);
                relocs.push(evocc_evoir::Relocation {
                    offset: offset as u64,
                    symbol,
                    addend: 0,
                    exposed: false,
                    difference: Some(evocc_evoir::BlockDifference {
                        base,
                        bits: (target_bytes.len() * 8) as u16,
                    }),
                });
                return Ok(());
            }
            // Absolute integer addresses have target bits but no symbol or
            // allocation provenance. Symbolic addresses still require relocs.
            if matches!(
                unit.types().get(destination).map(|ty| &ty.kind),
                Some(TypeKind::Pointer(_))
            ) {
                if let Some(integer) = absolute_static_integer(unit, target, value) {
                    if target_bytes.len() != target.pointer_layout().size_bytes as usize {
                        return Err(LowerError::StaticRelocation { span });
                    }
                    write_static_integer_bytes(
                        integer.cast(CIntegerType::ULONG).raw_bits(),
                        target,
                        target_bytes,
                    );
                    return Ok(());
                }
                if static_null_pointer(unit, target, value) {
                    target_bytes.fill(0);
                    return Ok(());
                }
                let (symbol, addend, exposed) = symbolic_address(
                    unit,
                    target,
                    value,
                    false,
                    origins,
                    strings,
                    label_addresses,
                )?;
                if target_bytes.len() != 8 {
                    return Err(LowerError::StaticRelocation { span });
                }
                target_bytes.fill(0);
                relocs.push(evocc_evoir::Relocation {
                    offset: offset as u64,
                    symbol,
                    addend,
                    exposed,
                    difference: None,
                });
                return Ok(());
            }
            let integer = match evocc_chir::interp::interpret_static_integer_expression(
                unit, value, target,
            ) {
                Ok(integer) => integer,
                Err(_) if target_bytes.len() == 8 => {
                    // A pointer-width integer stores the same link-time
                    // address bits as pointer storage. The destination's C
                    // type does not change the relocation's representation.
                    let (symbol, addend, _) = symbolic_address(
                        unit,
                        target,
                        value,
                        false,
                        origins,
                        strings,
                        label_addresses,
                    )?;
                    target_bytes.fill(0);
                    relocs.push(evocc_evoir::Relocation {
                        offset: offset as u64,
                        symbol,
                        addend,
                        exposed: true,
                        difference: None,
                    });
                    return Ok(());
                }
                Err(_) => {
                    return Err(LowerError::UnsupportedExpression {
                        span,
                        feature: "global initializer is not an admitted integer constant",
                    });
                }
            };
            let destination_integer = canonical_integer(unit.types(), target, destination, span)?;
            let raw = integer.cast(destination_integer).raw_bits();
            write_static_integer_bytes(raw, target, target_bytes);
        }
        InitAction::CopyAggregate { value, span, .. } => {
            let CHirExpr::CompoundLit(ty, initializer, _) = value else {
                return Err(LowerError::StaticRelocation { span: *span });
            };
            let mut copied_relocs = Vec::new();
            target_bytes.fill(0);
            for action in initializer.plan().actions() {
                apply_static_initializer(
                    unit,
                    target,
                    *ty,
                    action,
                    target_bytes,
                    &mut copied_relocs,
                    origins,
                    strings,
                    label_addresses,
                )?;
            }
            for mut relocation in copied_relocs {
                relocation.offset += offset as u64;
                relocs.push(relocation);
            }
        }
    }
    Ok(())
}

pub(super) fn write_static_integer_bytes(raw: u128, target: &TargetDataLayout, bytes: &mut [u8]) {
    let byte_len = bytes.len();
    for (index, byte) in bytes.iter_mut().enumerate() {
        let shift = match target.byte_order() {
            evocc_types::ByteOrder::Little => index * 8,
            evocc_types::ByteOrder::Big => (byte_len - index - 1) * 8,
        };
        *byte = raw
            .checked_shr(u32::try_from(shift).unwrap_or(u32::MAX))
            .unwrap_or(0) as u8;
    }
}

// Recognize numeric address constants without erasing intermediate integer
// casts. An object/function symbol deliberately cannot enter this path.
pub(super) fn absolute_static_integer(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> Option<CInteger> {
    evocc_chir::constant::eval_absolute_address_constant(
        expression,
        unit.types(),
        unit.symbols(),
        target,
    )
}

pub(super) fn subobject_offset(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    object_ty: TypeId,
    path: &SubobjectPath,
    span: SourceSpan,
) -> Result<(i64, TypeId, Option<BitFieldLayout>), LowerError> {
    let mut current = object_ty;
    let mut bit_field = None;
    let mut offset = 0_i64;
    for step in path.steps() {
        match step {
            SubobjectStep::Field { field_index } => {
                let tag = match unit.types().get(current).map(|ty| &ty.kind) {
                    Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty: current,
                            span,
                            feature: "initializer field path crosses a non-record",
                        });
                    }
                };
                let layout = target.record_layout(unit.types(), tag).map_err(|source| {
                    LowerError::Layout {
                        ty: current,
                        span,
                        source,
                    }
                })?;
                let field = layout
                    .fields
                    .iter()
                    .find(|field| field.field_index == *field_index)
                    .ok_or(LowerError::UnsupportedType {
                        ty: current,
                        span,
                        feature: "initializer field path has no target field",
                    })?;
                bit_field = field.bit_field;
                offset = offset
                    .checked_add(i64::try_from(field.byte_offset).map_err(|_| {
                        LowerError::UnsupportedType {
                            ty: current,
                            span,
                            feature: "record field offset exceeds i64",
                        }
                    })?)
                    .ok_or(LowerError::UnsupportedType {
                        ty: current,
                        span,
                        feature: "record field offset overflow",
                    })?;
                current = unit
                    .types()
                    .tag(tag)
                    .and_then(|definition| match &definition.state {
                        TagState::Complete(
                            TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                        ) => fields.get(*field_index).map(|field| field.ty),
                        _ => None,
                    })
                    .ok_or(LowerError::UnsupportedType {
                        ty: current,
                        span,
                        feature: "initializer field type is unavailable",
                    })?;
            }
            SubobjectStep::Index { index } => {
                let element = match unit.types().get(current).map(|ty| &ty.kind) {
                    Some(TypeKind::Array { element, .. } | TypeKind::Vector { element, .. }) => {
                        *element
                    }
                    _ => {
                        return Err(LowerError::UnsupportedType {
                            ty: current,
                            span,
                            feature: "initializer index path crosses a non-array",
                        });
                    }
                };
                let layout = target.layout_of(unit.types(), element).map_err(|source| {
                    LowerError::Layout {
                        ty: element,
                        span,
                        source,
                    }
                })?;
                let delta = index
                    .checked_mul(layout.size_bytes)
                    .and_then(|value| i64::try_from(value).ok())
                    .ok_or(LowerError::UnsupportedType {
                        ty: element,
                        span,
                        feature: "initializer array offset overflow",
                    })?;
                offset = offset
                    .checked_add(delta)
                    .ok_or(LowerError::UnsupportedType {
                        ty: element,
                        span,
                        feature: "initializer array offset overflow",
                    })?;
                current = element;
            }
        }
    }
    Ok((offset, current, bit_field))
}

fn pointer_descriptor(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> Result<evocc_chir::PointerTypeDesc, evocc_chir::place::MemoryPlaceError> {
    evocc_chir::PointerTypeDesc::from_expression_with_target(
        unit.symbols(),
        unit.types(),
        expression,
        target,
    )
}
