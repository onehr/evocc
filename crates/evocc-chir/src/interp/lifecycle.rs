use super::*;

/// A program invocation initializes one load instance. Function invocation
/// APIs deliberately do not run registrations.
pub fn interpret_program(
    unit: &CHirUnit<'_>,
    main: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
) -> Result<InterpretOutcome, InterpreterError> {
    interpret_program_with_arguments(unit, main, target, limits, None)
}

/// Explicit hosted arguments include argv[0]. None preserves the existing entry API.
pub fn interpret_program_with_arguments(
    unit: &CHirUnit<'_>,
    main: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
    arguments: Option<&[Vec<u8>]>,
) -> Result<InterpretOutcome, InterpreterError> {
    crate::validate_front_end_builtin_records(unit, target).map_err(|span| {
        InterpreterError::UnsupportedExpression {
            feature: "inconsistent front-end builtin record",
            span,
        }
    })?;
    if crate::inline::needs_execution_resolution(unit) {
        let arena = evocc_common::Arena::default();
        let resolved = crate::inline::substitute_for_execution(unit, &arena, target, true)
            .map_err(|e| InterpreterError::UnsupportedExpression {
                feature: "constant-query substitution",
                span: e.span,
            })?;
        return run_program(&resolved, main, target, limits, arguments);
    }
    run_program(unit, main, target, limits, arguments)
}

fn run_program(
    unit: &CHirUnit<'_>,
    main: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
    arguments: Option<&[Vec<u8>]>,
) -> Result<InterpretOutcome, InterpreterError> {
    use crate::lifecycle::Phase;
    let registrations = crate::lifecycle::registrations(unit).map_err(|span| {
        InterpreterError::UnsupportedExpression {
            feature: "invalid lifecycle registration",
            span,
        }
    })?;
    let linkage = unit.symbol_linkage().map_err(InterpreterError::Linkage)?;
    let mut requirements = reachable_memory_requirements(unit, main, linkage);
    requirements.requires_memory |= arguments.is_some();
    for r in &registrations {
        let extra = reachable_memory_requirements(unit, r.function, linkage);
        requirements.requires_memory |= extra.requires_memory;
        requirements.static_objects.extend(extra.static_objects);
    }
    let mut machine = Interpreter::new(unit, target, limits, requirements.requires_memory)?;
    machine.program = true;
    if arguments.is_none() {
        machine.validate_root_function(main)?;
    }
    for r in &registrations {
        machine.validate_root_function(r.function)?;
    }
    if requirements.requires_memory {
        machine.derive_allocation_manifest()?;
        machine.initialize_static_objects(&requirements.static_objects)?;
    }
    let arguments = match arguments {
        Some(values) => machine.main_arguments(main, values)?,
        None => Vec::new(),
    };
    let mut init: Vec<_> = registrations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.phase == Phase::Constructor)
        .collect();
    init.sort_by_key(|(i, r)| (r.priority, *i));
    for (_, r) in init {
        machine.run_root(r.function)?;
        if machine.termination.is_some() {
            break;
        }
    }
    let mut return_value = None;
    if machine.termination.is_none() {
        return_value = machine
            .run_root_arguments(main, arguments)?
            .and_then(|outcome| outcome.return_value);
    }
    let mut termination = machine.termination.take();
    if termination.is_none_or(|(_, normal)| normal) {
        let mut fini: Vec<_> = registrations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.phase == Phase::Destructor)
            .collect();
        fini.sort_by_key(|(i, r)| std::cmp::Reverse((r.priority, *i)));
        for (_, r) in fini {
            machine.run_root(r.function)?;
            if let Some(exit) = machine.termination.take() {
                termination = Some(exit);
                if !exit.1 {
                    break;
                }
            }
        }
    }
    if let Some((value, _)) = termination {
        let function = machine.function_definition(main)?;
        let TypeKind::Function { return_type, .. } = &unit
            .types()
            .get(function.ty)
            .expect("validated function")
            .kind
        else {
            unreachable!()
        };
        return_value = Some(ScalarValue {
            ty: *return_type,
            raw_bits: value,
        });
    }
    Ok(InterpretOutcome {
        return_value,
        steps: machine.consumed,
        memory: machine.memory,
        world: machine.world,
    })
}

