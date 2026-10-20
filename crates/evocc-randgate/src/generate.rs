//! The deterministic generator of well-typed, validator-clean EvoIR modules.
//!
//! Freedom from undefined behaviour is a property of the *shapes* emitted here,
//! not of a filter applied afterwards. A divisor is sanitized into `[1, 2^(N-1))`
//! before it reaches a division, a shift amount is masked below the shifted
//! width, every allocated byte is stored before anything loads it, every formed
//! pointer stays inside its object, a null pointer is only ever compared, and a
//! lifetime is only ever ended for an object nothing can reach afterwards. The
//! gate still discards and counts a program the interpreter reports as
//! undefined, because a generator defect must show up as a number rather than
//! as a silently weakened comparison.

use std::collections::BTreeMap;

use evocc_evoir::{
    AccessLayout, Block, BlockArg, BlockId, CmpOp, EffectSummary, Function, FunctionParam,
    GlobalKind, GlobalObject, GlobalVisibility, IndexSignedness, Inst, Module, OffsetDirection,
    RuntimeValue, UbReason, Value,
};
use evocc_types::{AddressSpace, EvoIRType, Region};

use crate::rng::Rng;

/// Bit widths the generator draws from. Every width the Day 7 profile executes
/// is representable; this list keeps the boundary widths and a spread of
/// non-byte widths so masking and extension defects cannot hide in a corner.
pub const WIDTHS: [u16; 14] = [1, 2, 3, 5, 7, 8, 13, 16, 17, 31, 32, 33, 47, 64];

/// Element widths a memory access uses.
///
/// The validator derives a load's access layout from the loaded type, so an
/// element width that is not a whole number of bytes could never match the
/// layout its object was allocated with.
const MEMORY_WIDTHS: [u16; 4] = [8, 16, 32, 64];

/// The object every block carries across its outgoing edges.
///
/// One shape, so a successor never has to ask what it received: eight bytes,
/// eight-byte aligned, every byte stored before the edge is taken, and owned by
/// the current frame, which keeps it live until the function returns.
const CARRIER: AccessLayout = AccessLayout::new(8, 8);
const CARRIER_WIDTH: u16 = 64;

/// Which seed space a generated module comes from.
///
/// EvoMIR Section 9, "The native corpus": "The randomized generator's seed
/// space is pinned by the middle-end campaigns (a seed names one module
/// forever), so the native lane does not change it. It draws from a second
/// generator mode with its own seed space, `native-main`, whose `main` takes
/// no arguments and returns one `bits<32>`."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeneratorMode {
    /// The mode the middle-end campaigns pinned: `@main` takes what the draw
    /// gives it and returns up to `max_results` ordinary values.
    #[default]
    Standard,
    /// The native corpus: `@main` takes no arguments and returns one
    /// `bits<32>`, so a process exit status can carry its result.
    NativeMain,
}

/// The width Section 9's native corpus returns, the width a process exit status
/// carries eight bits of.
pub const NATIVE_RESULT_WIDTH: u16 = 32;

impl GeneratorMode {
    /// The tag that separates the two seed spaces.
    ///
    /// Section 9 keeps the first seed space pinned, so the second one starts
    /// its stream somewhere else entirely rather than at the same state with a
    /// different interface: a seed of one space then names a module of that
    /// space alone.
    #[must_use]
    pub const fn seed_space(self) -> u64 {
        match self {
            Self::Standard => 0,
            Self::NativeMain => 0x4E41_5449_5645_4D41,
        }
    }
}

/// The shape ceilings one generated module stays within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorConfig {
    /// Which seed space and entry shape this configuration draws from.
    pub mode: GeneratorMode,
    /// Callable helper functions defined before `@main`.
    pub max_helper_functions: u32,
    /// Module globals available to `global_addr`.
    pub max_globals: u32,
    /// Straight-line instructions emitted into one segment of one block.
    pub max_segment_instructions: u32,
    /// Nesting budget for branch and loop regions.
    pub max_region_depth: u32,
    /// Blocks one function may hold.
    pub max_blocks: u32,
    /// The largest trip count a generated loop may run.
    pub max_loop_trips: u64,
    /// Ordinary return values one function may declare.
    pub max_results: u32,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            mode: GeneratorMode::Standard,
            max_helper_functions: 2,
            max_globals: 2,
            max_segment_instructions: 6,
            max_region_depth: 2,
            max_blocks: 24,
            max_loop_trips: 4,
            max_results: 3,
        }
    }
}

impl GeneratorConfig {
    /// The Section 9 native corpus: the default ceilings in the second seed
    /// space, whose `@main` a process exit status can report.
    #[must_use]
    pub fn native_main() -> Self {
        Self {
            mode: GeneratorMode::NativeMain,
            ..Self::default()
        }
    }
}

/// Produce the one module this seed names.
///
/// The same seed and configuration yield the same module forever; the result
/// passes `validate_module` and renders to canonical Section 9 bytes.
#[must_use]
pub fn generate_module(seed: u64, config: GeneratorConfig) -> Module {
    let mut rng = Rng::from_seed(seed ^ config.mode.seed_space());
    let mut origins = 0_u64;
    let globals = generate_globals(&mut rng, config, &mut origins);
    let symbols: Vec<String> = globals.iter().map(|global| global.symbol.clone()).collect();

    let mut signatures: Vec<Signature> = Vec::new();
    let mut functions: Vec<Function> = Vec::new();
    if rng.chance(45) {
        let (signature, function) = recursive_helper(&mut rng);
        signatures.push(signature);
        functions.push(function);
    }

    let helpers = rng.below(u64::from(config.max_helper_functions) + 1);
    for index in 0..helpers {
        let signature = random_signature(&mut rng, config, format!("h{index}"), &symbols, false);
        let function = generate_function(
            &mut rng,
            config,
            &signature,
            &signatures,
            &symbols,
            &mut origins,
        );
        signatures.push(signature);
        functions.push(function);
    }
    let entry = match config.mode {
        GeneratorMode::Standard => {
            random_signature(&mut rng, config, "main".to_owned(), &symbols, true)
        }
        // Section 9: "whose `main` takes no arguments and returns one
        // `bits<32>`". The interface is fixed rather than drawn, so no bits of
        // the stream are spent on it and the body sees the same source the
        // standard mode would have given it at this point.
        GeneratorMode::NativeMain => Signature {
            name: "main".to_owned(),
            params: Vec::new(),
            results: vec![EvoIRType::Bits(NATIVE_RESULT_WIDTH)],
            recursive: false,
        },
    };
    functions.push(generate_function(
        &mut rng,
        config,
        &entry,
        &signatures,
        &symbols,
        &mut origins,
    ));

    let mut module = Module {
        globals,
        functions,
        ..Module::default()
    };
    module.recompute_effect_summaries();
    module
}

/// Deterministic entry arguments for `@main` in `module`, derived from `seed`.
///
/// # Panics
/// Panics when `module` has no `@main` with an entry block.
#[must_use]
pub fn entry_arguments(module: &Module, seed: u64) -> Vec<RuntimeValue> {
    let mut rng = Rng::from_seed(seed ^ 0xA5A5_5A5A_1234_9876);
    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("a generated module always defines @main");
    let block = entry
        .blocks
        .first()
        .expect("@main always has an entry block");
    block
        .args
        .iter()
        .map(|arg| match &arg.value.ty {
            EvoIRType::Bits(width) => RuntimeValue::bits(*width, rng.next_u64() & mask(*width)),
            other => RuntimeValue::new(other.clone(), 0),
        })
        .collect()
}

fn mask(width: u16) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

fn memory_type() -> EvoIRType {
    EvoIRType::Mem(Region::All)
}

fn world_type() -> EvoIRType {
    EvoIRType::World
}

fn pointer_type() -> EvoIRType {
    EvoIRType::Ptr(AddressSpace::Generic)
}

