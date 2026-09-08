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

    /// Check the derived table against the inductive invariant of Section 8.10.
    ///
    /// The fixed point is an iteration, and an iteration's answer is only as
    /// good as the code that ran it. The invariant is the property the answer
    /// has to have whatever produced it, so checking it is evidence about the
    /// table rather than about the loop: entry arguments promise nothing, every
    /// instruction's transfer is at least as precise as the fact recorded for
    /// its result, and every edge carries facts at least as precise as the ones
    /// recorded for the arguments it binds.
    pub(crate) fn inductive(&self, module: &Module) -> Result<(), Violation> {
        for function in &module.functions {
            let empty = HashMap::new();
            let table = self.functions.get(&function.name).unwrap_or(&empty);
            inductive_invariant(function, table)?;
        }
        Ok(())
    }
}

/// Where a claimed table fails the inductive invariant, and on what.
///
/// The three arms are the three conditions, in the order Section 8.10 states
/// them; each names the value whose recorded fact is stronger than what the
/// program supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Violation {
    /// An entry argument carries a fact. A caller may pass anything.
    EntryArgument { function: String, value: u32 },
    /// `Transfer_op(I(operands))` is not at or below `I(result)`.
    Instruction {
        function: String,
        block: BlockId,
        value: u32,
    },
    /// What an edge carries is not at or below the destination's argument.
    Edge {
        function: String,
        source: BlockId,
        target: BlockId,
        value: u32,
    },
}

/// The three conditions, over every block and edge structurally reachable from
/// the entry.
///
/// Reachability is read off the terminators and nothing else: a table that
/// called a block unreachable would otherwise be believed about the one place
/// the invariant could not then be checked. Back edges are ordinary edges here,
/// which is the whole point -- a loop is where a table can claim a fact that
/// holds on the way in and not on the way round. A value the table does not
/// name is top, so an incomplete table is weak rather than rejected, and a table
/// that names nothing passes.
fn inductive_invariant(function: &Function, table: &HashMap<u32, Fact>) -> Result<(), Violation> {
    let Some(entry) = function.blocks.first() else {
        return Ok(());
    };
    for argument in &entry.args {
        let Some(width) = width_of(&argument.value) else {
            continue;
        };
        if table
            .get(&argument.value.id)
            .is_some_and(|fact| *fact != Fact::top(width))
        {
            return Err(Violation::EntryArgument {
                function: function.name.clone(),
                value: argument.value.id,
            });
        }
    }

    let admitted = admitted_values(function);
    let indices = block_indices(function);
    let mut seen = vec![false; function.blocks.len()];
    let mut queue = VecDeque::from([0_usize]);
    seen[0] = true;
    while let Some(index) = queue.pop_front() {
        let block = &function.blocks[index];
        let environment = claimed_environment(function, block, table)?;
        let mut failure = None;
        edges_of(block, |target, passed| {
            if failure.is_some() {
                return;
            }
            let Some(target_index) = indices.get(&target).copied() else {
                return;
            };
            if !seen[target_index] {
                seen[target_index] = true;
                queue.push_back(target_index);
            }
            for (value, parameter) in passed.iter().zip(&function.blocks[target_index].args) {
                let Some(width) = width_of(&parameter.value) else {
                    continue;
                };
                let carried = if admitted.contains(&value.id) {
                    environment
                        .get(&value.id)
                        .copied()
                        .unwrap_or_else(|| Fact::top(width))
                } else {
                    Fact::top(width)
                };
                let recorded = table
                    .get(&parameter.value.id)
                    .copied()
                    .unwrap_or_else(|| Fact::top(width));
                if !carried.is_at_or_below(recorded) {
                    failure = Some(Violation::Edge {
                        function: function.name.clone(),
                        source: block.id,
                        target,
                        value: parameter.value.id,
                    });
                    return;
                }
            }
        });
        if let Some(violation) = failure {
            return Err(violation);
        }
    }
    Ok(())
}

