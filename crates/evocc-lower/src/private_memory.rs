//! Promote private, fixed integer cells to block-local SSA values.
//!
//! Allocation, lifetime and address checks stay in EvoIR. Only ordinary loads
//! and stores whose values are known to be initialized can disappear. A
//! complete initialized integer-cell copy can become SSA or an external store.
//! The address must not escape, every access must name one complete nonoverlapping
//! cell, and all incoming paths must initialize a cell before a load. This is
//! a bounded lowering optimization, not a certified EqSat rewrite.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use evocc_evoir::{
    inst_results, inst_uses, remap_inst_uses, AccessLayout, BlockArg, CopyVolatility, Function,
    IndexSignedness, Inst, OffsetDirection, Value,
};
use evocc_types::EvoIRType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Address {
    root: u32,
    offset: i128,
}

type Addresses = BTreeMap<u32, Option<Address>>;
type Cell = (u32, i128);

fn address(facts: &Addresses, value: &Value) -> Option<Address> {
    facts.get(&value.id).copied().flatten()
}

fn edges(inst: &Inst) -> Vec<(u32, &[Value])> {
    match inst {
        Inst::Br(target, args) => vec![(*target, args)],
        Inst::CondBr(_, a, aa, b, ba) => vec![(*a, aa), (*b, ba)],
        _ => Vec::new(),
    }
}

fn cell_type(ty: &EvoIRType) -> Option<u64> {
    match ty {
        EvoIRType::Bits(width @ (8 | 16 | 32 | 64)) => Some(u64::from(*width) / 8),
        _ => None,
    }
}

fn constant_index(value: &Value, raw: u64, signed: IndexSignedness) -> Option<i128> {
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        return None;
    };
    let raw = u128::from(raw) & ((1u128 << width) - 1);
    Some(
        if signed == IndexSignedness::Signed && raw & (1u128 << (width - 1)) != 0 {
            raw as i128 - (1i128 << width)
        } else {
            raw as i128
        },
    )
}

/// An absent fact is not yet reached; `None` means an unknown address. Joins
/// include every predecessor, including loop backedges. A disagreement loses
/// the fact, so an unknown alias cannot be mistaken for a private object.
fn addresses(function: &Function, incoming: &[Vec<(usize, Vec<Value>)>]) -> Addresses {
    let constants: BTreeMap<_, _> = function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|i| {
            if let Inst::Const(v, raw) = i {
                Some((v.id, *raw))
            } else {
                None
            }
        })
        .collect();
    let mut facts = Addresses::new();
    for arg in &function.blocks[0].args {
        if matches!(arg.value.ty, EvoIRType::Ptr(_)) {
            facts.insert(arg.value.id, None);
        }
    }
    for block in &function.blocks {
        for inst in &block.insts {
            for result in inst_results(inst) {
                if matches!(result.ty, EvoIRType::Ptr(_))
                    && !matches!(inst, Inst::Alloca { .. } | Inst::PtrOffset { .. })
                {
                    facts.insert(result.id, None);
                }
            }
        }
    }
    loop {
        let mut changed = false;
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block_index != 0 {
                for (index, arg) in block.args.iter().enumerate() {
                    if !matches!(arg.value.ty, EvoIRType::Ptr(_)) {
                        continue;
                    }
                    let mut joined = None;
                    for (_, args) in &incoming[block_index] {
                        if let Some(fact) = facts.get(&args[index].id) {
                            joined = Some(match joined {
                                None => *fact,
                                Some(old) if old == *fact => old,
                                _ => None,
                            });
                        }
                    }
                    if incoming[block_index].is_empty() {
                        joined = Some(None);
                    }
                    if let Some(fact) = joined {
                        changed |= facts.insert(arg.value.id, fact) != Some(fact);
                    }
                }
            }
            for inst in &block.insts {
                let (result, fact) = match inst {
                    Inst::Alloca { result, .. } => (
                        result,
                        Some(Address {
                            root: result.id,
                            offset: 0,
                        }),
                    ),
                    Inst::PtrOffset {
                        result,
                        base,
                        index,
                        direction,
                        signedness,
                        scale,
                    } => {
                        let Some(base) = facts.get(&base.id) else {
                            continue;
                        };
                        let fact = base.and_then(|base| {
                            let delta =
                                constant_index(index, *constants.get(&index.id)?, *signedness)?
                                    .checked_mul(i128::from(*scale))?;
                            let offset = match direction {
                                OffsetDirection::Add => base.offset.checked_add(delta)?,
                                OffsetDirection::Subtract => base.offset.checked_sub(delta)?,
                            };
                            Some(Address {
                                root: base.root,
                                offset,
                            })
                        });
                        (result, fact)
                    }
                    _ => continue,
                };
                changed |= facts.insert(result.id, fact) != Some(fact);
            }
        }
        if !changed {
            return facts;
        }
    }
}

