use super::*;

/// A program invocation initializes one load instance. Function invocation
/// APIs deliberately do not run registrations.
pub fn interpret_program(
    unit: &CHirUnit<'_>,
    main: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
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
        return run_program(&resolved, main, target, limits);
    }
    run_program(unit, main, target, limits)
}

fn run_program(
    unit: &CHirUnit<'_>,
    main: SymbolId,
    target: &TargetDataLayout,
    limits: InterpreterLimits,
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
    for r in &registrations {
        let extra = reachable_memory_requirements(unit, r.function, linkage);
        requirements.requires_memory |= extra.requires_memory;
        requirements.static_objects.extend(extra.static_objects);
    }
    let mut machine = Interpreter::new(unit, target, limits, requirements.requires_memory)?;
    machine.program = true;
    machine.validate_root_function(main)?;
    for r in &registrations {
        machine.validate_root_function(r.function)?;
    }
    if requirements.requires_memory {
        machine.derive_allocation_manifest()?;
        machine.initialize_static_objects(&requirements.static_objects)?;
    }
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
            .run_root(main)?
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
