//! Independent allocated-machine dataflow and operand-constraint verification.
//!
//! This pass checks local constraints and then relates the unchanged input to
//! allocated registers and slots. It does not consume allocator analyses.

use std::collections::{BTreeMap, BTreeSet};

use super::allocation::{
    assembly_clobbers, AllocClass, AllocInst, AllocTerm, Allocated, AllocationTarget,
    ConstraintBody, ConstraintOperand, ConstraintValue,
};
use super::machine::{MachInst, MachineBlock};
use super::register::VirtualReg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierRefusal {
    pub code: &'static str,
    pub name: &'static str,
    pub block: u32,
    pub detail: String,
}

fn operand_register<R: Copy>(value: ConstraintValue<R>) -> Option<R> {
    match value {
        ConstraintValue::Reg(reg) => Some(reg),
        _ => None,
    }
}

fn verify_assembly<B, C, A>(
    target: &A,
    body: &B,
    block: u32,
    register: &impl Fn(VirtualReg<C>) -> Option<u8>,
    refuse: &impl Fn(u32, String) -> VerifierRefusal,
) -> Result<(), VerifierRefusal>
where
    B: ConstraintBody<Reg = VirtualReg<C>>,
    C: AllocClass,
    A: AllocationTarget<C>,
{
    let number = |operand: &B::Operand| operand_register(operand.value()).and_then(register);
    for operand in body.outputs().iter().chain(body.inputs()) {
        if matches!(operand.value(), ConstraintValue::Symbol) && !operand.accepts_symbol() {
            return Err(refuse(
                block,
                "only i admits a symbolic assembly operand".to_owned(),
            ));
        }
        if let Some(expected) = operand.fixed_register() {
            if !target.fixed_assembly_registers().contains(&expected)
                || number(operand) != Some(expected)
            {
                return Err(refuse(
                    block,
                    format!(
                        "assembly binding {} does not match its assigned register",
                        target.fixed_binding_spelling(expected)
                    ),
                ));
            }
        }
        if !operand.needs_register() {
            continue;
        }
        match operand.value() {
            ConstraintValue::Zero => {
                if !operand.accepts_zero() {
                    return Err(refuse(
                        block,
                        format!(
                            "the zero register answers a \"{}\" operand",
                            operand.spelling()
                        ),
                    ));
                }
            }
            ConstraintValue::Immediate | ConstraintValue::Symbol => {
                return Err(refuse(
                    block,
                    format!("an immediate answers a \"{}\" operand", operand.spelling()),
                ));
            }
            ConstraintValue::Reg(reg) => {
                let bank = if let Some(digit) = operand.tied_output() {
                    body.outputs().get(usize::from(digit)).map(|out| out.bank())
                } else {
                    Some(operand.bank())
                };
                if bank.is_none_or(|bank| bank != reg.class.bank()) {
                    return Err(refuse(
                        block,
                        "assembly constraint has an incompatible register bank".into(),
                    ));
                }
                if register(reg).is_some_and(|number| !target.bank(reg.class).contains(number)) {
                    return Err(refuse(
                        block,
                        "assembly allocation has an incompatible register bank".into(),
                    ));
                }
                if register(reg).is_none() {
                    return Err(refuse(
                        block,
                        format!("%v{} is an asm operand and was never assigned", reg.id),
                    ));
                }
            }
        }
    }

    let outputs: Vec<_> = body
        .outputs()
        .iter()
        .filter(|output| !output.memory())
        .filter_map(number)
        .collect();
    if outputs.iter().collect::<BTreeSet<_>>().len() != outputs.len() {
        return Err(refuse(
            block,
            "distinct assembly outputs share a physical register".into(),
        ));
    }

    for (index, operand) in body.outputs().iter().enumerate() {
        if !operand.early_clobber() {
            continue;
        }
        let Some(assigned) = number(operand) else {
            continue;
        };
        if body.inputs().iter().any(|input| {
            input.tied_output() != Some(index as u8) && number(input) == Some(assigned)
        }) {
            return Err(refuse(
                block,
                format!("an early-clobber output shares register {assigned} with an input"),
            ));
        }
    }

    for input in body.inputs() {
        let Some(digit) = input.tied_output() else {
            continue;
        };
        let Some(output) = body.outputs().get(usize::from(digit)) else {
            return Err(refuse(
                block,
                format!("a tied operand names output {digit}, which the asm does not have"),
            ));
        };
        if number(input) != number(output) {
            return Err(refuse(
                block,
                format!(
                    "a tied operand is in register {:?} and output {digit} in {:?}",
                    number(input),
                    number(output)
                ),
            ));
        }
    }

    let clobbered = body.clobbered_registers();
    for operand in body.outputs().iter().chain(body.inputs()) {
        if let Some(assigned) = number(operand) {
            if clobbered.contains(&assigned) {
                return Err(refuse(
                    block,
                    format!("register {assigned} is clobbered and holds an asm operand"),
                ));
            }
        }
    }
    Ok(())
}

/// A known virtual holder, or the meet of paths with different holders.
type State = BTreeMap<u8, Option<u32>>;

