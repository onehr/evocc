//! Shared frame layout over an ABI description.
//!
//! The target collects what a function needs (its fixed objects, spill slots,
//! outgoing area, variadic save request and preserved registers); this module
//! decides where each lives. Offsets are bytes above the aligned stack pointer
//! after the prologue. From the bottom: the outgoing argument area, the
//! variadic save areas, spill slots, fixed objects in origin order from the
//! top downward, then the callee-saved and partially preserved save slots.

use std::collections::{BTreeMap, BTreeSet};

use super::abi::AbiSpec;

/// One fixed frame object. Repeated declarations of one origin are allowed;
/// the layout keeps one logical entry per distinct object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FrameObject {
    pub origin: u64,
    pub size: u64,
    pub align: u64,
}

/// The closed instruction interval in which a private object is accessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectLifetime {
    pub block: usize,
    pub first: usize,
    pub last: usize,
}

impl ObjectLifetime {
    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.block == other.block && self.first <= other.last && other.first <= self.last
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameRequest {
    pub objects: Vec<FrameObject>,
    /// Proven nonescaping objects and their access intervals. Only these may
    /// share physical bytes with an identically shaped object.
    pub private: BTreeMap<u64, ObjectLifetime>,
    pub spill_slots: u32,
    pub outgoing: u64,
    /// Argument registers the named parameters of a variadic function consume.
    pub variadic_named: Option<u8>,
    /// Used callee-saved registers, in save order.
    pub callee_saved: Vec<u8>,
    /// Clobbered registers of which only the low bytes are preserved.
    pub partially_preserved: BTreeSet<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameLayout {
    /// Total bytes the prologue subtracts.
    pub size: u64,
    /// Required alignment of the stack pointer after the prologue.
    pub alignment: u64,
    pub saved: BTreeMap<u8, u64>,
    pub partially_saved: BTreeMap<u8, u64>,
    pub spills: BTreeMap<u32, u64>,
    pub slots: BTreeMap<u64, u64>,
    pub outgoing: u64,
    /// The named-register count and the offset of the variadic save areas.
    pub register_save: Option<(u8, u64)>,
}

const fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

/// Lay out one frame. Pure arithmetic over the request and the ABI facts.
#[must_use]
pub fn layout(abi: &impl AbiSpec, request: &FrameRequest) -> FrameLayout {
    let abi = abi.facts();
    let mut automatic = request.objects.clone();
    automatic.sort_unstable();
    automatic.dedup();
    let alignment = automatic
        .iter()
        .map(|object| object.align)
        .max()
        .unwrap_or(abi.stack_alignment_bytes)
        .max(abi.stack_alignment_bytes);

    let spill_bytes = u64::from(request.spill_slots) * abi.stack_slot_bytes;

    // The outgoing area is lowest, so a call writes its stack arguments at
    // increasing addresses from the stack pointer it calls with.
    let outgoing = request.outgoing;

    // The variadic save areas are adjacent and aligned, above the outgoing
    // area and below every spill.
    let register_save = request
        .variadic_named
        .map(|named| (named, align_up(outgoing, abi.variadic_save_alignment)));
    let above_save =
        register_save.map_or(outgoing, |(_, base)| base + abi.variadic_save_total_bytes());

    let mut spills = BTreeMap::new();
    for index in 0..request.spill_slots {
        spills.insert(index, above_save + u64::from(index) * abi.stack_slot_bytes);
    }

    // One slot per object, rounded to its alignment and laid out in origin
    // order from the top of the frame downward: walking the origins in reverse
    // from the bottom upward puts the lowest origin highest and gives each
    // object an offset its own alignment divides.
    let mut slots = BTreeMap::new();
    let mut cursor = above_save + spill_bytes;
    // Keep each origin's logical entry. Only identical physical layouts whose
    // closed access intervals cannot overlap may use the same bytes. Escaping
    // objects never enter this pool, even when their last explicit use is early.
    let mut reusable = BTreeMap::<(u64, u64), Vec<(u64, Vec<ObjectLifetime>)>>::new();
    for object in automatic.iter().rev() {
        let access = request.private.get(&object.origin).copied();
        let pool = reusable.entry((object.size, object.align)).or_default();
        if let Some(access) = access {
            if let Some((offset, intervals)) = pool
                .iter_mut()
                .find(|(_, intervals)| intervals.iter().all(|other| !access.overlaps(*other)))
            {
                slots.insert(object.origin, *offset);
                intervals.push(access);
                continue;
            }
        }
        cursor = align_up(cursor, object.align.max(1));
        slots.insert(object.origin, cursor);
        if let Some(access) = access {
            pool.push((cursor, vec![access]));
        }
        cursor += object.size;
    }

    // Every save slot is addressed with a scaled offset, so the save area
    // starts on a save-slot boundary whatever the objects below it needed.
    let saved_base = align_up(cursor, abi.save_slot_bytes);
    let saved_bytes = request.callee_saved.len() as u64 * abi.save_slot_bytes;
    let partial_base = saved_base + saved_bytes;
    let partially_saved: BTreeMap<u8, u64> = request
        .partially_preserved
        .iter()
        .enumerate()
        .map(|(index, number)| {
            (
                *number,
                partial_base + index as u64 * abi.partially_preserved_bytes,
            )
        })
        .collect();
    // An over-aligned frame realigns an incoming pointer that the ABI aligns
    // only to its own stack alignment, so it reserves the difference.
    let size = align_up(
        partial_base + partially_saved.len() as u64 * abi.partially_preserved_bytes,
        alignment,
    ) + alignment
        - abi.stack_alignment_bytes;

    let mut saved = BTreeMap::new();
    for (index, number) in request.callee_saved.iter().enumerate() {
        saved.insert(*number, saved_base + index as u64 * abi.save_slot_bytes);
    }

    FrameLayout {
        size,
        alignment,
        saved,
        partially_saved,
        spills,
        slots,
        outgoing,
        register_save,
    }
}
