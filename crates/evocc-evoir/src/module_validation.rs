//! EvoIR module validation.

use super::*;

pub fn validate_module(module: &Module) -> Result<(), String> {
    if module
        .extern_objects
        .iter()
        .any(|object| object.visibility == GlobalVisibility::Internal)
        || module
            .declares
            .iter()
            .any(|function| function.visibility == GlobalVisibility::Internal)
    {
        return Err("an external declaration cannot have internal linkage".into());
    }
    let mut functions = HashMap::new();
    let mut symbols = HashSet::new();
    for name in module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.declares.iter().map(|f| f.name.as_str()))
        .chain(module.globals.iter().map(|g| g.symbol.as_str()))
        .chain(module.extern_objects.iter().map(|g| g.symbol.as_str()))
        .chain(module.aliases.iter().map(|alias| alias.name.as_str()))
    {
        if !symbols.insert(name) {
            return Err(format!("E3041: symbol-declared-and-defined: '{name}'"));
        }
    }
    for (name, visibility, linkage) in module
        .functions
        .iter()
        .map(|f| (&f.name, f.visibility, f.linkage))
        .chain(
            module
                .globals
                .iter()
                .map(|g| (&g.symbol, g.visibility, g.linkage)),
        )
        .chain(
            module
                .aliases
                .iter()
                .map(|a| (&a.name, a.visibility, a.linkage)),
        )
    {
        if visibility == GlobalVisibility::Internal && linkage == Linkage::Weak {
            return Err(format!("E3060: weak-internal-symbol: '{name}'"));
        }
    }
    let definitions = module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.globals.iter().map(|g| g.symbol.as_str()))
        .collect::<HashSet<_>>();
    for alias in &module.aliases {
        if !definitions.contains(alias.target.as_str()) {
            return Err(format!(
                "E3061: alias-target-not-defined: '{}'",
                alias.target
            ));
        }
    }
    for function in &module.functions {
        if functions.insert(function.name.as_str(), function).is_some() {
            return Err(format!("duplicate function symbol '{}'", function.name));
        }
        validate_record_marks(
            &function
                .params
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect::<Vec<_>>(),
            &function.param_marks,
        )
        .map_err(|message| format!("function '{}': {message}", function.name))?;
        validate_ssa(function)
            .map_err(|message| format!("function '{}': {message}", function.name))?;
        let mut taken = address_taken_blocks(function);
        for symbol in module
            .globals
            .iter()
            .flat_map(|global| &global.relocs)
            .flat_map(Relocation::symbols)
        {
            if let Some((owner, label)) = symbol.split_once("^b") {
                if owner == function.name {
                    if let Ok(block) = label.parse::<BlockId>() {
                        taken.insert(block);
                    }
                }
            }
        }
        validate_indirect_destinations(function, &taken, true)?;
    }
    for declaration in &module.declares {
        validate_record_marks(&declaration.params, &declaration.param_marks)
            .map_err(|message| format!("declare '{}': {message}", declaration.name))?;
    }
    let mut registrations = HashSet::new();
    for r in &module.lifecycle {
        let function = functions.get(r.function.as_str()).ok_or_else(|| {
            format!(
                "lifecycle registration names undefined function '{}'",
                r.function
            )
        })?;
        let effects = [EvoIRType::Mem(Region::All), EvoIRType::World];
        if r.priority < 101
            || function.variadic
            || function.params.iter().map(|p| &p.ty).ne(effects.iter())
            || function.ret_tys != effects
            || !registrations.insert((r.function.as_str(), r.phase == LifecyclePhase::Constructor))
        {
            return Err(format!(
                "invalid lifecycle registration for '{}'",
                r.function
            ));
        }
    }
    let declarations = module
        .declares
        .iter()
        .map(FunctionDeclaration::signature)
        .collect::<Vec<_>>();
    for declaration in &declarations {
        for ty in declaration
            .params
            .iter()
            .map(|parameter| &parameter.ty)
            .chain(&declaration.ret_tys)
        {
            validate_value_type(ty)?;
        }
        callable_signature(declaration)?;
        functions.insert(declaration.name.as_str(), declaration);
    }
    for alias in &module.aliases {
        if let Some(function) = functions.get(alias.target.as_str()).copied() {
            functions.insert(alias.name.as_str(), function);
        }
    }
    let mut externs = HashSet::new();
    for object in &module.extern_objects {
        if !object.align.is_power_of_two() {
            return Err(format!(
                "external object '{}' has invalid alignment",
                object.symbol
            ));
        }
        externs.insert(object.symbol.as_str());
    }

    let mut globals = HashMap::new();
    let mut tls_symbols = HashSet::new();
    let declared_tls: HashSet<_> = module.tls.iter().map(|t| t.symbol.as_str()).collect();
    for symbol in &module.tls {
        if !tls_symbols.insert(symbol.symbol.as_str())
            || !(module.globals.iter().any(|g| {
                g.symbol == symbol.symbol
                    && g.kind != GlobalKind::StringLiteral
                    && g.initialized.iter().all(|b| *b)
            }) || externs.contains(symbol.symbol.as_str())
                || module
                    .aliases
                    .iter()
                    .any(|a| a.name == symbol.symbol && declared_tls.contains(a.target.as_str())))
        {
            return Err(format!("invalid TLS symbol '{}'", symbol.symbol));
        }
    }
    let mut origins = HashSet::new();
    for global in &module.globals {
        global.validate_section()?;
        validate_layout(global.layout, "module global", 0)?;
        let size = usize::try_from(global.layout.size_bytes)
            .map_err(|_| format!("global '{}' size exceeds host limits", global.symbol))?;
        if global.bytes.len() != size || global.initialized.len() != size {
            return Err(format!(
                "global '{}' bytes/initialized lengths must equal size {}",
                global.symbol, global.layout.size_bytes
            ));
        }
        let mut previous_end = 0;
        for relocation in &global.relocs {
            if tls_symbols.contains(module.symbol_target(&relocation.symbol)) {
                return Err("a TLS address is not a static relocation".into());
            }
            let code_target = relocation
                .symbol
                .split_once("^b")
                .and_then(|(name, block)| {
                    let id = block.parse::<BlockId>().ok()?;
                    (id.to_string() == block).then_some((name, id))
                });
            let valid_code_target = code_target.is_some_and(|(name, id)| {
                module.functions.iter().any(|function| {
                    function.name == name && function.blocks.iter().any(|block| block.id == id)
                })
            });
            if let Some(difference) = &relocation.difference {
                let valid_base = difference
                    .base
                    .split_once("^b")
                    .is_some_and(|(owner, label)| {
                        code_target.is_some_and(|(function, _)| function == owner)
                            && module.functions.iter().any(|function| {
                                function.name == owner
                                    && function
                                        .blocks
                                        .iter()
                                        .any(|block| block.id.to_string() == label)
                            })
                    });
                if !valid_code_target
                    || !valid_base
                    || relocation.exposed
                    || !matches!(difference.bits, 8 | 16 | 32 | 64)
                {
                    return Err("malformed relative block relocation".into());
                }
            }
            let end = relocation
                .offset
                .checked_add(relocation.size_bytes())
                .filter(|end| *end <= global.layout.size_bytes)
                .ok_or_else(|| {
                    format!(
                        "global '{}' relocation is outside its object",
                        global.symbol
                    )
                })?;
            if relocation.offset < previous_end
                || !(symbols.contains(relocation.symbol.as_str()) || valid_code_target)
                || (functions.contains_key(relocation.symbol.as_str()) && relocation.addend != 0)
                || global.bytes[relocation.offset as usize..end as usize]
                    .iter()
                    .any(|b| *b != 0)
                || global.initialized[relocation.offset as usize..end as usize]
                    .iter()
                    .any(|b| !*b)
            {
                return Err(format!(
                    "global '{}' has a malformed relocation",
                    global.symbol
                ));
            }
            previous_end = end;
        }
        if global.kind == GlobalKind::StringLiteral && global.initialized.iter().any(|bit| !*bit) {
            return Err(format!(
                "string-literal global '{}' must be fully initialized",
                global.symbol
            ));
        }
        match global.kind {
            GlobalKind::File => {
                if global.symbol.contains('.') {
                    return Err(format!(
                        "file global '{}' must use a source C identifier",
                        global.symbol
                    ));
                }
            }
            GlobalKind::FunctionStatic => {
                let expected = format!("__evocc.static.{}", global.origin);
                if global.symbol != expected || global.visibility != GlobalVisibility::Internal {
                    return Err(format!(
                        "function-static global '{}' must be internal symbol '{expected}'",
                        global.symbol
                    ));
                }
            }
            GlobalKind::StringLiteral => {
                let expected = format!("__evocc.string.{}", global.origin);
                if global.symbol != expected || global.visibility != GlobalVisibility::Internal {
                    return Err(format!(
                        "string-literal global '{}' must be internal symbol '{expected}'",
                        global.symbol
                    ));
                }
            }
            GlobalKind::CompoundLiteral => {
                let expected = format!("__evocc.compound.{}", global.origin);
                if global.symbol != expected || global.visibility != GlobalVisibility::Internal {
                    return Err(format!(
                        "compound-literal global '{}' must be internal symbol '{expected}'",
                        global.symbol
                    ));
                }
            }
        }
        if functions.contains_key(global.symbol.as_str()) {
            return Err(format!(
                "symbol '{}' is both a function and global",
                global.symbol
            ));
        }
        if globals.insert(global.symbol.as_str(), global).is_some() {
            return Err(format!("duplicate global symbol '{}'", global.symbol));
        }
        if !origins.insert(global.origin) {
            return Err(format!("duplicate allocation origin {}", global.origin));
        }
    }

    for alias in &module.aliases {
        if let Some(global) = globals.get(alias.target.as_str()).copied() {
            globals.insert(alias.name.as_str(), global);
        }
    }
    let mut effect_participants = HashSet::new();
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in block.insts.iter().chain(std::iter::once(&block.terminator)) {
                match instruction {
                    Inst::Call { callee, .. } => {
                        effect_participants.insert(function.name.as_str());
                        effect_participants.insert(module.symbol_target(callee));
                    }
                    Inst::IndirectCall { .. } => {
                        effect_participants.insert(function.name.as_str());
                    }
                    Inst::FuncAddr { symbol, .. } => {
                        effect_participants.insert(module.symbol_target(symbol));
                    }
                    Inst::Load { .. }
                    | Inst::Asm(_)
                    | Inst::AsmGoto { .. }
                    | Inst::StrictFloating(_)
                    | Inst::VaAccess(_)
                    | Inst::Atomic(_)
                    | Inst::TlsAddr(_)
                    | Inst::Store { .. }
                    | Inst::Copy { .. }
                    | Inst::Alloca { .. }
                    | Inst::DynamicAlloca { .. }
                    | Inst::LifetimeEnd { .. }
                    | Inst::PointerValid { .. }
                    | Inst::AccessInBounds { .. } => {
                        effect_participants.insert(function.name.as_str());
                    }
                    _ => {}
                }
            }
        }
    }
    for function in &module.functions {
        if effect_participants.contains(function.name.as_str()) {
            callable_signature(function)
                .map_err(|message| format!("call/effect participant: {message}"))?;
        }
    }

    for caller in &module.functions {
        for block in &caller.blocks {
            for (index, instruction) in block.insts.iter().enumerate() {
                match instruction {
                    Inst::TlsAddr(a) if !tls_symbols.contains(a.symbol.as_str()) => {
                        return Err(format!("tls_addr names non-TLS object '{}'", a.symbol));
                    }
                    Inst::GlobalAddr { symbol, .. }
                        if tls_symbols.contains(module.symbol_target(symbol)) =>
                    {
                        return Err(format!("global_addr cannot name TLS object '{symbol}'"));
                    }
                    Inst::GlobalAddr { symbol, .. }
                        if !globals.contains_key(symbol.as_str())
                            && !externs.contains(symbol.as_str()) =>
                    {
                        return Err(format!(
                            "function '{}' block {} instruction {} refers to missing global '{}'",
                            caller.name, block.id, index, symbol
                        ));
                    }
                    Inst::GlobalAddr { symbol, .. } if functions.contains_key(symbol.as_str()) => {
                        return Err(format!(
                            "function '{}' block {} instruction {} uses function '{}' as global",
                            caller.name, block.id, index, symbol
                        ));
                    }
                    Inst::FuncAddr { result, symbol } => {
                        let target = functions.get(symbol.as_str()).ok_or_else(|| {
                            format!(
                                "function '{}' block {} instruction {} refers to missing function '{}'",
                                caller.name, block.id, index, symbol
                            )
                        })?;
                        let expected = function_pointer_type(target)?;
                        if result.ty != expected {
                            return Err(format!(
                                "function '{}' block {} instruction {} func_addr type {:?} disagrees with '{}' signature {:?}",
                                caller.name, block.id, index, result.ty, symbol, expected
                            ));
                        }
                    }
                    Inst::BlockAddr { block: target, .. }
                        if !caller
                            .blocks
                            .iter()
                            .any(|candidate| candidate.id == *target) =>
                    {
                        return Err(format!(
                            "function '{}' block {} instruction {} takes the address of absent block {}",
                            caller.name, block.id, index, target
                        ));
                    }
                    Inst::Alloca { origin, .. } | Inst::DynamicAlloca { origin, .. }
                        if !origins.insert(*origin) =>
                    {
                        return Err(format!("duplicate allocation origin {origin}"));
                    }
                    _ => {}
                }
                // The returns-twice mark changes no signature rule. It selects
                // the top effect summary, which the last check below compares.
                let Inst::Call {
                    results,
                    callee,
                    args,
                    arg_marks,
                    returns_twice: _,
                    mem_in,
                    world_in,
                    mem_out,
                    world_out,
                } = instruction
                else {
                    continue;
                };
                let target = functions.get(callee.as_str()).ok_or_else(|| {
                    format!(
                        "function '{}' block {} instruction {} calls missing callee '{}'",
                        caller.name, block.id, index, callee
                    )
                })?;
                variadic::validate_anonymous_marks(
                    args,
                    arg_marks,
                    target.params.len().saturating_sub(2),
                )?;
                validate_call_signature(
                    caller, block.id, index, target, results, args, mem_in, world_in, mem_out,
                    world_out,
                )?;
            }
        }
    }
    for (function, expected) in module.functions.iter().zip(derive_effect_summaries(module)) {
        if function.effects != expected {
            return Err(format!(
                "function '{}' recorded effect summary {:?}, expected exact Section 4.6 summary {:?}",
                function.name, function.effects, expected
            ));
        }
    }
    Ok(())
}

