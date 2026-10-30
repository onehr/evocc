//! Lowering signatures.

use super::*;

pub(super) fn index_functions(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    memory_participant: bool,
    lifecycle: &[evocc_chir::lifecycle::Registration],
) -> Result<BTreeMap<SymbolId, FunctionInfo>, LowerError> {
    let linkage = unit.symbol_linkage().map_err(LowerError::Linkage)?;
    let mut required = BTreeSet::new();
    for declaration in unit.iter() {
        let mut references = FunctionReferences::default();
        collect_references_decl(declaration, &mut references);
        if let CHirDecl::Function(function) = declaration {
            if let Some(body) = function.body {
                required.insert(function.symbol);
                collect_references_stmt(body, &mut references);
            }
        }
        required.extend(references.direct);
    }
    let mut definitions = BTreeMap::new();
    required.retain(|symbol| {
        !unit.symbols().is_builtin_function(*symbol)
            || unit
                .symbols()
                .symbol(*symbol)
                .is_ok_and(|entry| evocc_types::neon::signature(entry.spelling()).is_none())
    });
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        if !required.contains(&function.symbol) {
            continue;
        }
        let info = function_info(unit, target, function)?;
        definitions
            .entry(function.symbol)
            .and_modify(|existing: &mut FunctionInfo| {
                if function.body.is_some() {
                    *existing = info.clone();
                }
            })
            .or_insert(info);
    }

    // A used function may have its only declaration inside a block. Its
    // canonical linkage symbol is still the unit-level callable authority.
    for symbol in required {
        if definitions.contains_key(&symbol) {
            continue;
        }
        let entry = unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| LowerError::InvalidGeneratedSsa {
                message: "required function symbol is absent".into(),
            })?;
        let Some(TypeKind::Function {
            return_type,
            parameters,
        }) = unit.types().get(entry.ty()).map(|ty| &ty.kind)
        else {
            return Err(LowerError::UnknownSymbol {
                symbol,
                span: entry.span(),
            });
        };
        if entry.kind() != SymbolKind::Function {
            return Err(LowerError::UnknownSymbol {
                symbol,
                span: entry.span(),
            });
        }
        let (types, variadic) = match parameters {
            FunctionParameters::Prototype { types, variadic } => (types.as_slice(), *variadic),
            FunctionParameters::Unspecified => {
                (unit.unprototyped_parameters(symbol).unwrap_or(&[]), false)
            }
        };
        let return_type = if unit
            .types()
            .get(*return_type)
            .is_some_and(|ty| matches!(ty.kind, TypeKind::Void))
        {
            None
        } else {
            Some(function_result_type(
                unit.types(),
                target,
                *return_type,
                entry.span(),
            )?)
        };
        definitions.insert(
            symbol,
            FunctionInfo {
                section: None,
                patchable_entry: None,
                name: entry.spelling().into(),
                variadic,
                ordinal: 0,
                parameter_symbols: Vec::new(),
                parameter_types: types
                    .iter()
                    .map(|ty| function_value_type(unit.types(), target, *ty, entry.span()))
                    .collect::<Result<Vec<_>, _>>()?,
                return_type,
                constant_result: None,
                participant: variadic,
                returns_twice: false,
                span: entry.span(),
            },
        );
    }
    let mut participants = BTreeSet::new();
    // Load-instance callbacks are calls too, even if their bodies are empty
    // and contain no source call expression. Their interface transports the
    // current program state rather than inventing an effect-free entry ABI.
    participants.extend(lifecycle.iter().map(|registration| registration.function));
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        let Some(body) = function.body else {
            continue;
        };
        let mut calls = FunctionReferences::default();
        collect_references_stmt(body, &mut calls);
        if calls.any {
            participants.insert(function.symbol);
        }
        participants.extend(calls.direct);
    }
    participants.extend(
        participants
            .iter()
            .map(|symbol| linkage.target(*symbol))
            .collect::<Vec<_>>(),
    );
    for symbol in participants {
        if unit.symbols().is_builtin_function(symbol)
            && unit
                .symbols()
                .symbol(symbol)
                .is_ok_and(|entry| evocc_types::neon::signature(entry.spelling()).is_some())
        {
            continue;
        }
        let Some(info) = definitions.get_mut(&symbol) else {
            let span = unit
                .symbols()
                .symbol(symbol)
                .map(|entry| entry.span())
                .unwrap_or_else(|_| SourceSpan::new(evocc_common::FileId::new(0), 0, 0, 0, 0));
            return Err(LowerError::UnknownSymbol { symbol, span });
        };
        info.participant = true;
    }
    if memory_participant {
        for info in definitions.values_mut() {
            info.participant = true;
        }
    }
    // A second return can deliver another value, so a returns-twice result
    // is never a known constant (EvoIR 3.10).
    let returns_twice = evocc_chir::returns_twice::returns_twice_functions(unit);
    for (ordinal, (symbol, info)) in definitions.iter_mut().enumerate() {
        if returns_twice.contains(symbol) {
            info.returns_twice = true;
            info.constant_result = None;
        }
        info.section = linkage.function_sections.get(symbol).cloned();
        info.patchable_entry = linkage.patchable_entries.get(symbol).copied();
        info.name = linkage.function_name(*symbol, &info.name).to_owned();
        info.ordinal = u32::try_from(ordinal).map_err(|_| LowerError::InvalidGeneratedSsa {
            message: "function count exceeds the aggregate temporary origin namespace".to_owned(),
        })?;
    }
    Ok(definitions)
}

