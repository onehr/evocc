//! Check an actual SSA/post-SSA pair without replaying the producer.
//!
//! The plan proposes demand, block-entry representatives and instruction
//! positions. Local checks establish the entry relation, then every real edge
//! establishes the next block's relation. Loop edges use the same check; no
//! execution bound or producer fixed point is trusted. This ordinary Rust
//! check does not extend the separately proved Lean machine profile.

use std::collections::{BTreeMap, BTreeSet};

use super::allocation::{AllocInst, AllocTerm};
use super::machine::{MachineBlock, MachineFunction};
use super::register::{RegisterClass, VirtualReg};

#[derive(Clone, Debug)]
pub struct BlockPlan<C> {
    pub prefix: usize,
    pub bindings: BTreeMap<VirtualReg<C>, VirtualReg<C>>,
    pub alignment: Vec<Option<usize>>,
}

#[derive(Clone, Debug)]
pub struct TransportPlan<C> {
    pub needed: BTreeSet<u32>,
    pub blocks: BTreeMap<u32, BlockPlan<C>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Input(u32),
    Unused(u32),
    Result(u32, usize, usize),
}

type Env<C> = BTreeMap<u32, (C, Origin)>;
type Check = Result<(), String>;

fn read<C: RegisterClass>(env: &Env<C>, reg: VirtualReg<C>) -> Option<Origin> {
    env.get(&reg.id)
        .filter(|(class, _)| *class == reg.class)
        .map(|(_, value)| *value)
}

fn uses<C: RegisterClass>(
    left: &[VirtualReg<C>],
    right: &[VirtualReg<C>],
    source: &Env<C>,
    target: &Env<C>,
) -> Check {
    if left.len() != right.len() {
        return Err("operand count changed".into());
    }
    for (&a, &b) in left.iter().zip(right) {
        if a.class != b.class || read(source, a).is_none() || read(source, a) != read(target, b) {
            return Err(format!(
                "value %v{} does not reach operand %v{}",
                a.id, b.id
            ));
        }
    }
    Ok(())
}

fn define<C: RegisterClass>(env: &mut Env<C>, regs: &[VirtualReg<C>], block: u32, position: usize) {
    for (index, &reg) in regs.iter().enumerate() {
        env.insert(reg.id, (reg.class, Origin::Result(block, position, index)));
    }
}

fn copies<I, C>(insts: &[I], env: &mut Env<C>) -> Check
where
    C: RegisterClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
{
    for inst in insts {
        let (dst, src) = inst.copy_operands().ok_or("extra non-copy instruction")?;
        if dst.class != src.class || dst.class.is_flags() || *inst != I::copy(dst, src) {
            return Err("invalid transport copy".into());
        }
        let value = read(env, src).ok_or("transport copy reads an unknown value")?;
        env.insert(dst.id, (dst.class, value));
    }
    Ok(())
}

fn shape<I, C>(inst: &I) -> I
where
    C: RegisterClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    let mut result = inst.clone();
    for reg in result.operand_slots(false) {
        reg.id = 0;
    }
    result
}

fn control<T: AllocTerm>(term: &T) -> T {
    let mut result = term.clone();
    result.clear_edge_args();
    result
}

fn term_shape<T, C>(term: &T) -> T
where
    C: RegisterClass,
    T: AllocTerm<Reg = VirtualReg<C>>,
{
    let mut result = control(term);
    // Computed branch addresses still name original labels. Their edge IDs
    // must remain exact, not be hidden by the normalization for split edges.
    if result.indirect_target().is_none() {
        for index in 0..result.edges().len() {
            result.retarget(index, 0);
        }
    }
    for reg in result.operand_slots() {
        reg.id = 0;
    }
    result
}

fn discarded<I, C>(inst: &I, needed: &BTreeSet<u32>) -> bool
where
    C: RegisterClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    inst.discardable() && inst.defs().iter().all(|r| !needed.contains(&r.id))
}

