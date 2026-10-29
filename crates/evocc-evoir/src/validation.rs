//! EvoIR validation.

use super::*;

/// Validate block-argument SSA without inferring cross-block dominance.
///
/// Function parameters are interface binders, not SSA definitions.  The first
/// block must therefore declare one block argument of the same type for every
/// parameter.  Within every block, an operand may name only a block argument
/// or an instruction result defined earlier in that same block.  Values that
/// cross a CFG edge must be passed explicitly to the destination arguments.
pub fn validate_ssa(func: &Function) -> Result<(), String> {
    if func
        .section
        .as_ref()
        .is_some_and(|name| name.is_empty() || name.chars().any(char::is_control))
    {
        return Err(format!("function '{}' has invalid section name", func.name));
    }
    if func
        .patchable_entry
        .is_some_and(|(total, before)| before > total)
    {
        return Err(format!(
            "function '{}' has invalid patchable entry",
            func.name
        ));
    }
    if func.alignment < 4 || !func.alignment.is_power_of_two() {
        return Err(format!(
            "function '{}' has invalid alignment {}",
            func.name, func.alignment
        ));
    }
    if func.variadic {
        callable_signature(func)?;
    }
    let Some(entry) = func.blocks.first() else {
        return Err(format!("function '{}' has no entry block", func.name));
    };

    let mut blocks = HashMap::new();
    for block in &func.blocks {
        if blocks.insert(block.id, block).is_some() {
            return Err(format!("duplicate block id {}", block.id));
        }
    }

    if entry.args.len() != func.params.len() {
        return Err(format!(
            "entry block {} has {} arguments but function '{}' has {} parameters",
            entry.id,
            entry.args.len(),
            func.name,
            func.params.len()
        ));
    }
    for (index, (param, arg)) in func.params.iter().zip(&entry.args).enumerate() {
        ensure_value_type(&param.ty, &format!("parameter {index}"))?;
        ensure_value_type(&arg.value.ty, &format!("entry argument {index}"))?;
        if param.ty != arg.value.ty {
            return Err(format!(
                "entry argument {} has type {:?}, expected parameter type {:?}",
                index, arg.value.ty, param.ty
            ));
        }
    }
    for (index, ty) in func.ret_tys.iter().enumerate() {
        ensure_value_type(ty, &format!("return type {index}"))?;
    }

    let mut definitions: HashMap<u32, String> = HashMap::new();
    for block in &func.blocks {
        let mut names = HashSet::new();
        for arg in &block.args {
            ensure_value_type(
                &arg.value.ty,
                &format!("argument '{}' in block {}", arg.name, block.id),
            )?;
            if !names.insert(arg.name.as_str()) {
                return Err(format!(
                    "duplicate block argument name '{}' in block {}",
                    arg.name, block.id
                ));
            }
            register_definition(
                &mut definitions,
                &arg.value,
                format!("block argument '{}' in block {}", arg.name, block.id),
            )?;
        }
    }

    for block in &func.blocks {
        let mut local: HashMap<u32, EvoIRType> = block
            .args
            .iter()
            .map(|arg| (arg.value.id, arg.value.ty.clone()))
            .collect();
        let mut effects = EffectState::from_block_args(block)?;
        let mut access_guards: HashMap<u32, (u32, AccessLayout)> = HashMap::new();
        let mut static_pointers = HashSet::new();

        for (index, inst) in block.insts.iter().enumerate() {
            if is_terminator(inst) {
                return Err(format!(
                    "terminator appears in instruction {} of block {}",
                    index, block.id
                ));
            }
            for result in inst_results(inst) {
                ensure_value_type(
                    &result.ty,
                    &format!(
                        "result v{} of instruction {} in block {}",
                        result.id, index, block.id
                    ),
                )?;
            }
            validate_embedded_instruction_types(inst, index, block.id)?;
            for used in inst_uses(inst) {
                ensure_value_type(
                    &used.ty,
                    &format!(
                        "use of v{} in instruction {} of block {}",
                        used.id, index, block.id
                    ),
                )?;
                require_local_value(&local, used, block.id, "instruction")?;
            }
            validate_instruction_types(inst, block.id)?;
            // These definitions preserve a statically named object's origin.
            // Unknown pointers still need the runtime lifetime check; a cast
            // or an offset cannot turn a known static object into an alloca.
            match inst {
                Inst::GlobalAddr { result, .. } => {
                    static_pointers.insert(result.id);
                }
                Inst::TlsAddr(access) => {
                    static_pointers.insert(access.result.id);
                }
                Inst::PointerCast { result, operand }
                | Inst::PtrOffset {
                    result,
                    base: operand,
                    ..
                } if static_pointers.contains(&operand.id) => {
                    static_pointers.insert(result.id);
                }
                Inst::LifetimeEnd { pointer, .. } if static_pointers.contains(&pointer.id) => {
                    return Err(format!(
                        "lifetime.end in block {} cannot end a static object",
                        block.id
                    ));
                }
                _ => {}
            }
            if matches!(
                inst,
                Inst::VaAccess(VariadicAccess {
                    op: VariadicOp::Start,
                    ..
                })
            ) && !func.variadic
            {
                return Err("E3037: va-start-outside-variadic-signature".into());
            }
            validate_memory_contract(
                inst,
                block.insts.get(index + 1),
                block.id,
                &effects,
                &mut access_guards,
            )?;
            effects.advance(inst, block.id)?;
            for result in inst_results(inst) {
                register_definition(
                    &mut definitions,
                    result,
                    format!("instruction {} in block {}", index, block.id),
                )?;
                local.insert(result.id, result.ty.clone());
            }
        }

        if !is_terminator(&block.terminator) {
            return Err(format!("block {} ends with a non-terminator", block.id));
        }
        if let Inst::AsmGoto {
            assembly, labels, ..
        } = &block.terminator
        {
            for used in assembly.operands() {
                ensure_value_type(&used.ty, "assembly input")?;
                require_local_value(&local, used, block.id, "assembly input")?;
            }
            assembly.validate(labels)?;
            effects.advance(&block.terminator, block.id)?;
            for result in assembly.results() {
                ensure_value_type(&result.ty, "assembly result")?;
                register_definition(
                    &mut definitions,
                    result,
                    format!("assembly terminator in block {}", block.id),
                )?;
                local.insert(result.id, result.ty.clone());
            }
        }
        for used in inst_uses(&block.terminator) {
            ensure_value_type(
                &used.ty,
                &format!("terminator use of v{} in block {}", used.id, block.id),
            )?;
            require_local_value(&local, used, block.id, "terminator")?;
        }

        match &block.terminator {
            Inst::IndirectBr { target, edges } => {
                if target.ty != EvoIRType::Ptr(AddressSpace::Generic) {
                    return Err("indirect branch target must be ptr<generic>".into());
                }
                if edges.is_empty()
                    || edges
                        .windows(2)
                        .any(|pair| pair[0].target >= pair[1].target)
                {
                    return Err("indirect successors must be nonempty, unique and sorted".into());
                }
                for edge in edges {
                    validate_edge(&blocks, block.id, edge.target, &edge.arguments)?;
                    validate_effect_edge(
                        &blocks,
                        block.id,
                        edge.target,
                        &edge.arguments,
                        &effects,
                    )?;
                }
            }
            Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            } => {
                for edge in labels.iter().map(|label| &label.edge).chain([fallthrough]) {
                    validate_edge(&blocks, block.id, edge.target, &edge.arguments)?;
                    validate_effect_edge(
                        &blocks,
                        block.id,
                        edge.target,
                        &edge.arguments,
                        &effects,
                    )?;
                }
            }
            Inst::Br(target, args) => {
                validate_edge(&blocks, block.id, *target, args)?;
                validate_effect_edge(&blocks, block.id, *target, args, &effects)?;
            }
            Inst::CondBr(cond, true_block, true_args, false_block, false_args) => {
                if cond.ty != EvoIRType::Bits(1) {
                    return Err(format!(
                        "conditional branch in block {} uses {:?}; expected bits<1>",
                        block.id, cond.ty
                    ));
                }
                validate_edge(&blocks, block.id, *true_block, true_args)?;
                validate_edge(&blocks, block.id, *false_block, false_args)?;
                validate_effect_edge(&blocks, block.id, *true_block, true_args, &effects)?;
                validate_effect_edge(&blocks, block.id, *false_block, false_args, &effects)?;
            }
            Inst::Ret(values) => {
                if values.len() != func.ret_tys.len() {
                    return Err(format!(
                        "return in block {} has {} values, expected {}",
                        block.id,
                        values.len(),
                        func.ret_tys.len()
                    ));
                }
                for (index, (value, expected)) in values.iter().zip(&func.ret_tys).enumerate() {
                    if &value.ty != expected {
                        return Err(format!(
                            "return value {} in block {} has type {:?}, expected {:?}",
                            index, block.id, value.ty, expected
                        ));
                    }
                }
                validate_effect_boundary(
                    &func.ret_tys,
                    values,
                    &effects,
                    &format!("return in block {}", block.id),
                )?;
            }
            Inst::Trap(_) => {}
            _ => return Err(format!("block {} has an invalid terminator", block.id)),
        }
    }

    validate_indirect_destinations(func, &address_taken_blocks(func), false)?;
    validate_reachability(func, entry.id)?;

    Ok(())
}