pub(super) fn function_info(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    function: &FunctionDecl<'_>,
) -> Result<FunctionInfo, LowerError> {
    let symbol = unit
        .symbols()
        .symbol(function.symbol)
        .map_err(|_| LowerError::UnknownSymbol {
            symbol: function.symbol,
            span: function.span,
        })?;
    if symbol.kind() != SymbolKind::Function
        || symbol.spelling() != function.name.spelling
        || !unit.types().compatible(symbol.ty(), function.ty)
        || !symbol
            .storage_class()
            .is_some_and(|storage| function_storage_compatible(storage, function.storage))
        || !written_declarator_matches(
            unit.types(),
            &function.written,
            function.ty,
            function.return_specifier.type_id(),
            Some(function),
            false,
            false,
            (target, &function.attributes),
        )
    {
        return Err(LowerError::MalformedFunction {
            function: function.name.spelling.clone(),
            span: function.span,
            reason: "function declaration disagrees with its semantic identity",
        });
    }
    let canonical = unit
        .types()
        .get(symbol.ty())
        .ok_or(LowerError::UnknownType {
            ty: symbol.ty(),
            span: function.span,
        })?;
    let TypeKind::Function {
        return_type,
        parameters,
    } = &canonical.kind
    else {
        return Err(LowerError::MalformedFunction {
            function: function.name.spelling.clone(),
            span: function.span,
            reason: "declaration does not have a canonical function type",
        });
    };
    let return_type = match &unit
        .types()
        .get(*return_type)
        .ok_or(LowerError::UnknownType {
            ty: *return_type,
            span: function.span,
        })?
        .kind
    {
        TypeKind::Void => None,
        _ => Some(function_result_type(
            unit.types(),
            target,
            *return_type,
            function.span,
        )?),
    };
    let types: &[TypeId] = match parameters {
        FunctionParameters::Prototype { types, .. } => types,
        FunctionParameters::Unspecified if function.body.is_none() => {
            unit.unprototyped_parameters(function.symbol).unwrap_or(&[])
        }
        FunctionParameters::Unspecified
            if matches!(function.parameters, ParameterSyntax::Unspecified) =>
        {
            &[]
        }
        FunctionParameters::Unspecified => {
            return Err(LowerError::MalformedFunction {
                function: function.name.spelling.clone(),
                span: function.span,
                reason: "function definition requires a prototype parameter list",
            });
        }
    };
    let parameter_types = types
        .iter()
        .map(|ty| function_value_type(unit.types(), target, *ty, function.span))
        .collect::<Result<Vec<_>, _>>()?;
    let parameter_symbols = match &function.parameters {
        ParameterSyntax::Unspecified if parameter_types.is_empty() => Vec::new(),
        ParameterSyntax::Prototype { parameters, .. }
        | ParameterSyntax::IdentifierList { parameters }
            if function.body.is_some() && parameters.len() == parameter_types.len() =>
        {
            let mut ids = BTreeSet::new();
            let mut spellings = BTreeSet::new();
            let mut function_scope = None;
            parameters
                .iter()
                .zip(types)
                .map(|(parameter, canonical_type)| {
                    if !function.parameters.parameter_type_matches(
                        unit.types(),
                        *canonical_type,
                        parameter.ty,
                    ) || !written_declarator_matches(
                        unit.types(),
                        &parameter.written,
                        parameter.ty,
                        parameter.specifier.type_id(),
                        None,
                        false,
                        true,
                        (target, &parameter.attributes),
                    ) || !matches!(
                        parameter.storage,
                        StorageClass::None | StorageClass::Register
                    ) {
                        return Err(LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter declaration disagrees with its semantic identity",
                        });
                    }
                    if parameter.symbol.is_none() && parameter.name.is_none() {
                        // An omitted C binder still occupies its positional ABI slot.
                        return Ok(None);
                    }
                    let (Some(parameter_symbol), Some(parameter_name)) =
                        (parameter.symbol, &parameter.name)
                    else {
                        return Err(LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter name and symbol disagree",
                        });
                    };
                    let symbol = unit.symbols().symbol(parameter_symbol).map_err(|_| {
                        LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter symbol is absent from the symbol table",
                        }
                    })?;
                    let scope = unit.symbols().scope(symbol.scope()).map_err(|_| {
                        LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter scope is absent from the symbol table",
                        }
                    })?;
                    let scope_is_valid = scope.kind() == ScopeKind::Function
                        && scope.parent() == Some(unit.symbols().file_scope())
                        && function_scope.is_none_or(|expected| expected == symbol.scope());
                    if symbol.kind() != SymbolKind::Parameter
                        || symbol.spelling() != parameter_name.spelling
                        || symbol.ty() != parameter.ty
                        || !matches!(
                            symbol.storage_class(),
                            Some(StorageClass::None | StorageClass::Register)
                        )
                        || symbol.storage_class() != Some(parameter.storage)
                        || !scope_is_valid
                        || !ids.insert(parameter_symbol)
                        || !spellings.insert(parameter_name.spelling.clone())
                    {
                        return Err(LowerError::MalformedFunction {
                            function: function.name.spelling.clone(),
                            span: parameter.span,
                            reason: "parameter declaration disagrees with its semantic identity",
                        });
                    }
                    function_scope = Some(symbol.scope());
                    Ok(Some(parameter_symbol))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        ParameterSyntax::Prototype {
            parameters,
            variadic: false,
        } if function.body.is_none() && parameters.len() == parameter_types.len() => Vec::new(),
        _ if function.body.is_none() => Vec::new(),
        _ => {
            return Err(LowerError::MalformedFunction {
                function: function.name.spelling.clone(),
                span: function.span,
                reason: "written parameters do not match the canonical prototype",
            });
        }
    };
    Ok(FunctionInfo {
        section: None,
        patchable_entry: None,
        name: function.name.spelling.clone(),
        variadic: matches!(
            parameters,
            FunctionParameters::Prototype { variadic: true, .. }
        ),
        ordinal: 0,
        parameter_symbols,
        parameter_types,
        return_type,
        constant_result: evocc_chir::constant::internal_constant_result(unit, function, target)
            .map(|(_, value)| value),
        participant: matches!(
            parameters,
            FunctionParameters::Prototype { variadic: true, .. }
        ),
        returns_twice: false,
        span: function.span,
    })
}