/// Validate source-local reads, unique definitions and the proposed demand
/// closure. Target opcode legality remains the instruction adapter's contract.
fn source_valid<I, T, C>(source: &MachineFunction<I, T, C>, needed: &BTreeSet<u32>) -> Check
where
    C: RegisterClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>>,
{
    let blocks: BTreeMap<_, _> = source.blocks.iter().map(|b| (b.id, b)).collect();
    if source.blocks.is_empty() || blocks.len() != source.blocks.len() {
        return Err("missing entry or duplicate source block".into());
    }
    let mut definitions = BTreeSet::new();
    let demanded = |regs: &[VirtualReg<C>]| regs.iter().all(|r| needed.contains(&r.id));
    for block in &source.blocks {
        let mut known = BTreeSet::new();
        for reg in block
            .args
            .iter()
            .copied()
            .chain(block.insts.iter().flat_map(|i| i.defs()))
            .chain(block.term.defs())
        {
            if !definitions.insert(reg.id) {
                return Err("duplicate source definition".into());
            }
        }
        known.extend(block.args.iter().copied());
        for inst in &block.insts {
            if !inst.uses().iter().all(|r| known.contains(r)) {
                return Err("source instruction reads a nonlocal value".into());
            }
            if !discarded(inst, needed) && !demanded(&inst.uses()) {
                return Err("demand omits a retained instruction input".into());
            }
            known.extend(inst.defs());
        }
        for pair in block.insts.windows(2) {
            let defs = pair[0].defs();
            if defs
                .iter()
                .any(|r| r.class.is_flags() && pair[1].uses().contains(r))
                && defs.iter().any(|r| needed.contains(&r.id))
                && !demanded(&pair[1].defs())
            {
                return Err("demand omits a flags consumer".into());
            }
        }
        let inputs = control(&block.term).uses();
        if !inputs.iter().all(|r| known.contains(r)) || !demanded(&inputs) {
            return Err("invalid source control inputs or demand".into());
        }
        known.extend(block.term.defs());
        for (id, args) in block.term.edges() {
            let next = blocks.get(&id).ok_or("missing source successor")?;
            if next.args.len() != args.len() {
                return Err("source edge arity mismatch".into());
            }
            for (&param, &arg) in next.args.iter().zip(args) {
                if param.class != arg.class
                    || !known.contains(&arg)
                    || (needed.contains(&param.id) && !needed.contains(&arg.id))
                {
                    return Err("source edge value or demand mismatch".into());
                }
            }
        }
    }
    Ok(())
}

struct Body<C> {
    source: Env<C>,
    target: Env<C>,
}

fn body<I, T, C>(
    before: &MachineBlock<I, T, C>,
    after: &MachineBlock<I, T, C>,
    plan: &BlockPlan<C>,
    needed: &BTreeSet<u32>,
) -> Result<Body<C>, String>
where
    C: RegisterClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
    T: AllocTerm<Reg = VirtualReg<C>> + PartialEq,
{
    if plan.alignment.len() != before.insts.len() || plan.prefix > after.insts.len() {
        return Err("invalid instruction alignment".into());
    }
    for (&param, &representative) in &plan.bindings {
        if !before.args.contains(&param)
            || !needed.contains(&param.id)
            || param.class != representative.class
        {
            return Err("invalid block-entry representative".into());
        }
    }
    let mut source = Env::new();
    let mut target = Env::new();
    for &param in &before.args {
        let value = if let Some(&reg) = plan.bindings.get(&param) {
            let value = Origin::Input(reg.id);
            if let Some((class, _)) = target.insert(reg.id, (reg.class, value)) {
                if class != reg.class {
                    return Err("representative class conflict".into());
                }
            }
            value
        } else {
            if needed.contains(&param.id) {
                return Err("missing demanded parameter".into());
            }
            Origin::Unused(param.id)
        };
        source.insert(param.id, (param.class, value));
    }
    let mut next = plan.prefix;
    for (index, (inst, position)) in before.insts.iter().zip(&plan.alignment).enumerate() {
        if let Some(position) = *position {
            if position < next || position >= after.insts.len() {
                return Err("instruction alignment is out of order or bounds".into());
            }
            copies(&after.insts[next..position], &mut target)?;
            let actual = &after.insts[position];
            if inst.defs() != actual.defs() || shape(inst) != shape(actual) {
                return Err("retained instruction or definitions changed".into());
            }
            uses(&inst.uses(), &actual.uses(), &source, &target)?;
            define(&mut target, &actual.defs(), before.id, index);
            next = position + 1;
        } else if !discarded(inst, needed) {
            return Err("removed an effect or demanded definition".into());
        }
        define(&mut source, &inst.defs(), before.id, index);
    }
    copies(&after.insts[next..], &mut target)?;
    if before.term.defs() != after.term.defs()
        || term_shape(&before.term) != term_shape(&after.term)
    {
        return Err("control operation or definitions changed".into());
    }
    uses(
        &control(&before.term).uses(),
        &control(&after.term).uses(),
        &source,
        &target,
    )?;
    // Assembly-goto outputs exist on every outgoing edge, after its inputs.
    define(
        &mut source,
        &before.term.defs(),
        before.id,
        before.insts.len(),
    );
    define(
        &mut target,
        &after.term.defs(),
        before.id,
        before.insts.len(),
    );
    Ok(Body { source, target })
}

