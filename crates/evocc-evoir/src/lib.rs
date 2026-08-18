//! EvoIR: typed block-argument SSA for optimization
//!
//! Values carry an identity (`id`) and a type so that the SSA validator can
//! verify single-static-definition form before any optimization transforms.

use std::collections::{HashMap, HashSet};
use std::fmt;

use evocc_types::{EvoIRType, Region};

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A value in the IR with a unique identifier and its type.
///
/// Every instruction that produces a result carries a `Value` as its first
/// field.  Operands and block-argument bindings also use `Value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Value {
    pub id: u32,
    pub ty: EvoIRType,
}

/// A non-SSA function-interface binder.
///
/// Parameters deliberately have no [`Value`] identity.  The entry block owns
/// the SSA values corresponding to the interface, one argument per parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: String,
    pub ty: EvoIRType,
}

/// A named SSA value bound at block entry by the incoming edge (or by the
/// function interface for the entry block).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockArg {
    pub name: String,
    pub value: Value,
}

/// Opaque identifier for a basic block.
pub type BlockId = u32;

// ---------------------------------------------------------------------------
// Comparison operators
// ---------------------------------------------------------------------------

/// Integer comparison operators for `ICmp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    SLt,
    SLe,
    ULt,
    ULe,
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

/// An SSA instruction.
///
/// Pure instructions carry their result `Value` as their first field.
/// Effectful operations use named fields so their memory and world token flow
/// cannot be confused with ordinary call arguments or results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    /// `Const(result, bits)`: define `result` from a non-negative raw bit
    /// pattern. The result type determines how many low bits are retained.
    Const(Value, u64),

    /// `Add(result, lhs, rhs)`: integer addition.
    Add(Value, Value, Value),
    /// `Sub(result, lhs, rhs)`: integer subtraction.
    Sub(Value, Value, Value),
    /// `Mul(result, lhs, rhs)`: integer multiplication.
    Mul(Value, Value, Value),
    /// `SDiv(result, lhs, rhs)`: signed integer division.
    SDiv(Value, Value, Value),
    /// `UDiv(result, lhs, rhs)`: unsigned integer division.
    UDiv(Value, Value, Value),

    /// `ICmp(result, op, lhs, rhs)`: integer comparison.
    ICmp(Value, CmpOp, Value, Value),

    /// `Br(target, args)`: unconditional branch, passing `args` to the
    /// destination block's block arguments.
    Br(BlockId, Vec<Value>),

    /// `CondBr(cond, true_block, true_args, false_block, false_args)`
    /// conditional branch.
    CondBr(Value, BlockId, Vec<Value>, BlockId, Vec<Value>),

    /// `Ret(values)`: return from function with the given values.
    Ret(Vec<Value>),

    /// Load through `address`, consuming and producing explicit effect tokens.
    /// The loaded type is owned by `result`.
    Load {
        result: Value,
        address: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
    },

    /// Store `value` through `address`, consuming and producing effect tokens.
    Store {
        address: Value,
        value: Value,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
    },

    /// `PtrOffset(result, base, offset)`: pointer arithmetic.
    PtrOffset(Value, Value, Value),

    /// Call `callee`. Ordinary `args` and `results` exclude the separately
    /// threaded memory and world tokens.
    Call {
        results: Vec<Value>,
        callee: String,
        args: Vec<Value>,
        mem_in: Value,
        world_in: Value,
        mem_out: Value,
        world_out: Value,
    },

    /// `SExt(result, src, target_ty)`: sign-extend `src` to `target_ty`.
    SExt(Value, Value, EvoIRType),

    /// `ZExt(result, src, target_ty)`: zero-extend `src` to `target_ty`.
    ZExt(Value, Value, EvoIRType),

    /// `Trunc(result, src, target_ty)`: truncate `src` to `target_ty`.
    Trunc(Value, Value, EvoIRType),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns all SSA values defined by an instruction, in textual result order.
#[must_use]
pub fn inst_results(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::Const(v, _)
        | Inst::Add(v, ..)
        | Inst::Sub(v, ..)
        | Inst::Mul(v, ..)
        | Inst::SDiv(v, ..)
        | Inst::UDiv(v, ..)
        | Inst::ICmp(v, ..)
        | Inst::PtrOffset(v, ..)
        | Inst::SExt(v, ..)
        | Inst::ZExt(v, ..)
        | Inst::Trunc(v, ..) => vec![v],
        Inst::Load {
            result,
            mem_out,
            world_out,
            ..
        } => vec![result, mem_out, world_out],
        Inst::Store {
            mem_out, world_out, ..
        } => vec![mem_out, world_out],
        Inst::Call {
            results,
            mem_out,
            world_out,
            ..
        } => {
            let mut definitions: Vec<&Value> = results.iter().collect();
            definitions.push(mem_out);
            definitions.push(world_out);
            definitions
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) => vec![],
    }
}

/// Returns all `Value` operands that this instruction *uses* (reads).
#[must_use]
pub fn inst_uses(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::Const(..) => vec![],

        Inst::Add(_, lhs, rhs)
        | Inst::Sub(_, lhs, rhs)
        | Inst::Mul(_, lhs, rhs)
        | Inst::SDiv(_, lhs, rhs)
        | Inst::UDiv(_, lhs, rhs) => vec![lhs, rhs],

        Inst::ICmp(_, _, lhs, rhs) => vec![lhs, rhs],

        Inst::Br(_, args) => args.iter().collect(),
        Inst::CondBr(cond, _, targs, _, fargs) => {
            let mut uses: Vec<&Value> = vec![cond];
            uses.extend(targs);
            uses.extend(fargs);
            uses
        }
        Inst::Ret(values) => values.iter().collect(),

        Inst::Load {
            address,
            mem_in,
            world_in,
            ..
        } => vec![address, mem_in, world_in],
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            ..
        } => vec![address, value, mem_in, world_in],
        Inst::PtrOffset(_, base, offset) => vec![base, offset],
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut uses: Vec<&Value> = args.iter().collect();
            uses.push(mem_in);
            uses.push(world_in);
            uses
        }
        Inst::SExt(_, src, _) | Inst::ZExt(_, src, _) | Inst::Trunc(_, src, _) => vec![src],
    }
}

// ---------------------------------------------------------------------------
// Blocks and functions
// ---------------------------------------------------------------------------