pub fn verify_allocation<I, T, C, A>(
    target: &A,
    allocated: &Allocated<I, T, C>,
) -> Result<(), VerifierRefusal>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body> + PartialEq,
    A: AllocationTarget<C>,
{
    let refuse = |block: u32, detail: String| VerifierRefusal {
        code: "E4020",
        name: "allocation: verifier-refused",
        block,
        detail,
    };
    let register = |reg: VirtualReg<C>| -> Option<u8> {
        if reg.class.is_flags() {
            None
        } else {
            allocated.assignment.get(&reg.id).copied()
        }
    };

    for reg in allocated.blocks.iter().flat_map(|block| {
        block
            .insts
            .iter()
            .flat_map(MachInst::defs)
            .chain(block.insts.iter().flat_map(MachInst::uses))
            .chain(block.term.uses())
    }) {
        if reg.class.is_flags() {
            continue;
        }
        let Some(number) = register(reg) else {
            return Err(refuse(0, format!("%v{} is unassigned", reg.id)));
        };
        if !target.bank(reg.class).contains(number) {
            return Err(refuse(
                0,
                format!("%v{} has an incompatible register bank", reg.id),
            ));
        }
    }

    // The frame saves exactly the callee-saved registers that the allocation
    // names. Every callee-saved register that the function writes, through an
    // assigned value or an assembly clobber, must be among them.
    let written = allocated
        .assignment
        .values()
        .copied()
        .chain(assembly_clobbers(&allocated.blocks));
    for number in written {
        if target.abi().callee_saved.contains(&number)
            && !allocated.used_callee_saved.contains(&number)
        {
            return Err(refuse(
                0,
                format!("callee-saved register {number} is written and is not saved"),
            ));
        }
    }

    // Recover live entry values independently of allocation. As before, the
    // admitted subset is bounded by the total ordinary parameter count in
    // each argument bank; do not strengthen this to signature reconstruction.
    let mut entry: State = BTreeMap::new();
    let boundary = live_on_entry(allocated);
    let ordinary = allocated
        .parameters
        .saturating_sub(usize::from(allocated.indirect_result));
    let limit = u8::try_from(ordinary).unwrap_or(u8::MAX);
    for value in &boundary {
        let Some(number) = allocated.assignment.get(value).copied() else {
            return Err(refuse(
                0,
                format!("%v{value} is live when the function is entered and was never assigned"),
            ));
        };
        if allocated.indirect_result && number == target.abi().indirect_result {
            if entry.insert(number, Some(*value)).is_some() {
                return Err(refuse(
                    0,
                    format!(
                        "two arguments are live when the function is entered in register {number}"
                    ),
                ));
            }
            continue;
        }
        if !target
            .abi()
            .argument_banks
            .iter()
            .any(|bank| bank.contains(number) && number - bank.first < limit.min(bank.count))
        {
            return Err(refuse(
                0,
                format!(
                    "%v{value} is live when the function is entered in register {number}, and Section 6 brings {} arguments in",
                    allocated.parameters
                ),
            ));
        }
        if entry.insert(number, Some(*value)).is_some() {
            return Err(refuse(
                0,
                format!("two arguments are live when the function is entered in register {number}"),
            ));
        }
    }

    let mut incoming: BTreeMap<u32, State> = BTreeMap::new();
    if let Some(first) = allocated.blocks.first() {
        incoming.insert(first.id, entry);
    }
    // A discovered state can only lose known holders, so this finite lattice
    // converges. Do not accept a partial result at an iteration limit.
    loop {
        let mut changed = false;
        for block in &allocated.blocks {
            let Some(state) = incoming.get(&block.id).cloned() else {
                continue;
            };
            let exit = transfer(target, &state, block, &register);
            for (target, _) in block.term.edges() {
                let merged = match incoming.get(&target) {
                    Some(existing) => meet(existing, &exit),
                    None => exit.clone(),
                };
                if incoming.get(&target) != Some(&merged) {
                    incoming.insert(target, merged);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    for block in &allocated.blocks {
        let Some(state) = incoming.get(&block.id).cloned() else {
            continue;
        };
        let mut state = state;
        for (index, inst) in block.insts.iter().enumerate() {
            if let Some(dst) = inst.exclusive_result() {
                if inst
                    .uses()
                    .iter()
                    .any(|input| register(*input) == register(dst))
                {
                    return Err(refuse(
                        block.id,
                        "atomic RMW output overlaps an input of its exclusive loop".into(),
                    ));
                }
            }
            for reg in inst.uses() {
                if reg.class.is_flags() {
                    continue;
                }
                let Some(number) = register(reg) else {
                    return Err(refuse(
                        block.id,
                        format!("%v{} is read and was never assigned", reg.id),
                    ));
                };
                match state.get(&number) {
                    Some(Some(holder)) if *holder == reg.id => {}
                    Some(Some(_)) => {
                        return Err(refuse(
                            block.id,
                            format!(
                                "register {number} holds another value where %v{} is read",
                                reg.id
                            ),
                        ));
                    }
                    Some(None) => {
                        return Err(refuse(
                            block.id,
                            format!(
                                "register {number} holds different values on the paths that reach the read of %v{}",
                                reg.id
                            ),
                        ));
                    }
                    None => {
                        return Err(refuse(
                            block.id,
                            format!("register {number} is read before it is written"),
                        ));
                    }
                }
            }

            if let Some(flags) = inst.defs().into_iter().find(|reg| reg.class.is_flags()) {
                let consumers = match block.insts.get(index + 1) {
                    Some(next) => next.uses(),
                    None => block.term.uses(),
                };
                if !consumers.contains(&flags) {
                    return Err(refuse(
                        block.id,
                        format!(
                            "%v{} sets the flags and the instruction after it does not read them",
                            flags.id
                        ),
                    ));
                }
            }
            if let Some(body) = inst.assembly() {
                verify_assembly(target, body, block.id, &register, &refuse)?;
            }
            apply(target, &mut state, inst, &register);
        }
        if let Some(body) = block.term.assembly() {
            verify_assembly(target, body, block.id, &register, &refuse)?;
        }
        for reg in block.term.uses() {
            if reg.class.is_flags() {
                continue;
            }
            let Some(number) = register(reg) else {
                return Err(refuse(
                    block.id,
                    format!(
                        "%v{} is read by the terminator and was never assigned",
                        reg.id
                    ),
                ));
            };
            match state.get(&number) {
                Some(Some(holder)) if *holder == reg.id => {}
                _ => {
                    return Err(refuse(
                        block.id,
                        format!(
                            "the terminator reads register {number}, which does not hold %v{}",
                            reg.id
                        ),
                    ));
                }
            }
        }
    }
    super::allocation_relation::verify(target, allocated).map_err(|detail| refuse(0, detail))
}

fn meet(left: &State, right: &State) -> State {
    let mut merged = BTreeMap::new();
    for (number, value) in left {
        if let Some(other) = right.get(number) {
            merged.insert(*number, if value == other { *value } else { None });
        }
    }
    merged
}

fn transfer<I, T, C, A>(
    target: &A,
    state: &State,
    block: &MachineBlock<I, T, C>,
    register: &impl Fn(VirtualReg<C>) -> Option<u8>,
) -> State
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
    A: AllocationTarget<C>,
{
    let mut state = state.clone();
    for inst in &block.insts {
        apply(target, &mut state, inst, register);
    }
    if let Some(body) = block.term.assembly() {
        for number in body.clobbered_registers() {
            state.remove(&number);
        }
        for reg in body.defined() {
            if let Some(number) = register(reg) {
                state.insert(number, Some(reg.id));
            }
        }
    }
    state
}

fn apply<I, C, A>(
    target: &A,
    state: &mut State,
    inst: &I,
    register: &impl Fn(VirtualReg<C>) -> Option<u8>,
) where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    A: AllocationTarget<C>,
{
    if inst.is_call() {
        for number in target.abi().call_clobbers {
            state.remove(number);
        }
    }
    // An assembly body writes every register of its clobber list, wherever
    // it stands in its block.
    if let Some(body) = inst.assembly() {
        for number in body.clobbered_registers() {
            state.remove(&number);
        }
    }
    for reg in inst.defs() {
        if reg.class.is_flags() {
            continue;
        }
        if let Some(number) = register(reg) {
            state.insert(number, Some(reg.id));
        }
    }
}

/// Separate backward analysis, derived only from the allocated blocks.
fn live_on_entry<I, T, C>(allocated: &Allocated<I, T, C>) -> Vec<u32>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut reads: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut writes: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for block in &allocated.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            for reg in inst.uses() {
                if !reg.class.is_flags() && !written.contains(&reg.id) {
                    read.insert(reg.id);
                }
            }
            for reg in inst.defs() {
                written.insert(reg.id);
            }
        }
        for reg in block.term.uses() {
            if !reg.class.is_flags() && !written.contains(&reg.id) {
                read.insert(reg.id);
            }
        }
        if let Some(body) = block.term.assembly() {
            written.extend(body.defined().iter().map(|reg| reg.id));
        }
        reads.insert(block.id, read);
        writes.insert(block.id, written);
    }

    let mut live_in: BTreeMap<u32, BTreeSet<u32>> = allocated
        .blocks
        .iter()
        .map(|block| (block.id, BTreeSet::new()))
        .collect();
    loop {
        let mut changed = false;
        for block in allocated.blocks.iter().rev() {
            let mut out: BTreeSet<u32> = BTreeSet::new();
            for (target, _) in block.term.edges() {
                if let Some(entering) = live_in.get(&target) {
                    out.extend(entering.iter().copied());
                }
            }
            let mut entering = reads[&block.id].clone();
            for value in out {
                if !writes[&block.id].contains(&value) {
                    entering.insert(value);
                }
            }
            if live_in[&block.id] != entering {
                live_in.insert(block.id, entering);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    allocated
        .blocks
        .first()
        .and_then(|block| live_in.get(&block.id))
        .map(|values| values.iter().copied().collect())
        .unwrap_or_default()
}
