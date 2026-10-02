//! N03a step 3: the shared argument cursor and frame driver on a toy ABI.
//!
//! The toy ABI deliberately differs from every AArch64 profile: three general
//! and one vector argument register at unrelated identities, four-byte stack
//! slots, 32-byte stack alignment and its own save sizes. Any fixed AArch64
//! constant left in the generic layer changes one of these results.

use std::collections::{BTreeMap, BTreeSet};

use evocc_mir::generic::abi::{AbiFacts, AbiSpec, ArgumentCursor};
use evocc_mir::generic::frame::{layout, FrameObject, FrameRequest, ObjectLifetime};
use evocc_mir::generic::register::RegisterBank;

const ARGUMENTS: [RegisterBank; 2] = [
    RegisterBank {
        first: 40,
        count: 3,
    },
    RegisterBank {
        first: 90,
        count: 1,
    },
];

const RESULTS: [RegisterBank; 2] = [
    RegisterBank {
        first: 40,
        count: 2,
    },
    RegisterBank {
        first: 90,
        count: 1,
    },
];

const TOY: AbiFacts = AbiFacts {
    name: "toy",
    argument_banks: &ARGUMENTS,
    result_banks: &RESULTS,
    indirect_result: 47,
    result_overflow: 41,
    stack_slot_bytes: 4,
    stack_alignment_bytes: 32,
    stack_arguments: true,
    callee_saved: &[44, 45],
    call_clobbers: &[40, 41, 42, 43, 90, 91],
    partially_preserved: &[92, 93],
    partially_preserved_bytes: 2,
    frame_record_bytes: 12,
    save_slot_bytes: 4,
    variadic_save_bytes: &[4, 8],
    variadic_save_alignment: 32,
};

#[test]
fn independent_bank_cursors_share_one_stack_cursor() {
    let mut cursor = ArgumentCursor::new(TOY.facts().argument_banks);
    assert_eq!(cursor.place(0, 1), Ok(40));
    assert_eq!(cursor.place(1, 2), Ok(90));
    // The vector bank is exhausted while general registers remain.
    assert_eq!(cursor.place(1, 1), Err(0));
    assert_eq!(cursor.place(0, 1), Ok(41));
    assert_eq!(cursor.pair_padding(0), 0);
    assert_eq!(cursor.place(0, 1), Ok(42));
    // Once a bank is exhausted, pair padding follows the shared stack cursor.
    assert_eq!(cursor.pair_padding(0), 1);
    // A two-word piece aligns the one stack cursor both banks share.
    assert_eq!(cursor.place(1, 2), Err(2));
    assert_eq!(cursor.place(0, 1), Err(4));
    assert_eq!(cursor.pair_padding(0), 1);
    assert_eq!((cursor.used(0), cursor.used(1)), (3, 1));
    assert_eq!(cursor.stack_words(), 5);
    assert_eq!(cursor.stack_bytes(&TOY), 20);

    let mut results = ArgumentCursor::new(TOY.result_banks);
    assert_eq!(results.place(0, 1), Ok(40));
    assert_eq!(results.place(0, 1), Ok(41));
    assert_eq!(results.place(0, 1), Err(0));
}

#[test]
fn derived_offsets_use_the_description_not_fixed_constants() {
    assert_eq!(TOY.stack_alignment_words(), 8);
    assert_eq!(TOY.variadic_save_area_bytes(0), 12);
    assert_eq!(TOY.variadic_save_area_bytes(1), 8);
    assert_eq!(TOY.variadic_save_total_bytes(), 20);
    assert_eq!(TOY.incoming_argument_offset(0), 12);
    assert_eq!(TOY.incoming_argument_offset(3), 24);
    assert_eq!(TOY.outgoing_argument_offset(3), 12);
    assert_eq!(TOY.outgoing_area_bytes(0), 0);
    assert_eq!(TOY.outgoing_area_bytes(5), 32);
    assert_eq!(TOY.outgoing_area_bytes(9), 64);
}

#[test]
fn frame_layout_uses_the_abi_slot_alignment_and_save_sizes() {
    let request = FrameRequest {
        objects: vec![
            FrameObject {
                origin: 2,
                size: 6,
                align: 2,
            },
            FrameObject {
                origin: 1,
                size: 8,
                align: 8,
            },
            FrameObject {
                origin: 2,
                size: 6,
                align: 2,
            },
        ],
        private: BTreeMap::new(),
        spill_slots: 3,
        outgoing: 8,
        variadic_named: Some(1),
        callee_saved: vec![44, 45],
        partially_preserved: BTreeSet::from([92]),
    };
    let frame = layout(&TOY, &request);
    assert_eq!(frame.alignment, 32);
    assert_eq!(frame.outgoing, 8);
    assert_eq!(frame.register_save, Some((1, 32)));
    assert_eq!(frame.spills, BTreeMap::from([(0, 52), (1, 56), (2, 60)]));
    // Origin order from the top: the lowest origin is placed highest.
    assert_eq!(frame.slots, BTreeMap::from([(1, 72), (2, 64)]));
    assert_eq!(frame.saved, BTreeMap::from([(44, 80), (45, 84)]));
    assert_eq!(frame.partially_saved, BTreeMap::from([(92, 88)]));
    assert_eq!(frame.size, 96);
}

#[test]
fn over_aligned_objects_reserve_slack_relative_to_the_abi_alignment() {
    let request = FrameRequest {
        objects: vec![FrameObject {
            origin: 0,
            size: 3,
            align: 64,
        }],
        ..FrameRequest::default()
    };
    let frame = layout(&TOY, &request);
    assert_eq!(frame.alignment, 64);
    assert_eq!(frame.slots, BTreeMap::from([(0, 0)]));
    // align_up(3, 64) plus the 64 - 32 slack for the incoming alignment.
    assert_eq!(frame.size, 96);
    assert_eq!(layout(&TOY, &FrameRequest::default()).size, 0);
    assert_eq!(layout(&TOY, &FrameRequest::default()).alignment, 32);
}

#[test]
fn only_disjoint_private_objects_of_one_shape_share_bytes() {
    let lifetime = |first, last| ObjectLifetime {
        block: 0,
        first,
        last,
    };
    let object = |origin| FrameObject {
        origin,
        size: 16,
        align: 8,
    };
    let disjoint = FrameRequest {
        objects: vec![object(1), object(2), object(3)],
        private: BTreeMap::from([(1, lifetime(0, 2)), (2, lifetime(3, 5))]),
        ..FrameRequest::default()
    };
    let frame = layout(&TOY, &disjoint);
    // Origin 3 escapes and keeps its own bytes; 1 and 2 never overlap.
    assert_eq!(frame.slots, BTreeMap::from([(1, 16), (2, 16), (3, 0)]));

    let overlapping = FrameRequest {
        private: BTreeMap::from([(1, lifetime(0, 3)), (2, lifetime(3, 5))]),
        ..disjoint
    };
    let frame = layout(&TOY, &overlapping);
    assert_eq!(frame.slots, BTreeMap::from([(1, 32), (2, 16), (3, 0)]));

    let other_block = ObjectLifetime {
        block: 1,
        first: 0,
        last: 9,
    };
    assert!(!lifetime(0, 9).overlaps(other_block));
    assert!(lifetime(0, 3).overlaps(lifetime(3, 5)));
}