/// A basic block in the control-flow graph.
///
/// `args` are block arguments: values bound at block entry.  The entry
/// block's args semantically include the function's `params`.
///
/// `insts` is the list of non-terminator instructions; `terminator` is
/// always present and must be one of `Br`, `CondBr`, or `Ret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub args: Vec<BlockArg>,
    pub insts: Vec<Inst>,
    pub terminator: Inst,
}

/// A function in EvoIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<FunctionParam>,
    pub ret_tys: Vec<EvoIRType>,
    pub blocks: Vec<Block>,
}

// ---------------------------------------------------------------------------
// SSA validator
// ---------------------------------------------------------------------------

/// Validate block-argument SSA without inferring cross-block dominance.
///
/// Function parameters are interface binders, not SSA definitions.  The first
/// block must therefore declare one block argument of the same type for every
/// parameter.  Within every block, an operand may name only a block argument
/// or an instruction result defined earlier in that same block.  Values that
/// cross a CFG edge must be passed explicitly to the destination arguments.
pub fn validate_ssa(func: &Function) -> Result<(), String> {
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
        for used in inst_uses(&block.terminator) {
            ensure_value_type(
                &used.ty,
                &format!("terminator use of v{} in block {}", used.id, block.id),
            )?;
            require_local_value(&local, used, block.id, "terminator")?;
        }

        match &block.terminator {
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
            _ => return Err(format!("block {} has an invalid terminator", block.id)),
        }
    }

    validate_reachability(func, entry.id)?;

    Ok(())
}

