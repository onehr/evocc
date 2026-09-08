//! Multiple entries to one lexical scope share its source allocation site.
//! Edge-local allocation requests are routed through a single block, carrying
//! a continuation selector and the live local values. No origin is cloned and
//! no allocation is moved across a dynamic scope entry.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use evocc_evoir::{
    inst_results, inst_uses, remap_inst_uses, Block, BlockArg, CmpOp, Function, Inst, Value,
};
use evocc_types::EvoIRType;

use crate::LowerError;

struct Ids {
    value: u32,
    block: u32,
}

impl Ids {
    fn value(&mut self, ty: EvoIRType) -> Value {
        let value = Value { id: self.value, ty };
        self.value += 1;
        value
    }

    fn block(&mut self) -> u32 {
        let block = self.block;
        self.block += 1;
        block
    }

    fn arguments(&mut self, types: &[EvoIRType]) -> Vec<BlockArg> {
        types
            .iter()
            .enumerate()
            .map(|(index, ty)| BlockArg {
                name: format!("carry{index}"),
                value: self.value(ty.clone()),
            })
            .collect()
    }
}

struct Entry {
    predecessor: usize,
    continuation: u32,
    mem: Value,
    world: Value,
    carried: Vec<Value>,
    slots: Vec<usize>,
}