impl Interpreter<'_, '_> {
    /// Keep argv and its writable strings in separate, provenance-bearing objects.
    fn main_arguments(
        &mut self,
        main: SymbolId,
        values: &[Vec<u8>],
    ) -> Result<Vec<CValue>, InterpreterError> {
        let function = self.function_definition(main)?;
        let span = function.span;
        let bad = || InterpreterError::UnsupportedFunctionSignature { symbol: main, span };
        let parameters = function.parameters.parameters();
        if parameters.is_empty() {
            self.validate_root_function(main)?;
            return Ok(Vec::new());
        }
        if parameters.len() != 2 || values.is_empty() || values.len() > i32::MAX as usize {
            return Err(bad());
        }
        self.validate_function_signature(function, 2)?;
        if !matches!(
            self.unit.types().get(parameters[0].ty).map(|t| &t.kind),
            Some(TypeKind::Int(evocc_types::IntRank::Int))
        ) {
            return Err(bad());
        }
        let pointer = PointerTypeDesc::from_canonical_pointer(self.unit.types(), parameters[1].ty)
            .map_err(|_| bad())?;
        let character =
            PointerTypeDesc::from_canonical_pointer(self.unit.types(), pointer.pointee())
                .map_err(|_| bad())?;
        if !matches!(
            self.unit.types().get(character.pointee()).map(|t| &t.kind),
            Some(TypeKind::Int(evocc_types::IntRank::Char))
        ) {
            return Err(bad());
        }
        let layout = AccessLayout::from_type_layout(self.target.pointer_layout());
        let bytes = layout
            .size_bytes
            .checked_mul(values.len() as u64 + 1)
            .ok_or_else(bad)?;
        let argv = self.argument_object(AccessLayout::new(bytes, layout.align_bytes), span)?;
        for (index, value) in values.iter().enumerate() {
            if value.contains(&0) {
                return Err(bad());
            }
            let mut bytes = value.clone();
            bytes.push(0);
            let string = self.argument_object(AccessLayout::new(bytes.len() as u64, 1), span)?;
            let bytes =
                ByteSequence::try_from_bytes(bytes).map_err(|e| self.memory_error(e, span))?;
            self.memory = self
                .memory
                .store_bytes(string, AccessLayout::new(bytes.len() as u64, 1), &bytes)
                .map_err(|e| self.memory_error(e, span))?;
            let slot = argv
                .checked_offset((index as u64 * layout.size_bytes) as i64)
                .map_err(|e| self.memory_error(e, span))?;
            self.memory = self
                .memory
                .store_pointer(slot, layout, string)
                .map_err(|e| self.memory_error(e, span))?;
        }
        let end = argv
            .checked_offset((values.len() as u64 * layout.size_bytes) as i64)
            .map_err(|e| self.memory_error(e, span))?;
        self.memory = self
            .memory
            .store_pointer_value(end, layout, PointerValue::Null)
            .map_err(|e| self.memory_error(e, span))?;
        Ok(vec![
            CValue::Integer(CInteger::int(values.len() as i32)),
            CValue::Pointer {
                value: argv,
                ty: pointer,
            },
        ])
    }

    fn argument_object(
        &mut self,
        layout: AccessLayout,
        span: SourceSpan,
    ) -> Result<Pointer, InterpreterError> {
        let site = self.temporary_origin_site;
        self.temporary_origin_site = site
            .checked_add(1)
            .ok_or(InterpreterError::MissingAllocationManifest)?;
        let (memory, pointer) = self
            .memory
            .allocate_static(site, layout, InitialState::Zeroed)
            .map_err(|e| self.memory_error(e, span))?;
        self.memory = memory
            .finish_static_initializer(pointer)
            .map_err(|e| self.memory_error(e, span))?;
        Ok(pointer)
    }
}
