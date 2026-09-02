//! The deterministic generator of well-typed, validator-clean EvoIR modules.
//!
//! Freedom from undefined behaviour is a property of the *shapes* emitted here,
//! not of a filter applied afterwards. A divisor is sanitized into `[1, 2^(N-1))`
//! before it reaches a division, a shift amount is masked below the shifted
//! width, every allocated byte is stored before anything loads it, and every
//! formed pointer stays inside its object. The gate still discards and counts a
//! program the interpreter reports as undefined, because a generator defect must
//! show up as a number rather than as a silently weakened comparison.

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

/// The shape ceilings one generated module stays within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorConfig {
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
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            max_helper_functions: 2,
            max_globals: 2,
            max_segment_instructions: 6,
            max_region_depth: 2,
            max_blocks: 24,
            max_loop_trips: 4,
        }
    }
}

/// Produce the one module this seed names.
///
/// The same seed and configuration yield the same module forever; the result
/// passes `validate_module` and renders to canonical Section 9 bytes.
#[must_use]
pub fn generate_module(seed: u64, config: GeneratorConfig) -> Module {
    let mut rng = Rng::from_seed(seed);
    let mut origins = 0_u64;
    let globals = generate_globals(&mut rng, config, &mut origins);
    let symbols: Vec<String> = globals.iter().map(|global| global.symbol.clone()).collect();

    let helpers = rng.below(u64::from(config.max_helper_functions) + 1);
    let mut signatures: Vec<Signature> = Vec::new();
    let mut functions: Vec<Function> = Vec::new();
    for index in 0..helpers {
        let signature = random_signature(&mut rng, format!("h{index}"));
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
    let entry = random_signature(&mut rng, "main".to_owned());
    functions.push(generate_function(
        &mut rng,
        config,
        &entry,
        &signatures,
        &symbols,
        &mut origins,
    ));

    let mut module = Module { globals, functions };
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
                symbol: format!("g{index}"),
                origin,
                kind: GlobalKind::File,
                visibility,
                layout: AccessLayout::new(8, 8),
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
    params: Vec<u16>,
    result: u16,
}

fn random_signature(rng: &mut Rng, name: String) -> Signature {
    let count = rng.below(4);
    let params = (0..count).map(|_| *rng.pick(&WIDTHS)).collect();
    let result = *rng.pick(&WIDTHS);
    Signature {
        name,
        params,
        result,
    }
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
    /// The enclosing loop counters, outermost first. Every edge forwards these
    /// unchanged; only a loop's own back edge increments its innermost one.
    ivs: Vec<Value>,
    pool: BTreeMap<u16, Vec<Value>>,
    buffers: Vec<Buffer>,
    mem: Value,
    world: Value,
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
    let mut active = vec![1_u16, 64, signature.result];
    let extra = 1 + rng.below(3);
    for _ in 0..extra {
        let width = if !signature.params.is_empty() && rng.chance(50) {
            *rng.pick(&signature.params)
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
    let result = generator.value_of(&mut block, signature.result);
    let terminator = Inst::Ret(vec![result, block.mem.clone(), block.world.clone()]);
    generator.close(block, terminator);

    Function {
        name: signature.name.clone(),
        params,
        ret_tys: vec![
            EvoIRType::Bits(signature.result),
            memory_type(),
            world_type(),
        ],
        effects: EffectSummary::default(),
        blocks: generator.blocks,
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

    fn open_entry(&mut self, signature: &Signature) -> (Vec<FunctionParam>, OpenBlock) {
        let id = self.fresh_block();
        let mut params = Vec::new();
        let mut args = Vec::new();
        let mut pool: BTreeMap<u16, Vec<Value>> = BTreeMap::new();
        for width in &signature.params {
            let value = self.fresh(EvoIRType::Bits(*width));
            params.push(FunctionParam {
                name: format!("arg{}", params.len()),
                ty: value.ty.clone(),
            });
            args.push(BlockArg {
                name: format!("v{}", value.id),
                value: value.clone(),
            });
            pool.entry(*width).or_default().push(value);
        }
        let mem = self.fresh(memory_type());
        params.push(FunctionParam {
            name: format!("arg{}", params.len()),
            ty: memory_type(),
        });
        args.push(BlockArg {
            name: format!("v{}", mem.id),
            value: mem.clone(),
        });
        let world = self.fresh(world_type());
        params.push(FunctionParam {
            name: format!("arg{}", params.len()),
            ty: world_type(),
        });
        args.push(BlockArg {
            name: format!("v{}", world.id),
            value: world.clone(),
        });
        (
            params,
            OpenBlock {
                id,
                args,
                insts: Vec::new(),
                ivs: Vec::new(),
                pool,
                buffers: Vec::new(),
                mem,
                world,
            },
        )
    }

    fn open_block(&mut self, id: BlockId, iv_count: usize) -> OpenBlock {
        let mut args = Vec::new();
        let mut pool: BTreeMap<u16, Vec<Value>> = BTreeMap::new();
        let mut ivs = Vec::new();
        for _ in 0..iv_count {
            let value = self.fresh(EvoIRType::Bits(64));
            args.push(BlockArg {
                name: format!("v{}", value.id),
                value: value.clone(),
            });
            pool.entry(64).or_default().push(value.clone());
            ivs.push(value);
        }
        for width in self.active.clone() {
            let value = self.fresh(EvoIRType::Bits(width));
            args.push(BlockArg {
                name: format!("v{}", value.id),
                value: value.clone(),
            });
            pool.entry(width).or_default().push(value);
        }
        let mem = self.fresh(memory_type());
        args.push(BlockArg {
            name: format!("v{}", mem.id),
            value: mem.clone(),
        });
        let world = self.fresh(world_type());
        args.push(BlockArg {
            name: format!("v{}", world.id),
            value: world.clone(),
        });
        OpenBlock {
            id,
            args,
            insts: Vec::new(),
            ivs,
            pool,
            buffers: Vec::new(),
            mem,
            world,
        }
    }

    /// The values one edge carries: the enclosing loop counters, one value per
    /// active width, then the current memory and world tokens.
    fn edge_args(&mut self, block: &mut OpenBlock, ivs: &[Value]) -> Vec<Value> {
        let mut values = ivs.to_vec();
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
        // never produces them would never exercise the rules it exists to guard.
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

    /// A counted loop whose trip count no generated value can influence.
    ///
    /// The counter is a dedicated block argument appended for the duration of
    /// the loop and dropped on the exit edge, so a nested loop adds its own
    /// slot instead of clobbering the enclosing one. That is what bounds the
    /// total iteration count by the product of the declared trip counts rather
    /// than leaving termination to the interpreter's fuel.
    fn emit_loop(&mut self, mut block: OpenBlock, depth: u32) -> OpenBlock {
        let trips = 1 + self.rng.below(self.config.max_loop_trips);
        let header_id = self.fresh_block();
        let body_id = self.fresh_block();
        let exit_id = self.fresh_block();
        let outer = block.ivs.clone();

        let zero = self.emit_const(&mut block, 64, 0);
        let mut entry_ivs = outer.clone();
        entry_ivs.push(zero);
        let args = self.edge_args(&mut block, &entry_ivs);
        self.close(block, Inst::Br(header_id, args));

        let mut header = self.open_block(header_id, outer.len() + 1);
        let counter = header.ivs.last().cloned().expect("the loop counter");
        let limit = self.emit_const(&mut header, 64, trips);
        let condition = self.fresh(EvoIRType::Bits(1));
        header
            .insts
            .push(Inst::ICmp(condition.clone(), CmpOp::ULt, counter, limit));
        self.record(&mut header, &condition);
        let header_ivs = header.ivs.clone();
        let body_args = self.edge_args(&mut header, &header_ivs);
        let exit_args = self.edge_args(&mut header, &header_ivs[..header_ivs.len() - 1]);
        self.close(
            header,
            Inst::CondBr(condition, body_id, body_args, exit_id, exit_args),
        );

        let body = self.open_block(body_id, outer.len() + 1);
        let mut body = self.emit_region(body, depth);
        let body_ivs = body.ivs.clone();
        let inner = body_ivs.last().cloned().expect("the loop counter");
        let one = self.emit_const(&mut body, 64, 1);
        let next = self.fresh(EvoIRType::Bits(64));
        body.insts.push(Inst::Add(next.clone(), inner, one));
        self.record(&mut body, &next);
        let mut back = body_ivs[..body_ivs.len() - 1].to_vec();
        back.push(next);
        let args = self.edge_args(&mut body, &back);
        self.close(body, Inst::Br(header_id, args));

        self.open_block(exit_id, outer.len())
    }

    // -- instructions -------------------------------------------------------

    fn emit_instruction(&mut self, block: &mut OpenBlock) {
        match self.rng.below(100) {
            0..=8 => {
                let width = *self.rng.pick(&self.active.clone());
                self.emit_random_const(block, width);
            }
            9..=27 => self.emit_binary(block),
            28..=35 => self.emit_shift(block),
            36..=44 => self.emit_div_rem(block),
            45..=54 => self.emit_compare(block),
            55..=62 => self.emit_cast(block),
            63..=70 => self.emit_predicate(block),
            71..=83 => self.emit_bait(block),
            84..=90 => self.emit_memory(block),
            91..=95 => self.emit_global(block),
            _ => self.emit_call(block),
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
        let operation = match self.rng.below(6) {
            0 => CmpOp::Eq,
            1 => CmpOp::Ne,
            2 => CmpOp::SLt,
            3 => CmpOp::SLe,
            4 => CmpOp::ULt,
            _ => CmpOp::ULe,
        };
        let result = self.fresh(EvoIRType::Bits(1));
        block
            .insts
            .push(Inst::ICmp(result.clone(), operation, lhs, rhs));
        self.record(block, &result);
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
                let result = self.fresh(EvoIRType::Bits(1));
                block.insts.push(Inst::ICmp(
                    result.clone(),
                    CmpOp::Eq,
                    operand.clone(),
                    operand,
                ));
                self.record(block, &result);
            }
        }
    }

    fn emit_memory(&mut self, block: &mut OpenBlock) {
        if !block.buffers.is_empty() && self.rng.chance(55) {
            self.emit_buffer_access(block);
            return;
        }
        let width = *self.rng.pick(&MEMORY_WIDTHS);
        let bytes = u64::from(width / 8);
        let align = u32::try_from(bytes).expect("a memory width is at most eight bytes");
        let count = 1 + self.rng.below(3);
        let origin = *self.origins;
        *self.origins += 1;

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

        // Every byte is written before anything reads the object, so no later
        // load can report an uninitialized byte.
        let layout = AccessLayout::new(bytes, align);
        for element in elements.clone() {
            let value = self.value_of(block, width);
            self.emit_store(block, &element, &value, layout, false);
        }
        block.buffers.push(Buffer {
            elements,
            width,
            layout,
        });
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
        let layout = AccessLayout::new(8, 8);
        block.buffers.push(Buffer {
            elements: vec![pointer],
            width: 64,
            layout,
        });
        self.emit_buffer_access(block);
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

    fn emit_call(&mut self, block: &mut OpenBlock) {
        if self.callees.is_empty() {
            self.emit_binary(block);
            return;
        }
        let index = self.rng.below(self.callees.len() as u64) as usize;
        let signature = self.callees[index].clone();
        let mut args = Vec::new();
        for width in &signature.params {
            let value = self.value_of(block, *width);
            args.push(value);
        }
        let result = self.fresh(EvoIRType::Bits(signature.result));
        let mem_out = self.fresh(memory_type());
        let world_out = self.fresh(world_type());
        block.insts.push(Inst::Call {
            results: vec![result.clone()],
            callee: signature.name.clone(),
            args,
            mem_in: block.mem.clone(),
            world_in: block.world.clone(),
            mem_out: mem_out.clone(),
            world_out: world_out.clone(),
        });
        block.mem = mem_out;
        block.world = world_out;
        self.record(block, &result);
    }
}
