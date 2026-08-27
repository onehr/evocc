//! Independent P0 eligibility, witness and seed derivation.
//!
//! This deliberately does not depend on `evocc-eqopt`. A common producer and
//! checker defect here would fail dangerous by admitting the same bad region.

use std::collections::{HashMap, HashSet};

use evocc_evoir::{inst_results, BlockId, EvoIRType, Function, Inst, Module, Value};
use evocc_proof::{EvpRegionId, EvpSeed, EvpSeedOrigin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EligibleRegion {
    pub(crate) id: EvpRegionId,
    pub(crate) root_value: u32,
    pub(crate) witnesses: Vec<EvpRegionId>,
    pub(crate) seeds: Vec<EvpSeed>,
}

#[derive(Clone, Copy)]
struct Definition {
    block_index: usize,
    instruction_index: usize,
}

pub(crate) fn derive_eligible_regions(module: &Module) -> Vec<EligibleRegion> {
    let mut regions = Vec::new();
    for function in &module.functions {
        derive_function(function, &mut regions);
    }
    regions
}

fn derive_function(function: &Function, regions: &mut Vec<EligibleRegion>) {
    let definitions = definitions(function);
    let block_arguments = function
        .blocks
        .iter()
        .flat_map(|block| block.args.iter().map(|argument| argument.value.id))
        .collect::<HashSet<_>>();
    let tainted = tainted_values(function);
    let excluded = excluded_values(function);

    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.insts.iter().enumerate() {
            if !allowlisted(instruction) {
                continue;
            }
            let mut closure = HashSet::new();
            collect_witness_closure(
                function,
                &definitions,
                block_index,
                instruction_index,
                &mut closure,
            );
            let witnesses = (0..block.insts.len())
                .filter(|index| closure.contains(index))
                .collect::<Vec<_>>();
            let mut seed_ids = Vec::new();
            let mut seen_seeds = HashSet::new();
            for witness in &witnesses {
                for operand in inst_operands(&block.insts[*witness]) {
                    let defined_in_closure =
                        definitions.get(&operand.id).is_some_and(|definition| {
                            definition.block_index == block_index
                                && closure.contains(&definition.instruction_index)
                        });
                    if !defined_in_closure && seen_seeds.insert(operand.id) {
                        seed_ids.push(operand.clone());
                    }
                }
            }
            let witness_values = witnesses.iter().flat_map(|index| {
                inst_results(&block.insts[*index])
                    .into_iter()
                    .map(|value| value.id)
            });
            if witness_values
                .chain(seed_ids.iter().map(|value| value.id))
                .any(|value| tainted.contains(&value) || excluded.contains(&value))
            {
                continue;
            }
            let Some(root) = inst_results(instruction).first().copied() else {
                continue;
            };
            regions.push(EligibleRegion {
                id: region_id(function, block_index, instruction_index),
                root_value: root.id,
                witnesses: witnesses
                    .into_iter()
                    .map(|index| region_id(function, block_index, index))
                    .collect(),
                seeds: seed_ids
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| EvpSeed {
                        value: value.id,
                        sort: packed_type(&value.ty),
                        origin: if block_arguments.contains(&value.id) {
                            EvpSeedOrigin::Input
                        } else {
                            EvpSeedOrigin::Definition
                        },
                        class: u32::try_from(index).expect("validated module seed count fits u32"),
                    })
                    .collect(),
            });
        }
    }
}

fn definitions(function: &Function) -> HashMap<u32, Definition> {
    function
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block_index, block)| {
            block
                .insts
                .iter()
                .enumerate()
                .flat_map(move |(instruction_index, instruction)| {
                    inst_results(instruction).into_iter().map(move |value| {
                        (
                            value.id,
                            Definition {
                                block_index,
                                instruction_index,
                            },
                        )
                    })
                })
        })
        .collect()
}

fn collect_witness_closure(
    function: &Function,
    definitions: &HashMap<u32, Definition>,
    root_block: usize,
    instruction_index: usize,
    closure: &mut HashSet<usize>,
) {
    if !closure.insert(instruction_index) {
        return;
    }
    let instruction = &function.blocks[root_block].insts[instruction_index];
    for operand in inst_operands(instruction) {
        let Some(definition) = definitions.get(&operand.id) else {
            continue;
        };
        if definition.block_index == root_block
            && definition.instruction_index < instruction_index
            && allowlisted(&function.blocks[root_block].insts[definition.instruction_index])
        {
            collect_witness_closure(
                function,
                definitions,
                root_block,
                definition.instruction_index,
                closure,
            );
        }
    }
}

fn tainted_values(function: &Function) -> HashSet<u32> {
    fixed_point(function, |instruction| {
        matches!(instruction, Inst::Uninitialized(_))
    })
}