fn validate_embedded_instruction_types(
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

fn register_definition(
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

fn require_local_value(
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

fn is_terminator(inst: &Inst) -> bool {
    matches!(inst, Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..))
}

fn validate_edge(
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

fn validate_effect_edge(
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

fn validate_effect_boundary(
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

fn validate_one_effect_boundary(
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

fn validate_reachability(func: &Function, entry: BlockId) -> Result<(), String> {
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
            Inst::Br(target, _) => pending.push(*target),
            Inst::CondBr(_, true_block, _, false_block, _) => {
                pending.push(*true_block);
                pending.push(*false_block);
            }
            Inst::Ret(_) => {}
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

const REGION_ORDER: [Region; 6] = [
    Region::All,
    Region::Unknown,
    Region::Global,
    Region::Stack,
    Region::Heap,
    Region::Escaped,
];

fn region_index(region: Region) -> usize {
    match region {
        Region::All => 0,
        Region::Unknown => 1,
        Region::Global => 2,
        Region::Stack => 3,
        Region::Heap => 4,
        Region::Escaped => 5,
    }
}

#[derive(Clone, Copy, Default)]
struct EffectState {
    mem: [Option<u32>; REGION_ORDER.len()],
    world: Option<u32>,
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
        let tokens = match inst {
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
            | Inst::Call {
                mem_in,
                world_in,
                mem_out,
                world_out,
                ..
            } => Some((mem_in, world_in, mem_out, world_out)),
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

fn validate_instruction_types(inst: &Inst, block: BlockId) -> Result<(), String> {
    match inst {
        Inst::Const(result, _) => expect_bits(&result.ty, "const result", block),
        Inst::Add(result, lhs, rhs)
        | Inst::Sub(result, lhs, rhs)
        | Inst::Mul(result, lhs, rhs)
        | Inst::SDiv(result, lhs, rhs)
        | Inst::UDiv(result, lhs, rhs) => {
            expect_same_bits(result, lhs, rhs, "integer arithmetic", block)
        }
        Inst::ICmp(result, _, lhs, rhs) => {
            if result.ty != EvoIRType::Bits(1) {
                return Err(format!(
                    "icmp result in block {} has type {:?}, expected bits<1>",
                    block, result.ty
                ));
            }
            if lhs.ty != rhs.ty {
                return Err(format!(
                    "icmp operands in block {} have different types {:?} and {:?}",
                    block, lhs.ty, rhs.ty
                ));
            }
            expect_bits(&lhs.ty, "icmp operand", block)
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            expect_pointer(&address.ty, "load address", block)?;
            expect_data_type(&result.ty, "load result", block)?;
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "load", block)
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            expect_pointer(&address.ty, "store address", block)?;
            expect_data_type(&value.ty, "stored value", block)?;
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "store", block)
        }
        Inst::Call {
            results,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            for argument in args {
                expect_data_type(&argument.ty, "ordinary call argument", block)?;
            }
            for result in results {
                expect_data_type(&result.ty, "call result", block)?;
            }
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "call", block)
        }
        Inst::PtrOffset(result, base, offset) => {
            expect_pointer(&result.ty, "pointer-offset result", block)?;
            expect_pointer(&base.ty, "pointer-offset base", block)?;
            if result.ty != base.ty {
                return Err(format!(
                    "pointer-offset result and base in block {} have different types {:?} and {:?}",
                    block, result.ty, base.ty
                ));
            }
            expect_bits(&offset.ty, "pointer-offset index", block)
        }
        Inst::SExt(result, source, target) => {
            validate_conversion(result, source, target, Conversion::SignExtend, block)
        }
        Inst::ZExt(result, source, target) => {
            validate_conversion(result, source, target, Conversion::ZeroExtend, block)
        }
        Inst::Trunc(result, source, target) => {
            validate_conversion(result, source, target, Conversion::Truncate, block)
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) => Err(format!(
            "terminator found in instruction list of block {}",
            block
        )),
    }
}

fn expect_same_bits(
    result: &Value,
    lhs: &Value,
    rhs: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != lhs.ty || lhs.ty != rhs.ty {
        return Err(format!(
            "{} in block {} has inconsistent types: result {:?}, lhs {:?}, rhs {:?}",
            operation, block, result.ty, lhs.ty, rhs.ty
        ));
    }
    expect_bits(&result.ty, operation, block)
}

fn expect_bits(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    match ty {
        EvoIRType::Bits(width) if *width > 0 => Ok(()),
        _ => Err(format!(
            "{} in block {} has type {:?}, expected non-empty bits<N>",
            role, block, ty
        )),
    }
}

fn expect_pointer(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    if matches!(ty, EvoIRType::Ptr(_)) {
        Ok(())
    } else {
        Err(format!(
            "{} in block {} has type {:?}, expected pointer",
            role, block, ty
        ))
    }
}

fn expect_data_type(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    ensure_value_type(ty, &format!("{role} in block {block}"))?;
    if contains_effect_token(ty)? {
        Err(format!(
            "{} in block {} contains a memory/world effect token",
            role, block
        ))
    } else {
        Ok(())
    }
}

/// Maximum accepted nesting for an attacker-provided EvoIR type.
pub const MAX_TYPE_NESTING: usize = 256;

/// Validate the recursive shape of one EvoIR type without recursive descent.
///
/// `Void` and the empty tuple are well-formed type spellings, although
/// [`validate_value_type`] rejects both when a concrete SSA value is required.
pub fn validate_type_well_formed(ty: &EvoIRType) -> Result<(), String> {
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Bits(0) => return Err("zero-width bits type is invalid".to_owned()),
            EvoIRType::Vec(0, _) => return Err("zero-lane vector type is invalid".to_owned()),
            EvoIRType::Vec(_, element) => {
                let forbidden_element = matches!(
                    element.as_ref(),
                    EvoIRType::Void | EvoIRType::Mem(_) | EvoIRType::World
                ) || matches!(element.as_ref(), EvoIRType::Tuple(elements) if elements.is_empty());
                if forbidden_element {
                    return Err("vector element is not a data value type".to_owned());
                }
                pending.push((element, depth + 1));
            }
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate a type at a concrete SSA value/interface position.
pub fn validate_value_type(ty: &EvoIRType) -> Result<(), String> {
    validate_type_well_formed(ty)?;
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Void => return Err("void is not an SSA value type".to_owned()),
            EvoIRType::Tuple(elements) if elements.is_empty() => {
                return Err(
                    "the empty tuple is void-equivalent and not an SSA value type".to_owned(),
                );
            }
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::Vec(_, element) => pending.push((element, depth + 1)),
            _ => {}
        }
    }
    Ok(())
}

fn ensure_value_type(ty: &EvoIRType, role: &str) -> Result<(), String> {
    validate_value_type(ty).map_err(|message| format!("{role}: {message}"))
}

fn contains_effect_token(ty: &EvoIRType) -> Result<bool, String> {
    validate_type_well_formed(ty)?;
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Mem(_) | EvoIRType::World => return Ok(true),
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::Vec(_, element) => pending.push((element, depth + 1)),
            _ => {}
        }
    }
    Ok(false)
}

fn expect_effect_tokens(
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    let input_region = match mem_in.ty {
        EvoIRType::Mem(region) => region,
        _ => {
            return Err(format!(
                "{} input memory token in block {} has type {:?}, expected mem<R>",
                operation, block, mem_in.ty
            ));
        }
    };
    let output_region = match mem_out.ty {
        EvoIRType::Mem(region) => region,
        _ => {
            return Err(format!(
                "{} output memory token in block {} has type {:?}, expected mem<R>",
                operation, block, mem_out.ty
            ));
        }
    };
    if input_region != output_region {
        return Err(format!(
            "{} in block {} changes memory region from {:?} to {:?}",
            operation, block, input_region, output_region
        ));
    }
    for (direction, token) in [("input", world_in), ("output", world_out)] {
        if token.ty != EvoIRType::World {
            return Err(format!(
                "{} {} world token in block {} has type {:?}, expected world",
                operation, direction, block, token.ty
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Conversion {
    SignExtend,
    ZeroExtend,
    Truncate,
}

fn validate_conversion(
    result: &Value,
    source: &Value,
    target: &EvoIRType,
    conversion: Conversion,
    block: BlockId,
) -> Result<(), String> {
    if &result.ty != target {
        return Err(format!(
            "conversion result in block {} has type {:?}, but target is {:?}",
            block, result.ty, target
        ));
    }
    let EvoIRType::Bits(source_width) = source.ty else {
        return Err(format!(
            "conversion source in block {} has non-integer type {:?}",
            block, source.ty
        ));
    };
    let EvoIRType::Bits(target_width) = target else {
        return Err(format!(
            "conversion target in block {} has non-integer type {:?}",
            block, target
        ));
    };
    let widths_valid = match conversion {
        Conversion::SignExtend | Conversion::ZeroExtend => target_width >= &source_width,
        Conversion::Truncate => target_width <= &source_width,
    };
    if !widths_valid {
        return Err(format!(
            "invalid conversion width in block {}: bits<{}> to bits<{}>",
            block, source_width, target_width
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

/// Resource limits for deterministic EvoIR execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterpreterLimits {
    pub step_budget: u64,
}

impl InterpreterLimits {
    #[must_use]
    pub const fn new(step_budget: u64) -> Self {
        Self { step_budget }
    }
}

/// A typed runtime value. `raw_bits` preserves the signless EvoIR bit pattern;
/// memory and world tokens use it as an opaque host-side identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeValue {
    pub ty: EvoIRType,
    pub raw_bits: u64,
}

impl RuntimeValue {
    #[must_use]
    pub const fn new(ty: EvoIRType, raw_bits: u64) -> Self {
        Self { ty, raw_bits }
    }

    #[must_use]
    pub const fn bits(width: u16, raw_bits: u64) -> Self {
        Self::new(EvoIRType::Bits(width), raw_bits)
    }
}

/// Successful EvoIR execution together with the exact charged step count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpreterOutcome {
    pub return_values: Vec<RuntimeValue>,
    pub steps: u64,
}

/// A defined failure from EvoIR validation or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterpreterError {
    InvalidFunction {
        message: String,
    },
    EntryArgumentCount {
        expected: usize,
        actual: usize,
    },
    EntryArgumentType {
        index: usize,
        expected: EvoIRType,
        actual: EvoIRType,
    },
    InvalidEntryArgument {
        index: usize,
        message: String,
    },
    UnsupportedRuntimeType {
        index: usize,
        ty: EvoIRType,
    },
    StepBudgetExceeded {
        budget: u64,
        steps: u64,
        block: BlockId,
    },
    MissingBlock {
        block: BlockId,
    },
    MissingValue {
        block: BlockId,
        value: u32,
    },
    UnsupportedBitWidth {
        block: BlockId,
        width: u16,
    },
    DivisionByZero {
        block: BlockId,
        operation: &'static str,
    },
    SignedDivisionOverflow {
        block: BlockId,
        width: u16,
    },
    UnsupportedInstruction {
        block: BlockId,
        operation: &'static str,
    },
}

impl fmt::Display for InterpreterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFunction { message } => write!(formatter, "invalid EvoIR: {message}"),
            Self::EntryArgumentCount { expected, actual } => write!(
                formatter,
                "entry argument count mismatch: expected {expected}, got {actual}"
            ),
            Self::EntryArgumentType {
                index,
                expected,
                actual,
            } => write!(
                formatter,
                "entry argument {index} has type {actual:?}, expected {expected:?}"
            ),
            Self::InvalidEntryArgument { index, message } => {
                write!(formatter, "entry argument {index} is invalid: {message}")
            }
            Self::UnsupportedRuntimeType { index, ty } => write!(
                formatter,
                "entry argument {index} has unsupported runtime type {ty:?}"
            ),
            Self::StepBudgetExceeded {
                budget,
                steps,
                block,
            } => write!(
                formatter,
                "EvoIR step budget {budget} exceeded after {steps} steps in block {block}"
            ),
            Self::MissingBlock { block } => write!(formatter, "missing EvoIR block {block}"),
            Self::MissingValue { block, value } => {
                write!(formatter, "missing EvoIR value v{value} in block {block}")
            }
            Self::UnsupportedBitWidth { block, width } => write!(
                formatter,
                "EvoIR interpreter does not support bits<{width}> in block {block}"
            ),
            Self::DivisionByZero { block, operation } => {
                write!(formatter, "{operation} by zero in block {block}")
            }
            Self::SignedDivisionOverflow { block, width } => write!(
                formatter,
                "signed division overflow for bits<{width}> in block {block}"
            ),
            Self::UnsupportedInstruction { block, operation } => write!(
                formatter,
                "EvoIR interpreter does not support {operation} in block {block}"
            ),
        }
    }
}

impl std::error::Error for InterpreterError {}

/// Interpret a validated function with explicit entry arguments and limits.
///
/// Each non-terminator instruction and each terminator costs one step. The
/// implementation advances an explicit block program counter, so loops and
/// backedges consume bounded host stack.
pub fn interpret_function(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    validate_ssa(func).map_err(|message| InterpreterError::InvalidFunction { message })?;
    let entry = func
        .blocks
        .first()
        .ok_or_else(|| InterpreterError::InvalidFunction {
            message: format!("function '{}' has no entry block", func.name),
        })?;
    if entry_args.len() != entry.args.len() {
        return Err(InterpreterError::EntryArgumentCount {
            expected: entry.args.len(),
            actual: entry_args.len(),
        });
    }

    let blocks: HashMap<BlockId, &Block> =
        func.blocks.iter().map(|block| (block.id, block)).collect();
    let mut values = HashMap::new();
    let mut incoming = entry_args
        .iter()
        .zip(&entry.args)
        .enumerate()
        .map(|(index, (value, argument))| {
            normalize_runtime_input(value, &argument.value.ty, index, entry.id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut program_counter = entry.id;
    let mut steps = 0;

    loop {
        let block =
            blocks
                .get(&program_counter)
                .copied()
                .ok_or(InterpreterError::MissingBlock {
                    block: program_counter,
                })?;
        if incoming.len() != block.args.len() {
            return Err(InterpreterError::EntryArgumentCount {
                expected: block.args.len(),
                actual: incoming.len(),
            });
        }
        for (argument, runtime_value) in block.args.iter().zip(incoming.drain(..)) {
            values.insert(argument.value.id, runtime_value);
        }

        for inst in &block.insts {
            charge_step(&mut steps, limits, block.id)?;
            execute_instruction(inst, block.id, &mut values)?;
        }

        charge_step(&mut steps, limits, block.id)?;
        match &block.terminator {
            Inst::Br(target, arguments) => {
                incoming = resolve_values(arguments, block.id, &values)?;
                program_counter = *target;
            }
            Inst::CondBr(condition, true_block, true_arguments, false_block, false_arguments) => {
                let condition_value = read_value(&values, condition, block.id)?;
                if condition_value != 0 {
                    incoming = resolve_values(true_arguments, block.id, &values)?;
                    program_counter = *true_block;
                } else {
                    incoming = resolve_values(false_arguments, block.id, &values)?;
                    program_counter = *false_block;
                }
            }
            Inst::Ret(returned) => {
                let raw_values = resolve_values(returned, block.id, &values)?;
                let return_values = raw_values
                    .into_iter()
                    .zip(returned)
                    .map(|(raw_bits, value)| RuntimeValue {
                        ty: value.ty.clone(),
                        raw_bits,
                    })
                    .collect();
                return Ok(InterpreterOutcome {
                    return_values,
                    steps,
                });
            }
            _ => {
                return Err(InterpreterError::InvalidFunction {
                    message: format!("block {} has a non-terminator", block.id),
                });
            }
        }
    }
}

/// Explicit compatibility spelling for [`interpret_function`].
pub fn eval_function(
    func: &Function,
    entry_args: &[RuntimeValue],
    limits: InterpreterLimits,
) -> Result<InterpreterOutcome, InterpreterError> {
    interpret_function(func, entry_args, limits)
}

fn charge_step(
    steps: &mut u64,
    limits: InterpreterLimits,
    block: BlockId,
) -> Result<(), InterpreterError> {
    if *steps >= limits.step_budget {
        return Err(InterpreterError::StepBudgetExceeded {
            budget: limits.step_budget,
            steps: *steps,
            block,
        });
    }
    *steps += 1;
    Ok(())
}

fn execute_instruction(
    inst: &Inst,
    block: BlockId,
    values: &mut HashMap<u32, u64>,
) -> Result<(), InterpreterError> {
    match inst {
        Inst::Const(result, immediate) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            values.insert(result.id, kernel.normalize(*immediate));
        }
        Inst::Add(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            values.insert(result.id, kernel.add(left, right));
        }
        Inst::Sub(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            values.insert(result.id, kernel.sub(left, right));
        }
        Inst::Mul(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            values.insert(result.id, kernel.mul(left, right));
        }
        Inst::SDiv(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let quotient = kernel.sdiv(left, right, block)?;
            values.insert(result.id, quotient);
        }
        Inst::UDiv(result, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(result, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let quotient = kernel.udiv(left, right, block)?;
            values.insert(result.id, quotient);
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            let kernel = IntegerKernel::for_value(lhs, block)?;
            let left = read_value(values, lhs, block)?;
            let right = read_value(values, rhs, block)?;
            let comparison = match operation {
                CmpOp::Eq => kernel.normalize(left) == kernel.normalize(right),
                CmpOp::Ne => kernel.normalize(left) != kernel.normalize(right),
                CmpOp::SLt => kernel.signed(left) < kernel.signed(right),
                CmpOp::SLe => kernel.signed(left) <= kernel.signed(right),
                CmpOp::ULt => kernel.normalize(left) < kernel.normalize(right),
                CmpOp::ULe => kernel.normalize(left) <= kernel.normalize(right),
            };
            values.insert(result.id, u64::from(comparison));
        }
        Inst::SExt(result, source, _) => {
            let source_kernel = IntegerKernel::for_value(source, block)?;
            let target_kernel = IntegerKernel::for_value(result, block)?;
            let raw = read_value(values, source, block)?;
            values.insert(
                result.id,
                target_kernel.encode_signed(source_kernel.signed(raw)),
            );
        }
        Inst::ZExt(result, source, _) | Inst::Trunc(result, source, _) => {
            let source_kernel = IntegerKernel::for_value(source, block)?;
            let target_kernel = IntegerKernel::for_value(result, block)?;
            let raw = read_value(values, source, block)?;
            values.insert(
                result.id,
                target_kernel.normalize(source_kernel.normalize(raw)),
            );
        }
        Inst::Load { .. } => {
            return Err(InterpreterError::UnsupportedInstruction {
                block,
                operation: "load",
            });
        }
        Inst::Store { .. } => {
            return Err(InterpreterError::UnsupportedInstruction {
                block,
                operation: "store",
            });
        }
        Inst::Call { .. } => {
            return Err(InterpreterError::UnsupportedInstruction {
                block,
                operation: "call",
            });
        }
        Inst::PtrOffset(..) => {
            return Err(InterpreterError::UnsupportedInstruction {
                block,
                operation: "pointer offset",
            });
        }
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) => {
            return Err(InterpreterError::InvalidFunction {
                message: format!("terminator appears in instructions of block {block}"),
            });
        }
    }
    Ok(())
}

fn read_value(
    values: &HashMap<u32, u64>,
    value: &Value,
    block: BlockId,
) -> Result<u64, InterpreterError> {
    values
        .get(&value.id)
        .copied()
        .ok_or(InterpreterError::MissingValue {
            block,
            value: value.id,
        })
}

fn resolve_values(
    arguments: &[Value],
    block: BlockId,
    values: &HashMap<u32, u64>,
) -> Result<Vec<u64>, InterpreterError> {
    arguments
        .iter()
        .map(|argument| read_value(values, argument, block))
        .collect()
}

fn normalize_runtime_input(
    value: &RuntimeValue,
    expected: &EvoIRType,
    index: usize,
    block: BlockId,
) -> Result<u64, InterpreterError> {
    validate_value_type(&value.ty)
        .map_err(|message| InterpreterError::InvalidEntryArgument { index, message })?;
    if &value.ty != expected {
        return Err(InterpreterError::EntryArgumentType {
            index,
            expected: expected.clone(),
            actual: value.ty.clone(),
        });
    }
    match expected {
        EvoIRType::Bits(_) => {
            Ok(IntegerKernel::for_type(expected, block)?.normalize(value.raw_bits))
        }
        EvoIRType::Mem(_) | EvoIRType::World => Ok(value.raw_bits),
        _ => Err(InterpreterError::UnsupportedRuntimeType {
            index,
            ty: expected.clone(),
        }),
    }
}

/// One width-aware bit-vector kernel shared by every executed integer opcode.
#[derive(Clone, Copy)]
struct IntegerKernel {
    width: u16,
    mask: u64,
}

impl IntegerKernel {
    fn for_value(value: &Value, block: BlockId) -> Result<Self, InterpreterError> {
        Self::for_type(&value.ty, block)
    }

    fn for_type(ty: &EvoIRType, block: BlockId) -> Result<Self, InterpreterError> {
        let EvoIRType::Bits(width @ 1..=64) = ty else {
            let width = match ty {
                EvoIRType::Bits(width) => *width,
                _ => 0,
            };
            return Err(InterpreterError::UnsupportedBitWidth { block, width });
        };
        let mask = if *width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        Ok(Self {
            width: *width,
            mask,
        })
    }

    fn normalize(self, value: u64) -> u64 {
        value & self.mask
    }

    fn add(self, left: u64, right: u64) -> u64 {
        self.normalize(left.wrapping_add(right))
    }

    fn sub(self, left: u64, right: u64) -> u64 {
        self.normalize(left.wrapping_sub(right))
    }

    fn mul(self, left: u64, right: u64) -> u64 {
        self.normalize(left.wrapping_mul(right))
    }

    fn signed(self, value: u64) -> i128 {
        let value = self.normalize(value);
        let sign_bit = 1_u64 << (self.width - 1);
        if value & sign_bit == 0 {
            i128::from(value)
        } else {
            i128::from(value) - (1_i128 << self.width)
        }
    }

    fn encode_signed(self, value: i128) -> u64 {
        self.normalize(value as u64)
    }

    fn sdiv(self, left: u64, right: u64, block: BlockId) -> Result<u64, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "signed division",
            });
        }
        let minimum = -(1_i128 << (self.width - 1));
        if left == minimum && right == -1 {
            return Err(InterpreterError::SignedDivisionOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left / right))
    }

    fn udiv(self, left: u64, right: u64, block: BlockId) -> Result<u64, InterpreterError> {
        let left = self.normalize(left);
        let right = self.normalize(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "unsigned division",
            });
        }
        Ok(left / right)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_types::{AddressSpace, Region};

    fn value(id: u32, ty: EvoIRType) -> Value {
        Value { id, ty }
    }

    fn bits(id: u32, width: u16) -> Value {
        value(id, EvoIRType::Bits(width))
    }

    fn argument(name: &str, value: Value) -> BlockArg {
        BlockArg {
            name: name.to_owned(),
            value,
        }
    }

    fn overly_nested_type() -> EvoIRType {
        let mut ty = EvoIRType::Bits(1);
        for _ in 0..=MAX_TYPE_NESTING {
            ty = EvoIRType::Tuple(vec![ty]);
        }
        ty
    }

    fn reject(function: &Function, fragment: &str) {
        let error = validate_ssa(function).expect_err("forged IR must be rejected");
        assert!(
            error.contains(fragment),
            "expected {fragment:?} in validator error, got {error:?}"
        );
    }

    fn threaded_function() -> Function {
        let produced = bits(0, 32);
        let incoming = bits(1, 32);
        let one = bits(2, 32);
        let result = bits(3, 32);
        Function {
            name: "threaded".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Const(produced.clone(), 41)],
                    terminator: Inst::Br(1, vec![produced]),
                },
                Block {
                    id: 1,
                    args: vec![argument("incoming", incoming.clone())],
                    insts: vec![
                        Inst::Const(one.clone(), 1),
                        Inst::Add(result.clone(), incoming, one),
                    ],
                    terminator: Inst::Ret(vec![result]),
                },
            ],
        }
    }

    fn token_call_function() -> Function {
        let mem_global = EvoIRType::Mem(Region::Global);
        let mem_stack = EvoIRType::Mem(Region::Stack);
        let world = EvoIRType::World;
        let global_in = value(0, mem_global.clone());
        let stack_in = value(1, mem_stack.clone());
        let world_in = value(2, world.clone());
        let global_out = value(3, mem_global.clone());
        let world_out = value(4, world.clone());
        Function {
            name: "tokens".to_owned(),
            params: vec![
                FunctionParam {
                    name: "global".to_owned(),
                    ty: mem_global.clone(),
                },
                FunctionParam {
                    name: "stack".to_owned(),
                    ty: mem_stack.clone(),
                },
                FunctionParam {
                    name: "world".to_owned(),
                    ty: world.clone(),
                },
            ],
            ret_tys: vec![mem_global, mem_stack, world],
            blocks: vec![Block {
                id: 0,
                args: vec![
                    argument("global", global_in.clone()),
                    argument("stack", stack_in.clone()),
                    argument("world", world_in.clone()),
                ],
                insts: vec![Inst::Call {
                    results: vec![],
                    callee: "side_effect".to_owned(),
                    args: vec![],
                    mem_in: global_in,
                    world_in,
                    mem_out: global_out.clone(),
                    world_out: world_out.clone(),
                }],
                terminator: Inst::Ret(vec![global_out, stack_in, world_out]),
            }],
        }
    }

    #[test]
    fn entry_interface_is_non_ssa_and_interpreter_accepts_explicit_arguments() {
        let input = bits(0, 32);
        let one = bits(1, 32);
        let result = bits(2, 32);
        let function = Function {
            name: "increment".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
            blocks: vec![Block {
                id: 0,
                args: vec![argument("x", input.clone())],
                insts: vec![
                    Inst::Const(one.clone(), 1),
                    Inst::Add(result.clone(), input, one),
                ],
                terminator: Inst::Ret(vec![result]),
            }],
        };
        validate_ssa(&function).expect("valid entry correspondence");
        let outcome = eval_function(
            &function,
            &[RuntimeValue::bits(32, 41)],
            InterpreterLimits::new(3),
        )
        .expect("bounded execution");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 42)]);
        assert_eq!(outcome.steps, 3);

        let mut missing_entry_arg = function.clone();
        missing_entry_arg.blocks[0].args.clear();
        reject(&missing_entry_arg, "has 0 arguments");
        assert!(matches!(
            eval_function(&function, &[], InterpreterLimits::new(3)),
            Err(InterpreterError::EntryArgumentCount {
                expected: 1,
                actual: 0
            })
        ));
        assert!(matches!(
            eval_function(
                &function,
                &[RuntimeValue::bits(64, 41)],
                InterpreterLimits::new(3)
            ),
            Err(InterpreterError::EntryArgumentType { index: 0, .. })
        ));
        assert!(matches!(
            eval_function(
                &function,
                &[RuntimeValue::new(overly_nested_type(), 41)],
                InterpreterLimits::new(3)
            ),
            Err(InterpreterError::InvalidEntryArgument { index: 0, .. })
        ));

        let pointer_type = EvoIRType::Ptr(AddressSpace::Stack);
        let pointer = value(10, pointer_type.clone());
        let pointer_function = Function {
            name: "pointer_passthrough".to_owned(),
            params: vec![FunctionParam {
                name: "pointer".to_owned(),
                ty: pointer_type.clone(),
            }],
            ret_tys: vec![pointer_type.clone()],
            blocks: vec![Block {
                id: 0,
                args: vec![argument("pointer", pointer.clone())],
                insts: vec![],
                terminator: Inst::Ret(vec![pointer]),
            }],
        };
        validate_ssa(&pointer_function).expect("pointer IR is structurally valid");
        assert!(matches!(
            eval_function(
                &pointer_function,
                &[RuntimeValue::new(pointer_type, 0x1000)],
                InterpreterLimits::new(1)
            ),
            Err(InterpreterError::UnsupportedRuntimeType { index: 0, .. })
        ));
    }

    #[test]
    fn real_block_argument_ids_are_unique_and_cross_block_values_are_threaded() {
        let function = threaded_function();
        validate_ssa(&function).expect("explicit block-argument threading is valid");

        let mut duplicate = function.clone();
        duplicate.blocks[1].args[0].value.id = 0;
        reject(&duplicate, "defined more than once");

        let mut cross_block = function.clone();
        cross_block.blocks[1].terminator = Inst::Ret(vec![bits(0, 32)]);
        reject(&cross_block, "not a prior local definition");

        let mut use_before_definition = function.clone();
        let late = bits(9, 32);
        use_before_definition.blocks[0].insts = vec![
            Inst::Add(bits(10, 32), late.clone(), late.clone()),
            Inst::Const(late, 1),
        ];
        use_before_definition.blocks[0].terminator = Inst::Br(1, vec![bits(10, 32)]);
        reject(&use_before_definition, "not a prior local definition");
    }

    #[test]
    fn edge_shape_targets_conditions_returns_and_reachability_are_checked() {
        let function = threaded_function();

        let mut arity = function.clone();
        arity.blocks[0].terminator = Inst::Br(1, vec![]);
        reject(&arity, "passes 0 values");

        let mut edge_type = function.clone();
        edge_type.blocks[1].args[0].value.ty = EvoIRType::Bits(64);
        reject(&edge_type, "edge 0 -> 1 value 0");

        let mut missing_target = function.clone();
        missing_target.blocks[0].terminator = Inst::Br(99, vec![bits(0, 32)]);
        reject(&missing_target, "targets missing block 99");

        let mut unreachable = function.clone();
        let unused = bits(20, 32);
        unreachable.blocks.push(Block {
            id: 20,
            args: vec![],
            insts: vec![Inst::Const(unused.clone(), 0)],
            terminator: Inst::Ret(vec![unused]),
        });
        reject(&unreachable, "is unreachable");

        let condition = bits(30, 32);
        let left = bits(31, 32);
        let right = bits(32, 32);
        let bad_condition = Function {
            name: "bad_condition".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![Inst::Const(condition.clone(), 1)],
                    terminator: Inst::CondBr(condition, 1, vec![], 2, vec![]),
                },
                Block {
                    id: 1,
                    args: vec![],
                    insts: vec![Inst::Const(left.clone(), 1)],
                    terminator: Inst::Ret(vec![left]),
                },
                Block {
                    id: 2,
                    args: vec![],
                    insts: vec![Inst::Const(right.clone(), 2)],
                    terminator: Inst::Ret(vec![right]),
                },
            ],
        };
        reject(&bad_condition, "expected bits<1>");

        let mut bad_return = function;
        bad_return.ret_tys = vec![EvoIRType::Bits(64)];
        reject(&bad_return, "return value 0");
    }

    #[test]
    fn value_type_validation_is_recursive_and_void_is_not_a_value() {
        assert!(validate_type_well_formed(&EvoIRType::Void).is_ok());
        assert!(validate_value_type(&EvoIRType::Void).is_err());
        assert!(validate_value_type(&EvoIRType::Tuple(vec![])).is_err());
        assert!(validate_value_type(&EvoIRType::Bits(0)).is_err());
        assert!(validate_value_type(&EvoIRType::Vec(4, Box::new(EvoIRType::World))).is_err());
        assert!(
            validate_value_type(&EvoIRType::Vec(4, Box::new(EvoIRType::Tuple(vec![])))).is_err()
        );
        assert!(validate_value_type(&EvoIRType::Ptr(AddressSpace::Stack)).is_ok());
        assert!(validate_value_type(&EvoIRType::Mem(Region::Heap)).is_ok());
        let nested_effect = EvoIRType::Vec(
            4,
            Box::new(EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)])),
        );
        assert!(validate_value_type(&nested_effect).is_ok());
        assert_eq!(contains_effect_token(&nested_effect), Ok(true));

        let too_deep = overly_nested_type();
        let error = validate_value_type(&too_deep).expect_err("deep forged type must be bounded");
        assert!(error.contains("nesting exceeds"));
    }

    #[test]
    fn instruction_result_and_embedded_types_are_bounded_before_comparison() {
        let left = bits(0, 32);
        let right = bits(1, 32);
        let mut deep_result = Function {
            name: "deep_result".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(left.clone(), 1),
                    Inst::Const(right.clone(), 2),
                    Inst::Add(value(2, overly_nested_type()), left.clone(), right),
                ],
                terminator: Inst::Ret(vec![left.clone()]),
            }],
        };
        reject(&deep_result, "nesting exceeds");

        deep_result.blocks[0].insts = vec![
            Inst::Const(left.clone(), 1),
            Inst::SExt(bits(3, 32), left.clone(), overly_nested_type()),
        ];
        deep_result.blocks[0].terminator = Inst::Ret(vec![bits(3, 32)]);
        reject(&deep_result, "nesting exceeds");
    }

    #[test]
    fn token_regions_are_linear_and_ordinary_call_values_exclude_tokens() {
        let function = token_call_function();
        validate_ssa(&function).expect("distinct memory regions thread independently");

        let mut stale = function.clone();
        stale.blocks[0].insts.push(Inst::Call {
            results: vec![],
            callee: "stale".to_owned(),
            args: vec![],
            mem_in: value(0, EvoIRType::Mem(Region::Global)),
            world_in: value(2, EvoIRType::World),
            mem_out: value(5, EvoIRType::Mem(Region::Global)),
            world_out: value(6, EvoIRType::World),
        });
        stale.blocks[0].terminator = Inst::Ret(vec![
            value(5, EvoIRType::Mem(Region::Global)),
            value(1, EvoIRType::Mem(Region::Stack)),
            value(6, EvoIRType::World),
        ]);
        reject(&stale, "consumes stale");

        let mut dropped = function.clone();
        dropped.ret_tys = vec![EvoIRType::Mem(Region::Global), EvoIRType::World];
        dropped.blocks[0].terminator = Inst::Ret(vec![
            value(3, EvoIRType::Mem(Region::Global)),
            value(4, EvoIRType::World),
        ]);
        reject(&dropped, "drops current mem<Stack>");

        let mut duplicate_region = function.clone();
        duplicate_region.params[1].ty = EvoIRType::Mem(Region::Global);
        duplicate_region.blocks[0].args[1].value.ty = EvoIRType::Mem(Region::Global);
        duplicate_region.ret_tys[1] = EvoIRType::Mem(Region::Global);
        duplicate_region.blocks[0].terminator = Inst::Ret(vec![
            value(3, EvoIRType::Mem(Region::Global)),
            value(1, EvoIRType::Mem(Region::Global)),
            value(4, EvoIRType::World),
        ]);
        reject(&duplicate_region, "more than one mem<Global>");

        let mut ordinary_token = function.clone();
        let Inst::Call { args, .. } = &mut ordinary_token.blocks[0].insts[0] else {
            panic!("fixture must contain a call");
        };
        args.push(value(1, EvoIRType::Mem(Region::Stack)));
        reject(&ordinary_token, "ordinary call argument");

        let mut nested_token = function.clone();
        let tuple_token = EvoIRType::Tuple(vec![EvoIRType::Mem(Region::Heap)]);
        nested_token.params.push(FunctionParam {
            name: "hidden".to_owned(),
            ty: tuple_token.clone(),
        });
        nested_token.blocks[0]
            .args
            .push(argument("hidden", value(5, tuple_token.clone())));
        let Inst::Call { args, .. } = &mut nested_token.blocks[0].insts[0] else {
            panic!("fixture must contain a call");
        };
        args.push(value(5, tuple_token));
        reject(&nested_token, "contains a memory/world effect token");

        let mut changed_region = function;
        let Inst::Call { mem_out, .. } = &mut changed_region.blocks[0].insts[0] else {
            panic!("fixture must contain a call");
        };
        mem_out.ty = EvoIRType::Mem(Region::Heap);
        reject(&changed_region, "changes memory region");
    }

    #[test]
    fn iterative_interpreter_executes_backedges_and_enforces_budget() {
        let initial_n = bits(0, 32);
        let initial_acc = bits(1, 32);
        let n = bits(2, 32);
        let acc = bits(3, 32);
        let zero = bits(4, 32);
        let condition = bits(5, 1);
        let body_n = bits(6, 32);
        let body_acc = bits(7, 32);
        let next_acc = bits(8, 32);
        let one = bits(9, 32);
        let next_n = bits(10, 32);
        let returned = bits(11, 32);
        let function = Function {
            name: "loop_sum".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![],
                    insts: vec![
                        Inst::Const(initial_n.clone(), 5),
                        Inst::Const(initial_acc.clone(), 0),
                    ],
                    terminator: Inst::Br(1, vec![initial_n, initial_acc]),
                },
                Block {
                    id: 1,
                    args: vec![argument("n", n.clone()), argument("acc", acc.clone())],
                    insts: vec![
                        Inst::Const(zero.clone(), 0),
                        Inst::ICmp(condition.clone(), CmpOp::SLt, zero, n.clone()),
                    ],
                    terminator: Inst::CondBr(condition, 2, vec![n, acc.clone()], 3, vec![acc]),
                },
                Block {
                    id: 2,
                    args: vec![
                        argument("n", body_n.clone()),
                        argument("acc", body_acc.clone()),
                    ],
                    insts: vec![
                        Inst::Add(next_acc.clone(), body_acc, body_n.clone()),
                        Inst::Const(one.clone(), 1),
                        Inst::Sub(next_n.clone(), body_n, one),
                    ],
                    terminator: Inst::Br(1, vec![next_n, next_acc]),
                },
                Block {
                    id: 3,
                    args: vec![argument("result", returned.clone())],
                    insts: vec![],
                    terminator: Inst::Ret(vec![returned]),
                },
            ],
        };
        let outcome = eval_function(&function, &[], InterpreterLimits::new(42))
            .expect("loop must terminate within its exact budget");
        assert_eq!(outcome.return_values, vec![RuntimeValue::bits(32, 15)]);
        assert_eq!(outcome.steps, 42);
        assert!(matches!(
            eval_function(&function, &[], InterpreterLimits::new(41)),
            Err(InterpreterError::StepBudgetExceeded {
                budget: 41,
                steps: 41,
                ..
            })
        ));
    }

    #[test]
    fn self_loop_arguments_are_resolved_simultaneously_before_rebinding() {
        let a = bits(0, 32);
        let b = bits(1, 32);
        let count = bits(2, 32);
        let zero = bits(3, 32);
        let one = bits(4, 32);
        let next_count = bits(5, 32);
        let condition = bits(6, 1);
        let out_a = bits(7, 32);
        let out_b = bits(8, 32);
        let function = Function {
            name: "swap_once".to_owned(),
            params: vec![
                FunctionParam {
                    name: "a".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
                FunctionParam {
                    name: "b".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
                FunctionParam {
                    name: "count".to_owned(),
                    ty: EvoIRType::Bits(32),
                },
            ],
            ret_tys: vec![EvoIRType::Bits(32), EvoIRType::Bits(32)],
            blocks: vec![
                Block {
                    id: 0,
                    args: vec![
                        argument("a", a.clone()),
                        argument("b", b.clone()),
                        argument("count", count.clone()),
                    ],
                    insts: vec![
                        Inst::Const(zero.clone(), 0),
                        Inst::Const(one.clone(), 1),
                        Inst::Sub(next_count.clone(), count.clone(), one),
                        Inst::ICmp(condition.clone(), CmpOp::SLt, zero, count),
                    ],
                    terminator: Inst::CondBr(
                        condition,
                        0,
                        vec![b, a.clone(), next_count],
                        1,
                        vec![a, bits(1, 32)],
                    ),
                },
                Block {
                    id: 1,
                    args: vec![argument("a", out_a.clone()), argument("b", out_b.clone())],
                    insts: vec![],
                    terminator: Inst::Ret(vec![out_a, out_b]),
                },
            ],
        };
        let outcome = eval_function(
            &function,
            &[
                RuntimeValue::bits(32, 1),
                RuntimeValue::bits(32, 2),
                RuntimeValue::bits(32, 1),
            ],
            InterpreterLimits::new(11),
        )
        .expect("one self-loop iteration must execute");
        assert_eq!(
            outcome.return_values,
            vec![RuntimeValue::bits(32, 2), RuntimeValue::bits(32, 1)]
        );
        assert_eq!(outcome.steps, 11);
    }

    #[test]
    fn integer_kernel_handles_raw_64_bit_patterns_and_equal_width_conversions() {
        let raw = bits(0, 64);
        let same = bits(1, 64);
        let function = Function {
            name: "raw".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(64)],
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(raw.clone(), u64::MAX),
                    Inst::SExt(same.clone(), raw, EvoIRType::Bits(64)),
                ],
                terminator: Inst::Ret(vec![same]),
            }],
        };
        let outcome = eval_function(&function, &[], InterpreterLimits::new(3))
            .expect("raw pattern must not be host-i64 constrained");
        assert_eq!(
            outcome.return_values,
            vec![RuntimeValue::bits(64, u64::MAX)]
        );
    }

    #[test]
    fn interpreter_reports_typed_arithmetic_and_unsupported_effect_errors() {
        let numerator = bits(0, 32);
        let zero = bits(1, 32);
        let quotient = bits(2, 32);
        let division = Function {
            name: "division".to_owned(),
            params: vec![],
            ret_tys: vec![EvoIRType::Bits(32)],
            blocks: vec![Block {
                id: 0,
                args: vec![],
                insts: vec![
                    Inst::Const(numerator.clone(), 7),
                    Inst::Const(zero.clone(), 0),
                    Inst::SDiv(quotient.clone(), numerator, zero),
                ],
                terminator: Inst::Ret(vec![quotient]),
            }],
        };
        assert!(matches!(
            eval_function(&division, &[], InterpreterLimits::new(4)),
            Err(InterpreterError::DivisionByZero {
                operation: "signed division",
                ..
            })
        ));

        assert!(matches!(
            eval_function(
                &token_call_function(),
                &[
                    RuntimeValue::new(EvoIRType::Mem(Region::Global), 10),
                    RuntimeValue::new(EvoIRType::Mem(Region::Stack), 20),
                    RuntimeValue::new(EvoIRType::World, 30),
                ],
                InterpreterLimits::new(2)
            ),
            Err(InterpreterError::UnsupportedInstruction {
                operation: "call",
                ..
            })
        ));
    }
}