/// FP8 storage does not yet have a call interface, including inside a
/// composite. A pointer to storage does not pass that storage by value.
pub(super) fn contains_opaque_fp8(
    types: &TypeInterner,
    ty: TypeId,
    active: &mut Vec<TypeId>,
) -> bool {
    if active.contains(&ty) {
        return false;
    }
    active.push(ty);
    let found = match types.get(ty).map(|ty| &ty.kind) {
        Some(TypeKind::TargetBuiltin(evocc_types::TargetBuiltinType::ArmMfp8)) => true,
        Some(TypeKind::Array { element, .. } | TypeKind::Vector { element, .. }) => {
            contains_opaque_fp8(types, *element, active)
        }
        Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => {
            match types.tag(*tag).map(|tag| &tag.state) {
                Some(TagState::Complete(
                    TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                )) => fields
                    .iter()
                    .any(|field| contains_opaque_fp8(types, field.ty, active)),
                _ => false,
            }
        }
        _ => false,
    };
    active.pop();
    found
}

pub(super) fn function_value_type(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
    span: SourceSpan,
) -> Result<FunctionValueType, LowerError> {
    if contains_opaque_fp8(types, ty, &mut Vec::new()) {
        return Err(LowerError::FloatingCallInterface { span });
    }
    if let Ok(integer) = canonical_integer(types, target, ty, span) {
        return Ok(FunctionValueType {
            source: ty,
            ir: bits(integer),
            integer,
        });
    }
    if let Some(kind) = floating::scalar_format(types, target, ty) {
        return Ok(FunctionValueType {
            source: ty,
            ir: floating::ir_type(kind).ok_or(LowerError::FloatingCallInterface { span })?,
            integer: CIntegerType::ULONG,
        });
    }
    // An HFA of `__bf16` members, and on Darwin a record that mixes the two
    // half formats, has no admitted convention (EvoIR 3.13.6).
    if evocc_chir::half_interface_refused(types, target, ty, false) {
        return Err(LowerError::FloatingCallInterface { span });
    }
    match types.get(ty).map(|canonical| &canonical.kind) {
        // A short vector of `float` or `double` lanes is a `vec<L, f32>` or
        // `vec<L, f64>` value; a wider one is an object (EvoIR 10.1.14).
        Some(TypeKind::Vector { lanes, .. })
            if evocc_chir::vector::floating(types, ty).is_some() =>
        {
            let (kind, _) = evocc_chir::vector::floating(types, ty).unwrap();
            let ir = if is_wide_vector(types, ty) {
                EvoIRType::Ptr(AddressSpace::Generic)
            } else {
                EvoIRType::Vec(*lanes, Box::new(floating::ir_type(kind).unwrap()))
            };
            Ok(FunctionValueType {
                source: ty,
                ir,
                integer: CIntegerType::ULONG,
            })
        }
        Some(TypeKind::Vector { element, lanes, .. }) => {
            let integer = canonical_integer(types, target, *element, span)?;
            let ir = if is_wide_vector(types, ty) {
                EvoIRType::Ptr(AddressSpace::Generic)
            } else {
                EvoIRType::Vec(*lanes, Box::new(bits(integer)))
            };
            Ok(FunctionValueType {
                source: ty,
                ir,
                integer,
            })
        }
        Some(TypeKind::Pointer(pointee))
            if matches!(
                types.get(*pointee).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            ) =>
        {
            Ok(FunctionValueType {
                source: ty,
                ir: function_pointer_ir_type(types, target, *pointee, span)?,
                integer: CIntegerType::ULONG,
            })
        }
        Some(TypeKind::Pointer(pointee))
            if !matches!(
                types.get(*pointee).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            ) =>
        {
            Ok(FunctionValueType {
                source: ty,
                ir: EvoIRType::Ptr(AddressSpace::Generic),
                integer: CIntegerType::ULONG,
            })
        }
        Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_)) => {
            let integer = canonical_integer(types, target, ty, span)?;
            Ok(FunctionValueType {
                source: ty,
                ir: bits(integer),
                integer,
            })
        }
        Some(
            TypeKind::Struct(_)
            | TypeKind::Union(_)
            | TypeKind::BuiltinVaList
            | TypeKind::Complex(evocc_types::ComplexBase::Integer(_)),
        ) => Ok(FunctionValueType {
            source: ty,
            ir: EvoIRType::Ptr(AddressSpace::Generic),
            integer: CIntegerType::ULONG,
        }),
        // AAPCS64 passes a floating complex value as an HFA of its two parts.
        Some(TypeKind::Complex(base)) if base.executable_part().is_some() => {
            Ok(FunctionValueType {
                source: ty,
                ir: EvoIRType::Ptr(AddressSpace::Generic),
                integer: CIntegerType::ULONG,
            })
        }
        Some(
            TypeKind::Complex(_)
            | TypeKind::TargetBuiltin(
                evocc_types::TargetBuiltinType::GnuFloat32
                | evocc_types::TargetBuiltinType::GnuFloat64
                | evocc_types::TargetBuiltinType::GnuFloat128
                | evocc_types::TargetBuiltinType::GnuFloat32x
                | evocc_types::TargetBuiltinType::GnuFloat64x,
            ),
        ) => Err(LowerError::FloatingCallInterface { span }),
        _ => Err(LowerError::UnsupportedType {
            ty,
            span,
            feature: "function value type is neither an integer nor object pointer",
        }),
    }
}