fn excluded_values(function: &Function) -> HashSet<u32> {
    let mut excluded = function
        .blocks
        .iter()
        .flat_map(|block| &block.args)
        .filter(|argument| !eligible_bits(&argument.value.ty))
        .map(|argument| argument.value.id)
        .collect::<HashSet<_>>();
    for block in &function.blocks {
        for instruction in &block.insts {
            if !allowlisted(instruction) {
                excluded.extend(inst_results(instruction).into_iter().map(|value| value.id));
            }
        }
    }
    propagate(function, &mut excluded);
    excluded
}

fn fixed_point(function: &Function, initial: impl Fn(&Inst) -> bool) -> HashSet<u32> {
    let mut values = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.insts {
            if initial(instruction) {
                values.extend(inst_results(instruction).into_iter().map(|value| value.id));
            }
        }
    }
    propagate(function, &mut values);
    values
}

fn propagate(function: &Function, values: &mut HashSet<u32>) {
    loop {
        let before = values.len();
        for block in &function.blocks {
            for instruction in &block.insts {
                if inst_operands(instruction)
                    .iter()
                    .any(|operand| values.contains(&operand.id))
                {
                    values.extend(inst_results(instruction).into_iter().map(|value| value.id));
                }
            }
            propagate_edges(function, &block.terminator, values);
        }
        if values.len() == before {
            break;
        }
    }
}

fn propagate_edges(function: &Function, terminator: &Inst, values: &mut HashSet<u32>) {
    match terminator {
        Inst::Br(target, arguments) => propagate_edge(function, *target, arguments, values),
        Inst::CondBr(_, true_target, true_arguments, false_target, false_arguments) => {
            propagate_edge(function, *true_target, true_arguments, values);
            propagate_edge(function, *false_target, false_arguments, values);
        }
        _ => {}
    }
}

fn propagate_edge(
    function: &Function,
    target: BlockId,
    arguments: &[Value],
    values: &mut HashSet<u32>,
) {
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == target)
        .expect("validated branch target exists");
    for (argument, parameter) in arguments.iter().zip(&block.args) {
        if values.contains(&argument.id) {
            values.insert(parameter.value.id);
        }
    }
}

fn region_id(function: &Function, block_index: usize, instruction: usize) -> EvpRegionId {
    EvpRegionId {
        function: function.name.clone(),
        block: if block_index == 0 {
            "entry".to_owned()
        } else {
            format!("b{}", function.blocks[block_index].id)
        },
        instruction: u64::try_from(instruction).expect("instruction index fits u64"),
    }
}

fn allowlisted(instruction: &Inst) -> bool {
    let shape_ok = match instruction {
        Inst::Const(result, _) => eligible_bits(&result.ty),
        Inst::Add(result, left, right)
        | Inst::Sub(result, left, right)
        | Inst::Mul(result, left, right)
        | Inst::And(result, left, right)
        | Inst::Or(result, left, right)
        | Inst::Xor(result, left, right) => {
            eligible_bits(&result.ty) && left.ty == result.ty && right.ty == result.ty
        }
        Inst::ICmp(result, _, left, right) => {
            result.ty == EvoIRType::Bits(1) && eligible_bits(&left.ty) && right.ty == left.ty
        }
        _ => false,
    };
    shape_ok && inst_results(instruction).len() == 1
}

fn eligible_bits(ty: &EvoIRType) -> bool {
    matches!(ty, EvoIRType::Bits(1..=64))
}

fn packed_type(ty: &EvoIRType) -> String {
    match ty {
        EvoIRType::Bits(width) => format!("bits<{width}>"),
        _ => unreachable!("eligible seed has an integer bit-vector type"),
    }
}

