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
//! The iteration is the classical one Section 8.10 describes. Every argument
//! outside the entry block starts at the combination's identity -- no edge has
//! spoken for it yet -- and a block none of whose predecessors has spoken
//! contributes nothing on its own edges, which is what makes a value carried
//! round a loop keep what every path into the loop agrees on. Entry arguments
//! are top, because a caller may pass anything. Each pass only loses knowledge,
//! so the iteration reaches a fixed point that does not depend on the order the
//! blocks were visited in.

use std::collections::HashMap;

use evocc_evoir::{Block, BlockId, Function, Inst, Value};

use crate::eligibility::Admission;
use crate::known_bits::KnownBits;
use crate::scalar_eq::{known_of, record_known, scalar_width, KnownEnv};

/// One function's block-argument facts, and what the fixed point cost.
pub(crate) struct SeedFacts {
    arguments: HashMap<u32, KnownBits>,
    passes: usize,
}

impl SeedFacts {
    pub(crate) fn of(function: &Function, admission: &Admission) -> Self {
        let mut current = entry_facts(function);
        let mut passes = 0_usize;
        loop {
            let mut next = entry_facts(function);
            for (index, block) in function.blocks.iter().enumerate() {
                if index != 0 && !spoken_for(block, &current) {
                    continue;
                }
                let env = block_environment(block, &current);
                cross_edges(function, block, &env, admission, &mut next);
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
                return Self {
                    arguments: current,
                    passes,
                };
            }
            current = next;
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

    /// How many passes the fixed point took, as a cost figure.
    pub(crate) const fn passes(&self) -> usize {
        self.passes
    }
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
/// what Section 8.10 says an argument not yet visited contributes.
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

/// Combine what this block passes into the facts of the arguments it reaches.
fn cross_edges(
    function: &Function,
    block: &Block,
    env: &KnownEnv,
    admission: &Admission,
    next: &mut HashMap<u32, KnownBits>,
) {
    let mut carry = |target: BlockId, arguments: &[Value]| {
        // Every edge counts; none is pruned by a fact, so a comparison the
        // analysis has already decided still contributes on both sides.
        let Some(index) = function.blocks.iter().position(|block| block.id == target) else {
            return;
        };
        if index == 0 {
            return;
        }
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
            let combined = match next.get(&parameter.value.id) {
                Some(existing) => existing.join(fact),
                None => Some(fact),
            };
            if let Some(combined) = combined {
                next.insert(parameter.value.id, combined);
            }
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