pub(super) fn function_pointer_ir_type(
    types: &TypeInterner,
    target: &TargetDataLayout,
    function_ty: TypeId,
    span: SourceSpan,
) -> Result<EvoIRType, LowerError> {
    let TypeKind::Function {
        return_type,
        parameters,
    } = &types
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
            feature: "fnptr pointee is not a function",
        });
    };
    let FunctionParameters::Prototype {
        types: parameter_types,
        variadic,
    } = parameters
    else {
        // The address can be stored before a call interface is known. Recover
        // an actual fnptr signature from promoted operands at an indirect call.
        return Ok(EvoIRType::Ptr(AddressSpace::Generic));
    };
    function_pointer_interface(
        types,
        target,
        *return_type,
        parameter_types,
        *variadic,
        span,
    )
}

pub(super) fn function_pointer_interface(
    types: &TypeInterner,
    target: &TargetDataLayout,
    return_type: TypeId,
    parameter_types: &[TypeId],
    variadic: bool,
    span: SourceSpan,
) -> Result<EvoIRType, LowerError> {
    let mut params = parameter_types
        .iter()
        .map(|ty| function_value_type(types, target, *ty, span).map(|value_type| value_type.ir))
        .collect::<Result<Vec<_>, _>>()?;
    let returned = match types.get(return_type).map(|ty| &ty.kind) {
        Some(TypeKind::Void) => None,
        Some(_) => Some(function_result_type(types, target, return_type, span)?),
        None => {
            return Err(LowerError::UnknownType {
                ty: return_type,
                span,
            });
        }
    };
    if returned
        .as_ref()
        .is_some_and(|value_type| is_aggregate_type(types, value_type.source))
    {
        params.push(EvoIRType::Ptr(AddressSpace::Generic));
    }
    params.push(EvoIRType::Mem(Region::All));
    params.push(EvoIRType::World);
    let mut results = returned
        .filter(|value_type| !is_aggregate_type(types, value_type.source))
        .map(|value_type| vec![value_type.ir])
        .unwrap_or_default();
    results.push(EvoIRType::Mem(Region::All));
    results.push(EvoIRType::World);
    let marks = record_signature_marks(
        types,
        target,
        parameter_types,
        Some(return_type),
        params.len(),
        span,
    )?;
    Ok(EvoIRType::FnPtr {
        params,
        results,
        variadic,
        marks,
    })
}