pub(super) fn function_pointer_type(function: &Function) -> Result<EvoIRType, String> {
    callable_signature(function)?;
    Ok(EvoIRType::FnPtr {
        variadic: function.variadic,
        params: function
            .params
            .iter()
            .map(|parameter| parameter.ty.clone())
            .collect(),
        results: function.ret_tys.clone(),
        // Revision 1.9.73: "the attribute is part of the signature for
        // equality", so the pointer a function's address produces carries the
        // function's marks and compares unequal to an unmarked signature.
        marks: function.param_marks.clone(),
    })
}

pub(super) fn callable_signature(function: &Function) -> Result<CallableSignature<'_>, String> {
    if function.params.len() < 2 || function.ret_tys.len() < 2 {
        return Err(format!(
            "function '{}' lacks trailing mem<R>, world parameters and returns",
            function.name
        ));
    }
    let parameter_split = function.params.len() - 2;
    let return_split = function.ret_tys.len() - 2;
    let memory_region = match (
        &function.params[parameter_split].ty,
        &function.params[parameter_split + 1].ty,
    ) {
        (EvoIRType::Mem(region), EvoIRType::World) => *region,
        _ => {
            return Err(format!(
                "function '{}' must end parameters with mem<R>, world",
                function.name
            ));
        }
    };
    match (
        &function.ret_tys[return_split],
        &function.ret_tys[return_split + 1],
    ) {
        (EvoIRType::Mem(region), EvoIRType::World) if *region == memory_region => {}
        _ => {
            return Err(format!(
                "function '{}' must end returns with mem<{:?}>, world",
                function.name, memory_region
            ));
        }
    }
    for parameter in &function.params[..parameter_split] {
        if contains_effect_token(&parameter.ty)? {
            return Err(format!(
                "function '{}' ordinary parameter '{}' contains an effect token",
                function.name, parameter.name
            ));
        }
    }
    for (index, ty) in function.ret_tys[..return_split].iter().enumerate() {
        if contains_effect_token(ty)? {
            return Err(format!(
                "function '{}' ordinary return {} contains an effect token",
                function.name, index
            ));
        }
    }
    Ok(CallableSignature {
        ordinary_params: &function.params[..parameter_split],
        ordinary_returns: &function.ret_tys[..return_split],
        memory_region,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_call_signature(
    caller: &Function,
    block: BlockId,
    instruction: usize,
    callee: &Function,
    results: &[Value],
    args: &[Value],
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
) -> Result<(), String> {
    let signature = callable_signature(callee).map_err(|message| {
        format!(
            "function '{}' block {} instruction {}: {message}",
            caller.name, block, instruction
        )
    })?;
    if args.len() < signature.ordinary_params.len()
        || (!callee.variadic && args.len() != signature.ordinary_params.len())
    {
        return Err(format!(
            "call from '{}' to '{}' has {} ordinary arguments, expected {}",
            caller.name,
            callee.name,
            args.len(),
            signature.ordinary_params.len()
        ));
    }
    for argument in &args[signature.ordinary_params.len()..] {
        if !variadic::promoted_type(&argument.ty) {
            return Err("anonymous argument is not a promoted scalar type".into());
        }
    }
    for (index, (argument, parameter)) in args.iter().zip(signature.ordinary_params).enumerate() {
        if argument.ty != parameter.ty {
            return Err(format!(
                "call from '{}' to '{}' argument {} has type {:?}, expected {:?}",
                caller.name, callee.name, index, argument.ty, parameter.ty
            ));
        }
    }
    if results.len() != signature.ordinary_returns.len() {
        return Err(format!(
            "call from '{}' to '{}' has {} ordinary results, expected {}",
            caller.name,
            callee.name,
            results.len(),
            signature.ordinary_returns.len()
        ));
    }
    for (index, (result, expected)) in results.iter().zip(signature.ordinary_returns).enumerate() {
        if &result.ty != expected {
            return Err(format!(
                "call from '{}' to '{}' result {} has type {:?}, expected {:?}",
                caller.name, callee.name, index, result.ty, expected
            ));
        }
    }
    let expected_memory = EvoIRType::Mem(signature.memory_region);
    if mem_in.ty != expected_memory || mem_out.ty != expected_memory {
        return Err(format!(
            "call from '{}' to '{}' must thread memory token {:?}",
            caller.name, callee.name, expected_memory
        ));
    }
    if world_in.ty != EvoIRType::World || world_out.ty != EvoIRType::World {
        return Err(format!(
            "call from '{}' to '{}' must thread a world token",
            caller.name, callee.name
        ));
    }
    Ok(())
}

pub(super) fn validate_embedded_instruction_types(
    inst: &Inst,
    index: usize,
    block: BlockId,
) -> Result<(), String> {
    let target = match inst {
        Inst::SExt(_, _, target) | Inst::ZExt(_, _, target) | Inst::Trunc(_, _, target) => {
            Some(target)
        }
        _ => None,
    };
    if let Some(target) = target {
        ensure_value_type(
            target,
            &format!("target type of instruction {index} in block {block}"),
        )?;
    }
    Ok(())
}
