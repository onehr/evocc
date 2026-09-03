//! Minimization of a refusing module.

use std::collections::{BTreeSet, HashMap};

use evocc_evoir::{
    inst_results, remap_inst_uses, validate_module, Block, EvoIRType, Function, Inst, Module, Value,
};

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
        changed |= share_congruent_instructions(&mut best, still_refuses);
        changed |= collapse_to_constants(&mut best, still_refuses);
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

/// Replace an instruction with a constant the module already names.
///
/// A chain in which every instruction feeds the next cannot be shortened by
/// deleting one: the deletion leaves a dangling use and the validator rejects
/// it. Turning the middle of the chain into a constant breaks the chain, and
/// then deleting what fed it succeeds. It is what reaches a covering mask,
/// whose whole shape is `or %v, K` standing between an operand and the rule
/// that reads its bits.
///
/// The constants tried are the ones the block already writes, plus zero and
/// all ones. Drawing from the module rather than from the whole width keeps
/// the number of attempts a property of the module's own text, and a mask is
/// always among them because the shape that needs collapsing wrote it.
fn collapse_to_constants(
    best: &mut Module,
    still_refuses: &mut dyn FnMut(&Module) -> bool,
) -> bool {
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
                let instruction = best.functions[function].blocks[block].insts[index].clone();
                if matches!(instruction, Inst::Const(..)) {
                    continue;
                }
                let Some(result) = sole_scalar_result(&instruction) else {
                    continue;
                };
                for raw in candidate_constants(&best.functions[function].blocks[block], &result) {
                    let mut candidate = best.clone();
                    candidate.functions[function].blocks[block].insts[index] =
                        Inst::Const(result.clone(), raw);
                    if accept(best, candidate, still_refuses) {
                        changed = true;
                        break;
                    }
                }
            }
        }
    }
    changed
}

/// The one `bits<N>` result an instruction defines, if it defines exactly one.
fn sole_scalar_result(instruction: &Inst) -> Option<Value> {
    let results = inst_results(instruction);
    let [result] = results.as_slice() else {
        return None;
    };
    matches!(result.ty, EvoIRType::Bits(1..=64)).then(|| (*result).clone())
}

fn candidate_constants(block: &Block, result: &Value) -> Vec<u64> {
    let EvoIRType::Bits(width) = result.ty else {
        return Vec::new();
    };
    let all_ones = if width >= 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let mut raws = vec![0, all_ones];
    for instruction in &block.insts {
        if let Inst::Const(defined, raw) = instruction {
            if defined.ty == result.ty && !raws.contains(raw) {
                raws.push(*raw);
            }
        }
    }
    raws
}

/// Replace one of two structurally identical instructions by the other.
///
/// Section 7.4 makes them one value, so every use of the second is a use of the
/// first; deleting it on its own would leave a dangling use, which is why
/// dropping instructions cannot reach this shape. This is the congruence of
/// Section 7.4 applied to the source text rather than to the e-graph, and it is
/// what reduces a reproducer built out of a duplicated expression.
fn share_congruent_instructions(
    best: &mut Module,
    still_refuses: &mut dyn FnMut(&Module) -> bool,
) -> bool {
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
                if share_one(&mut candidate.functions[function].blocks[block], index).is_none() {
                    continue;
                }
                changed |= accept(best, candidate, still_refuses);
            }
        }
    }
    changed
}

/// Point every use of the instruction at `index` at an identical earlier one,
/// and remove it.
fn share_one(block: &mut Block, index: usize) -> Option<()> {
    let duplicate = block.insts.get(index)?.clone();
    let result = sole_scalar_result(&duplicate)?;
    let first = block.insts.iter().take(index).find(|earlier| {
        sole_scalar_result(earlier).is_some_and(|earlier_result| {
            let mut renamed = (*earlier).clone();
            let bindings = HashMap::from([(earlier_result.id, result.clone())]);
            remap_result(&mut renamed, &bindings);
            renamed == duplicate
        })
    })?;
    let kept = sole_scalar_result(first)?;
    block.insts.remove(index);
    let bindings = HashMap::from([(result.id, kept)]);
    for instruction in &mut block.insts {
        remap_inst_uses(instruction, &bindings);
    }
    remap_inst_uses(&mut block.terminator, &bindings);
    Some(())
}

/// Rename an instruction's result so two otherwise identical instructions
/// compare equal. Only the result is renamed; the operands are what decides
/// whether they are the same node.
fn remap_result(instruction: &mut Inst, bindings: &HashMap<u32, Value>) {
    let renamed = match instruction {
        Inst::Const(result, _)
        | Inst::Add(result, ..)
        | Inst::Sub(result, ..)
        | Inst::Mul(result, ..)
        | Inst::And(result, ..)
        | Inst::Or(result, ..)
        | Inst::Xor(result, ..)
        | Inst::ICmp(result, ..) => result,
        _ => return,
    };
    if let Some(target) = bindings.get(&renamed.id) {
        *renamed = target.clone();
    }
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
