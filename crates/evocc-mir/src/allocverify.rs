//! Section 9, instrument two: the allocation verifier.
//!
//! "A separate pass over the allocated function checks the three properties of
//! Section 7 by dataflow; the allocator does not certify itself." Nothing here
//! reads an interval, a spill decision or any other record of what the
//! allocator concluded: the input is the allocated function, and the three
//! properties are re-derived from it.

use std::collections::BTreeMap;

use crate::{inst_defs, inst_uses, term_edges, term_uses, Allocated, MInst, MTerm, RegClass, VReg};

/// The Section 10 refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierRefusal {
    pub code: &'static str,
    pub name: &'static str,
    pub block: u32,
    pub detail: String,
}

/// What a physical register is known to hold on entry to a block.
///
/// `Some(value)` is one virtual register's contents; `None` is the meet of two
/// paths that disagree, which is what "no two simultaneously live values share
/// a register" forbids a use from reading.
type State = BTreeMap<u8, Option<u32>>;

/// The registers a call destroys: Section 6's caller-saved set.
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
    let limit = u8::try_from(allocated.parameters).unwrap_or(u8::MAX);
    for value in &boundary {
        let Some(number) = allocated.assignment.get(value).copied() else {
            return Err(refuse(
                0,
                format!("%v{value} is live when the function is entered and was never assigned"),
            ));
        };
        if number >= limit {
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

            apply(&mut state, inst, &register);
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
    state
}

fn apply(state: &mut State, inst: &MInst, register: &impl Fn(VReg) -> Option<u8>) {
    // A call destroys every caller-saved register before its results arrive.
    if matches!(inst, MInst::Bl { .. } | MInst::Blr { .. }) {
        for number in CLOBBERED {
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
