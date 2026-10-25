//! Relate the unchanged allocator input to registers and private stack slots.
//!
//! Copies and spill traffic can stutter on either side. Every other instruction
//! must have the same operation, attributes and ordered operands. An equation
//! `(place, value)` means that place holds the recorded low bits of a current
//! source value. A reload transports equations; it never invents a source value.
//! This is an ordinary Rust check, not an extension of the Lean proof profile.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::abi::ArgumentCursor;
use super::allocation::{
    AllocClass, AllocInst, AllocTerm, Allocated, AllocationTarget, ConstraintBody,
    ConstraintOperand, ConstraintValue, SpillKind,
};
use super::machine::MachineBlock;
use super::register::VirtualReg;

#[cfg(test)]
#[path = "../../tests/support/allocation_relation_state.rs"]
mod tests;

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

/// Keep the same equation set, with an index for source renames and deaths.
/// Large functions can have many live aliases of one value. A rename must
/// visit that identity's equations, not every other value at every copy.
#[derive(Clone, PartialEq, Eq)]
struct State<C> {
    equations: BTreeSet<(Place, Value<C>)>,
    identities: BTreeMap<Identity, BTreeSet<(Place, C)>>,
}

impl<C: Copy + Ord> State<C> {
    fn new() -> Self {
        Self {
            equations: BTreeSet::new(),
            identities: BTreeMap::new(),
        }
    }

    fn insert(&mut self, (place, value): (Place, Value<C>)) {
        if self.equations.insert((place, value)) {
            self.identities
                .entry(value.id)
                .or_default()
                .insert((place, value.class));
        }
    }

    fn iter(&self) -> impl Iterator<Item = &(Place, Value<C>)> {
        self.equations.iter()
    }

    fn for_identity(&self, id: Identity) -> impl Iterator<Item = (Place, C)> + '_ {
        self.identities
            .get(&id)
            .into_iter()
            .flat_map(|values| values.iter().copied())
    }

    fn project(&self, live: &BTreeSet<Identity>) -> Self {
        live.iter()
            .flat_map(|&id| {
                self.for_identity(id).map(move |(place, class)| {
                    #[cfg(test)]
                    tests::observe_projection_probe();
                    (place, Value { id, class })
                })
            })
            .collect()
    }

    fn forget_identity(&mut self, id: Identity) {
        if let Some(values) = self.identities.remove(&id) {
            for (place, class) in values {
                self.equations.remove(&(place, Value { id, class }));
            }
        }
    }

    fn retain(&mut self, mut keep: impl FnMut(&(Place, Value<C>)) -> bool) {
        self.equations.retain(|equation @ (place, value)| {
            if keep(equation) {
                true
            } else {
                let values = self.identities.get_mut(&value.id).unwrap();
                values.remove(&(*place, value.class));
                if values.is_empty() {
                    self.identities.remove(&value.id);
                }
                false
            }
        });
    }

    fn intersection<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = &'a (Place, Value<C>)> {
        self.equations.intersection(&other.equations)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.equations.len()
    }
}

impl<C: Copy + Ord> Extend<(Place, Value<C>)> for State<C> {
    fn extend<T: IntoIterator<Item = (Place, Value<C>)>>(&mut self, iter: T) {
        for equation in iter {
            self.insert(equation);
        }
    }
}

impl<C: Copy + Ord> FromIterator<(Place, Value<C>)> for State<C> {
    fn from_iter<T: IntoIterator<Item = (Place, Value<C>)>>(iter: T) -> Self {
        let mut state = Self::new();
        state.extend(iter);
        state
    }
}
type LiveIdentities = BTreeMap<u32, BTreeSet<Identity>>;
type Expirations = BTreeMap<u32, Vec<Vec<Identity>>>;
/// Section 9.2: spill places that hold a source value live across a
/// returns-twice call. A second return reads such a place again, and any point
/// that the call reaches can jump back. No other value may write the place at
/// such a point.
type Guards<C> = BTreeSet<(Place, Value<C>)>;
type Check = Result<(), String>;