/// The facts of one block's values under a claimed table.
///
/// A block argument takes what the table records for it, or top. An instruction
/// result takes what the table records for it, after the second condition has
/// confirmed that the transfer over its operands is at least that precise; a
/// result the table does not name takes the transfer itself. Reading the claim
/// back out is what lets a wrong claim reach the edge that refutes it, rather
/// than being quietly recomputed away.
fn claimed_environment(
    function: &Function,
    block: &Block,
    table: &HashMap<u32, Fact>,
) -> Result<HashMap<u32, Fact>, Violation> {
    let mut environment: HashMap<u32, Fact> = block
        .args
        .iter()
        .filter_map(|argument| Some((argument.value.id, *table.get(&argument.value.id)?)))
        .collect();
    for instruction in &block.insts {
        let Some(result) = inst_results(instruction).first().copied() else {
            continue;
        };
        let Some(width) = width_of(result) else {
            continue;
        };
        let derived =
            transfer(instruction, width, &environment).unwrap_or_else(|| Fact::top(width));
        match table.get(&result.id).copied() {
            Some(recorded) => {
                if !derived.is_at_or_below(recorded) {
                    return Err(Violation::Instruction {
                        function: function.name.clone(),
                        block: block.id,
                        value: result.id,
                    });
                }
                environment.insert(result.id, recorded);
            }
            None => {
                environment.insert(result.id, derived);
            }
        }
    }
    Ok(environment)
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
        Inst::AsmGoto {
            labels,
            fallthrough,
            ..
        } => {
            for edge in labels.iter().map(|label| &label.edge).chain([fallthrough]) {
                visit(edge.target, &edge.arguments);
            }
        }
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
    use std::collections::{BTreeMap, HashMap};

    use evocc_evoir::{
        validate_module, Block, BlockArg, CmpOp, EffectSummary, EvoIRType, Function,
        GlobalVisibility, Inst, Linkage, Module, Value,
    };
    use evocc_randgate::{generate_module, GeneratorConfig};

    use super::{agreement, arguments_of, inductive_invariant, Fact, Violation};

    const WIDTH: u16 = 8;

    fn bits(id: u32) -> Value {
        Value {
            id,
            ty: EvoIRType::Bits(WIDTH),
        }
    }

    fn flag(id: u32) -> Value {
        Value {
            id,
            ty: EvoIRType::Bits(1),
        }
    }

    fn argument(value: &Value) -> BlockArg {
        BlockArg {
            name: format!("v{}", value.id),
            value: value.clone(),
        }
    }

    /// A counted loop: the parameter enters as zero and comes round as `x + 1`.
    ///
    /// This is the shape the inductive invariant exists for. The fact that holds
    /// on the way in does not hold on the way round, so a table that recorded
    /// only what the entering edge supports would be wrong about every later
    /// iteration, and only the back edge shows it.
    fn counted_loop() -> Module {
        let zero = bits(1);
        let parameter = bits(2);
        let one = bits(3);
        let next = bits(4);
        let more = flag(5);
        let result = bits(6);
        let function = Function {
            linkage: Linkage::Strong,
            visibility: GlobalVisibility::External,
            variadic: false,
            name: "counted".to_owned(),
            params: Vec::new(),
            param_marks: Vec::new(),
            ret_tys: vec![EvoIRType::Bits(WIDTH)],
            effects: EffectSummary::default(),
            blocks: vec![
                Block {
                    id: 0,
                    args: Vec::new(),
                    insts: vec![Inst::Const(zero.clone(), 0)],
                    terminator: Inst::Br(1, vec![zero]),
                },
                Block {
                    id: 1,
                    args: vec![argument(&parameter)],
                    insts: vec![
                        Inst::Const(one.clone(), 1),
                        Inst::Add(next.clone(), parameter.clone(), one.clone()),
                        Inst::ICmp(more.clone(), CmpOp::ULt, next.clone(), one),
                    ],
                    terminator: Inst::CondBr(more, 1, vec![next], 2, vec![parameter]),
                },
                Block {
                    id: 2,
                    args: vec![argument(&result)],
                    insts: Vec::new(),
                    terminator: Inst::Ret(vec![result]),
                },
            ],
        };
        let mut module = Module {
            aliases: Vec::new(),
            assembly: Vec::new(),
            globals: Vec::new(),
            extern_objects: Vec::new(),
            declares: Vec::new(),
            functions: vec![function],
        };
        module.recompute_effect_summaries();
        validate_module(&module).expect("the counted loop is valid EvoIR");
        module
    }

    /// The table the checker derives satisfies the invariant it is checked
    /// against, which is what makes a refusal evidence rather than noise.
    #[test]
    fn the_derived_table_satisfies_the_inductive_invariant() {
        let module = counted_loop();
        let function = &module.functions[0];
        let derived = arguments_of(function);
        assert_eq!(
            inductive_invariant(function, &derived),
            Ok(()),
            "the checker's own fixed point must pass its own invariant"
        );
        // And the loop parameter is top, because the two edges disagree about
        // every bit once the increment has run.
        assert_eq!(derived.get(&2), Some(&Fact::top(WIDTH)));
    }

    /// The plan's negative case: a table claiming the loop parameter is always
    /// zero. The entering edge supports it and the back edge refutes it, so a
    /// check that walked only the forward edges would accept.
    #[test]
    fn a_table_claiming_a_loop_parameter_is_always_zero_is_refused() {
        let module = counted_loop();
        let function = &module.functions[0];
        let mut claimed: HashMap<u32, Fact> = HashMap::new();
        claimed.insert(2, Fact::constant(WIDTH, 0));
        claimed.insert(6, Fact::constant(WIDTH, 0));
        assert_eq!(
            inductive_invariant(function, &claimed),
            Err(Violation::Edge {
                function: "counted".to_owned(),
                source: 1,
                target: 1,
                value: 2,
            })
        );
    }

    /// The first condition on its own: a caller may pass anything, so an entry
    /// argument carrying a fact is refused whatever the body does with it.
    #[test]
    fn a_table_claiming_an_entry_argument_is_refused() {
        let module = counted_loop();
        let mut function = module.functions[0].clone();
        let opaque = bits(7);
        function.params.push(evocc_evoir::FunctionParam {
            name: "x".to_owned(),
            ty: EvoIRType::Bits(WIDTH),
        });
        function.blocks[0].args.push(argument(&opaque));
        let mut claimed: HashMap<u32, Fact> = HashMap::new();
        claimed.insert(7, Fact::constant(WIDTH, 3));
        assert_eq!(
            inductive_invariant(&function, &claimed),
            Err(Violation::EntryArgument {
                function: "counted".to_owned(),
                value: 7,
            })
        );
    }

    /// The second condition on its own: a table may record an instruction's
    /// result, and the transfer over its operands has to be at least that
    /// precise.
    #[test]
    fn a_table_claiming_more_than_a_transfer_supports_is_refused() {
        let module = counted_loop();
        let function = &module.functions[0];
        let mut claimed: HashMap<u32, Fact> = HashMap::new();
        // `%v4 = add %v2, 1` with `%v2` top is top; claiming it is seven is
        // claiming what the table's own operands do not support.
        claimed.insert(4, Fact::constant(WIDTH, 7));
        assert_eq!(
            inductive_invariant(function, &claimed),
            Err(Violation::Instruction {
                function: "counted".to_owned(),
                block: 1,
                value: 4,
            })
        );
    }

    /// The corpus every measurement here is taken over.
    const CORPUS: u64 = 256;

    /// The invariant holds over the whole generated corpus.
    ///
    /// The check runs on the checking path, so a table it refused would be a
    /// certificate refused: a single false positive here is a checker that
    /// rejects sound work. Every function of the corpus is a case where the
    /// answer must be "no violation".
    #[test]
    fn the_derived_table_satisfies_the_invariant_over_the_corpus() {
        let mut functions = 0_usize;
        for seed in 0..CORPUS {
            let module = generate_module(seed, GeneratorConfig::default());
            for function in &module.functions {
                let derived = arguments_of(function);
                assert_eq!(
                    inductive_invariant(function, &derived),
                    Ok(()),
                    "seed {seed}, function {}",
                    function.name
                );
                functions += 1;
            }
        }
        println!("inductiveInvariantFunctions={functions}");
        assert!(functions > 0, "the corpus must hold some function");
    }

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
