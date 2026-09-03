//! Section 8.10 "Facts of seeds", recomputed by the checker.
//!
//! A premise may name a seed's class, and a seed of origin `input` is a block
//! argument whose fact the producer derived from the whole function rather than
//! from the region. The checker cannot read that derivation off the
//! certificate: Section 11.6 puts the analysis on the unshared side, so the
//! fixed point is repeated here over `before.evo` and a premise is confirmed
//! against what this file derives.
//!
//! Every argument outside the entry block begins with no edge having spoken for
//! it, and a block none of whose predecessors has spoken contributes nothing on
//! its own edges. Entry arguments are top. A step only ever loses knowledge, so
//! the iteration stops and, in Section 8.10's words, "the order of visits does
//! not change the result".
//!
//! `by_whole_passes` is the sweep of the whole function, and `agreement` runs
//! it beside whatever iteration `by_current` names so that the sentence about
//! visit order is measured rather than believed. What the checker runs is
//! `by_current` alone; the oracle has no caller on the checking path.

use std::collections::{HashMap, VecDeque};

use evocc_evoir::{inst_results, Block, BlockId, CmpOp, EvoIRType, Function, Inst, Module, Value};

use crate::eligibility::{admitted_values, allowlisted};
use crate::facts::{Comparison, Fact};

/// Every function's block-argument facts, keyed by function name.
pub(crate) struct SeedFacts {
    functions: HashMap<String, HashMap<u32, Fact>>,
}

impl SeedFacts {
    pub(crate) fn of(module: &Module) -> Self {
        Self {
            functions: module
                .functions
                .iter()
                .map(|function| (function.name.clone(), arguments_of(function)))
                .collect(),
        }
    }

    /// The fact of one block argument, or `None` for a value that is not one
    /// or whose sort the analysis has no lattice for.
    pub(crate) fn of_value(&self, function: &str, value: u32) -> Option<Fact> {
        self.functions.get(function)?.get(&value).copied()
    }
}

fn width_of(value: &Value) -> Option<u16> {
    match value.ty {
        EvoIRType::Bits(width @ 1..=64) => Some(width),
        _ => None,
    }
}

fn arguments_of(function: &Function) -> HashMap<u32, Fact> {
    by_current(function).0
}

/// Where each block sits, by the identifier its edges name it with; the first
/// block carrying an identifier wins, as a linear search for it would find.
fn block_indices(function: &Function) -> HashMap<BlockId, usize> {
    let mut indices = HashMap::new();
    for (index, block) in function.blocks.iter().enumerate() {
        indices.entry(block.id).or_insert(index);
    }
    indices
}

/// The iteration the checker runs, and the block evaluations it took.
///
/// Every block is queued once at the start, because a block with no scalar
/// arguments has nothing to wait for and the entry block never waits at all; a
/// block no edge has spoken for yet is dropped when it comes up, and the edge
/// that first speaks for one of its arguments puts it back. A block whose
/// arguments an edge moved is queued again, and no other block is: nothing else
/// can change what this block passes on its edges, since its environment is
/// built from its own arguments and its own instructions and reads nothing
/// else.
fn by_current(function: &Function) -> (HashMap<u32, Fact>, usize) {
    let admitted = admitted_values(function);
    let indices = block_indices(function);
    let mut facts = entry_arguments(function);
    let mut queued = vec![true; function.blocks.len()];
    let mut queue: VecDeque<usize> = (0..function.blocks.len()).collect();
    let mut items = 0_usize;
    let mut reached = Vec::new();
    while let Some(index) = queue.pop_front() {
        queued[index] = false;
        let block = &function.blocks[index];
        if index != 0 && !spoken_for(block, &facts) {
            continue;
        }
        items += 1;
        let environment = environment_of(block, &facts);
        reached.clear();
        edges_of(block, |target, passed| {
            carry(
                function,
                &indices,
                target,
                passed,
                &environment,
                &admitted,
                &mut facts,
                &mut reached,
            );
        });
        for target in reached.drain(..) {
            if !queued[target] {
                queued[target] = true;
                queue.push_back(target);
            }
        }
    }
    (facts, items)
}

