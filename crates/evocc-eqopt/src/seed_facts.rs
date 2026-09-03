//! Section 8.10 "Facts of seeds": what a block argument knows, for the
//! producer.
//!
//! A seed of origin `input` is a block argument, and a block argument's fact is
//! the combination over every edge entering its block of the fact of the value
//! passed there. Section 11.6 keeps this unshared with the checker for the same
//! reason it keeps eligibility unshared: a defect held in common would make
//! both sides confirm a premise neither should, and the certificate would be
//! accepted.
//!
//! Every argument outside the entry block starts at the combination's identity
//! -- no edge has spoken for it yet -- and a block none of whose predecessors
//! has spoken contributes nothing on its own edges, which is what makes a value
//! carried round a loop keep what every path into the loop agrees on. Entry
//! arguments are top, because a caller may pass anything. Each step only loses
//! knowledge, so the iteration reaches a fixed point, and Section 8.10 says
//! that "the order of visits does not change the result".
//!
//! That sentence is what lets the iteration be a worklist rather than a sweep
//! of the whole function. A block's contribution on its edges is a function of
//! its own arguments' facts alone -- the environment below is built from those
//! and the block's instructions, and reads nothing else -- so a block has to be
//! re-evaluated exactly when one of its own arguments moves, and never because
//! some unrelated block moved. `by_whole_passes` keeps the sweep as the oracle
//! the worklist is measured against; nothing but the agreement hook calls it.

use std::collections::HashMap;

use evocc_evoir::{Block, BlockId, Function, Inst, Value};

use crate::eligibility::Admission;
use crate::known_bits::KnownBits;
use crate::scalar_eq::{known_of, record_known, scalar_width, KnownEnv};

/// One function's block-argument facts, and what reaching them cost.
pub(crate) struct SeedFacts {
    arguments: HashMap<u32, KnownBits>,
    work_items: usize,
}

impl SeedFacts {
    pub(crate) fn of(function: &Function, admission: &Admission) -> Self {
        let (arguments, _, work_items) = by_whole_passes(function, admission);
        Self {
            arguments,
            work_items,
        }
    }

    /// The facts of one block's arguments, and nothing else: the environment
    /// a reader of that block starts from, with every other value in it
    /// derived by the Section 8.10 transfers as the block is walked.
    pub(crate) fn arguments_of(&self, block: &Block) -> KnownEnv {
        block
            .args
            .iter()
            .filter_map(|argument| {
                Some((argument.value.id, *self.arguments.get(&argument.value.id)?))
            })
            .collect()
    }

    /// The fact of one block argument, or `None` for a value that is not one.
    pub(crate) fn of_value(&self, value: u32) -> Option<KnownBits> {
        self.arguments.get(&value).copied()
    }

    /// How many block evaluations the fixed point took, as a cost figure.
    pub(crate) const fn work_items(&self) -> usize {
        self.work_items
    }
}

/// Where each block sits, by the identifier its edges name it with.
///
/// The first block carrying an identifier wins, which is the block a linear
/// search for that identifier would have found.
fn block_indices(function: &Function) -> HashMap<BlockId, usize> {
    let mut indices = HashMap::new();
    for (index, block) in function.blocks.iter().enumerate() {
        indices.entry(block.id).or_insert(index);
    }
    indices
}

/// Entry arguments are top and stay top: the interface promises nothing about
/// what a caller passes, and an edge back into the entry block does not change
/// that.
fn entry_facts(function: &Function) -> HashMap<u32, KnownBits> {
    let Some(entry) = function.blocks.first() else {
        return HashMap::new();
    };
    entry
        .args
        .iter()
        .filter_map(|argument| {
            let width = scalar_width(&argument.value.ty)?;
            Some((argument.value.id, KnownBits::top(width)))
        })
        .collect()
}

/// Whether some edge has already spoken for this block's arguments.
///
/// Until one has, every value the block computes rests on the combination's
/// identity, and the block contributes that identity on its own edges: exactly
/// what Section 8.10 says an argument not yet visited contributes. Once true it
/// stays true, because a fact is never taken away again, which is why a block
/// enqueued by the first edge to reach it is never enqueued for that reason
/// twice.
fn spoken_for(block: &Block, facts: &HashMap<u32, KnownBits>) -> bool {
    block
        .args
        .iter()
        .filter(|argument| scalar_width(&argument.value.ty).is_some())
        .all(|argument| facts.contains_key(&argument.value.id))
}

fn block_environment(block: &Block, facts: &HashMap<u32, KnownBits>) -> KnownEnv {
    let mut env: KnownEnv = block
        .args
        .iter()
        .filter_map(|argument| Some((argument.value.id, *facts.get(&argument.value.id)?)))
        .collect();
    for instruction in &block.insts {
        record_known(instruction, &mut env);
    }
    env
}