pub(super) fn address_taken_blocks(function: &Function) -> HashSet<BlockId> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter_map(|inst| {
            if let Inst::BlockAddr { block, .. } = inst {
                Some(*block)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn validate_indirect_destinations(
    function: &Function,
    required: &HashSet<BlockId>,
    exact: bool,
) -> Result<(), String> {
    for block in &function.blocks {
        if let Inst::IndirectBr { edges, .. } = &block.terminator {
            let actual: HashSet<_> = edges.iter().map(|edge| edge.target).collect();
            if !required.is_subset(&actual) || (exact && &actual != required) {
                return Err(format!("indirect branch in block {} has an incomplete or foreign address-taken successor set", block.id));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_memory_contract(
    inst: &Inst,
    next: Option<&Inst>,
    block: BlockId,
    effects: &EffectState,
    access_guards: &mut HashMap<u32, (u32, AccessLayout)>,
) -> Result<(), String> {
    match inst {
        Inst::PtrOffset { result, .. } => {
            let Some(Inst::PointerValid {
                mem_in, pointer, ..
            }) = next
            else {
                return Err(format!(
                    "ptr.offset result v{} in block {block} must be immediately followed by pointer_valid",
                    result.id
                ));
            };
            if pointer.id != result.id || effects.mem[region_index(Region::All)] != Some(mem_in.id)
            {
                return Err(format!(
                    "ptr.offset result v{} in block {block} has a mismatched pointer_valid obligation",
                    result.id
                ));
            }
        }
        Inst::AccessInBounds {
            pointer,
            mem_out,
            layout,
            ..
        } => {
            access_guards.insert(mem_out.id, (pointer.id, *layout));
        }
        Inst::Atomic(access) => {
            let layout = access.layout()?;
            if access_guards.get(&access.mem_in.id) != Some(&(access.address.id, layout)) {
                return Err(format!(
                    "E3062: invalid-atomic-access: natural alignment guard in block {block}"
                ));
            }
        }
        Inst::Load {
            result,
            address,
            mem_in,
            ..
        } => validate_matching_access_guard(
            access_guards,
            mem_in,
            address,
            evoir_access_layout(&result.ty)?,
            "load",
            block,
        )?,
        Inst::Store {
            address,
            value,
            mem_in,
            ..
        } => validate_matching_access_guard(
            access_guards,
            mem_in,
            address,
            evoir_access_layout(&value.ty)?,
            "store",
            block,
        )?,
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_matching_access_guard(
    access_guards: &HashMap<u32, (u32, AccessLayout)>,
    mem_in: &Value,
    address: &Value,
    expected: Option<AccessLayout>,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    match access_guards.get(&mem_in.id) {
        Some((pointer, layout))
            if *pointer == address.id && expected.is_none_or(|expected|
                layout.size_bytes == expected.size_bytes
                && layout.align_bytes.is_power_of_two()
                && layout.align_bytes <= expected.align_bytes) =>
        {
            Ok(())
        }
        _ => Err(format!(
            "{operation} in block {block} must consume a matching access_in_bounds token for v{} with {expected:?}",
            address.id
        )),
    }
}

/// Target-profile scalar representation layout, also used for compiler-created storage.
pub fn evoir_access_layout(ty: &EvoIRType) -> Result<Option<AccessLayout>, String> {
    let size = match ty {
        EvoIRType::Vec(..) => {
            let (lanes, width) = vector_shape(ty).ok_or("unsupported vector memory type")?;
            u64::from(lanes) * u64::from(width / 8)
        }
        EvoIRType::Bits(width) if *width > 0 => u64::from(width.div_ceil(8)),
        EvoIRType::F16 | EvoIRType::BF16 => 2,
        EvoIRType::F32 => 4,
        EvoIRType::F64 => 8,
        EvoIRType::F128 => 16,
        // Both admitted AArch64 profiles use eight-byte pointers aligned to
        // eight bytes. Packed members may lower the alignment, not the size.
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => 8,
        _ => {
            return Err(format!(
                "Day 7 memory type {ty:?} has no scalar access layout"
            ))
        }
    };
    // EvoIR 10.1.19: a `bits<128>` is accessed "at 16 bytes and alignment 16",
    // so the natural alignment is capped at the widest executed scalar rather
    // than at a register. Every narrower width is unchanged, its size being at
    // most eight bytes already.
    let align = u32::try_from(size.min(16)).map_err(|_| "invalid access alignment".to_owned())?;
    Ok(Some(AccessLayout::new(size, align)))
}

/// Validate every function and all inter-function call signatures.
/// Section 10.1.3 (revision 1.9.73): a `byval` or `sret` mark rides on a
/// `ptr<generic>`, a signature carries at most one `sret`, and the mark list is
/// canonical, so it is empty or exactly as long as the parameter list.
///
/// The size and alignment are a record type's; size may be zero, while the
/// alignment is a power of two. Fixed arguments execute the pointer form;
/// anonymous arguments also retain the mark as the abstract va_arg type.
pub fn validate_record_marks(params: &[EvoIRType], marks: &[ParamMark]) -> Result<(), String> {
    if marks.is_empty() {
        return Ok(());
    }
    if marks.len() != params.len() {
        return Err("a record mark list must be as long as the parameter list".to_owned());
    }
    if marks.iter().all(|mark| matches!(mark, ParamMark::None)) {
        return Err("a record mark list with no mark is spelled by its absence".to_owned());
    }
    let mut destinations = 0_usize;
    for (index, mark) in marks.iter().enumerate() {
        if *mark == ParamMark::SignExtend {
            if !matches!(params[index], EvoIRType::Bits(2..=31)) {
                return Err(format!(
                    "parameter {index} carries sext but is not an integer narrower than 32 bits"
                ));
            }
            continue;
        }
        let Some(shape) = mark.shape() else {
            continue;
        };
        if params[index] != EvoIRType::Ptr(AddressSpace::Generic) {
            return Err(format!(
                "parameter {index} carries a record mark but is not ptr<generic>"
            ));
        }
        if !shape.valid() {
            return Err(format!(
                "parameter {index} marks a record whose alignment {} is not a power of two",
                shape.align
            ));
        }
        if mark.is_sret() {
            destinations += 1;
        }
    }
    if destinations > 1 {
        return Err("a signature carries at most one sret parameter".to_owned());
    }
    Ok(())
}

pub(super) fn register_definition(
    definitions: &mut HashMap<u32, String>,
    value: &Value,
    site: String,
) -> Result<(), String> {
    if let Some(previous) = definitions.insert(value.id, site.clone()) {
        return Err(format!(
            "value v{} defined more than once (previous: {}; duplicate: {})",
            value.id, previous, site
        ));
    }
    Ok(())
}

pub(super) fn require_local_value(
    local: &HashMap<u32, EvoIRType>,
    value: &Value,
    block: BlockId,
    context: &str,
) -> Result<(), String> {
    let Some(defined_ty) = local.get(&value.id) else {
        return Err(format!(
            "value v{} used by {} in block {} is not a prior local definition; thread it through a block argument",
            value.id, context, block
        ));
    };
    if defined_ty != &value.ty {
        return Err(format!(
            "value v{} used by {} in block {} has type {:?}, but its local definition has type {:?}",
            value.id, context, block, value.ty, defined_ty
        ));
    }
    Ok(())
}

pub(super) fn is_terminator(inst: &Inst) -> bool {
    matches!(
        inst,
        Inst::Br(..)
            | Inst::CondBr(..)
            | Inst::IndirectBr { .. }
            | Inst::Ret(..)
            | Inst::Trap(..)
            | Inst::AsmGoto { .. }
    )
}

pub(super) fn validate_edge(
    blocks: &HashMap<BlockId, &Block>,
    source: BlockId,
    target: BlockId,
    values: &[Value],
) -> Result<(), String> {
    let Some(destination) = blocks.get(&target) else {
        return Err(format!(
            "branch in block {} targets missing block {}",
            source, target
        ));
    };
    if values.len() != destination.args.len() {
        return Err(format!(
            "edge {} -> {} passes {} values, expected {}",
            source,
            target,
            values.len(),
            destination.args.len()
        ));
    }
    for (index, (value, arg)) in values.iter().zip(&destination.args).enumerate() {
        if value.ty != arg.value.ty {
            return Err(format!(
                "edge {} -> {} value {} has type {:?}, expected {:?}",
                source, target, index, value.ty, arg.value.ty
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_effect_edge(
    blocks: &HashMap<BlockId, &Block>,
    source: BlockId,
    target: BlockId,
    values: &[Value],
    effects: &EffectState,
) -> Result<(), String> {
    let destination = blocks.get(&target).ok_or_else(|| {
        format!(
            "branch in block {} targets missing block {}",
            source, target
        )
    })?;
    let types: Vec<EvoIRType> = destination
        .args
        .iter()
        .map(|arg| arg.value.ty.clone())
        .collect();
    validate_effect_boundary(
        &types,
        values,
        effects,
        &format!("edge {} -> {}", source, target),
    )
}

pub(super) fn validate_effect_boundary(
    types: &[EvoIRType],
    values: &[Value],
    effects: &EffectState,
    context: &str,
) -> Result<(), String> {
    let mut memory_positions: [Vec<usize>; REGION_ORDER.len()] = std::array::from_fn(|_| vec![]);
    for (index, ty) in types.iter().enumerate() {
        if let EvoIRType::Mem(region) = ty {
            memory_positions[region_index(*region)].push(index);
        }
    }
    for (region_index, region) in REGION_ORDER.iter().copied().enumerate() {
        let positions = &memory_positions[region_index];
        if positions.len() > 1 {
            return Err(format!(
                "{} passes mem<{:?}> more than once",
                context, region
            ));
        }
        match (effects.mem[region_index], positions.as_slice()) {
            (None, []) => {}
            (None, _) => {
                return Err(format!(
                    "{} introduces mem<{:?}> without a current token",
                    context, region
                ));
            }
            (Some(_), []) => {
                return Err(format!("{} drops current mem<{:?}> token", context, region));
            }
            (Some(expected), [position]) if values[*position].id == expected => {}
            (Some(expected), [position]) => {
                return Err(format!(
                    "{} passes stale mem<{:?}> token v{}; current token is v{}",
                    context, region, values[*position].id, expected
                ));
            }
            (Some(_), _) => {
                return Err(format!(
                    "{} passes mem<{:?}> more than once",
                    context, region
                ));
            }
        }
    }
    validate_one_effect_boundary(
        types,
        values,
        &EvoIRType::World,
        effects.world,
        "world",
        context,
    )
}

pub(super) fn validate_one_effect_boundary(
    types: &[EvoIRType],
    values: &[Value],
    token_type: &EvoIRType,
    current: Option<u32>,
    token_name: &str,
    context: &str,
) -> Result<(), String> {
    let positions: Vec<usize> = types
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| (ty == token_type).then_some(index))
        .collect();
    match (current, positions.as_slice()) {
        (None, []) => Ok(()),
        (None, _) => Err(format!(
            "{} introduces a {} token without a current token",
            context, token_name
        )),
        (Some(_), []) => Err(format!(
            "{} drops the current {} token",
            context, token_name
        )),
        (Some(expected), [position]) if values[*position].id == expected => Ok(()),
        (Some(expected), [position]) => Err(format!(
            "{} passes stale {} token v{}; current token is v{}",
            context, token_name, values[*position].id, expected
        )),
        (Some(_), _) => Err(format!(
            "{} passes the current {} token more than once",
            context, token_name
        )),
    }
}

pub(super) fn validate_reachability(func: &Function, entry: BlockId) -> Result<(), String> {
    let by_id: HashMap<BlockId, &Block> =
        func.blocks.iter().map(|block| (block.id, block)).collect();
    let mut reachable = HashSet::new();
    let mut pending = vec![entry];
    while let Some(block_id) = pending.pop() {
        if !reachable.insert(block_id) {
            continue;
        }
        let Some(block) = by_id.get(&block_id) else {
            return Err(format!("reachable block {} is missing", block_id));
        };
        match &block.terminator {
            Inst::IndirectBr { edges, .. } => pending.extend(edges.iter().map(|edge| edge.target)),
            Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            } => {
                pending.extend(labels.iter().map(|label| label.edge.target));
                pending.push(fallthrough.target);
            }
            Inst::Br(target, _) => pending.push(*target),
            Inst::CondBr(_, true_block, _, false_block, _) => {
                pending.push(*true_block);
                pending.push(*false_block);
            }
            Inst::Ret(_) | Inst::Trap(_) => {}
            _ => {
                return Err(format!("block {} has an invalid terminator", block_id));
            }
        }
    }
    if let Some(block) = func
        .blocks
        .iter()
        .find(|block| !reachable.contains(&block.id))
    {
        return Err(format!(
            "block {} is unreachable from entry block {}",
            block.id, entry
        ));
    }
    Ok(())
}

pub(super) fn region_index(region: Region) -> usize {
    match region {
        Region::All => 0,
        Region::Unknown => 1,
        Region::Global => 2,
        Region::Stack => 3,
        Region::Heap => 4,
        Region::Escaped => 5,
    }
}

impl EffectState {
    fn from_block_args(block: &Block) -> Result<Self, String> {
        let mut state = Self::default();
        for arg in &block.args {
            match &arg.value.ty {
                EvoIRType::Mem(region) => {
                    let slot = &mut state.mem[region_index(*region)];
                    if slot.replace(arg.value.id).is_some() {
                        return Err(format!(
                            "block {} has more than one mem<{:?}> argument",
                            block.id, region
                        ));
                    }
                }
                EvoIRType::World => {
                    if let Some(previous) = state.world.replace(arg.value.id) {
                        return Err(format!(
                            "block {} has more than one world argument (previous token v{})",
                            block.id, previous
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(state)
    }

    fn advance(&mut self, inst: &Inst, block: BlockId) -> Result<(), String> {
        if let Inst::StrictFloating(operation) = inst {
            if self.world != Some(operation.world_in.id) {
                return Err(format!(
                    "strict floating effect in block {block} consumes stale world token"
                ));
            }
            self.world = Some(operation.world_out.id);
            return Ok(());
        }
        let tokens = match inst {
            Inst::TlsAddr(a) => Some((&a.mem_in, &a.world_in, &a.mem_out, &a.world_out)),
            Inst::Atomic(access) => Some((
                &access.mem_in,
                &access.world_in,
                &access.mem_out,
                &access.world_out,
            )),
            Inst::VaAccess(access) => Some((
                &access.mem_in,
                &access.world_in,
                &access.mem_out,
                &access.world_out,
            )),
            Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => Some((
                &assembly.mem_in,
                &assembly.world_in,
                &assembly.mem_out,
                &assembly.world_out,
            )),
            Inst::Load {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Store {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Copy {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Alloca {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::DynamicAlloca {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::LifetimeEnd {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::Call {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            }
            | Inst::IndirectCall {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            } => Some((mem_in, world_in, mem_out, world_out)),
            Inst::PointerValid {
                mem_in, mem_out, ..
            }
            | Inst::AccessInBounds {
                mem_in, mem_out, ..
            } => {
                if mem_in.ty != EvoIRType::Mem(Region::All)
                    || mem_out.ty != EvoIRType::Mem(Region::All)
                {
                    return Err(format!(
                        "memory guard in block {block} must thread mem<all>"
                    ));
                }
                let current = self.mem[region_index(Region::All)];
                if current != Some(mem_in.id) {
                    return Err(format!(
                        "memory guard in block {block} consumes stale mem<all> token v{}",
                        mem_in.id
                    ));
                }
                self.mem[region_index(Region::All)] = Some(mem_out.id);
                return Ok(());
            }
            _ => None,
        };
        let Some((mem_in, world_in, mem_out, world_out)) = tokens else {
            return Ok(());
        };
        let region = match (&mem_in.ty, &mem_out.ty) {
            (EvoIRType::Mem(input), EvoIRType::Mem(output)) if input == output => *input,
            _ => {
                return Err(format!(
                    "effect in block {} has invalid memory token types",
                    block
                ));
            }
        };
        let current_mem = self.mem[region_index(region)];
        if current_mem != Some(mem_in.id) {
            return Err(format!(
                "effect in block {} consumes stale mem<{:?}> token v{}; current token is {}",
                block,
                region,
                mem_in.id,
                current_mem.map_or_else(|| "absent".to_owned(), |id| format!("v{id}"))
            ));
        }
        if self.world != Some(world_in.id) {
            return Err(format!(
                "effect in block {} consumes stale world token v{}; current token is {}",
                block,
                world_in.id,
                self.world
                    .map_or_else(|| "absent".to_owned(), |id| format!("v{id}"))
            ));
        }
        self.mem[region_index(region)] = Some(mem_out.id);
        self.world = Some(world_out.id);
        Ok(())
    }
}