/// The sweep of the whole function, kept as the oracle the iteration above is
/// measured against.
///
/// Returns the facts, the passes the sweep took and the block evaluations it
/// spent.
#[cfg(test)]
fn by_whole_passes(function: &Function) -> (HashMap<u32, Fact>, usize, usize) {
    let admitted = admitted_values(function);
    let indices = block_indices(function);
    let mut current = entry_arguments(function);
    let mut passes = 0_usize;
    let mut items = 0_usize;
    let mut reached = Vec::new();
    loop {
        let mut next = entry_arguments(function);
        for (index, block) in function.blocks.iter().enumerate() {
            if index != 0 && !spoken_for(block, &current) {
                continue;
            }
            items += 1;
            let environment = environment_of(block, &current);
            reached.clear();
            edges_of(block, |target, passed| {
                carry(
                    function,
                    &indices,
                    target,
                    passed,
                    &environment,
                    &admitted,
                    &mut next,
                    &mut reached,
                );
            });
        }
        // Knowledge only ever leaves, which is why this stops without a
        // ceiling of its own.
        for (value, fact) in &current {
            let combined = next.get(value).map_or(*fact, |fresh| fact.join(*fresh));
            next.insert(*value, combined);
        }
        passes += 1;
        if next == current {
            return (current, passes, items);
        }
        current = next;
    }
}

/// What two iterations derived over one function, and what each of them cost.
#[cfg(test)]
pub(crate) struct Agreement {
    pub(crate) arguments: usize,
    pub(crate) work_items: usize,
    pub(crate) whole_pass_items: usize,
    pub(crate) whole_passes: usize,
}

/// Run the checker's iteration beside the sweep and report whether they agree.
///
/// `Err` names the first argument they disagree about, in argument order.
#[cfg(test)]
pub(crate) fn agreement(function: &Function) -> Result<Agreement, String> {
    let (derived, work_items) = by_current(function);
    let (reference, whole_passes, whole_pass_items) = by_whole_passes(function);
    if derived.len() != reference.len() {
        return Err(format!(
            "{}: the iteration gave {} arguments a fact and the sweep gave {}",
            function.name,
            derived.len(),
            reference.len()
        ));
    }
    let mut ordered = derived.keys().copied().collect::<Vec<_>>();
    ordered.sort_unstable();
    for value in ordered {
        let left = derived.get(&value);
        let right = reference.get(&value);
        if left != right {
            return Err(format!(
                "{}: %{value} is {left:?} by the iteration and {right:?} by the sweep",
                function.name
            ));
        }
    }
    Ok(Agreement {
        arguments: derived.len(),
        work_items,
        whole_pass_items,
        whole_passes,
    })
}

/// A caller may pass anything, so the interface promises nothing; an edge that
/// re-enters the entry block does not change that.
fn entry_arguments(function: &Function) -> HashMap<u32, Fact> {
    let Some(entry) = function.blocks.first() else {
        return HashMap::new();
    };
    entry
        .args
        .iter()
        .filter_map(|argument| Some((argument.value.id, Fact::top(width_of(&argument.value)?))))
        .collect()
}

fn spoken_for(block: &Block, facts: &HashMap<u32, Fact>) -> bool {
    block
        .args
        .iter()
        .filter(|argument| width_of(&argument.value).is_some())
        .all(|argument| facts.contains_key(&argument.value.id))
}

/// The facts of every value the block defines, from its arguments forward.
fn environment_of(block: &Block, facts: &HashMap<u32, Fact>) -> HashMap<u32, Fact> {
    let mut environment: HashMap<u32, Fact> = block
        .args
        .iter()
        .filter_map(|argument| Some((argument.value.id, *facts.get(&argument.value.id)?)))
        .collect();
    for instruction in &block.insts {
        let Some(result) = inst_results(instruction).first().copied() else {
            continue;
        };
        let Some(width) = width_of(result) else {
            continue;
        };
        let fact = transfer(instruction, width, &environment).unwrap_or_else(|| Fact::top(width));
        environment.insert(result.id, fact);
    }
    environment
}

/// One row of the Section 8.10 table, over an instruction rather than a node.
///
/// `None` wherever the table says top: an instruction absent from Section 8.5's
/// allowlist, an operand with no lattice, a result the analysis has no row for.
fn transfer(instruction: &Inst, width: u16, environment: &HashMap<u32, Fact>) -> Option<Fact> {
    if !allowlisted(instruction) {
        return None;
    }
    let known = |value: &Value| -> Option<Fact> {
        let width = width_of(value)?;
        Some(
            environment
                .get(&value.id)
                .copied()
                .unwrap_or_else(|| Fact::top(width)),
        )
    };
    Some(match instruction {
        Inst::Const(_, raw) => Fact::constant(width, *raw),
        Inst::And(_, left, right) => known(left)?.and(known(right)?),
        Inst::Or(_, left, right) => known(left)?.or(known(right)?),
        Inst::Xor(_, left, right) => known(left)?.xor(known(right)?),
        Inst::Add(_, left, right) => known(left)?.add(known(right)?),
        Inst::Sub(_, left, right) => known(left)?.sub(known(right)?),
        Inst::Mul(_, left, right) => known(left)?.mul(known(right)?),
        Inst::ICmp(_, operation, left, right) => {
            known(left)?.compare(known(right)?, comparison(*operation))
        }
        Inst::Shl(_, shifted, amount) => known(shifted)?.shl(known(amount)?),
        Inst::LShr(_, shifted, amount) => known(shifted)?.lshr(known(amount)?),
        Inst::AShr(_, shifted, amount) => known(shifted)?.ashr(known(amount)?),
        Inst::ZExt(_, source, _) => known(source)?.zext(width),
        Inst::SExt(_, source, _) => known(source)?.sext(width),
        Inst::Trunc(_, source, _) => known(source)?.trunc(width),
        _ => return None,
    })
}

