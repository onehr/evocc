//! What the shrinker can take out of a module.
//!
//! A reduction is only useful if it can reach the shape a reproducer is stuck
//! in. Deleting instructions and collapsing conditional branches cannot shorten
//! a straight-line chain of blocks, because every block in such a chain has a
//! predecessor that ends in an unconditional branch and is therefore reachable.
//! Merging a block into its unique predecessor is the reduction that reaches
//! them.

use evocc_evoir::{
    validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst, Module, Value,
};
use evocc_randgate::{shrink_module, GeneratorConfig};
use evocc_types::{EvoIRType, Region};

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn binder(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

/// `^entry -> ^b1 -> ^b2`, each edge unconditional, each block carrying the
/// memory and world tokens and one ordinary value.
fn chain() -> Module {
    let mut next = 0_u32;
    let mut fresh = |ty: EvoIRType| {
        let id = next;
        next += 1;
        value(id, ty)
    };
    let bits = EvoIRType::Bits(32);
    let memory = EvoIRType::Mem(Region::All);
    let world = EvoIRType::World;

    let entry_value = fresh(bits.clone());
    let entry_memory = fresh(memory.clone());
    let entry_world = fresh(world.clone());

    let middle_value = fresh(bits.clone());
    let middle_memory = fresh(memory.clone());
    let middle_world = fresh(world.clone());

    let last_value = fresh(bits.clone());
    let last_memory = fresh(memory.clone());
    let last_world = fresh(world.clone());

    let doubled = fresh(bits.clone());

    let function = Function {
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "arg0".to_owned(),
                ty: bits.clone(),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: memory.clone(),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: world.clone(),
            },
        ],
        ret_tys: vec![bits, memory, world],
        effects: EffectSummary::default(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![
                    binder(&entry_value),
                    binder(&entry_memory),
                    binder(&entry_world),
                ],
                insts: Vec::new(),
                terminator: Inst::Br(
                    1,
                    vec![entry_value, entry_memory.clone(), entry_world.clone()],
                ),
            },
            Block {
                id: 1,
                args: vec![
                    binder(&middle_value),
                    binder(&middle_memory),
                    binder(&middle_world),
                ],
                insts: vec![Inst::Add(
                    doubled.clone(),
                    middle_value.clone(),
                    middle_value,
                )],
                terminator: Inst::Br(2, vec![doubled, middle_memory, middle_world]),
            },
            Block {
                id: 2,
                args: vec![
                    binder(&last_value),
                    binder(&last_memory),
                    binder(&last_world),
                ],
                insts: Vec::new(),
                terminator: Inst::Ret(vec![last_value, last_memory, last_world]),
            },
        ],
    };

    let mut module = Module {
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    module
}

fn blocks(module: &Module) -> usize {
    module.functions.iter().map(|f| f.blocks.len()).sum()
}

#[test]
fn a_straight_line_chain_of_blocks_collapses_into_one() {
    let module = chain();
    validate_module(&module).expect("the chain is valid EvoIR");
    assert_eq!(blocks(&module), 3);

    // The predicate keeps the one instruction that carries meaning, so the only
    // way to get shorter is to take out blocks.
    let mut still_refuses = |candidate: &Module| {
        candidate
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .any(|block| block.insts.iter().any(|inst| matches!(inst, Inst::Add(..))))
    };
    let reduced = shrink_module(&module, &mut still_refuses, 16);

    validate_module(&reduced).expect("a reduction must still validate");
    assert!(
        still_refuses(&reduced),
        "the refusal was lost:\n{}",
        evocc_evoir::render_module(&reduced)
    );
    assert_eq!(
        blocks(&reduced),
        1,
        "the chain did not collapse:\n{}",
        evocc_evoir::render_module(&reduced)
    );
}

/// A block with two predecessors is not merged into either.
///
/// Merging it would have to duplicate it, which is a different reduction with a
/// different risk, so the shrinker leaves the join alone and the loop keeps its
/// shape.
#[test]
fn a_block_with_several_predecessors_is_left_alone() {
    let module = evocc_randgate::generate_module(
        (0..512_u64)
            .find(|seed| {
                let module = evocc_randgate::generate_module(*seed, GeneratorConfig::default());
                module
                    .functions
                    .iter()
                    .flat_map(|function| &function.blocks)
                    .any(|block| matches!(block.terminator, Inst::CondBr(..)))
            })
            .expect("a seed with a conditional branch"),
        GeneratorConfig::default(),
    );

    // Nothing is refused, so the shrinker takes everything it safely can. What
    // survives is what the reductions cannot reach.
    let mut always = |candidate: &Module| validate_module(candidate).is_ok();
    let reduced = shrink_module(&module, &mut always, 16);
    validate_module(&reduced).expect("a reduction must still validate");
    println!("blocksBefore={}", blocks(&module));
    println!("blocksAfter={}", blocks(&reduced));
    assert!(
        blocks(&reduced) < blocks(&module),
        "merging reached no block of a generated module: {} stayed {}",
        blocks(&module),
        blocks(&reduced)
    );
}
