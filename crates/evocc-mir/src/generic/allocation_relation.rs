//! Relate the unchanged allocator input to registers and private stack slots.
//!
//! Copies and spill traffic can stutter on either side. Every other instruction
//! must have the same operation, attributes and ordered operands. An equation
//! `(place, value)` means that place holds the recorded low bits of a current
//! source value. A reload transports equations; it never invents a source value.
//! This is an ordinary Rust check, not an extension of the Lean proof profile.

use std::collections::{BTreeMap, BTreeSet};

use super::abi::ArgumentCursor;
use super::allocation::{
    AllocClass, AllocInst, AllocTerm, Allocated, AllocationTarget, ConstraintBody,
    ConstraintOperand, ConstraintValue, SpillKind,
};
use super::machine::MachineBlock;
use super::register::VirtualReg;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Register(u8),
    Flags,
    Spill(u32),
    Incoming(u32),
    Outgoing(u32),
}

#[derive(Clone, Copy)]
struct Location<C> {
    place: Place,
    class: C,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Identity {
    Register(u32),
    Slot(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Value<C> {
    // The class bounds the known low bits, not the original value's full width.
    // A narrow transport cannot establish equality of a later wider read.
    id: Identity,
    class: C,
}

impl<C> From<VirtualReg<C>> for Value<C> {
    fn from(reg: VirtualReg<C>) -> Self {
        Self {
            id: Identity::Register(reg.id),
            class: reg.class,
        }
    }
}

type State<C> = BTreeSet<(Place, Value<C>)>;
type Check = Result<(), String>;

fn overlaps<C: AllocClass>(left: Location<C>, right: Location<C>) -> bool {
    let offsets = match (left.place, right.place) {
        (Place::Spill(a), Place::Spill(b))
        | (Place::Incoming(a), Place::Incoming(b))
        | (Place::Outgoing(a), Place::Outgoing(b)) => Some((a, b)),
        _ => None,
    };
    if let Some((a, b)) = offsets {
        u64::from(a) < u64::from(b) + u64::from(right.class.spill_words())
            && u64::from(b) < u64::from(a) + u64::from(left.class.spill_words())
    } else {
        left.place == right.place
    }
}

fn forget<C: AllocClass>(state: &mut State<C>, reg: VirtualReg<C>) {
    state.retain(|(_, value)| value.id != Identity::Register(reg.id));
}

fn kill<C: AllocClass>(state: &mut State<C>, loc: Location<C>) {
    state.retain(|(place, value)| {
        !overlaps(
            loc,
            Location {
                place: *place,
                class: value.class,
            },
        )
    });
}

fn copy<C: AllocClass>(state: &mut State<C>, dst: Location<C>, src: Location<C>) {
    let values: Vec<_> = state
        .iter()
        .filter(|(place, _)| *place == src.place)
        .map(|(_, value)| {
            (
                dst.place,
                Value {
                    id: value.id,
                    class: *[value.class, src.class, dst.class]
                        .iter()
                        .min_by_key(|c| c.value_bits())
                        .unwrap(),
                },
            )
        })
        .collect();
    kill(state, dst);
    state.extend(values);
}

fn rename<C: AllocClass>(state: &mut State<C>, dst: Value<C>, src: Value<C>) {
    // Capture before forgetting: a self-copy must preserve its equations.
    let values: Vec<_> = state
        .iter()
        .filter(|(_, value)| value.id == src.id)
        .map(|(place, value)| {
            (
                *place,
                Value {
                    id: dst.id,
                    class: *[value.class, src.class, dst.class]
                        .iter()
                        .min_by_key(|c| c.value_bits())
                        .unwrap(),
                },
            )
        })
        .collect();
    // Source spill slots are distinct logical identities. Only allocated slots
    // use physical word offsets and can overlap another slot's second word.
    state.retain(|(_, value)| value.id != dst.id);
    state.extend(values);
}

fn normalize_body<B: ConstraintBody<Reg = VirtualReg<C>>, C: Copy>(body: &mut B) {
    // A tied input and its output name one physical operand. Allocation can
    // repeat either side's fixed binding on both sides without changing the
    // constraint. Compare that implied binding, not its redundant spelling.
    // Conflicting explicit bindings stay unchanged and cannot be normalized
    // into a valid constraint.
    for index in 0..body.outputs().len() {
        let bindings: BTreeSet<_> = body.outputs()[index]
            .fixed_register()
            .into_iter()
            .chain(
                body.inputs()
                    .iter()
                    .filter(|operand| operand.tied_output().map(usize::from) == Some(index))
                    .filter_map(ConstraintOperand::fixed_register),
            )
            .collect();
        if bindings.len() == 1 {
            let binding = bindings.first().copied();
            body.outputs_mut()[index].set_fixed_register(binding);
            for operand in body.inputs_mut() {
                if operand.tied_output().map(usize::from) == Some(index) {
                    operand.set_fixed_register(binding);
                }
            }
        }
    }
    for operand in body.outputs_mut().iter_mut() {
        if let ConstraintValue::Reg(mut reg) = operand.value() {
            reg.id = 0;
            operand.set_register(reg);
        }
    }
    for operand in body.inputs_mut().iter_mut() {
        if let ConstraintValue::Reg(mut reg) = operand.value() {
            reg.id = 0;
            operand.set_register(reg);
        }
    }
}

fn shape<I: AllocInst<Reg = VirtualReg<C>>, C: Copy>(inst: &I) -> I {
    let mut result = inst.clone();
    for slot in result.operand_slots(false) {
        slot.id = 0;
    }
    for slot in result.operand_slots(true) {
        slot.id = 0;
    }
    if let Some(body) = result.assembly_mut() {
        normalize_body(body);
    }
    // Stack arguments are represented by stores after allocation, not by call
    // operands. Boundary checks below compare every original argument.
    if let Some((args, results)) = result.call_operands_mut() {
        args.clear();
        results.clear();
    }
    result
}

struct Relation<'a, I, T, C, A> {
    target: &'a A,
    allocated: &'a Allocated<I, T, C>,
}

impl<I, T, C, A> Relation<'_, I, T, C, A>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body> + PartialEq,
    A: AllocationTarget<C>,
{
    fn location(&self, reg: VirtualReg<C>) -> Result<Location<C>, String> {
        let place = if reg.class.is_flags() {
            Place::Flags
        } else {
            Place::Register(
                *self
                    .allocated
                    .assignment
                    .get(&reg.id)
                    .ok_or_else(|| format!("unassigned operand %v{}", reg.id))?,
            )
        };
        Ok(Location {
            place,
            class: reg.class,
        })
    }

    fn require(
        &self,
        state: &State<C>,
        loc: Location<C>,
        value: VirtualReg<C>,
        check: bool,
    ) -> Check {
        if check
            && (loc.class.value_bits() < value.class.value_bits()
                || !state.iter().any(|(place, held)| {
                    *place == loc.place
                        && held.id == Identity::Register(value.id)
                        && held.class.value_bits() >= value.class.value_bits()
                }))
        {
            Err(format!(
                "allocation does not preserve source value %v{}",
                value.id
            ))
        } else {
            Ok(())
        }
    }

    fn uses(
        &self,
        state: &State<C>,
        original: &[VirtualReg<C>],
        actual: &[VirtualReg<C>],
        check: bool,
    ) -> Check {
        if original.len() != actual.len() {
            return Err("allocation changed operand count".into());
        }
        for (value, reg) in original.iter().zip(actual) {
            if value.class != reg.class {
                return Err("allocation changed operand class".into());
            }
            self.require(state, self.location(*reg)?, *value, check)?;
        }
        Ok(())
    }

    fn define(
        &self,
        state: &mut State<C>,
        original: &[VirtualReg<C>],
        actual: &[VirtualReg<C>],
    ) -> Check {
        if original.len() != actual.len() {
            return Err("allocation changed definition count".into());
        }
        // Separate results cannot share a physical destination. Otherwise the
        // equations below would falsely assert that one location contains both
        // results, even when their values differ. Place also unifies narrow and
        // wide register views, so a view change cannot hide this overlap.
        let mut destinations = BTreeSet::new();
        // Simultaneous definitions invalidate all old equalities first.
        for (value, reg) in original.iter().zip(actual) {
            if value.class != reg.class {
                return Err("allocation changed definition class".into());
            }
            let location = self.location(*reg)?;
            if !destinations.insert(location.place) {
                return Err("allocation overlaps simultaneous definitions".into());
            }
            forget(state, *value);
            kill(state, location);
        }
        for (value, reg) in original.iter().zip(actual) {
            state.insert((self.location(*reg)?.place, (*value).into()));
        }
        Ok(())
    }

    fn transport(&self, state: &mut State<C>, inst: &I) -> Result<bool, String> {
        if let Some((dst, src)) = inst.copy_operands() {
            if dst.class == src.class {
                copy(state, self.location(dst)?, self.location(src)?);
                return Ok(true);
            }
        }
        let spill = inst.spill();
        if let Some(access) = spill.or_else(|| inst.stack_argument()) {
            let end = u64::from(access.slot) + u64::from(access.reg.class.spill_words());
            let place = if spill.is_some() {
                if end > u64::from(self.allocated.frame_slots) {
                    return Err("spill exceeds frame slots".into());
                }
                Place::Spill(access.slot)
            } else if access.kind == SpillKind::Reload {
                Place::Incoming(access.slot)
            } else {
                if end * self.target.abi().stack_slot_bytes > self.allocated.outgoing {
                    return Err("argument store exceeds outgoing area".into());
                }
                Place::Outgoing(access.slot)
            };
            let memory = Location {
                place,
                class: access.reg.class,
            };
            let register = self.location(access.reg)?;
            let (dst, src) = if access.kind == SpillKind::Store {
                (memory, register)
            } else {
                (register, memory)
            };
            copy(state, dst, src);
            return Ok(true);
        }
        Ok(false)
    }

    fn boundary(
        &self,
        state: &State<C>,
        original: &[VirtualReg<C>],
        actual: &[VirtualReg<C>],
        results: bool,
        indirect: bool,
        check: bool,
    ) -> Check {
        let abi = self.target.abi();
        let mut cursor = ArgumentCursor::new(if results {
            abi.result_banks
        } else {
            abi.argument_banks
        });
        let mut actual = actual.iter();
        for (index, value) in original.iter().enumerate() {
            let location = if indirect && index + 1 == original.len() {
                Ok(abi.indirect_result)
            } else {
                cursor.place(usize::from(value.class.bank()), value.class.spill_words())
            };
            let place = match location {
                Ok(number) => {
                    let reg = actual.next().ok_or("missing boundary register")?;
                    if self.location(*reg)?.place != Place::Register(number)
                        || reg.class != value.class
                    {
                        return Err("allocation changed ABI operand location".into());
                    }
                    Place::Register(number)
                }
                Err(slot) if !results => Place::Outgoing(slot),
                Err(_) => return Err("result exceeds ABI registers".into()),
            };
            if !results {
                self.require(
                    state,
                    Location {
                        place,
                        class: value.class,
                    },
                    *value,
                    check,
                )?;
            }
        }
        if actual.next().is_some() {
            return Err("extra boundary register".into());
        }
        Ok(())
    }

    fn walk(
        &self,
        original: &MachineBlock<I, T, C>,
        actual: &MachineBlock<I, T, C>,
        state: &mut State<C>,
        check: bool,
    ) -> Check {
        let (mut i, mut j) = (0, 0);
        let mut indirect = false;
        loop {
            if let Some(inst) = actual.insts.get(j) {
                let source_transport = original.insts.get(i).is_some_and(|source| {
                    source.spill().is_some()
                        || source
                            .copy_operands()
                            .is_some_and(|(d, s)| d.class == s.class)
                });
                let paired_argument = inst.stack_argument().is_some()
                    && (source_transport
                        || original
                            .insts
                            .get(i)
                            .and_then(AllocInst::stack_argument)
                            .is_some());
                if !paired_argument && self.transport(state, inst)? {
                    j += 1;
                    continue;
                }
            }
            if let Some((dst, src)) = original.insts.get(i).and_then(AllocInst::copy_operands) {
                if dst.class == src.class {
                    rename(state, dst.into(), src.into());
                    i += 1;
                    continue;
                }
            }
            // Public machine-level allocator tests also supply explicit spill
            // traffic. Relate their logical slots, independently of compaction,
            // with a separate identity namespace from virtual registers.
            if let Some(access) = original.insts.get(i).and_then(AllocInst::spill) {
                let slot = Value {
                    id: Identity::Slot(access.slot),
                    class: access.reg.class,
                };
                let reg = access.reg.into();
                let (dst, src) = if access.kind == SpillKind::Store {
                    (slot, reg)
                } else {
                    (reg, slot)
                };
                rename(state, dst, src);
                i += 1;
                continue;
            }
            match (original.insts.get(i), actual.insts.get(j)) {
                (None, None) => break,
                (Some(before), Some(after)) => {
                    if shape(before) != shape(after) {
                        return Err(format!("instruction {i} changed during allocation"));
                    }
                    if let Some((args, results)) = before.call_operands() {
                        let (moved, outputs) = after.call_operands().ok_or("call changed")?;
                        self.boundary(state, args, moved, false, indirect, check)?;
                        self.boundary(state, results, outputs, true, false, check)?;
                        // Indirect call targets remain in uses after the argument list.
                        let mut a = before.clone();
                        let mut b = after.clone();
                        a.call_operands_mut().unwrap().0.clear();
                        b.call_operands_mut().unwrap().0.clear();
                        self.uses(state, &a.uses(), &b.uses(), check)?;
                        let abi = self.target.abi();
                        state.retain(|(place, value)| match place {
                            Place::Register(n) => {
                                !abi.call_clobbers.contains(n)
                                    && (!abi.partially_preserved.contains(n)
                                        || u64::from(value.class.value_bits())
                                            <= 8 * abi.partially_preserved_bytes)
                            }
                            Place::Flags | Place::Outgoing(_) => false,
                            _ => true,
                        });
                        self.define(state, results, outputs)?;
                        indirect = false;
                    } else {
                        self.uses(state, &before.uses(), &after.uses(), check)?;
                        if after.stack_argument().is_some() {
                            self.transport(state, after)?;
                        }
                        if let Some(body) = after.assembly() {
                            let clobbers = body.clobbered_registers();
                            state.retain(|(place, _)| !matches!(place, Place::Register(n) if clobbers.contains(n)));
                        }
                        if after.writes_flags() {
                            state.retain(|(place, _)| *place != Place::Flags);
                        }
                        self.define(state, &before.defs(), &after.defs())?;
                        if before.is_indirect_result() {
                            // The first entry marker describes the function's result,
                            // not the first call in its body.
                            indirect =
                                !(original.id == self.allocated.input.blocks[0].id && i == 0);
                        }
                    }
                }
                _ => return Err("allocated instructions do not match their input".into()),
            }
            i += 1;
            j += 1;
        }
        if let Some(values) = original.term.return_values() {
            let actual_values = actual.term.return_values().ok_or("return changed")?;
            self.boundary(state, values, actual_values, true, false, check)?;
        }
        self.uses(state, &original.term.uses(), &actual.term.uses(), check)?;
        if let Some(body) = actual.term.assembly() {
            let clobbers = body.clobbered_registers();
            state.retain(|(place, _)| !matches!(place, Place::Register(n) if clobbers.contains(n)));
            // An asm-goto can clobber condition codes without defining a flag
            // operand. Never retain a prior flags equation across this boundary.
            state.retain(|(place, _)| *place != Place::Flags);
        }
        self.define(state, &original.term.defs(), &actual.term.defs())?;
        Ok(())
    }
}

/// Check the source relation independently of intervals, pins and spill choices.
pub(super) fn verify<I, T, C, A>(target: &A, allocated: &Allocated<I, T, C>) -> Check
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body> + PartialEq,
    A: AllocationTarget<C>,
{
    let relation = Relation { target, allocated };
    let input = &allocated.input;
    let source: BTreeMap<_, _> = input.blocks.iter().map(|b| (b.id, b)).collect();
    let output: BTreeMap<_, _> = allocated.blocks.iter().map(|b| (b.id, b)).collect();
    if source.len() != input.blocks.len()
        || output.len() != allocated.blocks.len()
        || input.name != allocated.name
        || input.blocks.first().map(|b| b.id) != allocated.blocks.first().map(|b| b.id)
    {
        return Err("allocation changed function identity or duplicated blocks".into());
    }
    // Match each original edge through zero or more transport-only bridges.
    // Bridges must be acyclic and must end at that exact original successor.
    let mut paths = BTreeMap::new();
    let mut covered: BTreeSet<_> = source.keys().copied().collect();
    for before in &input.blocks {
        let after = output.get(&before.id).ok_or("allocated block missing")?;
        let expected = before.term.edges();
        let edges = after.term.edges();
        if expected.len() != edges.len() || !after.args.is_empty() {
            return Err("allocation changed CFG operands".into());
        }
        let mut term = after.term.clone();
        for (index, ((wanted, args), (first, actual_args))) in
            expected.iter().zip(&edges).enumerate()
        {
            if !args.is_empty() || !actual_args.is_empty() {
                return Err("allocation input is not post-SSA".into());
            }
            let mut next = *first;
            let mut path = Vec::new();
            let mut visited = BTreeSet::new();
            while !source.contains_key(&next) {
                if !visited.insert(next) {
                    return Err("cyclic allocation bridge".into());
                }
                let block = output.get(&next).ok_or("allocation edge target missing")?;
                let exits = block.term.edges();
                if !block.args.is_empty()
                    || exits.len() != 1
                    || !exits[0].1.is_empty()
                    || block.term != T::jump(exits[0].0, vec![])
                {
                    return Err("non-transport allocation bridge".into());
                }
                path.push(next);
                covered.insert(next);
                next = exits[0].0;
            }
            if next != *wanted {
                return Err("allocation changed branch destination".into());
            }
            if *first != *wanted {
                if term.indirect_target().is_some() {
                    return Err("allocation changed computed label identity".into());
                }
                term.retarget(index, *wanted);
            }
            paths.insert((before.id, index), path);
        }
        let mut original = before.term.clone();
        for slot in original.operand_slots() {
            slot.id = 0;
        }
        for slot in term.operand_slots() {
            slot.id = 0;
        }
        if let Some(body) = original.assembly_mut() {
            normalize_body(body);
        }
        if let Some(body) = term.assembly_mut() {
            normalize_body(body);
        }
        if original != term {
            return Err("allocation changed terminator".into());
        }
    }
    if covered.len() != output.len() {
        return Err("unmatched allocation block".into());
    }
    let Some(first) = input.blocks.first() else {
        return Ok(());
    };
    let indirect = first
        .insts
        .first()
        .is_some_and(AllocInst::is_indirect_result);
    if allocated.parameters != first.args.len() || allocated.indirect_result != indirect {
        return Err("allocation changed function signature".into());
    }
    let mut entry = State::new();
    let mut cursor = ArgumentCursor::new(target.abi().argument_banks);
    for (index, value) in first.args.iter().enumerate() {
        let place = if indirect && index + 1 == first.args.len() {
            Place::Register(target.abi().indirect_result)
        } else {
            match cursor.place(usize::from(value.class.bank()), value.class.spill_words()) {
                Ok(n) => Place::Register(n),
                Err(slot) => Place::Incoming(slot),
            }
        };
        entry.insert((place, (*value).into()));
    }
    let mut incoming = BTreeMap::from([(first.id, entry)]);
    // States only lose equations after first discovery. The finite equation
    // domain guarantees convergence; no iteration cap may turn a partial
    // solution into an accepted allocation.
    loop {
        let mut changed = false;
        for before in &input.blocks {
            let Some(mut state) = incoming.get(&before.id).cloned() else {
                continue;
            };
            relation.walk(before, output[&before.id], &mut state, false)?;
            for (index, (successor, _)) in before.term.edges().iter().enumerate() {
                let mut exit = state.clone();
                for bridge in &paths[&(before.id, index)] {
                    for inst in &output[bridge].insts {
                        if !relation.transport(&mut exit, inst)? {
                            return Err("operation in allocation bridge".into());
                        }
                    }
                }
                let merged = incoming.get(successor).map_or_else(
                    || exit.clone(),
                    |old| old.intersection(&exit).copied().collect(),
                );
                if incoming.get(successor) != Some(&merged) {
                    incoming.insert(*successor, merged);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    for before in &input.blocks {
        if let Some(mut state) = incoming.get(&before.id).cloned() {
            relation
                .walk(before, output[&before.id], &mut state, true)
                .map_err(|reason| format!("block {}: {reason}", before.id))?;
        }
    }
    Ok(())
}