const fn comparison(operation: CmpOp) -> Comparison {
    match operation {
        CmpOp::Eq => Comparison::Eq,
        CmpOp::Ne => Comparison::Ne,
        CmpOp::SLt => Comparison::SLt,
        CmpOp::SLe => Comparison::SLe,
        CmpOp::ULt => Comparison::ULt,
        CmpOp::ULe => Comparison::ULe,
    }
}

/// Every edge the terminator leaves on, in the order Section 9.3 renders them.
fn edges_of(block: &Block, mut visit: impl FnMut(BlockId, &[Value])) {
    match &block.terminator {
        Inst::Br(target, arguments) => visit(*target, arguments),
        Inst::CondBr(_, true_target, true_arguments, false_target, false_arguments) => {
            visit(*true_target, true_arguments);
            visit(*false_target, false_arguments);
        }
        _ => {}
    }
}

/// Combine what one edge passes into the facts of the arguments it binds,
/// naming the block whose arguments moved.
#[allow(clippy::too_many_arguments)]
fn carry(
    function: &Function,
    indices: &HashMap<BlockId, usize>,
    target: BlockId,
    passed: &[Value],
    environment: &HashMap<u32, Fact>,
    admitted: &std::collections::HashSet<u32>,
    next: &mut HashMap<u32, Fact>,
    reached: &mut Vec<usize>,
) {
    let Some(index) = indices.get(&target).copied() else {
        return;
    };
    if index == 0 {
        return;
    }
    let mut moved = false;
    for (value, parameter) in passed.iter().zip(&function.blocks[index].args) {
        let Some(width) = width_of(&parameter.value) else {
            continue;
        };
        // An excluded value, a pointer and an unlisted instruction's result are
        // all top: the analysis states nothing about any of them.
        let fact = if admitted.contains(&value.id) {
            environment
                .get(&value.id)
                .copied()
                .unwrap_or_else(|| Fact::top(width))
        } else {
            Fact::top(width)
        };
        match next.get(&parameter.value.id) {
            Some(existing) => {
                let combined = existing.join(fact);
                if combined != *existing {
                    next.insert(parameter.value.id, combined);
                    moved = true;
                }
            }
            None => {
                next.insert(parameter.value.id, fact);
                moved = true;
            }
        }
    }
    if moved {
        reached.push(index);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use evocc_randgate::{generate_module, GeneratorConfig};

    use super::agreement;

    /// The corpus every measurement here is taken over.
    const CORPUS: u64 = 256;

    /// Section 8.10: "the order of visits does not change the result".
    ///
    /// The iteration the checker runs and the sweep of the whole function must
    /// derive the same fact for every block argument of every function of the
    /// corpus, bit for bit, and the iteration must cost strictly less than the
    /// sweep it replaced.
    #[test]
    fn the_iteration_and_the_whole_function_sweep_agree_on_the_corpus() {
        let mut arguments = 0_usize;
        let mut work_items = 0_usize;
        let mut whole_pass_items = 0_usize;
        let mut deepest = 0_usize;
        let mut distribution: BTreeMap<usize, usize> = BTreeMap::new();
        for seed in 0..CORPUS {
            let module = generate_module(seed, GeneratorConfig::default());
            for function in &module.functions {
                let measured = agreement(function).unwrap_or_else(|error| panic!("{error}"));
                arguments += measured.arguments;
                work_items += measured.work_items;
                whole_pass_items += measured.whole_pass_items;
                deepest = deepest.max(measured.whole_passes);
                *distribution.entry(measured.work_items).or_insert(0) += 1;
            }
        }
        println!("arguments={arguments}");
        println!("workItems={work_items}");
        println!("wholePassItems={whole_pass_items}");
        println!("deepestWholePassCount={deepest}");
        println!("workItemsPerFunction={distribution:?}");
        assert!(
            arguments > 0,
            "the corpus must give some block argument a fact"
        );
        assert!(
            deepest >= 2,
            "a corpus with branches must need more than one sweep somewhere"
        );
        assert!(
            work_items < whole_pass_items,
            "the iteration spent {work_items} block evaluations and the sweep \
             {whole_pass_items}: re-evaluating only what an edge change reaches has to cost less"
        );
    }
}
