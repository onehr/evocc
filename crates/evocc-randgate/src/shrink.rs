//! Minimization of a refusing module.

use std::collections::BTreeSet;

use evocc_evoir::{validate_module, Function, Inst, Module};

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