/// What a walk does besides transporting equations.
enum Mode<'a, C> {
    /// A fixpoint round: no read is checked.
    Explore,
    /// A walk from a final state that collects the guards of each
    /// returns-twice call in the block. No read is checked.
    Collect(&'a mut Guards<C>),
    /// The final walk: every read is checked, and each spill store must keep
    /// every guard that reaches it. The walk collects the guards of each
    /// returns-twice call in the block.
    Check(&'a mut Guards<C>),
}

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
    state.forget_identity(Identity::Register(reg.id));
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
        .for_identity(src.id)
        .map(|(place, class)| {
            #[cfg(test)]
            tests::observe_identity_probe();
            (
                place,
                Value {
                    id: dst.id,
                    class: *[class, src.class, dst.class]
                        .iter()
                        .min_by_key(|c| c.value_bits())
                        .unwrap(),
                },
            )
        })
        .collect();
    // Source spill slots are distinct logical identities. Only allocated slots
    // use physical word offsets and can overlap another slot's second word.
    state.forget_identity(dst.id);
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
    /// Source identities live at the exit of each input block. Only a
    /// function with a returns-twice call needs them, so others leave them
    /// empty.
    live_out: BTreeMap<u32, BTreeSet<Identity>>,
    /// Spill words that the allocated program reads after the exit of each
    /// of its blocks before it writes them; empty like `live_out`.
    slots_out: BTreeMap<u32, BTreeSet<u32>>,
    /// Identities whose last source use or unused definition is at an
    /// instruction. Empty for returns-twice functions, like ordinary pruning.
    dead_after: Expirations,
}