pub(super) fn share_allocation_sites(function: &mut Function) -> Result<(), LowerError> {
    let mut occurrences = BTreeMap::<u64, usize>::new();
    for block in &function.blocks {
        for inst in &block.insts {
            if let Inst::Alloca { origin, .. } = inst {
                *occurrences.entry(*origin).or_default() += 1;
            }
        }
    }
    if occurrences.values().all(|count| *count == 1) {
        return Ok(());
    }
    let mut ids = Ids {
        value: function
            .blocks
            .iter()
            .flat_map(|block| {
                block.args.iter().map(|arg| arg.value.id).chain(
                    block
                        .insts
                        .iter()
                        .chain([&block.terminator])
                        .flat_map(inst_results)
                        .map(|v| v.id),
                )
            })
            .max()
            .unwrap_or(0)
            + 1,
        block: function
            .blocks
            .iter()
            .map(|block| block.id)
            .max()
            .unwrap_or(0)
            + 1,
    };
    for (origin, count) in occurrences {
        if count == 1 {
            continue;
        }
        let mut entries = Vec::new();
        let mut layout = None;
        let mut result_types = Vec::new();
        let mut slot_types = Vec::new();
        let mut index = 0;
        while index < function.blocks.len() {
            let Some(position) = function.blocks[index].insts.iter().position(
                |inst| matches!(inst, Inst::Alloca { origin: site, .. } if *site == origin),
            ) else {
                index += 1;
                continue;
            };
            let continuation = ids.block();
            let block = &mut function.blocks[index];
            let mut suffix = block.insts.split_off(position + 1);
            let Inst::Alloca {
                result,
                mem_in,
                world_in,
                mem_out,
                world_out,
                layout: current,
                ..
            } = block.insts.pop().unwrap()
            else {
                unreachable!()
            };
            let outputs = vec![result, mem_out, world_out];
            let types = outputs
                .iter()
                .map(|value| value.ty.clone())
                .collect::<Vec<_>>();
            if layout.is_some_and(|previous| previous != current)
                || (!result_types.is_empty() && result_types != types)
            {
                return Err(LowerError::InvalidGeneratedSsa {
                    message: "one source allocation has inconsistent layouts or result types"
                        .to_owned(),
                });
            }
            layout = Some(current);
            result_types = types;
            let mut terminator =
                std::mem::replace(&mut block.terminator, Inst::Br(continuation, vec![]));
            let defined = outputs
                .iter()
                .map(|value| value.id)
                .chain(
                    suffix
                        .iter()
                        .chain([&terminator])
                        .flat_map(inst_results)
                        .map(|value| value.id),
                )
                .collect::<BTreeSet<_>>();
            let carried = suffix
                .iter()
                .chain([&terminator])
                .flat_map(inst_uses)
                .filter(|value| !defined.contains(&value.id))
                .map(|value| (value.id, value.clone()))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect::<Vec<_>>();
            let mut used = BTreeSet::new();
            let mut slots = Vec::new();
            for value in &carried {
                if matches!(value.ty, EvoIRType::Mem(_) | EvoIRType::World) {
                    return Err(LowerError::InvalidGeneratedSsa {
                        message: "allocation continuation reuses a consumed effect token"
                            .to_owned(),
                    });
                }
                let slot = slot_types
                    .iter()
                    .enumerate()
                    .position(|(slot, ty)| *ty == value.ty && !used.contains(&slot))
                    .unwrap_or_else(|| {
                        slot_types.push(value.ty.clone());
                        slot_types.len() - 1
                    });
                used.insert(slot);
                slots.push(slot);
            }
            let mut args = outputs
                .into_iter()
                .enumerate()
                .map(|(index, value)| BlockArg {
                    name: format!("allocation{index}"),
                    value,
                })
                .collect::<Vec<_>>();
            let carried_args = ids.arguments(
                &carried
                    .iter()
                    .map(|value| value.ty.clone())
                    .collect::<Vec<_>>(),
            );
            let aliases = carried
                .iter()
                .zip(&carried_args)
                .map(|(before, after)| (before.id, after.value.clone()))
                .collect::<HashMap<_, _>>();
            for inst in suffix.iter_mut().chain([&mut terminator]) {
                remap_inst_uses(inst, &aliases);
            }
            args.extend(carried_args);
            function.blocks.push(Block {
                id: continuation,
                args,
                insts: suffix,
                terminator,
            });
            entries.push(Entry {
                predecessor: index,
                continuation,
                mem: mem_in,
                world: world_in,
                carried,
                slots,
            });
            index += 1;
        }
        let allocation = ids.block();
        let mut input_types = vec![
            entries[0].mem.ty.clone(),
            entries[0].world.ty.clone(),
            EvoIRType::Bits(32),
        ];
        input_types.extend(slot_types.clone());
        let args = ids.arguments(&input_types);
        for (selector, entry) in entries.iter().enumerate() {
            let block = &mut function.blocks[entry.predecessor];
            let tag = ids.value(EvoIRType::Bits(32));
            block.insts.push(Inst::Const(tag.clone(), selector as u64));
            let mut actual = vec![entry.mem.clone(), entry.world.clone(), tag];
            for (slot, ty) in slot_types.iter().enumerate() {
                if let Some(position) = entry.slots.iter().position(|candidate| *candidate == slot)
                {
                    actual.push(entry.carried[position].clone());
                } else {
                    // This slot belongs exclusively to another continuation;
                    // it is transported, never evaluated, on this entry.
                    let value = ids.value(ty.clone());
                    block.insts.push(Inst::Uninitialized(value.clone()));
                    actual.push(value);
                }
            }
            block.terminator = Inst::Br(allocation, actual);
        }
        let outputs = result_types
            .iter()
            .map(|ty| ids.value(ty.clone()))
            .collect::<Vec<_>>();
        let mut instructions = vec![Inst::Alloca {
            result: outputs[0].clone(),
            mem_in: args[0].value.clone(),
            world_in: args[1].value.clone(),
            mem_out: outputs[1].clone(),
            world_out: outputs[2].clone(),
            origin,
            layout: layout.unwrap(),
        }];
        let mut packet = outputs;
        packet.extend(args.iter().skip(2).map(|arg| arg.value.clone()));
        let packet_types = packet
            .iter()
            .map(|value| value.ty.clone())
            .collect::<Vec<_>>();
        let mut block_id = allocation;
        let mut block_args = args;
        for (selector, entry) in entries.iter().enumerate() {
            let mut actual = packet[..3].to_vec();
            actual.extend(entry.slots.iter().map(|slot| packet[4 + slot].clone()));
            if selector + 1 == entries.len() {
                function.blocks.push(Block {
                    id: block_id,
                    args: block_args,
                    insts: instructions,
                    terminator: Inst::Br(entry.continuation, actual),
                });
                break;
            }
            let tag = ids.value(EvoIRType::Bits(32));
            let selected = ids.value(EvoIRType::Bits(1));
            instructions.push(Inst::Const(tag.clone(), selector as u64));
            instructions.push(Inst::ICmp(
                selected.clone(),
                CmpOp::Eq,
                packet[3].clone(),
                tag,
            ));
            let next = ids.block();
            function.blocks.push(Block {
                id: block_id,
                args: block_args,
                insts: instructions,
                terminator: Inst::CondBr(selected, entry.continuation, actual, next, packet),
            });
            block_id = next;
            block_args = ids.arguments(&packet_types);
            packet = block_args.iter().map(|arg| arg.value.clone()).collect();
            instructions = vec![];
        }
    }
    Ok(())
}