/// AAPCS64 homogeneous short-vector aggregate: a record or array of one to
/// four short vectors (8 or 16 bytes) of one size. AAPCS64 tests only the
/// size for homogeneity, not the lane type, as GCC 14 and Apple clang do
/// (EvoMIR 6.3). The mark keeps the lane count and lane width of the first member,
/// and a record with padding has no HVA shape.
pub(super) fn homogeneous_vector_aggregate(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
) -> Option<evocc_types::VectorAggregate> {
    fn visit(
        types: &TypeInterner,
        target: &TargetDataLayout,
        ty: TypeId,
        visiting: &mut BTreeSet<TypeId>,
    ) -> Option<evocc_types::VectorAggregate> {
        if !visiting.insert(ty) {
            return None;
        }
        let result = (|| match &types.get(ty)?.kind {
            TypeKind::Vector { element, lanes, .. } => {
                let bytes = target.layout_of(types, ty).ok()?.size_bytes;
                let lane = target.layout_of(types, *element).ok()?.size_bytes;
                let bits = u16::try_from(lane * 8).ok()?;
                (matches!(bytes, 8 | 16)
                    && u64::from(*lanes) * lane == bytes
                    && matches!(bits, 8 | 16 | 32 | 64 | 128))
                .then_some(evocc_types::VectorAggregate {
                    lanes: *lanes,
                    bits,
                    count: 1,
                })
            }
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } => {
                let mut member = visit(types, target, *element, visiting)?;
                member.count = u8::try_from(u64::from(member.count).checked_mul(*count)?).ok()?;
                (1..=4).contains(&member.count).then_some(member)
            }
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let TagState::Complete(
                    TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                ) = &types.tag(*tag)?.state
                else {
                    return None;
                };
                let mut shape: Option<evocc_types::VectorAggregate> = None;
                for field in fields {
                    if field.bit_field.is_some() {
                        return None;
                    }
                    let member = visit(types, target, field.ty, visiting)?;
                    if let Some(old) = &mut shape {
                        let size = |v: &evocc_types::VectorAggregate| {
                            u32::from(v.lanes) * u32::from(v.bits)
                        };
                        if size(old) != size(&member) {
                            return None;
                        }
                        old.count = if matches!(types.get(ty)?.kind, TypeKind::Union(_)) {
                            old.count.max(member.count)
                        } else {
                            old.count.checked_add(member.count)?
                        };
                        if old.count > 4 {
                            return None;
                        }
                    } else {
                        shape = Some(member);
                    }
                }
                shape
            }
            _ => None,
        })();
        visiting.remove(&ty);
        result
    }
    let shape = visit(types, target, ty, &mut BTreeSet::new())?;
    let size = target.layout_of(types, ty).ok()?.size_bytes;
    (size == u64::from(shape.lanes) * u64::from(shape.bits) / 8 * u64::from(shape.count))
        .then_some(shape)
}