impl<I, T, C, A> Relation<'_, I, T, C, A>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>> + PartialEq,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body> + PartialEq,
    A: AllocationTarget<C>,
{
    fn forget_expired(&self, state: &mut State<C>, block: u32, index: usize) {
        if let Some(dead) = self.dead_after.get(&block).map(|steps| &steps[index]) {
            for id in dead {
                state.forget_identity(*id);
            }
        }
    }

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

    /// Source identities live after original instruction `index` of `block`.
    fn live_after(&self, block: &MachineBlock<I, T, C>, index: usize) -> BTreeSet<Identity> {
        let mut live = self.live_out.get(&block.id).cloned().unwrap_or_default();
        for reg in block.term.defs() {
            live.remove(&Identity::Register(reg.id));
        }
        for reg in block.term.uses() {
            if !reg.class.is_flags() {
                live.insert(Identity::Register(reg.id));
            }
        }
        for inst in block.insts[index + 1..].iter().rev() {
            step_backward(&mut live, inst, false);
        }
        live
    }

    /// Spill words that the allocated program reads after instruction `index`
    /// of `block` before it writes them.
    fn slots_after(&self, block: &MachineBlock<I, T, C>, index: usize) -> BTreeSet<u32> {
        let mut live = self.slots_out.get(&block.id).cloned().unwrap_or_default();
        for inst in block.insts[index + 1..].iter().rev() {
            slot_step_backward(&mut live, inst);
        }
        live
    }

    /// Section 9.2: a second return of `call` keeps no register. Every source
    /// value live across the call must be in a stack slot. A slot that holds
    /// such a value and that the allocated program reads after the call is
    /// guarded: no other value may write it at a point that the call reaches,
    /// because a jump back can occur there. A slot that only a copy of the
    /// value occupies, and that nothing reads after the call, is not the
    /// value's slot.
    fn second_return(
        &self,
        state: &mut State<C>,
        block: &MachineBlock<I, T, C>,
        index: usize,
        actual: &MachineBlock<I, T, C>,
        position: usize,
        check: bool,
    ) -> Result<Guards<C>, String> {
        state.retain(|(place, _)| matches!(place, Place::Spill(_) | Place::Incoming(_)));
        let defined: BTreeSet<_> = block.insts[index]
            .defs()
            .iter()
            .map(|reg| Identity::Register(reg.id))
            .collect();
        let live = self.live_after(block, index);
        for identity in live.difference(&defined) {
            if check && !state.iter().any(|(_, value)| value.id == *identity) {
                return Err(match identity {
                    Identity::Register(id) => format!(
                        "%v{id} is live across a returns-twice call, and no stack slot holds it"
                    ),
                    Identity::Slot(slot) => format!(
                        "logical slot {slot} is live across a returns-twice call, and no stack slot holds it"
                    ),
                });
            }
        }
        let read = self.slots_after(actual, position);
        Ok(state
            .iter()
            .copied()
            .filter(|(place, value)| {
                let Place::Spill(slot) = place else {
                    return false;
                };
                live.contains(&value.id)
                    && !defined.contains(&value.id)
                    && (*slot..slot.saturating_add(value.class.spill_words()))
                        .any(|word| read.contains(&word))
            })
            .collect())
    }

    /// Section 9.2: at a point that a returns-twice call reaches, a spill store
    /// keeps each guard. A store into a guarded slot must write the full
    /// guarded value into that exact slot. `pending` holds the source
    /// instructions from the walk's source position: the walk renames their
    /// leading copies after this store, and the store can implement such a
    /// copy, for example when it spills the copy's result directly.
    fn guard_store(&self, state: &State<C>, guards: &Guards<C>, pending: &[I], inst: &I) -> Check {
        let Some(access) = inst
            .spill()
            .filter(|access| access.kind == SpillKind::Store)
        else {
            return Ok(());
        };
        let written = Location {
            place: Place::Spill(access.slot),
            class: access.reg.class,
        };
        let hits: Vec<_> = guards
            .iter()
            .filter(|(place, value)| {
                overlaps(
                    written,
                    Location {
                        place: *place,
                        class: value.class,
                    },
                )
            })
            .collect();
        if hits.is_empty() {
            return Ok(());
        }
        let mut renamed = state.clone();
        for source in pending {
            if !rename_source(&mut renamed, source) {
                break;
            }
        }
        let register = self.location(access.reg)?.place;
        for (place, value) in hits {
            let keeps = *place == written.place
                && access.reg.class.value_bits() >= value.class.value_bits()
                && renamed.iter().any(|(held_place, held)| {
                    *held_place == register
                        && held.id == value.id
                        && held.class.value_bits() >= value.class.value_bits()
                });
            if !keeps {
                let Place::Spill(slot) = place else {
                    unreachable!("only spill places are guarded");
                };
                let name = match value.id {
                    Identity::Register(id) => format!("%v{id}"),
                    Identity::Slot(logical) => format!("logical slot {logical}"),
                };
                return Err(format!(
                    "slot {slot} holds {name}, which is live across a returns-twice call, and another value overwrites it"
                ));
            }
        }
        Ok(())
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
        mode: &mut Mode<'_, C>,
    ) -> Check {
        let check = matches!(mode, Mode::Check(_));
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
                if !paired_argument {
                    if let Mode::Check(guards) = mode {
                        self.guard_store(state, guards, &original.insts[i..], inst)?;
                    }
                    if self.transport(state, inst)? {
                        j += 1;
                        continue;
                    }
                }
            }
            if original
                .insts
                .get(i)
                .is_some_and(|source| rename_source(state, source))
            {
                self.forget_expired(state, original.id, i);
                #[cfg(test)]
                tests::observe_step(state.len());
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
                        if before.returns_twice() {
                            let added = self.second_return(state, original, i, actual, j, check)?;
                            if let Mode::Collect(guards) | Mode::Check(guards) = mode {
                                guards.extend(added);
                            }
                        }
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
            self.forget_expired(state, original.id, i);
            #[cfg(test)]
            tests::observe_step(state.len());
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
    // Validate block identities and edges before running either dataflow.
    // Duplicate block IDs do not define a monotone liveness equation system.
    let returns_twice = input
        .blocks
        .iter()
        .any(|block| block.insts.iter().any(|inst| inst.returns_twice()));
    let (live_in, live_out) = input_liveness(input, !returns_twice);
    let dead_after = if returns_twice {
        BTreeMap::new()
    } else {
        input_expirations(input, &live_out)
    };
    let relation = Relation {
        target,
        allocated,
        live_out: if returns_twice {
            live_out
        } else {
            drop(live_out);
            BTreeMap::new()
        },
        slots_out: if returns_twice {
            slot_live_out(&allocated.blocks)
        } else {
            BTreeMap::new()
        },
        dead_after,
    };
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
    // Ordinary reads only need identities live at the source block's entry.
    // Derive that demand from the unchanged input, not allocator intervals.
    // Keep the full relation for returns-twice functions: a propagated spill
    // guard can need an identity after its last ordinary source use.
    if !returns_twice {
        entry.retain(|(_, value)| live_in[&first.id].contains(&value.id));
    }
    let mut incoming = BTreeMap::from([(first.id, entry)]);
    let mut pending = VecDeque::from([first.id]);
    let mut queued = BTreeSet::from([first.id]);
    // States only lose equations after first discovery. The finite equation
    // domain guarantees convergence; no iteration cap may turn a partial
    // solution into an accepted allocation. A changed input schedules all
    // successors, but an unchanged input cannot affect a block's transfer.
    while let Some(id) = pending.pop_front() {
        queued.remove(&id);
        let before = source[&id];
        let mut state = incoming[&id].clone();
        #[cfg(test)]
        tests::observe_incoming(state.len());
        relation.walk(before, output[&id], &mut state, &mut Mode::Explore)?;
        for (index, (successor, _)) in before.term.edges().iter().enumerate() {
            let mut exit = if returns_twice {
                state.clone()
            } else {
                state.project(&live_in[successor])
            };
            for bridge in &paths[&(id, index)] {
                for inst in &output[bridge].insts {
                    if !relation.transport(&mut exit, inst)? {
                        return Err("operation in allocation bridge".into());
                    }
                }
            }
            // Bridge transports preserve source identities. Projection
            // therefore commutes with every bridge copy and physical kill;
            // all bridge shapes and slot bounds are still checked above.
            let merged = incoming.get(successor).map_or_else(
                || exit.clone(),
                |old| old.intersection(&exit).copied().collect(),
            );
            if incoming.get(successor) != Some(&merged) {
                incoming.insert(*successor, merged);
                if queued.insert(*successor) {
                    pending.push_back(*successor);
                }
            }
        }
    }
    // Section 9.2: a guard holds at every point that its returns-twice call
    // reaches, so guards join by union along each edge. They come from the
    // final states only, so no guard of a partial solution remains.
    let mut guarded = BTreeMap::<u32, Guards<C>>::new();
    if returns_twice {
        let mut added = BTreeMap::new();
        for before in &input.blocks {
            if let Some(mut state) = incoming.get(&before.id).cloned() {
                let mut guards = Guards::new();
                relation.walk(
                    before,
                    output[&before.id],
                    &mut state,
                    &mut Mode::Collect(&mut guards),
                )?;
                added.insert(before.id, guards);
            }
        }
        loop {
            let mut changed = false;
            for before in &input.blocks {
                let Some(own) = added.get(&before.id) else {
                    continue;
                };
                let mut exit = guarded.get(&before.id).cloned().unwrap_or_default();
                exit.extend(own.iter().copied());
                if exit.is_empty() {
                    continue;
                }
                for (successor, _) in before.term.edges().iter() {
                    let joined = guarded.entry(*successor).or_default();
                    let count = joined.len();
                    joined.extend(exit.iter().copied());
                    changed |= joined.len() != count;
                }
            }
            if !changed {
                break;
            }
        }
    }
    for before in &input.blocks {
        if let Some(mut state) = incoming.get(&before.id).cloned() {
            let mut guards = guarded.get(&before.id).cloned().unwrap_or_default();
            relation
                .walk(
                    before,
                    output[&before.id],
                    &mut state,
                    &mut Mode::Check(&mut guards),
                )
                .map_err(|reason| format!("block {}: {reason}", before.id))?;
            if guards.is_empty() {
                continue;
            }
            // Bridges hold only transports, so only their spill stores can
            // break a guard.
            for index in 0..before.term.edges().len() {
                let mut exit = state.clone();
                for bridge in &paths[&(before.id, index)] {
                    for inst in &output[bridge].insts {
                        relation
                            .guard_store(&exit, &guards, &[], inst)
                            .and_then(|()| relation.transport(&mut exit, inst).map(drop))
                            .map_err(|reason| format!("block {bridge}: {reason}"))?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Apply a source copy or a source spill as a renaming of source values.
/// Return false for any other source instruction.
fn rename_source<I, C>(state: &mut State<C>, inst: &I) -> bool
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    if let Some((dst, src)) = inst.copy_operands() {
        if dst.class == src.class {
            rename(state, dst.into(), src.into());
            return true;
        }
    }
    // Public machine-level allocator tests also supply explicit spill
    // traffic. Relate their logical slots, independently of compaction,
    // with a separate identity namespace from virtual registers.
    let Some(access) = inst.spill() else {
        return false;
    };
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
    true
}

/// One backward step of source liveness over an input instruction. Logical
/// spill slots of a machine-level input are source identities too.
fn step_backward<I, C>(live: &mut BTreeSet<Identity>, inst: &I, checked_reads: bool)
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    // A same-class source copy is a renaming, not a checked operation in
    // `walk`. It propagates demand only when its result is needed. Otherwise
    // dead phi-copy cycles would keep their own operands artificially live.
    // Non-copy operations still demand every input even if their result is
    // dead. Stack-guard liveness retains the original conservative rule.
    if checked_reads {
        if let Some((dst, src)) = inst.copy_operands().filter(|(d, s)| d.class == s.class) {
            if live.remove(&Identity::Register(dst.id)) {
                live.insert(Identity::Register(src.id));
            }
            return;
        }
    }
    if let Some(access) = inst.spill() {
        match access.kind {
            SpillKind::Store => {
                live.remove(&Identity::Slot(access.slot));
            }
            SpillKind::Reload => {
                live.insert(Identity::Slot(access.slot));
            }
        }
    }
    for reg in inst.defs() {
        live.remove(&Identity::Register(reg.id));
    }
    for reg in inst.uses() {
        if checked_reads || !reg.class.is_flags() {
            live.insert(Identity::Register(reg.id));
        }
    }
}

/// One backward step of spill-word liveness over an allocated instruction.
fn slot_step_backward<I, C>(live: &mut BTreeSet<u32>, inst: &I)
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
{
    if let Some(access) = inst.spill() {
        let words = access.slot..access.slot.saturating_add(access.reg.class.spill_words());
        match access.kind {
            SpillKind::Store => {
                for word in words {
                    live.remove(&word);
                }
            }
            SpillKind::Reload => live.extend(words),
        }
    }
}

/// Spill words that each allocated block reads after its exit before it
/// writes them, through every allocated edge, transport blocks included.
fn slot_live_out<I, T, C>(blocks: &[MachineBlock<I, T, C>]) -> BTreeMap<u32, BTreeSet<u32>>
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut live_in = BTreeMap::<u32, BTreeSet<u32>>::new();
    let mut live_out = BTreeMap::<u32, BTreeSet<u32>>::new();
    loop {
        let mut changed = false;
        for block in blocks.iter().rev() {
            let out: BTreeSet<u32> = block
                .term
                .edges()
                .iter()
                .filter_map(|(target, _)| live_in.get(target))
                .flatten()
                .copied()
                .collect();
            let mut live = out.clone();
            for inst in block.insts.iter().rev() {
                slot_step_backward(&mut live, inst);
            }
            if live_in.get(&block.id) != Some(&live) {
                live_in.insert(block.id, live);
                changed = true;
            }
            live_out.insert(block.id, out);
        }
        if !changed {
            return live_out;
        }
    }
}

/// Source liveness of the unchanged input, independent of the allocation.
/// Ordinary demand includes flags and starts at checked reads; source copies
/// propagate it. Stack guards keep all transport reads and exclude flags.
fn input_liveness<I, T, C>(
    input: &super::machine::MachineFunction<I, T, C>,
    checked_reads: bool,
) -> (LiveIdentities, LiveIdentities)
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut live_in = BTreeMap::<u32, BTreeSet<Identity>>::new();
    let mut live_out = BTreeMap::<u32, BTreeSet<Identity>>::new();
    loop {
        let mut changed = false;
        for block in input.blocks.iter().rev() {
            let out: BTreeSet<_> = block
                .term
                .edges()
                .iter()
                .filter_map(|(target, _)| live_in.get(target))
                .flatten()
                .copied()
                .collect();
            let mut live = out.clone();
            for reg in block.term.defs() {
                live.remove(&Identity::Register(reg.id));
            }
            for reg in block.term.uses() {
                if checked_reads || !reg.class.is_flags() {
                    live.insert(Identity::Register(reg.id));
                }
            }
            for inst in block.insts.iter().rev() {
                step_backward(&mut live, inst, checked_reads);
            }
            if live_in.get(&block.id) != Some(&live) {
                live_in.insert(block.id, live);
                changed = true;
            }
            live_out.insert(block.id, out);
        }
        if !changed {
            return (live_in, live_out);
        }
    }
}

/// Compute last-use removals in one backward pass per source block. Store
/// only the expiring operands, not a full live set at every instruction.
/// Remove them after checking/renaming the instruction, never before its reads.
fn input_expirations<I, T, C>(
    input: &super::machine::MachineFunction<I, T, C>,
    live_out: &LiveIdentities,
) -> Expirations
where
    C: AllocClass,
    I: AllocInst<Reg = VirtualReg<C>>,
    T: AllocTerm<Reg = VirtualReg<C>, Body = I::Body>,
{
    let mut result = BTreeMap::new();
    for block in &input.blocks {
        let mut live = live_out[&block.id].clone();
        for reg in block.term.defs() {
            live.remove(&Identity::Register(reg.id));
        }
        live.extend(
            block
                .term
                .uses()
                .iter()
                .map(|reg| Identity::Register(reg.id)),
        );
        let mut steps = vec![Vec::new(); block.insts.len()];
        for (index, inst) in block.insts.iter().enumerate().rev() {
            let mut dead: Vec<_> = inst
                .defs()
                .into_iter()
                .chain(inst.uses())
                .map(|reg| Identity::Register(reg.id))
                .collect();
            if let Some(access) = inst.spill() {
                dead.push(Identity::Slot(access.slot));
            }
            dead.sort_unstable();
            dead.dedup();
            dead.retain(|id| !live.contains(id));
            steps[index] = dead;
            step_backward(&mut live, inst, true);
        }
        result.insert(block.id, steps);
    }
    result
}