/// Validate all proposed correspondences against the unchanged input and the
/// actual output. No call to forwarding, demand search or copy scheduling is
/// made here. A failure must prevent emission, not switch to an unchecked path.
pub fn verify<I, T, C>(
    source: &MachineFunction<I, T, C>,
    target: &MachineFunction<I, T, C>,
    plan: &TransportPlan<C>,
) -> Check
where
    C: RegisterClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
    T: AllocTerm<Reg = VirtualReg<C>> + PartialEq,
{
    source_valid(source, &plan.needed)?;
    let before: BTreeMap<_, _> = source.blocks.iter().map(|b| (b.id, b)).collect();
    let after: BTreeMap<_, _> = target.blocks.iter().map(|b| (b.id, b)).collect();
    if source.name != target.name
        || after.len() != target.blocks.len()
        || before.keys().ne(plan.blocks.keys())
        || source.blocks.first().map(|b| b.id) != target.blocks.first().map(|b| b.id)
    {
        return Err("function identity, block set or entry changed".into());
    }
    let entry = source.blocks[0].id;
    for actual in &target.blocks {
        if before.contains_key(&actual.id) {
            continue;
        }
        let edges = actual.term.edges();
        if !actual.args.is_empty()
            || edges.len() != 1
            || actual.term != T::jump(edges[0].0, vec![])
            || actual.insts.iter().any(|i| match i.copy_operands() {
                Some((dst, src)) => {
                    dst.class != src.class || dst.class.is_flags() || *i != I::copy(dst, src)
                }
                None => true,
            })
        {
            return Err("non-transport administrative block".into());
        }
    }
    for original in &source.blocks {
        let actual = *after.get(&original.id).ok_or("missing original block")?;
        let annotation = &plan.blocks[&original.id];
        if original.id == entry {
            if actual.args != original.args
                || annotation.prefix != 0
                || annotation.bindings.iter().any(|(a, b)| a != b)
            {
                return Err("function argument identity changed".into());
            }
        } else if !actual.args.is_empty() {
            return Err("post-SSA block still has parameters".into());
        }
        let state = body(original, actual, annotation, &plan.needed)
            .map_err(|error| format!("block {}: {error}", original.id))?;
        let expected_edges = original.term.edges();
        let actual_edges = actual.term.edges();
        if expected_edges.len() != actual_edges.len() {
            return Err("edge count changed".into());
        }
        for ((destination, values), (mut current, args)) in
            expected_edges.into_iter().zip(actual_edges)
        {
            if !args.is_empty() {
                return Err("post-SSA edge still has arguments".into());
            }
            let mut env = state.target.clone();
            let mut visited = BTreeSet::new();
            loop {
                let block = *after.get(&current).ok_or("missing transport destination")?;
                if current == destination {
                    let prefix = plan.blocks[&destination].prefix;
                    copies(
                        block.insts.get(..prefix).ok_or("prefix out of bounds")?,
                        &mut env,
                    )?;
                    break;
                }
                if before.contains_key(&current) || !visited.insert(current) {
                    return Err("edge changed destination or has an administrative cycle".into());
                }
                copies(&block.insts, &mut env)?;
                current = block.term.edges()[0].0;
            }
            for (&param, &value) in before[&destination].args.iter().zip(values) {
                if plan.needed.contains(&param.id) {
                    let representative = *plan.blocks[&destination]
                        .bindings
                        .get(&param)
                        .ok_or("missing edge binding")?;
                    uses(&[value], &[representative], &state.source, &env)?;
                }
            }
        }
    }
    Ok(())
}
