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
//! its own edges. Entry arguments are top. A pass only ever loses knowledge, so
//! the iteration stops and the block order it ran in does not change what it
//! reaches.

use std::collections::HashMap;

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
    let admitted = admitted_values(function);
    let mut current = entry_arguments(function);
    loop {
        let mut next = entry_arguments(function);
        for (index, block) in function.blocks.iter().enumerate() {
            if index != 0 && !spoken_for(block, &current) {
                continue;
            }
            let environment = environment_of(block, &current);
            edges_of(block, |target, passed| {
                carry(function, target, passed, &environment, &admitted, &mut next);
            });
        }
        // Knowledge only ever leaves, which is why this stops without a
        // ceiling of its own.
        for (value, fact) in &current {
            let combined = next.get(value).map_or(*fact, |fresh| fact.join(*fresh));
            next.insert(*value, combined);
        }
        if next == current {
            return current;
        }
        current = next;
    }
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

/// Combine what one edge passes into the facts of the arguments it binds.
fn carry(
    function: &Function,
    target: BlockId,
    passed: &[Value],
    environment: &HashMap<u32, Fact>,
    admitted: &std::collections::HashSet<u32>,
    next: &mut HashMap<u32, Fact>,
) {
    let Some(index) = function.blocks.iter().position(|block| block.id == target) else {
        return;
    };
    if index == 0 {
        return;
    }
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
        let combined = next
            .get(&parameter.value.id)
            .map_or(fact, |existing| existing.join(fact));
        next.insert(parameter.value.id, combined);
    }
}