/// These operations either stop on a poison read or produce ordinary bits.
/// A transported poison is different: it is not read until its consumer.
fn initialized_values(function: &Function, incoming: &[Vec<(usize, Vec<Value>)>]) -> BTreeSet<u32> {
    let mut initialized: BTreeSet<_> = function.blocks[0].args.iter().map(|a| a.value.id).collect();
    for inst in function.blocks.iter().flat_map(|b| &b.insts) {
        if matches!(
            inst,
            Inst::Const(..)
                | Inst::Add(..)
                | Inst::Sub(..)
                | Inst::Mul(..)
                | Inst::SDiv(..)
                | Inst::UDiv(..)
                | Inst::SRem(..)
                | Inst::URem(..)
                | Inst::And(..)
                | Inst::Or(..)
                | Inst::Xor(..)
                | Inst::Shl(..)
                | Inst::AShr(..)
                | Inst::LShr(..)
                | Inst::ICmp(..)
                | Inst::ZExt(..)
                | Inst::SExt(..)
                | Inst::Trunc(..)
                | Inst::BitOperation { .. }
                | Inst::Load { .. }
        ) {
            initialized.extend(inst_results(inst).into_iter().map(|v| v.id));
        }
    }
    loop {
        let mut changed = false;
        for (bi, block) in function.blocks.iter().enumerate().skip(1) {
            for (ai, arg) in block.args.iter().enumerate() {
                if !incoming[bi].is_empty()
                    && incoming[bi]
                        .iter()
                        .all(|(_, args)| initialized.contains(&args[ai].id))
                {
                    changed |= initialized.insert(arg.value.id);
                }
            }
        }
        if !changed {
            return initialized;
        }
    }
}

fn cell(facts: &Addresses, pointer: &Value) -> Option<Cell> {
    address(facts, pointer).map(|a| (a.root, a.offset))
}

/// Only a complete integer cell has no padding or pointer representation to
/// lose. Initialization is checked separately at each executed copy.
fn copy_cell(
    facts: &Addresses,
    layouts: &BTreeMap<u32, AccessLayout>,
    pointer: &Value,
    layout: &AccessLayout,
) -> Option<Cell> {
    let at = address(facts, pointer)?;
    let object = layouts.get(&at.root)?;
    (at.offset == 0
        && object.size_bytes == layout.size_bytes
        && matches!(layout.size_bytes, 1 | 2 | 4 | 8))
    .then_some((at.root, 0))
}

