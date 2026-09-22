//! Section 9, instrument two: the allocation verifier.
//!
//! "A separate pass over the allocated function checks the three properties of
//! Section 7 by dataflow; the allocator does not certify itself." Nothing here
//! reads an interval, a spill decision or any other record of what the
//! allocator concluded: the input is the allocated function, and the three
//! properties are re-derived from it.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    inst_defs, inst_uses, term_edges, term_uses, Allocated, AsmBody, AsmConstraint, AsmOperand,
    AsmValue, MInst, MTerm, RegClass, VReg,
};

/// The Section 10 refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierRefusal {
    pub code: &'static str,
    pub name: &'static str,
    pub block: u32,
    pub detail: String,
}

/// Section 9's second instrument, over an `asm`: every constraint class of
/// Section 7 is checked against the registers the allocator actually gave.
///
/// The allocator does not certify itself here either. Each row is recomputed
/// from the operand list and the assignment alone.
fn verify_assembly(
    body: &AsmBody,
    block: u32,
    register: &impl Fn(VReg) -> Option<u8>,
    refuse: &impl Fn(u32, String) -> VerifierRefusal,
) -> Result<(), VerifierRefusal> {
    let number = |operand: &AsmOperand| -> Option<u8> { operand.register().and_then(register) };

    // Every operand that needs a register has one.
    for operand in body.numbered() {
        if matches!(&operand.value, AsmValue::Symbol { .. })
            && !matches!(
                operand.constraint,
                AsmConstraint::Immediate(crate::AsmImmediate::Integer)
            )
        {
            return Err(refuse(
                block,
                "only i admits a symbolic assembly operand".to_owned(),
            ));
        }
        if let Some(expected) = operand.fixed_register {
            if !(matches!(expected, 0..=15 | 19..=28 | 30)
                || crate::linscan::SIMD_CALLER_SAVED.contains(&expected))
                || number(operand) != Some(expected)
            {
                return Err(refuse(
                    block,
                    format!("assembly binding x{expected} does not match its assigned register"),
                ));
            }
        }
        if !operand.constraint.needs_register() {
            continue;
        }
        match operand.value {
            AsmValue::Zero(_) => {
                if !matches!(operand.constraint, AsmConstraint::RegisterOrZero) {
                    return Err(refuse(
                        block,
                        format!(
                            "the zero register answers a \"{}\" operand",
                            operand.constraint.spelling()
                        ),
                    ));
                }
            }
            AsmValue::Imm(_) | AsmValue::Symbol { .. } => {
                return Err(refuse(
                    block,
                    format!(
                        "an immediate answers a \"{}\" operand",
                        operand.constraint.spelling()
                    ),
                ))
            }
            AsmValue::Reg(reg) => {
                let constraint = if let AsmConstraint::Tied(digit) = operand.constraint {
                    body.outputs
                        .get(usize::from(digit))
                        .map(|out| out.constraint)
                } else {
                    Some(operand.constraint)
                };
                if constraint.is_none_or(|constraint| constraint.simd() != reg.class.is_simd()) {
                    return Err(refuse(
                        block,
                        "assembly constraint has an incompatible register bank".into(),
                    ));
                }
                if register(reg).is_some_and(|number| {
                    if reg.class.is_simd() {
                        !(64..=95).contains(&number)
                    } else {
                        number >= 31
                    }
                }) {
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
        .outputs
        .iter()
        .filter(|output| !matches!(output.constraint, AsmConstraint::Memory { .. }))
        .filter_map(number)
        .collect();
    if outputs.iter().collect::<BTreeSet<_>>().len() != outputs.len() {
        return Err(refuse(
            block,
            "distinct assembly outputs share a physical register".into(),
        ));
    }

    // Early clobber: no input holds the register.
    for (index, operand) in body.outputs.iter().enumerate() {
        if !operand.constraint.early_clobber() {
            continue;
        }
        let Some(assigned) = number(operand) else {
            continue;
        };
        if body.inputs.iter().any(|input| {
            input.constraint != AsmConstraint::Tied(index as u8) && number(input) == Some(assigned)
        }) {
            return Err(refuse(
                block,
                format!("an early-clobber output shares register {assigned} with an input"),
            ));
        }
    }

    // A tied operand names an output that exists and shares its register.
    for input in &body.inputs {
        let AsmConstraint::Tied(digit) = input.constraint else {
            continue;
        };
        let Some(output) = body.outputs.get(usize::from(digit)) else {
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

    // A clobbered register holds no operand: the asm destroys it.
    let clobbered = body.clobbered_registers();
    for operand in body.numbered() {
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

/// What a physical register is known to hold on entry to a block.
///
/// `Some(value)` is one virtual register's contents; `None` is the meet of two
/// paths that disagree, which is what "no two simultaneously live values share
/// a register" forbids a use from reading.
type State = BTreeMap<u8, Option<u32>>;

/// The registers a call destroys: Section 6's caller-saved set.
/// Section 6 (revision 0.18): the register the destination of a record result
/// above sixteen bytes arrives in.
const INDIRECT_RESULT: u8 = 8;

const CLOBBERED: [u8; 18] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];

/// Check an allocated function.
pub fn verify_allocation(allocated: &Allocated) -> Result<(), VerifierRefusal> {
    let refuse = |block: u32, detail: String| VerifierRefusal {
        code: "E4020",
        name: "allocation: verifier-refused",
        block,
        detail,
    };

    let register = |reg: VReg| -> Option<u8> {
        if reg.class == RegClass::Flags {
            None
        } else {
            allocated.assignment.get(&reg.id).copied()
        }
    };

    for reg in allocated.blocks.iter().flat_map(|block| {
        block
            .insts
            .iter()
            .flat_map(inst_defs)
            .chain(block.insts.iter().flat_map(inst_uses))
            .chain(term_uses(&block.term))
    }) {
        if reg.class == RegClass::Flags {
            continue;
        }
        let Some(number) = register(reg) else {
            return Err(refuse(0, format!("%v{} is unassigned", reg.id)));
        };
        let valid = if reg.class.is_simd() {
            (64..=95).contains(&number)
        } else {
            number < 31
        };
        if !valid {
            return Err(refuse(
                0,
                format!("%v{} has an incompatible register bank", reg.id),
            ));
        }
    }

    // Section 6: the parameters arrive in x0 onwards, and nothing else is
    // defined when the function is entered. Which virtual registers those are
    // is recovered here rather than taken from the allocator: they are exactly
    // the values the entry block reads without first writing them, and the
    // registers they were given must be x0 upwards with nothing missing.
    let mut entry: State = BTreeMap::new();
    let boundary = live_on_entry(allocated);
    // A function is free to ignore an argument, so the live values are a
    // subset of the argument registers rather than all of them: what Section 6
    // fixes is where an argument arrives, not that the body reads it. Two
    // values arriving in one register would still be two arguments in one
    // place, which is refused here rather than left to the meet below.
    // Section 6 (revision 0.18): the destination of a record result above
    // sixteen bytes is the last parameter and arrives in `x8`, which is not an
    // argument register, so it is admitted beside the argument registers
    // rather than counted among them.
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
        if allocated.indirect_result && number == INDIRECT_RESULT {
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
        if if number >= crate::linscan::SIMD_BASE {
            number - crate::linscan::SIMD_BASE >= limit.min(8)
        } else {
            number >= limit.min(8)
        } {
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

    // The state on entry to a block is the meet of its predecessors' exits,
    // taken to a fixed point; a register the paths disagree about becomes
    // unknown and any read of it is refused.
    for _ in 0..(allocated.blocks.len() + 1) * 4 {
        let mut changed = false;
        for block in &allocated.blocks {
            let Some(state) = incoming.get(&block.id).cloned() else {
                continue;
            };
            let exit = transfer(&state, block, &register);
            for (target, _) in term_edges(&block.term) {
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
            // A block no path reaches carries no obligation.
            continue;
        };
        let mut state = state;
        for (index, inst) in block.insts.iter().enumerate() {
            if let MInst::AtomicRmw { dst, .. } = inst {
                if inst_uses(inst)
                    .iter()
                    .any(|input| register(*input) == register(*dst))
                {
                    return Err(refuse(
                        block.id,
                        "atomic RMW output overlaps an input of its exclusive loop".into(),
                    ));
                }
            }
            for reg in inst_uses(inst) {
                if reg.class == RegClass::Flags {
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
                        ))
                    }
                    Some(None) => {
                        return Err(refuse(
                            block.id,
                            format!(
                                "register {number} holds different values on the paths that reach the read of %v{}",
                                reg.id
                            ),
                        ))
                    }
                    None => {
                        return Err(refuse(
                            block.id,
                            format!("register {number} is read before it is written"),
                        ))
                    }
                }
            }

            // Section 2: a flags value is consumed by the instruction that
            // immediately follows its producer, and nothing between them
            // writes flags.
            if let Some(flags) = inst_defs(inst)
                .into_iter()
                .find(|reg| reg.class == RegClass::Flags)
            {
                let consumers: Vec<VReg> = match block.insts.get(index + 1) {
                    Some(next) => inst_uses(next),
                    None => term_uses(&block.term),
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

            if let MInst::Asm(body) = inst {
                verify_assembly(body, block.id, &register, &refuse)?;
            }
            apply(&mut state, inst, &register);
        }

        if let MTerm::AsmGoto { body, .. } = &block.term {
            verify_assembly(body, block.id, &register, &refuse)?;
        }

        for reg in term_uses(&block.term) {
            if reg.class == RegClass::Flags {
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
                    ))
                }
            }
        }
    }
    Ok(())
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

fn transfer(state: &State, block: &crate::MBlock, register: &impl Fn(VReg) -> Option<u8>) -> State {
    let mut state = state.clone();
    for inst in &block.insts {
        apply(&mut state, inst, register);
    }
    if let MTerm::AsmGoto { body, .. } = &block.term {
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

fn apply(state: &mut State, inst: &MInst, register: &impl Fn(VReg) -> Option<u8>) {
    // A call destroys every caller-saved register before its results arrive.
    if matches!(inst, MInst::Bl { .. } | MInst::Blr { .. }) {
        for number in CLOBBERED
            .into_iter()
            .chain(crate::linscan::SIMD_CALLER_SAVED)
            .chain([94, 95])
        {
            state.remove(&number);
        }
    }
    for reg in inst_defs(inst) {
        if reg.class == RegClass::Flags {
            continue;
        }
        if let Some(number) = register(reg) {
            state.insert(number, Some(reg.id));
        }
    }
}

/// Whether a terminator ends the function.
#[must_use]
pub const fn is_exit(term: &MTerm) -> bool {
    matches!(term, MTerm::Ret { .. } | MTerm::Brk { .. })
}

/// The virtual registers a function reads before writing: its arguments.
///
/// This is the verifier's own backward dataflow, computed from the allocated
/// blocks alone, so that nothing the allocator concluded is trusted.
fn live_on_entry(allocated: &Allocated) -> Vec<u32> {
    use std::collections::BTreeSet;

    let mut reads: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let mut writes: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for block in &allocated.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            for reg in inst_uses(inst) {
                if reg.class != RegClass::Flags && !written.contains(&reg.id) {
                    read.insert(reg.id);
                }
            }
            for reg in inst_defs(inst) {
                written.insert(reg.id);
            }
        }
        for reg in term_uses(&block.term) {
            if reg.class != RegClass::Flags && !written.contains(&reg.id) {
                read.insert(reg.id);
            }
        }
        if let MTerm::AsmGoto { body, .. } = &block.term {
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
            for (target, _) in term_edges(&block.term) {
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
