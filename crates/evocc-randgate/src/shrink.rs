//! Minimization of a refusing module.

use std::collections::{BTreeSet, HashMap};

use evocc_evoir::{remap_inst_uses, validate_module, Block, Function, Inst, Module};

/// Delete instructions, blocks and functions for as long as `still_refuses`
/// keeps answering true and the reduced module still validates.
///
/// The predicate is the only authority on whether a reduction preserved the
/// failure; the shrinker never reasons about why the module failed. Validation
/// is what makes the reductions safe to attempt blindly: a deletion that leaves
/// a dangling use, breaks an effect-token chain or orphans a callee is rejected
/// by the validator rather than by a rule this file would have to keep in step
/// with the specification.
#[must_use]
pub fn shrink_module(
    module: &Module,
    still_refuses: &mut dyn FnMut(&Module) -> bool,
    max_rounds: u32,
) -> Module {
    let mut best = module.clone();
    for _ in 0..max_rounds {
        let mut changed = false;
        changed |= drop_functions(&mut best, still_refuses);
        changed |= collapse_branches(&mut best, still_refuses);
        changed |= merge_blocks(&mut best, still_refuses);
        changed |= drop_instructions(&mut best, still_refuses);
        if !changed {
            break;
        }
    }
    best
}

fn accept(
    best: &mut Module,
    mut candidate: Module,
    still_refuses: &mut dyn FnMut(&Module) -> bool,
) -> bool {
    candidate.recompute_effect_summaries();
    if validate_module(&candidate).is_ok() && still_refuses(&candidate) {
        *best = candidate;
        true
    } else {
        false
    }
}

fn drop_functions(best: &mut Module, still_refuses: &mut dyn FnMut(&Module) -> bool) -> bool {
    let mut changed = false;
    let mut index = best.functions.len();
    while index > 0 {
        index -= 1;
        if best.functions[index].name == "main" {
            continue;
        }
        let mut candidate = best.clone();
        candidate.functions.remove(index);
        changed |= accept(best, candidate, still_refuses);
    }
    changed
}

fn collapse_branches(best: &mut Module, still_refuses: &mut dyn FnMut(&Module) -> bool) -> bool {
    let mut changed = false;
    let mut function = best.functions.len();
    while function > 0 {
        function -= 1;
        let mut block = best.functions[function].blocks.len();
        while block > 0 {
            block -= 1;
            let Inst::CondBr(_, true_block, true_args, false_block, false_args) =
                best.functions[function].blocks[block].terminator.clone()
            else {
                continue;
            };
            for terminator in [
                Inst::Br(true_block, true_args),
                Inst::Br(false_block, false_args),
            ] {
                let mut candidate = best.clone();
                candidate.functions[function].blocks[block].terminator = terminator;
                prune_unreachable(&mut candidate.functions[function]);
                if accept(best, candidate, still_refuses) {
                    changed = true;
                    break;
                }
            }
        }
    }
    changed
}

fn drop_instructions(best: &mut Module, still_refuses: &mut dyn FnMut(&Module) -> bool) -> bool {
    let mut changed = false;
    let mut function = best.functions.len();
    while function > 0 {
        function -= 1;
        let mut block = best.functions[function].blocks.len();
        while block > 0 {
            block -= 1;
            let mut index = best.functions[function].blocks[block].insts.len();
            while index > 0 {
                index -= 1;
                let mut candidate = best.clone();
                candidate.functions[function].blocks[block]
                    .insts
                    .remove(index);
                changed |= accept(best, candidate, still_refuses);
            }
        }
    }
    changed
}

/// Fold a block into the one block that branches to it.
///
/// Neither deleting instructions nor collapsing a conditional branch can
/// shorten a straight-line chain: every block in one is reachable from a
/// predecessor whose terminator is already unconditional. This is the reduction
/// that reaches them, and it is the only one here that rewrites operands, since
/// the successor's block arguments become the values its predecessor passed.
///
/// A block with two predecessors is left alone. Merging it would mean copying
/// it into both, which grows the module before it shrinks it and is a different
/// reduction with a different risk.
fn merge_blocks(best: &mut Module, still_refuses: &mut dyn FnMut(&Module) -> bool) -> bool {
    let mut changed = false;
    let mut function = best.functions.len();
    while function > 0 {
        function -= 1;
        let mut block = best.functions[function].blocks.len();
        while block > 0 {
            block -= 1;
            let mut candidate = best.clone();
            if merge_one(&mut candidate.functions[function], block).is_none() {
                continue;
            }
            changed |= accept(best, candidate, still_refuses);
        }
    }
    changed
}

/// Merge the block at `index` into its unique predecessor, if it has one.
fn merge_one(function: &mut Function, index: usize) -> Option<()> {
    let target = function.blocks.get(index)?.id;
    if function.blocks.first()?.id == target {
        return None;
    }
    let mut predecessors =
        function
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| match &block.terminator {
                Inst::Br(successor, _) => *successor == target,
                Inst::CondBr(_, first, _, second, _) => *first == target || *second == target,
                _ => false,
            });
    let (source, predecessor) = predecessors.next()?;
    if predecessors.next().is_some() || source == index {
        return None;
    }
    let Inst::Br(_, arguments) = &predecessor.terminator else {
        return None;
    };

    let merged = function.blocks.get(index)?.clone();
    if merged.args.len() != arguments.len() {
        return None;
    }
    let bindings: HashMap<u32, evocc_evoir::Value> = merged
        .args
        .iter()
        .zip(arguments)
        .map(|(argument, incoming)| (argument.value.id, incoming.clone()))
        .collect();

    let source_block: &mut Block = function.blocks.get_mut(source)?;
    for mut inst in merged.insts {
        remap_inst_uses(&mut inst, &bindings);
        source_block.insts.push(inst);
    }
    let mut terminator = merged.terminator;
    remap_inst_uses(&mut terminator, &bindings);
    source_block.terminator = terminator;
    function.blocks.remove(index);
    Some(())
}

fn prune_unreachable(function: &mut Function) {
    let Some(entry) = function.blocks.first() else {
        return;
    };
    let mut reachable = BTreeSet::new();
    let mut pending = vec![entry.id];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(block) = function.blocks.iter().find(|block| block.id == id) else {
            continue;
        };
        match &block.terminator {
            Inst::Br(target, _) => pending.push(*target),
            Inst::CondBr(_, true_block, _, false_block, _) => {
                pending.push(*true_block);
                pending.push(*false_block);
            }
            _ => {}
        }
    }
    function
        .blocks
        .retain(|block| reachable.contains(&block.id));
}