#[allow(clippy::too_many_lines)]
fn inst_operands(instruction: &Inst) -> Vec<&Value> {
    match instruction {
        Inst::Const(..)
        | Inst::Uninitialized(..)
        | Inst::Trap(..)
        | Inst::PtrNull(..)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. } => Vec::new(),
        Inst::Add(_, left, right)
        | Inst::Sub(_, left, right)
        | Inst::Mul(_, left, right)
        | Inst::SRem(_, left, right)
        | Inst::URem(_, left, right)
        | Inst::Shl(_, left, right)
        | Inst::AShr(_, left, right)
        | Inst::LShr(_, left, right)
        | Inst::And(_, left, right)
        | Inst::Or(_, left, right)
        | Inst::Xor(_, left, right)
        | Inst::SignedAddDefined(_, left, right)
        | Inst::SignedSubDefined(_, left, right)
        | Inst::SignedMulDefined(_, left, right)
        | Inst::SignedDivRemNoOverflow(_, left, right)
        | Inst::SDiv(_, left, right)
        | Inst::UDiv(_, left, right)
        | Inst::ICmp(_, _, left, right) => vec![left, right],
        Inst::DivisorNonZero(_, divisor) | Inst::UbAssume(divisor, _) => vec![divisor],
        Inst::ShiftCountDefined {
            shifted, amount, ..
        }
        | Inst::SignedShlDefined {
            shifted, amount, ..
        } => vec![shifted, amount],
        Inst::SExt(_, source, _) | Inst::ZExt(_, source, _) | Inst::Trunc(_, source, _) => {
            vec![source]
        }
        Inst::Br(_, arguments) | Inst::Ret(arguments) => arguments.iter().collect(),
        Inst::CondBr(condition, _, true_arguments, _, false_arguments) => {
            let mut values = vec![condition];
            values.extend(true_arguments);
            values.extend(false_arguments);
            values
        }
        Inst::PtrOffset { base, index, .. } => vec![base, index],
        Inst::Alloca {
            mem_in, world_in, ..
        } => vec![mem_in, world_in],
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            ..
        } => vec![mem_in, world_in, pointer],
        Inst::PointerValid {
            mem_in, pointer, ..
        }
        | Inst::AccessInBounds {
            mem_in, pointer, ..
        } => vec![mem_in, pointer],
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
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            ..
        } => vec![destination, source, mem_in, world_in],
        Inst::Call {
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut values = args.iter().collect::<Vec<_>>();
            values.push(mem_in);
            values.push(world_in);
            values
        }
        Inst::IndirectCall {
            callee,
            args,
            mem_in,
            world_in,
            ..
        } => {
            let mut values = vec![callee];
            values.extend(args);
            values.push(mem_in);
            values.push(world_in);
            values
        }
    }
}

#[cfg(test)]
mod tests {
    use evocc_evoir::{Block, BlockArg, FunctionParam};

    use super::*;

    fn bits(id: u32) -> Value {
        Value {
            id,
            ty: EvoIRType::Bits(32),
        }
    }

    fn function(blocks: Vec<Block>) -> Function {
        Function {
            name: "f".to_owned(),
            params: vec![FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(32),
            }],
            ret_tys: vec![EvoIRType::Bits(32)],
            effects: Default::default(),
            blocks,
        }
    }

    #[test]
    fn derives_witness_and_seed_order_without_producer_help() {
        let module = Module {
            globals: Vec::new(),
            functions: vec![function(vec![Block {
                id: 0,
                args: vec![BlockArg {
                    name: "x".to_owned(),
                    value: bits(7),
                }],
                insts: vec![
                    Inst::Const(bits(8), 0),
                    Inst::Add(bits(9), bits(7), bits(8)),
                ],
                terminator: Inst::Ret(vec![bits(9)]),
            }])],
        };
        let regions = derive_eligible_regions(&module);
        let root = regions
            .iter()
            .find(|region| region.root_value == 9)
            .expect("add region");
        assert_eq!(root.witnesses.len(), 2);
        assert_eq!(root.witnesses[0].instruction, 0);
        assert_eq!(root.witnesses[1].instruction, 1);
        assert_eq!(root.seeds.len(), 1);
        assert_eq!(root.seeds[0].value, 7);
        assert_eq!(root.seeds[0].origin, EvpSeedOrigin::Input);
    }

    #[test]
    fn poison_is_not_laundered_through_a_block_argument() {
        let module = Module {
            globals: Vec::new(),
            functions: vec![function(vec![
                Block {
                    id: 0,
                    args: vec![BlockArg {
                        name: "x".to_owned(),
                        value: bits(0),
                    }],
                    insts: vec![Inst::Uninitialized(bits(1))],
                    terminator: Inst::Br(1, vec![bits(1)]),
                },
                Block {
                    id: 1,
                    args: vec![BlockArg {
                        name: "p".to_owned(),
                        value: bits(2),
                    }],
                    insts: vec![
                        Inst::Const(bits(3), 0),
                        Inst::Add(bits(4), bits(2), bits(3)),
                    ],
                    terminator: Inst::Ret(vec![bits(4)]),
                },
            ])],
        };
        assert!(
            derive_eligible_regions(&module)
                .iter()
                .all(|region| region.root_value != 4),
            "tainted edge argument must make the downstream add ineligible"
        );
    }

    #[test]
    fn a_non_allowlisted_scalar_result_remains_excluded_from_total_arithmetic() {
        let narrow = Value {
            id: 1,
            ty: EvoIRType::Bits(16),
        };
        let extended = bits(2);
        let module = Module {
            globals: Vec::new(),
            functions: vec![function(vec![Block {
                id: 0,
                args: vec![BlockArg {
                    name: "n".to_owned(),
                    value: narrow.clone(),
                }],
                insts: vec![
                    Inst::ZExt(extended.clone(), narrow, EvoIRType::Bits(32)),
                    Inst::Const(bits(3), 0),
                    Inst::Add(bits(4), extended, bits(3)),
                ],
                terminator: Inst::Ret(vec![bits(4)]),
            }])],
        };
        assert!(derive_eligible_regions(&module)
            .iter()
            .all(|region| region.root_value != 4));
    }
}