fn generate_globals(
    rng: &mut Rng,
    config: GeneratorConfig,
    origins: &mut u64,
) -> Vec<GlobalObject> {
    let count = rng.below(u64::from(config.max_globals) + 1);
    (0..count)
        .map(|index| {
            let origin = *origins;
            *origins += 1;
            let visibility = if rng.chance(50) {
                GlobalVisibility::Internal
            } else {
                GlobalVisibility::External
            };
            let bytes = (0..8).map(|_| (rng.next_u64() & 0xff) as u8).collect();
            GlobalObject {
                read_only: false,
                section: None,
                linkage: evocc_evoir::Linkage::Strong,
                relocs: Vec::new(),
                symbol: format!("g{index}"),
                origin,
                kind: GlobalKind::File,
                visibility,
                layout: CARRIER,
                bytes,
                initialized: vec![true; 8],
            }
        })
        .collect()
}

/// One generated function's call interface, in widths.
#[derive(Debug, Clone)]
struct Signature {
    name: String,
    params: Vec<EvoIRType>,
    results: Vec<EvoIRType>,
    /// The function calls itself, guarded by a depth parameter it decrements.
    /// A caller must therefore hand it a small constant rather than an
    /// arbitrary value, which is what bounds the recursion.
    recursive: bool,
}

impl Signature {
    /// The complete lowered signature `func_addr` must write.
    fn pointer_type(&self) -> EvoIRType {
        EvoIRType::FnPtr {
            variadic: false,
            params: self
                .params
                .iter()
                .cloned()
                .chain([memory_type(), world_type()])
                .collect(),
            results: self
                .results
                .iter()
                .cloned()
                .chain([memory_type(), world_type()])
                .collect(),
            marks: Vec::new(),
        }
    }
}

/// The depth a recursive call starts from. Small enough that a call inside a
/// nested loop still finishes well inside the interpreter's fuel.
const MAX_RECURSION_DEPTH: u64 = 3;
const DEPTH_WIDTH: u16 = 8;

fn random_signature(
    rng: &mut Rng,
    config: GeneratorConfig,
    name: String,
    globals: &[String],
    // `@main` is entered by the module driver, which starts from the
    // declaration-order memory snapshot and an empty world (2.1) and has no
    // pointer to hand in. A pointer parameter is therefore a shape only a
    // called function can have.
    entry: bool,
) -> Signature {
    let params = (0..rng.below(4))
        .map(|_| {
            if !entry && rng.chance(20) {
                pointer_type()
            } else {
                EvoIRType::Bits(*rng.pick(&WIDTHS))
            }
        })
        .collect();
    let count = 1 + rng.below(u64::from(config.max_results));
    let results = (0..count)
        .map(|_| {
            // A returned pointer is a global's address, so a signature may only
            // declare one when the module has a global to name. Returning a
            // pointer to the callee's own frame would dangle the moment the
            // return ended its lifetime. `@main` is excluded for the same
            // reason as its parameters: the driver hands its results back as
            // public values, and Section 10.4 gives no runtime spelling for a
            // pointer there.
            if !entry && !globals.is_empty() && rng.chance(15) {
                pointer_type()
            } else {
                EvoIRType::Bits(*rng.pick(&WIDTHS))
            }
        })
        .collect();
    Signature {
        name,
        params,
        results,
        recursive: false,
    }
}

/// A function that calls itself, bounded by the depth it was handed.
///
/// It is written out rather than drawn, because the one property that matters
/// is the one a random body cannot be trusted with: the recursive edge is taken
/// only when the depth is nonzero and is entered with that depth minus one, so
/// the chain is exactly as long as the constant the caller passed.
fn recursive_helper(rng: &mut Rng) -> (Signature, Function) {
    let bits = EvoIRType::Bits(DEPTH_WIDTH);
    let mut next = 0_u32;
    let mut fresh = |ty: EvoIRType| {
        let id = next;
        next += 1;
        Value { id, ty }
    };
    let binder = |value: &Value| BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    };

    let depth = fresh(bits.clone());
    let memory = fresh(memory_type());
    let world = fresh(world_type());
    let zero = fresh(bits.clone());
    let at_base = fresh(EvoIRType::Bits(1));

    let base_depth = fresh(bits.clone());
    let base_memory = fresh(memory_type());
    let base_world = fresh(world_type());

    let deep_depth = fresh(bits.clone());
    let deep_memory = fresh(memory_type());
    let deep_world = fresh(world_type());
    let one = fresh(bits.clone());
    let next_depth = fresh(bits.clone());
    let returned = fresh(bits.clone());
    let call_memory = fresh(memory_type());
    let call_world = fresh(world_type());
    let bump = fresh(bits.clone());
    let total = fresh(bits.clone());

    let addend = u64::from(rng.next_u64() as u8);

    let function = Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        variadic: false,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        name: "r0".to_owned(),
        params: vec![
            FunctionParam {
                name: "arg0".to_owned(),
                ty: bits.clone(),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: memory_type(),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: world_type(),
            },
        ],
        ret_tys: vec![bits.clone(), memory_type(), world_type()],
        effects: EffectSummary::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![binder(&depth), binder(&memory), binder(&world)],
                insts: vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::ICmp(at_base.clone(), CmpOp::Eq, depth.clone(), zero),
                ],
                terminator: Inst::CondBr(
                    at_base,
                    1,
                    vec![depth.clone(), memory.clone(), world.clone()],
                    2,
                    vec![depth, memory, world],
                ),
            },
            Block {
                id: 1,
                args: vec![
                    binder(&base_depth),
                    binder(&base_memory),
                    binder(&base_world),
                ],
                insts: Vec::new(),
                terminator: Inst::Ret(vec![base_depth, base_memory, base_world]),
            },
            Block {
                id: 2,
                args: vec![
                    binder(&deep_depth),
                    binder(&deep_memory),
                    binder(&deep_world),
                ],
                insts: vec![
                    Inst::Const(one.clone(), 1),
                    Inst::Sub(next_depth.clone(), deep_depth, one.clone()),
                    Inst::Call {
                        arg_marks: Vec::new(),
                        returns_twice: false,
                        results: vec![returned.clone()],
                        callee: "r0".to_owned(),
                        args: vec![next_depth],
                        mem_in: deep_memory,
                        world_in: deep_world,
                        mem_out: call_memory.clone(),
                        world_out: call_world.clone(),
                    },
                    Inst::Const(bump.clone(), addend),
                    Inst::Add(total.clone(), returned, bump),
                ],
                terminator: Inst::Ret(vec![total, call_memory, call_world]),
            },
        ],
        param_marks: Vec::new(),
    };

    (
        Signature {
            name: "r0".to_owned(),
            params: vec![bits.clone()],
            results: vec![bits],
            recursive: true,
        },
        function,
    )
}

/// One allocated object and the block-local pointers naming its elements.
#[derive(Debug, Clone)]
struct Buffer {
    elements: Vec<Value>,
    width: u16,
    layout: AccessLayout,
}

/// A block under construction.
struct OpenBlock {
    id: BlockId,
    args: Vec<BlockArg>,
    insts: Vec<Inst>,
    /// Two per enclosing loop: the counter and the limit it is compared
    /// against. Every edge forwards them unchanged; only a loop's own back edge
    /// increments its counter, and only its entry edge chooses its limit.
    ivs: Vec<Value>,
    pool: BTreeMap<u16, Vec<Value>>,
    buffers: Vec<Buffer>,
    /// The pointer this block hands to its successors.
    carrier: Value,
    mem: Value,
    world: Value,
}

impl OpenBlock {
    fn carrier_buffer(carrier: &Value) -> Buffer {
        Buffer {
            elements: vec![carrier.clone()],
            width: CARRIER_WIDTH,
            layout: CARRIER,
        }
    }
}