/// Combine what this block passes into the facts of the arguments it reaches,
/// naming every block whose arguments moved.
fn cross_edges(
    function: &Function,
    block: &Block,
    env: &KnownEnv,
    admission: &Admission,
    indices: &HashMap<BlockId, usize>,
    facts: &mut HashMap<u32, KnownBits>,
    reached: &mut Vec<usize>,
) {
    let mut carry = |target: BlockId, arguments: &[Value]| {
        // Every edge counts; none is pruned by a fact, so a comparison the
        // analysis has already decided still contributes on both sides.
        let Some(index) = indices.get(&target).copied() else {
            return;
        };
        if index == 0 {
            return;
        }
        let mut moved = false;
        for (passed, parameter) in arguments.iter().zip(&function.blocks[index].args) {
            let Some(width) = scalar_width(&parameter.value.ty) else {
                continue;
            };
            // An unlisted instruction's result, a pointer and any excluded
            // value are top: the analysis states nothing about them.
            let fact = if admission.admits(passed.id) {
                known_of(passed, env).unwrap_or_else(|| KnownBits::top(width))
            } else {
                KnownBits::top(width)
            };
            match facts.get(&parameter.value.id) {
                Some(existing) => {
                    // A combination of unequal widths is no combination at all;
                    // the argument keeps what it had, and nothing moved.
                    if let Some(combined) = existing.join(fact) {
                        if combined != *existing {
                            facts.insert(parameter.value.id, combined);
                            moved = true;
                        }
                    }
                }
                None => {
                    facts.insert(parameter.value.id, fact);
                    moved = true;
                }
            }
        }
        if moved {
            reached.push(index);
        }
    };
    match &block.terminator {
        Inst::Br(target, arguments) => carry(*target, arguments),
        Inst::CondBr(_, true_target, true_arguments, false_target, false_arguments) => {
            carry(*true_target, true_arguments);
            carry(*false_target, false_arguments);
        }
        _ => {}
    }
}

/// The whole-function sweep the worklist replaced, kept as its oracle.
///
/// Returns the facts, the passes the sweep took, and the block evaluations it
/// spent reaching them. Nothing on the optimizing path calls this; it exists so
/// that Section 8.10's "the order of visits does not change the result" is
/// measured on a corpus rather than assumed.
fn by_whole_passes(
    function: &Function,
    admission: &Admission,
) -> (HashMap<u32, KnownBits>, usize, usize) {
    let indices = block_indices(function);
    let mut current = entry_facts(function);
    let mut passes = 0_usize;
    let mut items = 0_usize;
    let mut reached = Vec::new();
    loop {
        let mut next = entry_facts(function);
        for (index, block) in function.blocks.iter().enumerate() {
            if index != 0 && !spoken_for(block, &current) {
                continue;
            }
            items += 1;
            let env = block_environment(block, &current);
            reached.clear();
            cross_edges(
                function,
                block,
                &env,
                admission,
                &indices,
                &mut next,
                &mut reached,
            );
        }
        // A pass that only ever loses knowledge cannot oscillate, so this
        // is where termination comes from rather than from a ceiling.
        for (value, fact) in &current {
            let combined = match next.get(value) {
                Some(fresh) => fact.join(*fresh),
                None => Some(*fact),
            };
            if let Some(combined) = combined {
                next.insert(*value, combined);
            }
        }
        passes += 1;
        if next == current {
            return (current, passes, items);
        }
        current = next;
    }
}

/// What the two iterations derived, and what each of them cost.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SeedFactAgreement {
    /// Block arguments the fixed point gave a fact.
    pub arguments: usize,
    /// Blocks in the function.
    pub blocks: usize,
    /// Block evaluations the worklist spent.
    pub work_items: usize,
    /// Block evaluations the whole-function sweep spent.
    pub whole_pass_items: usize,
    /// Passes the whole-function sweep took.
    pub whole_passes: usize,
}

/// Run both iterations over one function and report whether they agree.
///
/// `Err` names the first argument they disagree about, in argument order. This
/// is the only caller of the oracle, and it exists for the tests: Section 8.10
/// says the order of visits does not change the result, and this is where that
/// is checked rather than believed.
///
/// # Errors
///
/// Returns the first block argument whose fact the two iterations derived
/// differently, with both facts, or the count of arguments only one of them
/// gave a fact at all.
#[doc(hidden)]
pub fn seed_fact_agreement(function: &Function) -> Result<SeedFactAgreement, String> {
    let admission = Admission::of(function);
    let worklist = SeedFacts::of(function, &admission);
    let (reference, whole_passes, whole_pass_items) = by_whole_passes(function, &admission);
    if worklist.arguments.len() != reference.len() {
        return Err(format!(
            "{}: the worklist gave {} arguments a fact and the sweep gave {}",
            function.name,
            worklist.arguments.len(),
            reference.len()
        ));
    }
    let mut ordered = worklist.arguments.keys().copied().collect::<Vec<_>>();
    ordered.sort_unstable();
    for value in ordered {
        let derived = worklist.arguments.get(&value);
        let expected = reference.get(&value);
        if derived != expected {
            return Err(format!(
                "{}: %{value} is {derived:?} by the worklist and {expected:?} by the sweep",
                function.name
            ));
        }
    }
    Ok(SeedFactAgreement {
        arguments: worklist.arguments.len(),
        blocks: function.blocks.len(),
        work_items: worklist.work_items(),
        whole_pass_items,
        whole_passes,
    })
}