pub(super) fn record_signature_marks(
    types: &TypeInterner,
    target: &TargetDataLayout,
    parameters: &[TypeId],
    return_type: Option<TypeId>,
    parameter_count: usize,
    span: SourceSpan,
) -> Result<Vec<ParamMark>, LowerError> {
    let shape = |ty| {
        let layout = target
            .layout_of(types, ty)
            .map_err(|source| LowerError::Layout { ty, span, source })?;
        Ok(RecordShape {
            size: layout.size_bytes,
            align: layout.align_bytes,
            vector: homogeneous_vector_aggregate(types, target, ty),
            floating: evocc_chir::homogeneous_floating_aggregate(types, target, ty),
        })
    };
    let mut marks = parameters
        .iter()
        .map(|ty| {
            if is_aggregate_type(types, *ty) {
                shape(*ty).map(ParamMark::ByVal)
            } else if signed_narrow_integer(types, target, *ty, span) {
                // EvoIR 10.1.3: on the Darwin target the caller extends a
                // signed integer narrower than 32 bits by its sign.
                Ok(ParamMark::SignExtend)
            } else {
                Ok(ParamMark::None)
            }
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    if let Some(ty) = return_type.filter(|ty| is_aggregate_type(types, *ty)) {
        marks.push(ParamMark::Sret(shape(ty)?));
    }
    Ok(canonical_marks(marks, parameter_count))
}

pub(super) fn is_aggregate_type(types: &TypeInterner, ty: TypeId) -> bool {
    is_wide_vector(types, ty)
        || evocc_chir::complex::admitted(types, ty)
        || matches!(
            types.get(ty).map(|canonical| &canonical.kind),
            Some(TypeKind::Struct(_) | TypeKind::Union(_) | TypeKind::BuiltinVaList)
        )
}

pub(super) fn is_wide_vector(types: &TypeInterner, ty: TypeId) -> bool {
    let Some(TypeKind::Vector { element, lanes, .. }) = types.get(ty).map(|ty| &ty.kind) else {
        return false;
    };
    let bits = match types.get(*element).map(|ty| &ty.kind) {
        Some(TypeKind::Int(rank)) => u32::from(integer_type_for_rank(*rank).bits()),
        Some(TypeKind::GnuInt128(_)) => 128,
        _ => match evocc_chir::vector::floating(types, ty) {
            Some((evocc_types::FloatKind::F32, _)) => 32,
            Some(_) => 64,
            None => return false,
        },
    };
    u32::from(*lanes) * bits > 128
}

/// EvoIR 10.1.3: on a target whose calls extend narrow integers, a signed
/// integer result narrower than 32 bits travels as `bits<32>`, the sign
/// extension of the value. The callee extends it and the caller truncates it.
pub(super) fn function_result_type(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
    span: SourceSpan,
) -> Result<FunctionValueType, LowerError> {
    let mut value = function_value_type(types, target, ty, span)?;
    if signed_narrow_integer(types, target, ty, span) {
        value.ir = EvoIRType::Bits(32);
    }
    Ok(value)
}

/// Whether a call on `target` extends a value of `ty` by its sign: a signed
/// integer narrower than 32 bits, on a target whose calls extend narrow
/// integers (EvoIR 10.1.3).
pub(super) fn signed_narrow_integer(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
    span: SourceSpan,
) -> bool {
    target.extends_narrow_call_integers()
        && canonical_integer(types, target, ty, span)
            .is_ok_and(|integer| integer.is_signed() && integer.bits() < 32)
}

pub(super) fn canonical_integer(
    types: &TypeInterner,
    target: &TargetDataLayout,
    ty: TypeId,
    span: SourceSpan,
) -> Result<CIntegerType, LowerError> {
    let canonical = types.get(ty).ok_or(LowerError::UnknownType { ty, span })?;
    let integer = match &canonical.kind {
        TypeKind::TargetBuiltin(kind) => evocc_chir::integer::integer_type_for_target(*kind)
            .ok_or(LowerError::FloatingValue { span })?,
        TypeKind::Float(_)
        | TypeKind::Complex(
            evocc_types::ComplexBase::CF16
            | evocc_types::ComplexBase::CF32
            | evocc_types::ComplexBase::CF64
            | evocc_types::ComplexBase::CF128,
        ) => return Err(LowerError::FloatingValue { span }),
        TypeKind::Int(rank) => integer_type_for_rank(*rank),
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Signed) => CIntegerType::INT128,
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Unsigned) => CIntegerType::UINT128,
        TypeKind::Enum(tag) => {
            let tag = types.tag(*tag).ok_or(LowerError::UnsupportedType {
                ty,
                span,
                feature: "unknown enum type",
            })?;
            if let Some(rank) = tag.fixed_underlying {
                return Ok(integer_type_for_rank(rank));
            }
            let TagState::Complete(TagDefinition::Enum(constants)) = &tag.state else {
                return Err(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "incomplete or non-enum scalar type",
                });
            };
            let minimum = constants
                .iter()
                .map(|constant| constant.value)
                .min()
                .ok_or(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "empty enum has no canonical integer representation",
                })?;
            let maximum = constants
                .iter()
                .map(|constant| constant.value)
                .max()
                .ok_or(LowerError::UnsupportedType {
                    ty,
                    span,
                    feature: "empty enum has no canonical integer representation",
                })?;
            integer_type_for_rank(enum_integer_rank(minimum, maximum, tag.layout.packed))
        }
        _ => {
            return Err(LowerError::UnsupportedType {
                ty,
                span,
                feature: "non-integer scalar type is outside Day 6",
            });
        }
    };
    let layout = target
        .layout_of(types, ty)
        .map_err(|source| LowerError::Layout { ty, span, source })?;
    let expected_storage_bits = u64::from(integer.bits().max(8));
    if layout.size_bytes.saturating_mul(8) != expected_storage_bits {
        return Err(LowerError::UnsupportedType {
            ty,
            span,
            feature: "integer value width disagrees with target data layout",
        });
    }
    Ok(integer)
}