struct FunctionGen<'a> {
    rng: &'a mut Rng,
    config: GeneratorConfig,
    origins: &'a mut u64,
    callees: &'a [Signature],
    globals: &'a [String],
    active: Vec<u16>,
    next_value: u32,
    next_block: BlockId,
    blocks: Vec<Block>,
}

fn generate_function(
    rng: &mut Rng,
    config: GeneratorConfig,
    signature: &Signature,
    callees: &[Signature],
    globals: &[String],
    origins: &mut u64,
) -> Function {
    let mut active = vec![1_u16, 64];
    active.extend(signature.results.iter().filter_map(|ty| match ty {
        EvoIRType::Bits(width) => Some(*width),
        _ => None,
    }));
    let extra = 1 + rng.below(3);
    for _ in 0..extra {
        let scalars = signature
            .params
            .iter()
            .filter_map(|ty| match ty {
                EvoIRType::Bits(width) => Some(*width),
                _ => None,
            })
            .collect::<Vec<_>>();
        let width = if !scalars.is_empty() && rng.chance(50) {
            *rng.pick(&scalars)
        } else {
            *rng.pick(&WIDTHS)
        };
        active.push(width);
    }
    active.sort_unstable();
    active.dedup();

    let mut generator = FunctionGen {
        rng,
        config,
        origins,
        callees,
        globals,
        active,
        next_value: 0,
        next_block: 0,
        blocks: Vec::new(),
    };

    let (params, entry) = generator.open_entry(signature);
    let mut block = generator.emit_region(entry, config.max_region_depth);
    let mut returned = Vec::new();
    for ty in signature.results.clone() {
        let value = match ty {
            EvoIRType::Bits(width) => generator.value_of(&mut block, width),
            // A pointer result is a global's address: static storage, so it
            // outlives the frame that returns it. A pointer into this frame
            // would be dangling the moment the return ended its lifetime.
            _ => generator.emit_global_address(&mut block),
        };
        returned.push(value);
    }
    returned.push(block.mem.clone());
    returned.push(block.world.clone());
    generator.close(block, Inst::Ret(returned));

    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        variadic: false,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        name: signature.name.clone(),
        params,
        ret_tys: signature
            .results
            .iter()
            .cloned()
            .chain([memory_type(), world_type()])
            .collect(),
        effects: EffectSummary::default(),
        blocks: generator.blocks,
        param_marks: Vec::new(),
    }
}