/// If one endpoint of a private-to-private copy cannot be promoted, keep its
/// entire copy component in memory. A stale memory copy must never read an
/// object whose stores have been removed. An external destination is a write,
/// not an address escape, and is emitted as an ordinary checked store.
fn close_copy_components(
    function: &Function,
    facts: &Addresses,
    layouts: &BTreeMap<u32, AccessLayout>,
    eligible: &mut BTreeSet<u32>,
) {
    loop {
        let mut changed = false;
        for inst in function.blocks.iter().flat_map(|b| &b.insts) {
            if let Inst::Copy {
                destination,
                source,
                ..
            } = inst
            {
                if let (Some(dst), Some(src)) =
                    (address(facts, destination), address(facts, source))
                {
                    if layouts.contains_key(&dst.root)
                        && layouts.contains_key(&src.root)
                        && (!eligible.contains(&dst.root) || !eligible.contains(&src.root))
                    {
                        changed |= eligible.remove(&dst.root);
                        changed |= eligible.remove(&src.root);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn is_cyclic(function: &Function, indices: &BTreeMap<u32, usize>, start: usize) -> bool {
    let mut pending: Vec<_> = edges(&function.blocks[start].terminator)
        .into_iter()
        .map(|(id, _)| indices[&id])
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if index == start {
            return true;
        }
        if seen.insert(index) {
            pending.extend(
                edges(&function.blocks[index].terminator)
                    .into_iter()
                    .map(|(id, _)| indices[&id]),
            );
        }
    }
    false
}

pub(super) fn promote(function: &mut Function) {
    if function.blocks.is_empty()
        || function.blocks.iter().any(|b| {
            !matches!(
                b.terminator,
                Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Trap(..)
            ) || b.insts.iter().any(|i| matches!(i, Inst::Asm(_)))
        })
    {
        return;
    }
    // Inspect layouts first: most functions have no candidate, so they need
    // no pointer or initialization fixed point at all.
    let layouts: BTreeMap<_, _> = function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|inst| match inst {
            Inst::Alloca { result, layout, .. } if (1..=64).contains(&layout.size_bytes) => {
                Some((result.id, *layout))
            }
            _ => None,
        })
        .collect();
    if layouts.is_empty() || layouts.len() > 64 || function.blocks.len() > 4096 {
        return;
    }
    let indices: BTreeMap<_, _> = function
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect();
    let mut incoming = vec![Vec::new(); function.blocks.len()];
    for (bi, block) in function.blocks.iter().enumerate() {
        for (target, args) in edges(&block.terminator) {
            incoming[indices[&target]].push((bi, args.to_vec()));
        }
    }
    if !incoming[0].is_empty() {
        return;
    }
    let facts = addresses(function, &incoming);
    let initialized = initialized_values(function, &incoming);
    // Bounds cap analysis and edge growth, not the source language. Declined
    // objects retain their original operations and diagnostics.
    let mut eligible: BTreeSet<_> = layouts.keys().copied().collect();
    // One allocation site is one abstract object only when it executes at
    // most once. Repeated allocation needs a distinct instance relation and
    // stays in memory, even if a particular C loop appears harmless.
    for (bi, block) in function.blocks.iter().enumerate() {
        if block.insts.iter().any(|i| matches!(i, Inst::Alloca { .. }))
            && is_cyclic(function, &indices, bi)
        {
            for inst in &block.insts {
                if let Inst::Alloca { result, .. } = inst {
                    eligible.remove(&result.id);
                }
            }
        }
    }
    let mut cells = BTreeMap::<Cell, EvoIRType>::new();
    for block in &function.blocks {
        for inst in block.insts.iter().chain([&block.terminator]) {
            for used in inst_uses(inst) {
                let Some(at) = address(&facts, used) else {
                    continue;
                };
                let Some(layout) = layouts.get(&at.root) else {
                    continue;
                };
                let mut access = None;
                let permitted = match inst {
                    Inst::PtrOffset { result, base, .. } if base.id == used.id => {
                        address(&facts, result).is_some_and(|next| {
                            next.root == at.root
                                && (0..=i128::from(layout.size_bytes)).contains(&next.offset)
                        })
                    }
                    Inst::PointerValid { pointer, .. } | Inst::LifetimeEnd { pointer, .. } => {
                        pointer.id == used.id
                    }
                    Inst::AccessInBounds { pointer, .. } => pointer.id == used.id,
                    Inst::Load {
                        address,
                        result,
                        volatile: false,
                        ..
                    } if address.id == used.id => {
                        access = Some(result.ty.clone());
                        true
                    }
                    Inst::Store {
                        address,
                        value,
                        volatile: false,
                        ..
                    } if address.id == used.id && initialized.contains(&value.id) => {
                        access = Some(value.ty.clone());
                        true
                    }
                    Inst::Copy {
                        destination,
                        source,
                        layout: copied,
                        volatile: CopyVolatility::None,
                        ..
                    } => {
                        let src = copy_cell(&facts, &layouts, source, copied);
                        let here = copy_cell(&facts, &layouts, used, copied);
                        // Reading external representation may transfer unknown
                        // bytes. It is not a scalar read and is not admitted.
                        let permitted = src.is_some()
                            && here.is_some()
                            && (used.id == destination.id || used.id == source.id);
                        if permitted {
                            access = Some(EvoIRType::Bits((copied.size_bytes * 8) as u16));
                        }
                        permitted
                    }
                    Inst::Br(..) | Inst::CondBr(..) => edges(inst).iter().all(|(target, args)| {
                        args.iter().zip(&function.blocks[indices[target]].args).all(
                            |(value, param)| {
                                value.id != used.id || address(&facts, &param.value) == Some(at)
                            },
                        )
                    }),
                    _ => false,
                };
                if !permitted {
                    eligible.remove(&at.root);
                }
                if let Some(ty) = access {
                    let Some(size) = cell_type(&ty) else {
                        eligible.remove(&at.root);
                        continue;
                    };
                    if at.offset < 0 || at.offset + i128::from(size) > i128::from(layout.size_bytes)
                    {
                        eligible.remove(&at.root);
                        continue;
                    }
                    let key = (at.root, at.offset);
                    if cells.insert(key, ty.clone()).is_some_and(|old| old != ty) {
                        eligible.remove(&at.root);
                    }
                }
            }
        }
    }
    // A byte view or a differently sized overlapping cell invalidates the
    // complete object, not merely the second access encountered in the scan.
    let mut previous = None;
    for (&(root, offset), ty) in &cells {
        if previous.is_some_and(|(r, end)| r == root && end > offset) {
            eligible.remove(&root);
        }
        previous = Some((root, offset + i128::from(cell_type(ty).unwrap())));
    }
    close_copy_components(function, &facts, &layouts, &mut eligible);
    cells.retain(|(root, _), _| eligible.contains(root));
    if cells.is_empty()
        || cells.len() > 64
        || cells.len().saturating_mul(function.blocks.len()) > 4096
    {
        return;
    }
    let keys: Vec<_> = cells.keys().copied().collect();
    let cell_indices: BTreeMap<_, _> = keys.iter().enumerate().map(|(i, k)| (*k, i)).collect();
    // A masked store initializes only some bits of its cell, so it never
    // makes the cell definitely initialized (EvoIR 3.8).
    let transfer = |inst: &Inst, state: &mut Vec<bool>| match inst {
        Inst::Alloca { result, .. } => {
            for (i, key) in keys.iter().enumerate() {
                if key.0 == result.id {
                    state[i] = false;
                }
            }
        }
        Inst::Store {
            address,
            mask: None,
            ..
        } => {
            if let Some(index) = cell(&facts, address).and_then(|k| cell_indices.get(&k)) {
                state[*index] = true;
            }
        }
        Inst::Copy {
            destination,
            source,
            ..
        } => {
            if let Some(dst) = cell(&facts, destination).and_then(|k| cell_indices.get(&k)) {
                state[*dst] = cell(&facts, source)
                    .and_then(|k| cell_indices.get(&k))
                    .is_some_and(|&src| state[src]);
            }
        }
        Inst::LifetimeEnd { pointer, .. } => {
            if let Some(at) = address(&facts, pointer) {
                for (i, key) in keys.iter().enumerate() {
                    if key.0 == at.root {
                        state[i] = false;
                    }
                }
            }
        }
        _ => {}
    };
    // Greatest fixed point of definite initialization, anchored at an empty
    // entry state. A backedge cannot initialize the first loop iteration.
    let mut starts = vec![vec![true; keys.len()]; function.blocks.len()];
    let mut ends = starts.clone();
    loop {
        let mut changed = false;
        for (bi, block) in function.blocks.iter().enumerate() {
            let mut state = vec![bi != 0 && !incoming[bi].is_empty(); keys.len()];
            for (pred, _) in &incoming[bi] {
                for (i, value) in state.iter_mut().enumerate() {
                    *value &= ends[*pred][i];
                }
            }
            starts[bi] = state.clone();
            for inst in &block.insts {
                transfer(inst, &mut state);
            }
            changed |= ends[bi] != state;
            ends[bi] = state;
        }
        if !changed {
            break;
        }
    }
    for (bi, block) in function.blocks.iter().enumerate() {
        let mut state = starts[bi].clone();
        for inst in &block.insts {
            if let Inst::Load { address, .. } = inst {
                if let Some(key) = cell(&facts, address) {
                    if cell_indices.get(&key).is_some_and(|&i| !state[i]) {
                        eligible.remove(&key.0);
                    }
                }
            }
            if let Inst::Copy { source, .. } = inst {
                if let Some(key) = cell(&facts, source) {
                    if cell_indices.get(&key).is_some_and(|&i| !state[i]) {
                        eligible.remove(&key.0);
                    }
                }
            }
            transfer(inst, &mut state);
        }
    }
    close_copy_components(function, &facts, &layouts, &mut eligible);
    cells.retain(|(root, _), _| eligible.contains(root));
    if cells.is_empty() {
        return;
    }
    // This is the C lowering's strong-protector decision, not permission for
    // native code to ignore an explicit IR stack_protect request. Clear it
    // only when every frame object is promoted and no dynamic or activation
    // address survives. Declined, escaping and unused candidate objects keep
    // the conservative source decision.
    let clear_protection = function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .all(|inst| match inst {
            Inst::Alloca { result, .. } => cells.keys().any(|(root, _)| *root == result.id),
            Inst::DynamicAlloca { .. } | Inst::FrameAddress { .. } => false,
            _ => true,
        });
    let mut next = function
        .blocks
        .iter()
        .flat_map(|b| {
            b.args
                .iter()
                .map(|a| a.value.id)
                .chain(b.insts.iter().flat_map(inst_results).map(|v| v.id))
        })
        .max()
        .unwrap_or(0);
    let copies = function
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|i| matches!(i, Inst::Copy { .. }))
        .count();
    let Some(extra) = cells
        .len()
        .checked_mul(function.blocks.len() + 1)
        .and_then(|n| n.checked_add(copies.checked_mul(2)?))
        .and_then(|n| u32::try_from(n).ok())
    else {
        return;
    };
    let Some(last) = next.checked_add(extra) else {
        return;
    };
    let _ = last;
    if clear_protection {
        function.stack_protect = false;
    }
    let mut fresh = |ty: &EvoIRType| {
        next += 1;
        Value {
            id: next,
            ty: ty.clone(),
        }
    };
    for (bi, block) in function.blocks.iter_mut().enumerate() {
        let mut current = BTreeMap::new();
        let mut output = Vec::new();
        for (key, ty) in &cells {
            let value = fresh(ty);
            if bi == 0 {
                output.push(Inst::Uninitialized(value.clone()));
            } else {
                block.args.push(BlockArg {
                    name: format!("cell{}_{}", key.0, key.1),
                    value: value.clone(),
                });
            }
            current.insert(*key, value);
        }
        let mut aliases = HashMap::new();
        for mut inst in std::mem::take(&mut block.insts) {
            remap_inst_uses(&mut inst, &aliases);
            match &inst {
                Inst::Alloca { result, .. } => {
                    // Start the object's value lifetime here, not at a
                    // preceding join. Repeated allocation sites were declined.
                    for (key, ty) in &cells {
                        if key.0 == result.id {
                            let value = fresh(ty);
                            output.push(Inst::Uninitialized(value.clone()));
                            current.insert(*key, value);
                        }
                    }
                    output.push(inst);
                }
                Inst::Load {
                    result,
                    address,
                    mem_in,
                    mem_out,
                    world_in,
                    world_out,
                    ..
                } if cell(&facts, address).is_some_and(|k| current.contains_key(&k)) => {
                    aliases.insert(result.id, current[&cell(&facts, address).unwrap()].clone());
                    aliases.insert(mem_out.id, mem_in.clone());
                    aliases.insert(world_out.id, world_in.clone());
                }
                Inst::Store {
                    value,
                    address,
                    mem_in,
                    mem_out,
                    world_in,
                    world_out,
                    ..
                } if cell(&facts, address).is_some_and(|k| current.contains_key(&k)) => {
                    current.insert(cell(&facts, address).unwrap(), value.clone());
                    aliases.insert(mem_out.id, mem_in.clone());
                    aliases.insert(world_out.id, world_in.clone());
                }
                Inst::Copy {
                    destination,
                    source,
                    mem_in,
                    mem_out,
                    world_in,
                    world_out,
                    layout,
                    ..
                } if cell(&facts, source).is_some_and(|k| current.contains_key(&k)) => {
                    // Source bytes are initialized integer bits. Both ranges
                    // still get their access checks; distinct private roots
                    // cannot overlap, and a same-root copy covers the same cell.
                    let checked_source = fresh(&mem_in.ty);
                    let checked_destination = fresh(&mem_in.ty);
                    output.push(Inst::AccessInBounds {
                        pointer: source.clone(),
                        mem_in: mem_in.clone(),
                        mem_out: checked_source.clone(),
                        layout: *layout,
                    });
                    output.push(Inst::AccessInBounds {
                        pointer: destination.clone(),
                        mem_in: checked_source,
                        mem_out: checked_destination.clone(),
                        layout: *layout,
                    });
                    let value = current[&cell(&facts, source).unwrap()].clone();
                    if let Some(key) = cell(&facts, destination).filter(|k| current.contains_key(k))
                    {
                        current.insert(key, value);
                        aliases.insert(mem_out.id, checked_destination);
                        aliases.insert(world_out.id, world_in.clone());
                    } else {
                        output.push(Inst::Store {
                            address: destination.clone(),
                            value,
                            mem_in: checked_destination,
                            mem_out: mem_out.clone(),
                            world_in: world_in.clone(),
                            world_out: world_out.clone(),
                            volatile: false,
                            mask: None,
                        });
                    }
                }
                _ => output.push(inst),
            }
        }
        block.insts = output;
        remap_inst_uses(&mut block.terminator, &aliases);
        match &mut block.terminator {
            Inst::Br(_, args) => args.extend(current.values().cloned()),
            Inst::CondBr(_, _, a, _, b) => {
                a.extend(current.values().cloned());
                b.extend(current.values().cloned());
            }
            _ => {}
        }
    }
}