pub(super) fn function_storage_compatible(entity: StorageClass, written: StorageClass) -> bool {
    match entity {
        StorageClass::Static => matches!(
            written,
            StorageClass::None | StorageClass::Extern | StorageClass::Static
        ),
        StorageClass::None | StorageClass::Extern => {
            matches!(written, StorageClass::None | StorageClass::Extern)
        }
        StorageClass::Typedef
        | StorageClass::Auto
        | StorageClass::Register
        | StorageClass::ThreadLocal
        | StorageClass::StaticThreadLocal
        | StorageClass::ExternThreadLocal => false,
    }
}

pub(super) fn function_parameters_match(
    interner: &TypeInterner,
    canonical: &FunctionParameters,
    function: &FunctionDecl<'_>,
) -> bool {
    match (canonical, &function.parameters) {
        (_, ParameterSyntax::Unspecified) if function.body.is_none() => {
            interner.compatible_parameters(canonical, &FunctionParameters::Unspecified)
        }
        (FunctionParameters::Unspecified, ParameterSyntax::Unspecified) => true,
        (
            FunctionParameters::Prototype {
                types,
                variadic: false,
            },
            ParameterSyntax::Unspecified,
        ) => types.is_empty(),
        (FunctionParameters::Prototype { types, variadic }, syntax)
            if !matches!(syntax, ParameterSyntax::Unspecified) =>
        {
            let parameters = syntax.parameters();
            *variadic == matches!(syntax, ParameterSyntax::Prototype { variadic: true, .. })
                && types.len() == parameters.len()
                && types.iter().zip(parameters).all(|(canonical, parameter)| {
                    syntax.parameter_type_matches(interner, *canonical, parameter.ty)
                })
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn written_declarator_matches(
    types: &TypeInterner,
    written: &WrittenDeclarator,
    mut current: TypeId,
    terminal: TypeId,
    function_declaration: Option<&FunctionDecl<'_>>,
    initialized_object: bool,
    adjusted_parameter: bool,
    representation: (&TargetDataLayout, &[evocc_chir::Attribute]),
) -> bool {
    // Undo only the outer function-to-pointer parameter adjustment.
    if adjusted_parameter
        && (matches!(
            written.layers.first(),
            Some(WrittenDeclaratorLayer::Function { .. })
        ) || (written.layers.is_empty()
            && types
                .get(terminal)
                .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))))
    {
        if let Some(canonical) = types.get(current) {
            if let TypeKind::Pointer(pointee) = canonical.kind {
                if canonical.qualifiers == evocc_types::Qualifiers::default()
                    && types
                        .get(pointee)
                        .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))
                {
                    current = pointee;
                }
            }
        }
    }
    if let Some(function) = function_declaration {
        if !matches!(
            written.layers.first(),
            Some(WrittenDeclaratorLayer::Function { .. })
        ) {
            return written.layers.is_empty()
                && (current == terminal
                    || (function.body.is_none() && types.compatible(current, terminal)))
                && types.get(current).is_some_and(|canonical| {
                    matches!(
                        &canonical.kind,
                        TypeKind::Function { parameters, .. }
                            if function_parameters_match(types, parameters, function)
                    )
                });
        }
    }
    for (index, layer) in written.layers.iter().enumerate() {
        let Some(canonical) = types.get(current) else {
            return false;
        };
        current = match (layer, &canonical.kind) {
            (
                WrittenDeclaratorLayer::Function { .. },
                TypeKind::Function {
                    return_type,
                    parameters,
                },
            ) => {
                if index == 0
                    && function_declaration.is_some_and(|function| {
                        !function_parameters_match(types, parameters, function)
                    })
                {
                    return false;
                }
                *return_type
            }
            (WrittenDeclaratorLayer::Pointer { qualifiers, .. }, TypeKind::Pointer(pointee))
                if canonical.qualifiers == *qualifiers =>
            {
                *pointee
            }
            (
                WrittenDeclaratorLayer::Array {
                    parameter_qualifiers,
                    ..
                },
                TypeKind::Pointer(pointee),
            ) if adjusted_parameter
                && index == 0
                && canonical.qualifiers == *parameter_qualifiers =>
            {
                *pointee
            }
            (
                WrittenDeclaratorLayer::Array { bound, .. },
                TypeKind::Array {
                    element,
                    bound: canonical_bound,
                },
            ) if bound == canonical_bound
                || (initialized_object
                    && index == 0
                    && *bound == ArrayBound::Unbounded
                    && matches!(canonical_bound, ArrayBound::Fixed(_))) =>
            {
                *element
            }
            _ => return false,
        };
    }
    current == terminal
        || evocc_chir::vector::attribute_matches_base(
            types,
            representation.0,
            representation.1,
            current,
            terminal,
        )
        || (initialized_object
            && written.layers.is_empty()
            && evocc_chir::type_at_use_matches_binding(types, terminal, current))
        || (adjusted_parameter
            && written.layers.is_empty()
            && matches!(
                (types.get(current).map(|ty| &ty.kind), types.get(terminal).map(|ty| &ty.kind)),
                (Some(TypeKind::Pointer(pointee)), Some(TypeKind::Array { element, .. })) if pointee == element
            ))
}
