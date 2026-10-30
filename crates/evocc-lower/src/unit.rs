//! Lowering unit.

use super::*;

pub(super) fn lower_resolved_unit(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    source_manifest: Option<&AllocationSiteManifest<'_>>,
    optimize: bool,
    locations: &mut Option<SourceLocations>,
) -> Result<Module, LowerError> {
    let linkage = unit.symbol_linkage().map_err(LowerError::Linkage)?;
    let lifecycle = evocc_chir::lifecycle::registrations(unit).map_err(|span| {
        LowerError::UnsupportedExpression {
            span,
            feature: "invalid lifecycle registration",
        }
    })?;
    let global_register_symbols = collect_global_register_symbols(unit);
    let global_registers = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) if global_register_symbols.contains(&object.symbol) => Some((
                object.symbol,
                object.asm_label.as_ref().expect("register label").clone(),
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let mut memory_symbols = collect_memory_symbols(unit);
    memory_symbols.retain(|symbol| !global_register_symbols.contains(symbol));
    let manifest = AllocationSiteManifest::for_target(unit, target).map_err(|error| {
        LowerError::InvalidGeneratedSsa {
            message: format!("cannot derive the authoritative allocation-site manifest: {error:?}"),
        }
    })?;
    let origin_of = |site: &evocc_chir::AllocationSite<'_>| {
        let Some(source) = source_manifest else {
            return site.ordinal().raw();
        };
        // Selection retains allocation sites from the validated source unit.
        let original = match site.kind() {
            evocc_chir::AllocationSiteKind::Object(symbol) => source.object_site(symbol),
            evocc_chir::AllocationSiteKind::StringLiteral(expression) => {
                source.string_literal_site(expression)
            }
            evocc_chir::AllocationSiteKind::CompoundLiteral { expression, .. } => {
                source.compound_literal_site(expression)
            }
            evocc_chir::AllocationSiteKind::DynamicStack(expression) => {
                source.dynamic_stack_site(expression)
            }
        };
        original
            .expect("retained allocation site belongs to the source unit")
            .ordinal()
            .raw()
    };
    let allocation_origins = manifest
        .sites()
        .iter()
        .filter_map(|site| {
            site.kind()
                .object_symbol()
                .map(|symbol| (symbol, origin_of(site)))
        })
        .collect::<BTreeMap<_, _>>();
    let compound_origins = manifest
        .sites()
        .iter()
        .filter_map(|site| {
            site.kind()
                .compound_literal()
                .map(|(expression, static_storage)| {
                    (
                        expression as *const CHirExpr<'_> as usize,
                        (origin_of(site), static_storage),
                    )
                })
        })
        .collect::<BTreeMap<_, _>>();
    let dynamic_origins = manifest
        .sites()
        .iter()
        .filter_map(|site| {
            site.kind()
                .dynamic_stack()
                .map(|expression| (expression as *const CHirExpr<'_> as usize, origin_of(site)))
        })
        .collect::<BTreeMap<_, _>>();
    let memory_profile = !dynamic_origins.is_empty()
        || !linkage.aliases.is_empty()
        || !global_registers.is_empty()
        || !memory_symbols.is_empty()
        || unit.iter().any(|declaration| match declaration {
            CHirDecl::Function(function) => function.body.is_some_and(|body| {
                evocc_chir::interp::statement_requires_memory(unit, body, target)
            }),
            _ => false,
        })
        || manifest.sites().iter().any(|site| {
            site.kind().object_symbol().is_some_and(|symbol| {
                unit.symbols()
                    .symbol(symbol)
                    .ok()
                    .and_then(|entry| unit.types().get(entry.ty()))
                    .is_some_and(|ty| {
                        matches!(
                            ty.kind,
                            TypeKind::Pointer(_)
                                | TypeKind::Array { .. }
                                | TypeKind::Struct(_)
                                | TypeKind::Union(_)
                        ) || ty.qualifiers.is_volatile
                    })
            })
        });
    let functions = index_functions(unit, target, memory_profile, &lifecycle)?;
    let size_symbols = unit
        .types()
        .captured_array_bounds()
        .filter_map(|raw| {
            unit.symbols()
                .symbols_in_source_order()
                .get(raw as usize)
                .map(|symbol| symbol.id())
        })
        .collect::<BTreeSet<_>>();
    prevalidate_unit_for_lowering(unit, target, &global_register_symbols)?;
    let mut globals = Vec::new();
    let mut string_origins = BTreeMap::new();
    for site in manifest.sites() {
        let Some(expression) = site.kind().string_literal() else {
            continue;
        };
        let CHirExpr::StringLit(literal, span) = expression else {
            unreachable!("manifest string site")
        };
        let (bytes, align) =
            literal
                .object_bytes(target)
                .ok_or(LowerError::UnsupportedExpression {
                    span: *span,
                    feature: "invalid string representation",
                })?;
        let origin = origin_of(site);
        string_origins.insert(expression as *const CHirExpr<'_> as usize, origin);
        globals.push(GlobalObject {
            read_only: false, // String storage is implicitly read-only in old and new IR.
            section: None,
            relocs: Vec::new(),
            symbol: format!("__evocc.string.{origin}"),
            origin,
            kind: GlobalKind::StringLiteral,
            linkage: evocc_evoir::Linkage::Strong,
            visibility: GlobalVisibility::Internal,
            layout: AccessLayout::new(bytes.len() as u64, align),
            initialized: vec![true; bytes.len()],
            bytes,
        });
    }
    let mut static_literal_origins = string_origins.clone();
    for site in manifest.sites() {
        if let Some((expression, true)) = site.kind().compound_literal() {
            static_literal_origins
                .insert(expression as *const CHirExpr<'_> as usize, origin_of(site));
        }
    }
    // These are source-proven immutable definitions, not a claim inferred
    // from their section name. Reuse the normal initializer representation.
    let mut immutable_globals = globals
        .iter()
        .filter(|_| optimize)
        .cloned()
        .map(|global| (global.symbol.clone(), global))
        .collect::<BTreeMap<_, _>>();
    if optimize {
        for object in unit
            .iter()
            .filter_map(|decl| match decl {
                CHirDecl::Object(object) => Some(object),
                _ => None,
            })
            .chain(manifest.function_static_objects().iter().copied())
        {
            let mut ty = object.ty;
            let mut immutable = false;
            while let Some(layer) = unit.types().get(ty) {
                if layer.qualifiers.is_volatile || layer.qualifiers.is_atomic {
                    immutable = false;
                    break;
                }
                immutable |= layer.qualifiers.is_const;
                match layer.kind {
                    TypeKind::Array { element, .. } => ty = element,
                    _ => break,
                }
            }
            if !immutable
                || object.storage.base() != StorageClass::Static
                || object.initializer.is_none()
                || object.asm_label.is_some()
            {
                continue;
            }
            if let Ok(global) = lower_global_object(
                unit,
                target,
                object,
                &allocation_origins,
                &static_literal_origins,
                &BTreeMap::new(),
            ) {
                immutable_globals.insert(global.symbol.clone(), global);
            }
        }
    }
    let mut lowered = Vec::new();
    let mut label_addresses = BTreeMap::new();
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        if function.body.is_none() {
            continue;
        }
        let info =
            functions
                .get(&function.symbol)
                .ok_or_else(|| LowerError::MalformedFunction {
                    function: function.name.spelling.clone(),
                    span: function.span,
                    reason: "definition is absent from the function index",
                })?;
        let (function, addresses) = FunctionLowerer::new(
            unit,
            target,
            &functions,
            info,
            &memory_symbols,
            &size_symbols,
            &global_registers,
            &allocation_origins,
            &compound_origins,
            &dynamic_origins,
            &string_origins,
            &immutable_globals,
        )
        .lower(function, optimize, locations)?;
        lowered.push(function);
        label_addresses.extend(addresses);
    }
    for site in manifest.sites() {
        let Some((CHirExpr::CompoundLit(ty, initializer, span), true)) =
            site.kind().compound_literal()
        else {
            continue;
        };
        let layout = target
            .layout_of(unit.types(), *ty)
            .map_err(|source| LowerError::Layout {
                ty: *ty,
                span: *span,
                source,
            })?;
        let mut bytes = vec![
            0;
            usize::try_from(layout.size_bytes)
                .map_err(|_| LowerError::StaticRelocation { span: *span })?
        ];
        let mut relocs = Vec::new();
        for action in initializer.plan().actions() {
            apply_static_initializer(
                unit,
                target,
                *ty,
                action,
                &mut bytes,
                &mut relocs,
                &allocation_origins,
                &static_literal_origins,
                &label_addresses,
            )?;
        }
        relocs.sort_by_key(|relocation| relocation.offset);
        globals.push(GlobalObject {
            section: None,
            symbol: format!("__evocc.compound.{}", origin_of(site)),
            read_only: unit.types().is_const_qualified_object(*ty),
            origin: origin_of(site),
            kind: GlobalKind::CompoundLiteral,
            linkage: evocc_evoir::Linkage::Strong,
            visibility: GlobalVisibility::Internal,
            layout: AccessLayout::new(layout.size_bytes, layout.align_bytes),
            initialized: vec![true; bytes.len()],
            bytes,
            relocs,
        });
    }
    globals.extend(lower_globals(
        unit,
        target,
        manifest.function_static_objects(),
        &memory_symbols,
        &allocation_origins,
        &static_literal_origins,
        &label_addresses,
    )?);
    let defined = lowered
        .iter()
        .map(|function| function.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut declares = BTreeMap::new();
    for (symbol, info) in functions
        .iter()
        .filter(|(_, info)| !defined.contains(info.name.as_str()))
    {
        if unit.symbols().is_builtin_function(*symbol)
            && matches!(
                info.name.as_str(),
                "__builtin_extract_return_addr"
                    | "__builtin_prefetch"
                    | "__sync_synchronize"
                    | "__atomic_thread_fence"
                    | "__atomic_signal_fence"
                    | "alloca"
            )
        {
            continue;
        }
        let aggregate_return = info
            .return_type
            .as_ref()
            .is_some_and(|value| is_aggregate_type(unit.types(), value.source));
        let mut params = info
            .parameter_types
            .iter()
            .map(|value| value.ir.clone())
            .collect::<Vec<_>>();
        if aggregate_return {
            params.push(EvoIRType::Ptr(AddressSpace::Generic));
        }
        params.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
        let mut ret_tys = info
            .return_type
            .iter()
            .filter(|_| !aggregate_return)
            .map(|value| value.ir.clone())
            .collect::<Vec<_>>();
        ret_tys.extend([EvoIRType::Mem(Region::All), EvoIRType::World]);
        let param_marks = record_signature_marks(
            unit.types(),
            target,
            &info
                .parameter_types
                .iter()
                .map(|value| value.source)
                .collect::<Vec<_>>(),
            info.return_type.as_ref().map(|value| value.source),
            params.len(),
            info.span,
        )?;
        declares.insert(
            info.name.clone(),
            FunctionDeclaration {
                visibility: GlobalVisibility::External,
                linkage: evocc_evoir::Linkage::Strong,
                name: info.name.clone(),
                variadic: info.variadic,
                params,
                ret_tys,
                param_marks,
            },
        );
    }
    let global_names = globals
        .iter()
        .map(|global| global.symbol.as_str())
        .collect::<BTreeSet<_>>();
    let referenced_objects = lowered
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .filter_map(|instruction| match instruction {
            Inst::GlobalAddr { symbol, .. } if !global_names.contains(symbol.as_str()) => {
                Some(symbol.as_str())
            }
            _ => None,
        })
        .chain(
            globals
                .iter()
                .flat_map(|global| global.relocs.iter())
                .filter_map(|relocation| {
                    (!global_names.contains(relocation.symbol.as_str()))
                        .then_some(relocation.symbol.as_str())
                }),
        )
        .collect::<BTreeSet<_>>();
    let mut extern_objects = BTreeMap::new();
    for symbol in &memory_symbols {
        let entry =
            unit.symbols()
                .symbol(*symbol)
                .map_err(|_| LowerError::InvalidGeneratedSsa {
                    message: "external object has no symbol".into(),
                })?;
        if !has_linked_object_storage(unit, *symbol)
            || !referenced_objects.contains(entry.spelling())
        {
            continue;
        }
        let (size, align) = target
            .external_object_layout(unit.types(), entry.ty())
            .map_err(|source| LowerError::Layout {
                ty: entry.ty(),
                span: entry.span(),
                source,
            })?;
        extern_objects.insert(
            entry.spelling().to_owned(),
            ExternalObject {
                visibility: GlobalVisibility::External,
                linkage: evocc_evoir::Linkage::Strong,
                symbol: entry.spelling().to_owned(),
                size,
                align: align.max(entry.declared_alignment()),
            },
        );
    }
    // The builtin namespace does not declare a C identifier. Its emitted
    // library symbol can nevertheless collide with a source object. Keep the
    // namespaces separate and refuse that linkage before constructing invalid
    // IR; do not silently call or rename an object as a library function.
    for entry in unit.symbols().symbols_in_source_order() {
        let name = entry.spelling();
        if has_linked_object_storage(unit, entry.id())
            && (global_names.contains(name) || extern_objects.contains_key(name))
            && declares.contains_key(name)
            && unit.symbols().builtin_function(name).is_some()
        {
            return Err(LowerError::Linkage(evocc_chir::linkage::LinkageError {
                code: "E2946",
                message: "builtin-local-binding-conflict",
                span: entry.span(),
            }));
        }
    }
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: lifecycle
            .into_iter()
            .map(|r| evocc_evoir::LifecycleRegistration {
                phase: match r.phase {
                    evocc_chir::lifecycle::Phase::Constructor => {
                        evocc_evoir::LifecyclePhase::Constructor
                    }
                    evocc_chir::lifecycle::Phase::Destructor => {
                        evocc_evoir::LifecyclePhase::Destructor
                    }
                },
                function: functions
                    .get(&r.function)
                    .expect("registered function")
                    .name
                    .clone(),
                priority: r.priority,
            })
            .collect(),
        aliases: Vec::new(),
        assembly: unit
            .iter()
            .filter_map(|decl| match decl {
                CHirDecl::Asm(assembly) => Some(assembly.template.clone()),
                _ => None,
            })
            .collect(),
        globals,
        extern_objects: extern_objects.into_values().collect(),
        declares: declares.into_values().collect(),
        functions: lowered,
    };
    for (symbol, target_id) in &linkage.aliases {
        let alias = unit.symbols().symbol(*symbol).expect("resolved alias");
        let target = unit
            .symbols()
            .symbol(*target_id)
            .expect("resolved alias target");
        module.aliases.push(evocc_evoir::SymbolAlias {
            name: linkage.function_name(*symbol, alias.spelling()).to_owned(),
            target: linkage
                .function_name(*target_id, target.spelling())
                .to_owned(),
            visibility: if alias.storage_class().map(StorageClass::base)
                == Some(StorageClass::Static)
            {
                GlobalVisibility::Internal
            } else {
                GlobalVisibility::External
            },
            linkage: if linkage.weak.contains(symbol) {
                evocc_evoir::Linkage::Weak
            } else {
                evocc_evoir::Linkage::Strong
            },
        });
    }
    let alias_names = module
        .aliases
        .iter()
        .map(|a| a.name.as_str())
        .collect::<BTreeSet<_>>();
    module
        .extern_objects
        .retain(|g| !alias_names.contains(g.symbol.as_str()));
    module
        .declares
        .retain(|f| !alias_names.contains(f.name.as_str()));
    let weak_names = linkage
        .weak
        .iter()
        .map(|symbol| {
            let entry = unit
                .symbols()
                .symbol(*symbol)
                .expect("resolved weak symbol");
            linkage.function_name(*symbol, entry.spelling())
        })
        .collect::<BTreeSet<_>>();
    for f in &mut module.functions {
        if weak_names.contains(f.name.as_str()) {
            f.linkage = evocc_evoir::Linkage::Weak;
        }
    }
    for f in &mut module.declares {
        if weak_names.contains(f.name.as_str()) {
            f.linkage = evocc_evoir::Linkage::Weak;
        }
    }
    for g in &mut module.globals {
        if weak_names.contains(g.symbol.as_str()) {
            g.linkage = evocc_evoir::Linkage::Weak;
        }
    }
    for g in &mut module.extern_objects {
        if weak_names.contains(g.symbol.as_str()) {
            g.linkage = evocc_evoir::Linkage::Weak;
        }
    }
    let symbol_visibility = linkage
        .visibility
        .iter()
        .map(|(symbol, visibility)| {
            let entry = unit
                .symbols()
                .symbol(*symbol)
                .expect("resolved attributed symbol");
            let visibility = match visibility {
                evocc_chir::linkage::SymbolVisibility::Default => GlobalVisibility::External,
                evocc_chir::linkage::SymbolVisibility::Hidden => GlobalVisibility::Hidden,
                evocc_chir::linkage::SymbolVisibility::Protected => GlobalVisibility::Protected,
            };
            (linkage.function_name(*symbol, entry.spelling()), visibility)
        })
        .collect::<BTreeMap<_, _>>();
    for (name, visibility) in module
        .functions
        .iter_mut()
        .map(|f| (&f.name, &mut f.visibility))
        .chain(
            module
                .declares
                .iter_mut()
                .map(|f| (&f.name, &mut f.visibility)),
        )
        .chain(
            module
                .globals
                .iter_mut()
                .map(|g| (&g.symbol, &mut g.visibility)),
        )
        .chain(
            module
                .extern_objects
                .iter_mut()
                .map(|g| (&g.symbol, &mut g.visibility)),
        )
        .chain(
            module
                .aliases
                .iter_mut()
                .map(|a| (&a.name, &mut a.visibility)),
        )
    {
        if *visibility == GlobalVisibility::External {
            if let Some(effective) = symbol_visibility.get(name.as_str()) {
                *visibility = *effective;
            }
        }
    }
    let mut preserved: BTreeSet<String> = unit
        .iter()
        .filter_map(|decl| {
            let CHirDecl::Function(function) = decl else {
                return None;
            };
            function
                .attributes
                .iter()
                .any(|a| matches!(a.semantic_name(), Some("used" | "retain")))
                .then(|| {
                    linkage
                        .function_name(function.symbol, &function.name.spelling)
                        .to_owned()
                })
        })
        .collect();
    let mut preserved_origins = BTreeSet::new();
    evocc_chir::inline::for_each_declaration(unit, |decl| {
        if let CHirDecl::Object(object) = decl {
            if object
                .attributes
                .iter()
                .any(|a| matches!(a.semantic_name(), Some("used" | "retain")))
            {
                if let Some(origin) = allocation_origins.get(&object.symbol) {
                    preserved_origins.insert(*origin);
                }
            }
        }
    });
    preserved.extend(
        module
            .globals
            .iter()
            .filter(|g| preserved_origins.contains(&g.origin))
            .map(|g| g.symbol.clone()),
    );
    if optimize {
        preserved.extend(module.lifecycle.iter().map(|r| r.function.clone()));
        constants::remove_unreferenced_functions(&mut module, preserved);
        fold_constant_shift_counts(&mut module);
    }
    tls::resolve(unit, &allocation_origins, &mut module)?;
    module.recompute_effect_summaries();
    validate_module(&module).map_err(|message| LowerError::InvalidGeneratedSsa { message })?;
    Ok(module)
}