impl FunctionGen<'_> {
    fn fresh(&mut self, ty: EvoIRType) -> Value {
        let id = self.next_value;
        self.next_value += 1;
        Value { id, ty }
    }

    fn fresh_block(&mut self) -> BlockId {
        let id = self.next_block;
        self.next_block += 1;
        id
    }

    fn take_origin(&mut self) -> u64 {
        let origin = *self.origins;
        *self.origins += 1;
        origin
    }

    fn record(&mut self, block: &mut OpenBlock, value: &Value) {
        if let EvoIRType::Bits(width) = value.ty {
            block.pool.entry(width).or_default().push(value.clone());
        }
    }

    fn close(&mut self, block: OpenBlock, terminator: Inst) {
        self.blocks.push(Block {
            id: block.id,
            args: block.args,
            insts: block.insts,
            terminator,
        });
    }

    fn binder(value: &Value) -> BlockArg {
        BlockArg {
            name: format!("v{}", value.id),
            value: value.clone(),
        }
    }

    fn open_entry(&mut self, signature: &Signature) -> (Vec<FunctionParam>, OpenBlock) {
        let id = self.fresh_block();
        let mut params = Vec::new();
        let mut args = Vec::new();
        let mut pool: BTreeMap<u16, Vec<Value>> = BTreeMap::new();
        let mut incoming = Vec::new();
        for ty in &signature.params {
            let value = self.fresh(ty.clone());
            params.push(FunctionParam {
                name: format!("arg{}", params.len()),
                ty: ty.clone(),
            });
            args.push(Self::binder(&value));
            match ty {
                EvoIRType::Bits(width) => pool.entry(*width).or_default().push(value),
                // The caller hands over a live, fully initialized carrier, so
                // the callee may read and write it exactly like one of its own.
                _ => incoming.push(OpenBlock::carrier_buffer(&value)),
            }
        }
        let mem = self.fresh(memory_type());
        params.push(FunctionParam {
            name: format!("arg{}", params.len()),
            ty: memory_type(),
        });
        args.push(Self::binder(&mem));
        let world = self.fresh(world_type());
        params.push(FunctionParam {
            name: format!("arg{}", params.len()),
            ty: world_type(),
        });
        args.push(Self::binder(&world));

        let mut block = OpenBlock {
            id,
            args,
            insts: Vec::new(),
            ivs: Vec::new(),
            pool,
            buffers: incoming,
            carrier: mem.clone(),
            mem,
            world,
        };
        // The entry block owns the first carrier, so every block after it can
        // assume one exists without asking where it came from.
        let carrier = self.emit_carrier_object(&mut block);
        block.buffers.push(OpenBlock::carrier_buffer(&carrier));
        block.carrier = carrier;
        (params, block)
    }

    fn open_block(&mut self, id: BlockId, iv_count: usize) -> OpenBlock {
        let mut args = Vec::new();
        let mut pool: BTreeMap<u16, Vec<Value>> = BTreeMap::new();
        let mut ivs = Vec::new();
        for _ in 0..iv_count {
            let value = self.fresh(EvoIRType::Bits(64));
            args.push(Self::binder(&value));
            pool.entry(64).or_default().push(value.clone());
            ivs.push(value);
        }
        let carrier = self.fresh(pointer_type());
        args.push(Self::binder(&carrier));
        for width in self.active.clone() {
            let value = self.fresh(EvoIRType::Bits(width));
            args.push(Self::binder(&value));
            pool.entry(width).or_default().push(value);
        }
        let mem = self.fresh(memory_type());
        args.push(Self::binder(&mem));
        let world = self.fresh(world_type());
        args.push(Self::binder(&world));
        OpenBlock {
            id,
            args,
            insts: Vec::new(),
            ivs,
            pool,
            buffers: vec![OpenBlock::carrier_buffer(&carrier)],
            carrier,
            mem,
            world,
        }
    }

    /// The values one edge carries: the enclosing loop counters and limits, the
    /// carried pointer, one value per active width, then the current memory and
    /// world tokens.
    fn edge_args(&mut self, block: &mut OpenBlock, ivs: &[Value]) -> Vec<Value> {
        let mut values = ivs.to_vec();
        values.push(block.carrier.clone());
        for width in self.active.clone() {
            let value = self.value_of(block, width);
            values.push(value);
        }
        values.push(block.mem.clone());
        values.push(block.world.clone());
        values
    }

    fn value_of(&mut self, block: &mut OpenBlock, width: u16) -> Value {
        if let Some(values) = block.pool.get(&width) {
            if !values.is_empty() {
                let index = self.rng.below(values.len() as u64) as usize;
                return values[index].clone();
            }
        }
        self.emit_random_const(block, width)
    }

    fn emit_random_const(&mut self, block: &mut OpenBlock, width: u16) -> Value {
        // Zero and one are drawn far more often than chance would give them:
        // they are the operands every P0 identity matches on, and a gate that
        // never produced them would never exercise the rules it exists to guard.
        let raw = if self.rng.chance(45) {
            self.rng.below(2)
        } else {
            self.rng.next_u64()
        };
        self.emit_const(block, width, raw)
    }

    fn emit_const(&mut self, block: &mut OpenBlock, width: u16, raw: u64) -> Value {
        let value = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::Const(value.clone(), raw & mask(width)));
        self.record(block, &value);
        value
    }

    // -- regions ------------------------------------------------------------

    fn budget_left(&self) -> bool {
        self.next_block + 4 <= self.config.max_blocks
    }

    fn emit_region(&mut self, mut block: OpenBlock, depth: u32) -> OpenBlock {
        let segments = 1 + self.rng.below(2);
        for _ in 0..segments {
            self.emit_linear(&mut block);
            if depth == 0 || !self.budget_left() {
                continue;
            }
            match self.rng.below(4) {
                0 | 1 => {}
                2 => block = self.emit_branch(block, depth - 1),
                _ => block = self.emit_loop(block, depth - 1),
            }
        }
        self.emit_linear(&mut block);
        block
    }

    fn emit_linear(&mut self, block: &mut OpenBlock) {
        let count = self
            .rng
            .below(u64::from(self.config.max_segment_instructions) + 1);
        for _ in 0..count {
            self.emit_instruction(block);
        }
    }

    fn emit_branch(&mut self, mut block: OpenBlock, depth: u32) -> OpenBlock {
        let condition = self.value_of(&mut block, 1);
        let then_id = self.fresh_block();
        let else_id = self.fresh_block();
        let join_id = self.fresh_block();
        let ivs = block.ivs.clone();
        let then_args = self.edge_args(&mut block, &ivs);
        let else_args = self.edge_args(&mut block, &ivs);
        self.close(
            block,
            Inst::CondBr(condition, then_id, then_args, else_id, else_args),
        );

        for id in [then_id, else_id] {
            let arm = self.open_block(id, ivs.len());
            let mut arm = self.emit_region(arm, depth);
            let arm_ivs = arm.ivs.clone();
            let args = self.edge_args(&mut arm, &arm_ivs);
            self.close(arm, Inst::Br(join_id, args));
        }

        self.open_block(join_id, ivs.len())
    }

    /// A counted loop whose limit is threaded rather than written into the
    /// header.
    ///
    /// The counter and its limit are a pair of block arguments appended for the
    /// duration of the loop and dropped on the exit edge, so a nested loop adds
    /// its own pair instead of clobbering the enclosing one. The limit may be
    /// derived from a generated value, but it is masked below the configured
    /// ceiling before it reaches the edge: the trip count is data-dependent and
    /// still bounded by construction rather than by interpreter fuel.
    fn emit_loop(&mut self, mut block: OpenBlock, depth: u32) -> OpenBlock {
        let header_id = self.fresh_block();
        let body_id = self.fresh_block();
        let exit_id = self.fresh_block();
        let outer = block.ivs.clone();

        let zero = self.emit_const(&mut block, 64, 0);
        let limit = if self.rng.chance(50) {
            let trips = 1 + self.rng.below(self.config.max_loop_trips);
            self.emit_const(&mut block, 64, trips)
        } else {
            let raw = self.value_of(&mut block, 64);
            let ceiling = self.emit_const(&mut block, 64, self.config.max_loop_trips - 1);
            let clamped = self.fresh(EvoIRType::Bits(64));
            block.insts.push(Inst::And(clamped.clone(), raw, ceiling));
            self.record(&mut block, &clamped);
            clamped
        };
        let mut entry_ivs = outer.clone();
        entry_ivs.push(zero);
        entry_ivs.push(limit);
        let args = self.edge_args(&mut block, &entry_ivs);
        self.close(block, Inst::Br(header_id, args));

        let mut header = self.open_block(header_id, outer.len() + 2);
        let counter = header.ivs[header.ivs.len() - 2].clone();
        let limit = header.ivs[header.ivs.len() - 1].clone();
        let condition = self.fresh(EvoIRType::Bits(1));
        header
            .insts
            .push(Inst::ICmp(condition.clone(), CmpOp::ULt, counter, limit));
        self.record(&mut header, &condition);
        let header_ivs = header.ivs.clone();
        let body_args = self.edge_args(&mut header, &header_ivs);
        let exit_args = self.edge_args(&mut header, &header_ivs[..header_ivs.len() - 2]);
        self.close(
            header,
            Inst::CondBr(condition, body_id, body_args, exit_id, exit_args),
        );

        let body = self.open_block(body_id, outer.len() + 2);
        let mut body = self.emit_region(body, depth);
        let body_ivs = body.ivs.clone();
        let counter = body_ivs[body_ivs.len() - 2].clone();
        let limit = body_ivs[body_ivs.len() - 1].clone();
        let one = self.emit_const(&mut body, 64, 1);
        let next = self.fresh(EvoIRType::Bits(64));
        body.insts.push(Inst::Add(next.clone(), counter, one));
        self.record(&mut body, &next);
        let mut back = body_ivs[..body_ivs.len() - 2].to_vec();
        back.push(next);
        back.push(limit);
        let args = self.edge_args(&mut body, &back);
        self.close(body, Inst::Br(header_id, args));

        self.open_block(exit_id, outer.len())
    }

    // -- instructions -------------------------------------------------------

    fn emit_instruction(&mut self, block: &mut OpenBlock) {
        match self.rng.below(100) {
            0..=7 => {
                let width = *self.rng.pick(&self.active.clone());
                self.emit_random_const(block, width);
            }
            8..=24 => self.emit_binary(block),
            25..=31 => self.emit_shift(block),
            32..=39 => self.emit_div_rem(block),
            40..=48 => self.emit_compare(block),
            49..=55 => self.emit_cast(block),
            56..=62 => self.emit_predicate(block),
            63..=70 => self.emit_bait(block),
            71..=74 => self.emit_fact_bait(block),
            75..=80 => self.emit_memory(block),
            81..=84 => self.emit_global(block),
            85..=87 => self.emit_call(block),
            88..=90 => self.emit_indirect_call(block),
            91..=92 => self.emit_copy(block),
            93..=94 => self.emit_lifetime_end(block),
            95..=96 => self.emit_null_comparison(block),
            97..=98 => self.emit_defined_left_shift(block),
            _ => self.emit_carrier_rebind(block),
        }
    }

    fn emit_binary(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&self.active.clone());
        let lhs = self.value_of(block, width);
        let rhs = self.value_of(block, width);
        let result = self.fresh(EvoIRType::Bits(width));
        let inst = match self.rng.below(6) {
            0 => Inst::Add(result.clone(), lhs, rhs),
            1 => Inst::Sub(result.clone(), lhs, rhs),
            2 => Inst::Mul(result.clone(), lhs, rhs),
            3 => Inst::And(result.clone(), lhs, rhs),
            4 => Inst::Or(result.clone(), lhs, rhs),
            _ => Inst::Xor(result.clone(), lhs, rhs),
        };
        block.insts.push(inst);
        self.record(block, &result);
    }

    fn emit_shift(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&self.active.clone());
        let amount_width = *self.rng.pick(&self.active.clone());
        let shifted = self.value_of(block, width);
        let raw = self.value_of(block, amount_width);

        // The largest power-of-two bound below the shifted width. Masking with
        // it is a rule about the type rather than a list of safe amounts: it
        // holds for every width, including one.
        let bound = ((1_u64 << width.ilog2()) - 1) & mask(amount_width);
        let limit = self.emit_const(block, amount_width, bound);
        let amount = self.fresh(EvoIRType::Bits(amount_width));
        block
            .insts
            .push(Inst::And(amount.clone(), raw, limit.clone()));
        self.record(block, &amount);

        let defined = self.fresh(EvoIRType::Bits(1));
        block.insts.push(Inst::ShiftCountDefined {
            result: defined.clone(),
            shifted: shifted.clone(),
            amount: amount.clone(),
            amount_signed: false,
        });
        self.record(block, &defined);
        block
            .insts
            .push(Inst::UbAssume(defined, UbReason::InvalidShiftCount));

        let result = self.fresh(EvoIRType::Bits(width));
        let inst = match self.rng.below(3) {
            0 => Inst::Shl(result.clone(), shifted, amount),
            1 => Inst::LShr(result.clone(), shifted, amount),
            _ => Inst::AShr(result.clone(), shifted, amount),
        };
        block.insts.push(inst);
        self.record(block, &result);
    }

    fn emit_div_rem(&mut self, block: &mut OpenBlock) {
        let widths: Vec<u16> = self
            .active
            .iter()
            .copied()
            .filter(|width| *width >= 2)
            .collect();
        if widths.is_empty() {
            self.emit_binary(block);
            return;
        }
        let width = *self.rng.pick(&widths);
        let lhs = self.value_of(block, width);
        let raw = self.value_of(block, width);

        // Clearing the sign bit and then setting the low bit lands the divisor
        // in [1, 2^(N-1)): never zero, and never the signed minus one that is
        // the other half of the signed overflow obligation.
        let positive = self.emit_const(block, width, mask(width) >> 1);
        let cleared = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(cleared.clone(), raw, positive.clone()));
        self.record(block, &cleared);
        let one = self.emit_const(block, width, 1);
        let divisor = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::Or(divisor.clone(), cleared, one.clone()));
        self.record(block, &divisor);

        let signed = self.rng.chance(50);
        let remainder = self.rng.chance(50);
        let nonzero = self.fresh(EvoIRType::Bits(1));
        block
            .insts
            .push(Inst::DivisorNonZero(nonzero.clone(), divisor.clone()));
        self.record(block, &nonzero);
        let reason = if remainder {
            UbReason::RemainderByZero
        } else {
            UbReason::DivisionByZero
        };
        block.insts.push(Inst::UbAssume(nonzero, reason));

        if signed {
            let no_overflow = self.fresh(EvoIRType::Bits(1));
            block.insts.push(Inst::SignedDivRemNoOverflow(
                no_overflow.clone(),
                lhs.clone(),
                divisor.clone(),
            ));
            self.record(block, &no_overflow);
            block
                .insts
                .push(Inst::UbAssume(no_overflow, UbReason::SignedOverflow));
        }

        let result = self.fresh(EvoIRType::Bits(width));
        let inst = match (signed, remainder) {
            (true, false) => Inst::SDiv(result.clone(), lhs, divisor),
            (true, true) => Inst::SRem(result.clone(), lhs, divisor),
            (false, false) => Inst::UDiv(result.clone(), lhs, divisor),
            (false, true) => Inst::URem(result.clone(), lhs, divisor),
        };
        block.insts.push(inst);
        self.record(block, &result);
    }

    fn emit_compare(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&self.active.clone());
        let lhs = self.value_of(block, width);
        let rhs = self.value_of(block, width);
        let operation = self.random_comparison();
        let result = self.fresh(EvoIRType::Bits(1));
        block
            .insts
            .push(Inst::ICmp(result.clone(), operation, lhs, rhs));
        self.record(block, &result);
    }

    fn random_comparison(&mut self) -> CmpOp {
        match self.rng.below(6) {
            0 => CmpOp::Eq,
            1 => CmpOp::Ne,
            2 => CmpOp::SLt,
            3 => CmpOp::SLe,
            4 => CmpOp::ULt,
            _ => CmpOp::ULe,
        }
    }

    fn emit_cast(&mut self, block: &mut OpenBlock) {
        let first = *self.rng.pick(&self.active.clone());
        let second = *self.rng.pick(&self.active.clone());
        let (narrow, wide) = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        match self.rng.below(3) {
            0 => {
                let source = self.value_of(block, narrow);
                let result = self.fresh(EvoIRType::Bits(wide));
                block
                    .insts
                    .push(Inst::SExt(result.clone(), source, EvoIRType::Bits(wide)));
                self.record(block, &result);
            }
            1 => {
                let source = self.value_of(block, narrow);
                let result = self.fresh(EvoIRType::Bits(wide));
                block
                    .insts
                    .push(Inst::ZExt(result.clone(), source, EvoIRType::Bits(wide)));
                self.record(block, &result);
            }
            _ => {
                let source = self.value_of(block, wide);
                let result = self.fresh(EvoIRType::Bits(narrow));
                block
                    .insts
                    .push(Inst::Trunc(result.clone(), source, EvoIRType::Bits(narrow)));
                self.record(block, &result);
            }
        }
    }

    /// Definedness predicates emitted as ordinary values.
    ///
    /// Only a predicate the sanitizing shapes above have already made true is
    /// ever handed to `ub.assume`; these are computed and left in the value
    /// pool, because the predicates are total and a false one is a fact, not a
    /// fault.
    fn emit_predicate(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&self.active.clone());
        let lhs = self.value_of(block, width);
        let rhs = self.value_of(block, width);
        let result = self.fresh(EvoIRType::Bits(1));
        let inst = match self.rng.below(6) {
            0 => Inst::SignedAddDefined(result.clone(), lhs, rhs),
            1 => Inst::SignedSubDefined(result.clone(), lhs, rhs),
            2 => Inst::SignedMulDefined(result.clone(), lhs, rhs),
            3 => Inst::DivisorNonZero(result.clone(), rhs),
            4 => Inst::SignedDivRemNoOverflow(result.clone(), lhs, rhs),
            _ => Inst::SignedShlDefined {
                result: result.clone(),
                shifted: lhs,
                amount: rhs,
                amount_signed: self.rng.chance(50),
            },
        };
        block.insts.push(inst);
        self.record(block, &result);
    }

    /// The six P0 identities, emitted in the exact shape the reviewed registry
    /// matches. Without them the gate would depend on chance to produce the
    /// operands the rules it guards need.
    fn emit_bait(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&self.active.clone());
        let operand = self.value_of(block, width);
        match self.rng.below(6) {
            0 => {
                let zero = self.emit_const(block, width, 0);
                let result = self.fresh(EvoIRType::Bits(width));
                block.insts.push(Inst::Add(result.clone(), operand, zero));
                self.record(block, &result);
            }
            1 => {
                let one = self.emit_const(block, width, 1);
                let result = self.fresh(EvoIRType::Bits(width));
                block.insts.push(Inst::Mul(result.clone(), operand, one));
                self.record(block, &result);
            }
            2 => {
                let zero = self.emit_const(block, width, 0);
                let result = self.fresh(EvoIRType::Bits(width));
                block.insts.push(Inst::And(result.clone(), operand, zero));
                self.record(block, &result);
            }
            3 => {
                let zero = self.emit_const(block, width, 0);
                let result = self.fresh(EvoIRType::Bits(width));
                block.insts.push(Inst::Or(result.clone(), operand, zero));
                self.record(block, &result);
            }
            4 => {
                let zero = self.emit_const(block, width, 0);
                let inner = self.fresh(EvoIRType::Bits(width));
                block
                    .insts
                    .push(Inst::Sub(inner.clone(), zero.clone(), operand));
                self.record(block, &inner);
                let result = self.fresh(EvoIRType::Bits(width));
                block.insts.push(Inst::Sub(result.clone(), zero, inner));
                self.record(block, &result);
            }
            _ => {
                let operation = self.random_comparison();
                let result = self.fresh(EvoIRType::Bits(1));
                block.insts.push(Inst::ICmp(
                    result.clone(),
                    operation,
                    operand.clone(),
                    operand,
                ));
                self.record(block, &result);
            }
        }
    }

    /// The shapes the fact-driven rules match on.
    ///
    /// Masking is the only way a generator can put a known bit into a value it
    /// does not otherwise control: `and(%v, 0x0f)` knows its high half zero and
    /// `or(%w, 0x0f)` knows its low half one, so one covers the other and their
    /// conjunction is the masked operand. A value confined to a range below a
    /// constant decides every comparison the table decides. Without these the
    /// gate would depend on chance to produce a covering mask or a decidable
    /// ordering, and the rules that read them would go unmeasured.
    fn emit_fact_bait(&mut self, block: &mut OpenBlock) {
        // A multiply by a literal two, which nothing else in this generator
        // writes. The other two spellings of a doubling -- a self-addition and
        // a shift by one -- already occur, so this is the only shape the
        // emitting rules of `p0.scalar.v4` would otherwise have no source for.
        // It is emitted at every site rather than drawn as one group of
        // several, for the reason the known-zero identity is.
        self.emit_doubling(block);
        let wide = self
            .active
            .iter()
            .copied()
            .filter(|width| *width >= 8)
            .collect::<Vec<_>>();
        if wide.is_empty() {
            // Under eight bits a half mask is zero or one, which the canonical
            // reductions already match; the known-zero identity still applies.
            self.emit_known_zero_identity(block);
            return;
        }
        let width = *self.rng.pick(&wide);
        let half = width / 2;
        let low = (1_u64 << half) - 1;
        // Drawn rather than taken in turn: a generator is built per function,
        // so a counter would give the last group only to functions with four
        // bait sites, and most have fewer.
        // The known-zero identity is emitted at every site rather than drawn
        // as one group of several. Its two rules have no other source in the
        // generator, it is three instructions, and leaving a rule's only source
        // to a draw is what made coverage move under every earlier change.
        self.emit_known_zero_identity(block);
        match self.rng.below(5) {
            0 => self.emit_covering_mask(block, width, low),
            1 => self.emit_bounded_comparison(block, width, low),
            2 => self.emit_shared_subexpression(block, width),
            3 => self.emit_shifted_and_widened(block, width, low),
            _ => self.emit_known_constants(block, width, low),
        }
    }

    /// A value whose bits a shift or a width change makes known, handed to a
    /// rule that reads them.
    ///
    /// `shl` by half the width knows its low half zero and `zext` from half the
    /// width knows its high half zero, both by rows Section 8.10 gained at
    /// revision 1.9.43. Each is then masked by a constant that covers what it
    /// left live, which is the covering identity's shape. Whether either fact
    /// can reach a region is a separate question and a measured one: Section
    /// 8.5's operator allowlist admits neither operator as a witness.
    fn emit_shifted_and_widened(&mut self, block: &mut OpenBlock, width: u16, low: u64) {
        let half = width / 2;
        let high = mask(width) & !low;

        // The shift, guarded exactly as `emit_shift` guards one: the amount is
        // masked into range and the predicate is asserted before the shift.
        let value = self.value_of(block, width);
        let amount = self.emit_const(block, width, u64::from(half));
        let defined = self.fresh(EvoIRType::Bits(1));
        block.insts.push(Inst::ShiftCountDefined {
            result: defined.clone(),
            shifted: value.clone(),
            amount: amount.clone(),
            amount_signed: false,
        });
        self.record(block, &defined);
        block
            .insts
            .push(Inst::UbAssume(defined, UbReason::InvalidShiftCount));
        let shifted = self.fresh(EvoIRType::Bits(width));
        block.insts.push(Inst::Shl(shifted.clone(), value, amount));
        self.record(block, &shifted);
        let high_mask = self.emit_const(block, width, high);
        let covered = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(covered.clone(), shifted, high_mask));
        self.record(block, &covered);

        // The width change, when the profile's widths leave room for one.
        if half >= 1 {
            let narrow = self.value_of(block, width);
            let truncated = self.fresh(EvoIRType::Bits(half));
            block.insts.push(Inst::Trunc(
                truncated.clone(),
                narrow,
                EvoIRType::Bits(half),
            ));
            self.record(block, &truncated);
            let widened = self.fresh(EvoIRType::Bits(width));
            block.insts.push(Inst::ZExt(
                widened.clone(),
                truncated,
                EvoIRType::Bits(width),
            ));
            self.record(block, &widened);
            let low_mask = self.emit_const(block, width, low);
            let masked = self.fresh(EvoIRType::Bits(width));
            block
                .insts
                .push(Inst::And(masked.clone(), widened, low_mask));
            self.record(block, &masked);
        }
    }

    /// One expression written twice, and every comparison of the two spellings.
    ///
    /// Section 7.4 makes the two conjunctions one node, so each comparison is a
    /// comparison of a value with itself and the reviewed registry answers it.
    /// A matcher that compared operands by name would see two different values
    /// and answer nothing, which is the whole point of the shape. The first
    /// copy is written through a value a canonical reduction replaces, so the
    /// two are congruent only after that merge rather than as written.
    fn emit_shared_subexpression(&mut self, block: &mut OpenBlock, width: u16) {
        let left = self.value_of(block, width);
        let right = self.value_of(block, width);
        let zero = self.emit_const(block, width, 0);
        let shifted = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::Add(shifted.clone(), left.clone(), zero));
        self.record(block, &shifted);
        let first = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(first.clone(), shifted, right.clone()));
        self.record(block, &first);
        let second = self.fresh(EvoIRType::Bits(width));
        block.insts.push(Inst::And(second.clone(), left, right));
        self.record(block, &second);
        for operation in [
            CmpOp::Eq,
            CmpOp::Ne,
            CmpOp::SLt,
            CmpOp::SLe,
            CmpOp::ULt,
            CmpOp::ULe,
        ] {
            let result = self.fresh(EvoIRType::Bits(1));
            block.insts.push(Inst::ICmp(
                result.clone(),
                operation,
                first.clone(),
                second.clone(),
            ));
            self.record(block, &result);
        }
    }

    /// The shapes whose every bit the analysis knows without any operand being
    /// a constant class of its own.
    fn emit_known_constants(&mut self, block: &mut OpenBlock, width: u16, low: u64) {
        let value = self.value_of(block, width);
        let other = self.value_of(block, width);
        let low_mask = self.emit_const(block, width, low);
        let high_mask = self.emit_const(block, width, mask(width) & !low);
        let cleared = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(cleared.clone(), value.clone(), low_mask));
        self.record(block, &cleared);
        let raised = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(raised.clone(), other, high_mask));
        self.record(block, &raised);
        // Every bit is known zero, because one operand or the other knows it,
        // while neither operand's own class is known.
        let vanished = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(vanished.clone(), cleared, raised));
        self.record(block, &vanished);
        // Every bit is known one.
        let ones = self.emit_const(block, width, mask(width));
        let saturated = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::Or(saturated.clone(), value.clone(), ones));
        self.record(block, &saturated);
        // A product with a known zero is known zero whatever it multiplies.
        let zero = self.emit_const(block, width, 0);
        let product = self.fresh(EvoIRType::Bits(width));
        block.insts.push(Inst::Mul(product.clone(), value, zero));
        self.record(block, &product);
    }

    /// A masked value and a mask that covers it, combined in both orders and by
    /// both operators, which is the complete set of covering rules.
    fn emit_covering_mask(&mut self, block: &mut OpenBlock, width: u16, low: u64) {
        let value = self.value_of(block, width);
        let other = self.value_of(block, width);
        let mask = self.emit_const(block, width, low);
        let masked = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(masked.clone(), value, mask.clone()));
        self.record(block, &masked);
        let covering = self.fresh(EvoIRType::Bits(width));
        block.insts.push(Inst::Or(covering.clone(), other, mask));
        self.record(block, &covering);
        // Both operators in both operand orders, which is the complete set of
        // four covering rules. Drawing one of the four at random would leave
        // the campaign's coverage of the other three to luck.
        for masked_on_the_left in [true, false] {
            let (left, right) = if masked_on_the_left {
                (masked.clone(), covering.clone())
            } else {
                (covering.clone(), masked.clone())
            };
            for conjunction in [true, false] {
                let result = self.fresh(EvoIRType::Bits(width));
                block.insts.push(if conjunction {
                    Inst::And(result.clone(), left.clone(), right.clone())
                } else {
                    Inst::Or(result.clone(), left.clone(), right.clone())
                });
                self.record(block, &result);
            }
        }
    }

    /// A multiply by two, in both operand orders.
    ///
    /// Section 11.6's emitting rules match `mul(?x, const_like(?x, 2))` in
    /// either order under `width_min(?x, 2)`; both orders are emitted, for the
    /// reason the covering bait emits all four of its combinations. A width of
    /// one is skipped because the side condition excludes it.
    fn emit_doubling(&mut self, block: &mut OpenBlock) {
        let widths = self
            .active
            .iter()
            .copied()
            .filter(|width| *width >= 2)
            .collect::<Vec<_>>();
        if widths.is_empty() {
            return;
        }
        let width = *self.rng.pick(&widths);
        let value = self.value_of(block, width);
        let two = self.emit_const(block, width, 2);
        for two_on_the_left in [false, true] {
            let (left, right) = if two_on_the_left {
                (two.clone(), value.clone())
            } else {
                (value.clone(), two.clone())
            };
            let result = self.fresh(EvoIRType::Bits(width));
            block.insts.push(Inst::Mul(result.clone(), left, right));
            self.record(block, &result);
        }
    }

    /// `xor` against a zero, in both operand orders.
    fn emit_known_zero_identity(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&self.active.clone());
        let value = self.value_of(block, width);
        let zero = self.emit_const(block, width, 0);
        // Both orders, for the reason the covering bait emits all four of its
        // combinations: one of the two rules would otherwise be left to luck.
        for zero_on_the_left in [false, true] {
            let (left, right) = if zero_on_the_left {
                (zero.clone(), value.clone())
            } else {
                (value.clone(), zero.clone())
            };
            let result = self.fresh(EvoIRType::Bits(width));
            block.insts.push(Inst::Xor(result.clone(), left, right));
            self.record(block, &result);
        }
    }

    /// A value confined to `0..=low` compared against `low + 1`.
    ///
    /// Every ordering is decided by the interval rule, the sign bit of the
    /// masked operand is known zero so the signed orderings are decided too,
    /// and equality is decided by a bit the two disagree on.
    fn emit_bounded_comparison(&mut self, block: &mut OpenBlock, width: u16, low: u64) {
        let value = self.value_of(block, width);
        let mask = self.emit_const(block, width, low);
        let masked = self.fresh(EvoIRType::Bits(width));
        block.insts.push(Inst::And(masked.clone(), value, mask));
        self.record(block, &masked);
        let bound = self.emit_const(block, width, low + 1);
        // All six comparisons against the same bound, for the reason the
        // covering bait emits all four of its combinations: drawing one would
        // leave the campaign's coverage of the other five to luck, and a
        // region is admitted only when its whole closure is scalar.
        for operation in [
            CmpOp::Eq,
            CmpOp::Ne,
            CmpOp::SLt,
            CmpOp::SLe,
            CmpOp::ULt,
            CmpOp::ULe,
        ] {
            let result = self.fresh(EvoIRType::Bits(1));
            block.insts.push(Inst::ICmp(
                result.clone(),
                operation,
                masked.clone(),
                bound.clone(),
            ));
            self.record(block, &result);
        }
    }

    // -- memory -------------------------------------------------------------

    /// Allocate one object of `count` elements and store every element, so no
    /// later load can read an uninitialized byte.
    fn emit_buffer(&mut self, block: &mut OpenBlock, width: u16, count: u64) -> Buffer {
        let bytes = u64::from(width / 8);
        let align = u32::try_from(bytes).expect("a memory width is at most eight bytes");
        let origin = self.take_origin();

        let pointer = self.fresh(pointer_type());
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Alloca {
            result: pointer.clone(),
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
            origin,
            layout: AccessLayout::new(bytes * count, align),
        });
        block.mem = mem_out;
        block.world = world_out;

        let mut elements = vec![pointer.clone()];
        for index in 1..count {
            let offset = self.emit_const(block, 64, index);
            let element = self.fresh(pointer_type());
            block.insts.push(Inst::PtrOffset {
                result: element.clone(),
                base: pointer.clone(),
                index: offset,
                direction: OffsetDirection::Add,
                signedness: IndexSignedness::Unsigned,
                scale: bytes,
            });
            let guarded = self.fresh(memory_type());
            block.insts.push(Inst::PointerValid {
                mem_in: block.mem.clone(),
                pointer: element.clone(),
                mem_out: guarded.clone(),
            });
            block.mem = guarded;
            elements.push(element);
        }

        let layout = AccessLayout::new(bytes, align);
        for element in elements.clone() {
            let value = self.value_of(block, width);
            self.emit_store(block, &element, &value, layout, false);
        }
        let buffer = Buffer {
            elements,
            width,
            layout,
        };
        block.buffers.push(buffer.clone());
        buffer
    }

    /// A fresh, fully initialized carrier object.
    fn emit_carrier_object(&mut self, block: &mut OpenBlock) -> Value {
        self.emit_buffer(block, CARRIER_WIDTH, 1).elements[0].clone()
    }

    fn emit_carrier_rebind(&mut self, block: &mut OpenBlock) {
        let carrier = self.emit_carrier_object(block);
        block.carrier = carrier;
    }

    /// A signed left shift the C obligation admits.
    ///
    /// `signed_shl_defined` is false for an invalid count, a negative left
    /// operand, or a result that does not fit the signed width. Masking the
    /// operand down to its low bit and shifting by one satisfies all three at
    /// every width of at least four, so the assumption this emits is true by
    /// the shape rather than by luck. It is the last of the five undefined-
    /// behaviour obligations the gate had never asserted.
    fn emit_defined_left_shift(&mut self, block: &mut OpenBlock) {
        let widths: Vec<u16> = self
            .active
            .iter()
            .copied()
            .filter(|width| *width >= 4)
            .collect();
        if widths.is_empty() {
            self.emit_binary(block);
            return;
        }
        let width = *self.rng.pick(&widths);
        let raw = self.value_of(block, width);
        let mask = self.emit_const(block, width, 1);
        let operand = self.fresh(EvoIRType::Bits(width));
        block
            .insts
            .push(Inst::And(operand.clone(), raw, mask.clone()));
        self.record(block, &operand);
        let amount = self.emit_const(block, width, 1);

        let counted = self.fresh(EvoIRType::Bits(1));
        block.insts.push(Inst::ShiftCountDefined {
            result: counted.clone(),
            shifted: operand.clone(),
            amount: amount.clone(),
            amount_signed: false,
        });
        self.record(block, &counted);
        block
            .insts
            .push(Inst::UbAssume(counted, UbReason::InvalidShiftCount));

        let representable = self.fresh(EvoIRType::Bits(1));
        block.insts.push(Inst::SignedShlDefined {
            result: representable.clone(),
            shifted: operand.clone(),
            amount: amount.clone(),
            amount_signed: false,
        });
        self.record(block, &representable);
        block.insts.push(Inst::UbAssume(
            representable,
            UbReason::InvalidSignedLeftShift,
        ));

        let result = self.fresh(EvoIRType::Bits(width));
        block.insts.push(Inst::Shl(result.clone(), operand, amount));
        self.record(block, &result);
    }

    fn emit_memory(&mut self, block: &mut OpenBlock) {
        if self.rng.chance(55) {
            self.emit_buffer_access(block);
            return;
        }
        let width = *self.rng.pick(&MEMORY_WIDTHS);
        let count = 1 + self.rng.below(3);
        self.emit_buffer(block, width, count);
    }

    fn emit_buffer_access(&mut self, block: &mut OpenBlock) {
        let index = self.rng.below(block.buffers.len() as u64) as usize;
        let buffer = block.buffers[index].clone();
        let element = self.rng.pick(&buffer.elements).clone();
        let volatile = self.rng.chance(30);
        if self.rng.chance(50) {
            let value = self.value_of(block, buffer.width);
            self.emit_store(block, &element, &value, buffer.layout, volatile);
        } else {
            self.emit_load(block, &element, buffer.width, buffer.layout, volatile);
        }
    }

    fn emit_global(&mut self, block: &mut OpenBlock) {
        if self.globals.is_empty() {
            self.emit_binary(block);
            return;
        }
        let index = self.rng.below(self.globals.len() as u64) as usize;
        let symbol = self.globals[index].clone();
        let pointer = self.fresh(pointer_type());
        block.insts.push(Inst::GlobalAddr {
            result: pointer.clone(),
            symbol,
        });
        block.buffers.push(Buffer {
            elements: vec![pointer],
            width: CARRIER_WIDTH,
            layout: CARRIER,
        });
        self.emit_buffer_access(block);
    }

    /// A copy between two ranges of one object.
    ///
    /// The destination and source are either the same element or two different
    /// elements of a two-element object, so the ranges are identical or
    /// disjoint and never the partial overlap that reports `E3027`.
    fn emit_copy(&mut self, block: &mut OpenBlock) {
        let width = *self.rng.pick(&MEMORY_WIDTHS);
        let buffer = self.emit_buffer(block, width, 2);
        let destination = buffer.elements[0].clone();
        let source = if self.rng.chance(30) {
            destination.clone()
        } else if self.rng.chance(50) {
            buffer.elements[1].clone()
        } else {
            // Two separate objects cannot overlap at all, which is the other
            // way Section 3.8 admits a copy. Every byte of the second is
            // written by `emit_buffer` before the copy reads it.
            self.emit_buffer(block, width, 1).elements[0].clone()
        };
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Copy {
            destination,
            source,
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
            layout: buffer.layout,
            volatile: evocc_evoir::CopyVolatility::None,
        });
        block.mem = mem_out;
        block.world = world_out;
    }

    /// End the lifetime of an object nothing can reach afterwards.
    ///
    /// The object is allocated for this purpose and never recorded as a buffer
    /// or handed to a successor, so no later access and no frame cleanup can
    /// find it alive. Ending a carrier instead would make every access in every
    /// successor a dangling one.
    fn emit_lifetime_end(&mut self, block: &mut OpenBlock) {
        let origin = self.take_origin();
        let pointer = self.fresh(pointer_type());
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Alloca {
            result: pointer.clone(),
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
            origin,
            layout: CARRIER,
        });
        block.mem = mem_out;
        block.world = world_out;

        let mem_after = self.fresh(memory_type());
        let world_after = self.fresh(world_type());
        block.insts.push(Inst::LifetimeEnd {
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            pointer,
            mem_out: mem_after.clone(),
            world_out: world_after.clone(),
        });
        block.mem = mem_after;
        block.world = world_after;
    }

    /// A null pointer that is compared and never dereferenced.
    ///
    /// Pointer equality is total and compares provenance and offset, so this is
    /// defined. The value is not recorded anywhere a later access could reach
    /// it, and it never becomes a carrier: forming an offset from it or reading
    /// through it would be `E3009`.
    fn emit_null_comparison(&mut self, block: &mut OpenBlock) {
        let null = self.fresh(pointer_type());
        block.insts.push(Inst::PtrNull(null.clone()));
        let other = if self.rng.chance(50) {
            block.carrier.clone()
        } else {
            null.clone()
        };
        let operation = if self.rng.chance(50) {
            CmpOp::Eq
        } else {
            CmpOp::Ne
        };
        let result = self.fresh(EvoIRType::Bits(1));
        block
            .insts
            .push(Inst::ICmp(result.clone(), operation, null, other));
        self.record(block, &result);
    }

    fn emit_store(
        &mut self,
        block: &mut OpenBlock,
        address: &Value,
        value: &Value,
        layout: AccessLayout,
        volatile: bool,
    ) {
        let guarded = self.fresh(memory_type());
        block.insts.push(Inst::AccessInBounds {
            mem_in: block.mem.clone(),
            pointer: address.clone(),
            mem_out: guarded.clone(),
            layout,
        });
        block.mem = guarded.clone();
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Store {
            address: address.clone(),
            value: value.clone(),
            mem_in: guarded,
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
            volatile,
        });
        block.mem = mem_out;
        block.world = world_out;
    }

    fn emit_load(
        &mut self,
        block: &mut OpenBlock,
        address: &Value,
        width: u16,
        layout: AccessLayout,
        volatile: bool,
    ) {
        let guarded = self.fresh(memory_type());
        block.insts.push(Inst::AccessInBounds {
            mem_in: block.mem.clone(),
            pointer: address.clone(),
            mem_out: guarded.clone(),
            layout,
        });
        block.mem = guarded.clone();
        let result = self.fresh(EvoIRType::Bits(width));
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Load {
            result: result.clone(),
            address: address.clone(),
            mem_in: guarded,
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
            volatile,
        });
        block.mem = mem_out;
        block.world = world_out;
        self.record(block, &result);
    }

    // -- calls --------------------------------------------------------------

    fn call_arguments(&mut self, block: &mut OpenBlock, signature: &Signature) -> Vec<Value> {
        let mut arguments = Vec::new();
        for (index, ty) in signature.params.clone().into_iter().enumerate() {
            let value = match ty {
                // A recursive callee decrements its first parameter until it
                // reaches zero, so the chain is exactly as long as the constant
                // handed to it. An arbitrary value would still terminate, but
                // its length would be a draw rather than a bound.
                EvoIRType::Bits(width) if signature.recursive && index == 0 => {
                    let depth = self.rng.below(MAX_RECURSION_DEPTH + 1);
                    self.emit_const(block, width, depth)
                }
                EvoIRType::Bits(width) => self.value_of(block, width),
                _ => block.carrier.clone(),
            };
            arguments.push(value);
        }
        arguments
    }

    fn call_results(&mut self, signature: &Signature) -> Vec<Value> {
        signature
            .results
            .clone()
            .into_iter()
            .map(|ty| self.fresh(ty))
            .collect()
    }

    /// Register a pointer a call returned, so something reads it.
    ///
    /// It is a global's address by construction, which is eight initialized
    /// bytes of static storage, so the carrier shape applies to it unchanged.
    fn adopt_results(&mut self, block: &mut OpenBlock, results: &[Value]) {
        for result in results {
            if matches!(result.ty, EvoIRType::Ptr(_)) {
                block.buffers.push(OpenBlock::carrier_buffer(result));
            } else {
                self.record(block, result);
            }
        }
    }

    /// The address of one module global.
    fn emit_global_address(&mut self, block: &mut OpenBlock) -> Value {
        let index = self.rng.below(self.globals.len() as u64) as usize;
        let symbol = self.globals[index].clone();
        let pointer = self.fresh(pointer_type());
        block.insts.push(Inst::GlobalAddr {
            result: pointer.clone(),
            symbol,
        });
        pointer
    }

    fn emit_call(&mut self, block: &mut OpenBlock) {
        if self.callees.is_empty() {
            self.emit_binary(block);
            return;
        }
        let index = self.rng.below(self.callees.len() as u64) as usize;
        let signature = self.callees[index].clone();
        let args = self.call_arguments(block, &signature);
        let results = self.call_results(&signature);
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Call {
            arg_marks: Vec::new(),
            returns_twice: false,
            results: results.clone(),
            callee: signature.name.clone(),
            args,
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
        });
        block.mem = mem_out;
        block.world = world_out;
        self.adopt_results(block, &results);
    }

    /// A call through the address of a function this module defines.
    ///
    /// The written signature is the callee's complete lowered interface, so
    /// module validation accepts it and the runtime provenance check finds the
    /// symbol it named.
    fn emit_indirect_call(&mut self, block: &mut OpenBlock) {
        if self.callees.is_empty() {
            self.emit_binary(block);
            return;
        }
        let index = self.rng.below(self.callees.len() as u64) as usize;
        let signature = self.callees[index].clone();
        let callee = self.fresh(signature.pointer_type());
        block.insts.push(Inst::FuncAddr {
            result: callee.clone(),
            symbol: signature.name.clone(),
        });
        let args = self.call_arguments(block, &signature);
        let results = self.call_results(&signature);
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::IndirectCall {
            arg_marks: Vec::new(),
            results: results.clone(),
            callee,
            args,
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
        });
        block.mem = mem_out;
        block.world = world_out;
        self.adopt_results(block, &results);
    }
}
