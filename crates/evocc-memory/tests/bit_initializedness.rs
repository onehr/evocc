//! Memory initializedness is one flag per bit (EvoIR 5.2, D23). A masked
//! integer load checks only the bits it demands, and a masked integer store
//! initializes only the bits it names. Every other access keeps its byte
//! rule: a byte is initialized when all eight of its bits are.

use evocc_memory::{
    AccessLayout, ByteSequence, InitialState, MemoryConfig, MemoryFault, MemoryFaultKind,
    MemoryLimits, MemoryState, Pointer, WorldLimits, WorldState,
};
use evocc_types::TargetDataLayout;

const BYTE: AccessLayout = AccessLayout::new(1, 1);
const HALF: AccessLayout = AccessLayout::new(2, 1);

fn memory() -> MemoryState {
    let limits = MemoryLimits {
        max_objects: 8,
        max_object_bytes: 64,
        max_total_bytes: 256,
    };
    MemoryState::new(
        MemoryConfig::for_target(&TargetDataLayout::aarch64_unknown_linux_gnu(), limits).unwrap(),
    )
    .unwrap()
}

fn object(memory: MemoryState, origin: u64, size: u64) -> (MemoryState, Pointer) {
    memory
        .allocate_static(
            origin,
            AccessLayout::new(size, 1),
            InitialState::Uninitialized,
        )
        .unwrap()
}

fn uninitialized(result: Result<impl std::fmt::Debug, MemoryFault>) {
    assert_eq!(
        result.unwrap_err().kind(),
        MemoryFaultKind::UninitializedRead
    );
}

#[test]
fn a_masked_store_initializes_only_its_bits() {
    let (memory, byte) = object(memory(), 1, 1);
    // Bits 0 to 2 hold 5. The other bits are written but stay uninitialized.
    let memory = memory
        .store_integer_bits(byte, BYTE, 0b0000_0101, 0b111)
        .unwrap();
    assert_eq!(
        memory.load_integer_bits(byte, BYTE, 0b111).unwrap() & 0b111,
        5
    );
    assert_eq!(memory.load_integer_bits(byte, BYTE, 0b001).unwrap() & 1, 1);
    uninitialized(memory.load_integer_bits(byte, BYTE, 0b1111_1000));
    uninitialized(memory.load_integer_bits(byte, BYTE, 0b0000_1000));
    uninitialized(memory.load_integer(byte, BYTE));
    assert!(!memory.is_range_initialized(byte, BYTE).unwrap());
    // A load that demands no bit reads what memory holds and reports nothing.
    assert_eq!(memory.load_integer_bits(byte, BYTE, 0).unwrap(), 5);
}

#[test]
fn neighbouring_stores_complete_a_byte_in_either_order() {
    let (memory, byte) = object(memory(), 1, 1);
    let memory = memory
        .store_integer_bits(byte, BYTE, 0b1110_0000, 0b1111_1000)
        .unwrap();
    uninitialized(memory.load_integer(byte, BYTE));
    let held = memory.load_integer_bits(byte, BYTE, 0).unwrap();
    let memory = memory
        .store_integer_bits(byte, BYTE, (held & !0b111) | 0b011, 0b111)
        .unwrap();
    assert_eq!(memory.load_integer(byte, BYTE).unwrap(), 0b1110_0011);
    assert!(memory.is_range_initialized(byte, BYTE).unwrap());
}

#[test]
fn a_masked_store_keeps_initialized_neighbours_initialized() {
    let (memory, byte) = object(memory(), 1, 1);
    let memory = memory.store_integer(byte, BYTE, 0xa5).unwrap();
    let memory = memory.store_integer_bits(byte, BYTE, 0xa2, 0b111).unwrap();
    assert_eq!(memory.load_integer(byte, BYTE).unwrap(), 0xa2);
}

#[test]
fn a_mask_follows_the_target_byte_order() {
    let (memory, pair) = object(memory(), 1, 2);
    // On a little-endian target bits 8 to 11 are the low half of byte 1.
    let memory = memory
        .store_integer_bits(pair, HALF, 0x0300, 0x0f00)
        .unwrap();
    let second = pair.checked_offset(1).unwrap();
    assert_eq!(
        memory.load_integer_bits(second, BYTE, 0x0f).unwrap() & 0x0f,
        3
    );
    uninitialized(memory.load_integer_bits(pair, BYTE, 1));
    uninitialized(memory.load_integer_bits(second, BYTE, 0x10));
    let error = memory.load_integer_bits(pair, HALF, 0x1f00).unwrap_err();
    assert!(
        matches!(
            error,
            MemoryFault::UninitializedRead {
                first_uninitialized: 1,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn copies_and_representations_move_each_bit_state() {
    let (memory, source) = object(memory(), 1, 2);
    let (memory, copied) = object(memory, 2, 2);
    let (memory, carried) = object(memory, 3, 2);
    let memory = memory
        .store_integer_bits(source, BYTE, 0b0110_0000, 0b0111_0000)
        .unwrap();
    let second = source.checked_offset(1).unwrap();
    let memory = memory.store_integer(second, BYTE, 0x7e).unwrap();
    // A copy reads nothing and moves the bit states unchanged.
    let memory = memory.copy_bytes(copied, source, HALF).unwrap();
    assert_eq!(
        memory.load_integer_bits(copied, BYTE, 0b0111_0000).unwrap() & 0b0111_0000,
        0b0110_0000
    );
    uninitialized(memory.load_integer_bits(copied, BYTE, 0b1000_0000));
    assert_eq!(
        memory
            .load_integer(copied.checked_offset(1).unwrap(), BYTE)
            .unwrap(),
        0x7e
    );
    // A transported representation keeps them too.
    let world = WorldState::new(WorldLimits {
        max_volatile_events: 4,
    });
    let (representation, world) = memory
        .load_representation(&world, source, HALF, false)
        .unwrap();
    assert_eq!(representation.initialized(), vec![false, true]);
    let (memory, _) = memory
        .store_representation(&world, carried, HALF, &representation, false)
        .unwrap();
    assert_eq!(
        memory
            .load_integer_bits(carried, BYTE, 0b0111_0000)
            .unwrap()
            & 0b0111_0000,
        0b0110_0000
    );
    uninitialized(memory.load_integer_bits(carried, BYTE, 0b0000_1111));
}

#[test]
fn a_masked_store_obeys_the_ordinary_store_checks() {
    let (memory, byte) = object(memory(), 1, 1);
    let past = byte.checked_offset(1).unwrap();
    assert_eq!(
        memory
            .store_integer_bits(past, BYTE, 1, 1)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::OutOfBounds
    );
    assert_eq!(
        memory.load_integer_bits(past, BYTE, 0).unwrap_err().kind(),
        MemoryFaultKind::OutOfBounds
    );
    let text = ByteSequence::try_from_bytes(b"a\0".to_vec()).unwrap();
    let (memory, text) = memory
        .allocate_string_literal(2, AccessLayout::new(2, 1), &text)
        .unwrap();
    assert_eq!(
        memory
            .store_integer_bits(text, BYTE, 1, 1)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::ReadOnlyObject
    );
}

/// D25: the masked volatile forms keep the rule of the masked forms and add
/// one event each. An event records the bytes and, when one of them is not
/// completely initialized, the initializedness of each byte.
#[test]
fn a_masked_volatile_access_is_one_event_under_the_same_rule() {
    let (memory, unit) = object(memory(), 1, 2);
    let world = WorldState::new(WorldLimits {
        max_volatile_events: 8,
    });
    // The read of a volatile bit-field store demands no bit.
    let (old, world) = memory
        .volatile_load_integer_bits(&world, unit, HALF, 0)
        .unwrap();
    let (memory, world) = memory
        .volatile_store_integer_bits(&world, unit, HALF, old & !0x0ff0 | 0x0ab0, 0x0ff0)
        .unwrap();
    let (value, world) = memory
        .volatile_load_integer_bits(&world, unit, HALF, 0x0ff0)
        .unwrap();
    assert_eq!(value & 0x0ff0, 0x0ab0);
    uninitialized(memory.volatile_load_integer_bits(&world, unit, HALF, 0x1000));
    uninitialized(memory.volatile_load_integer(&world, unit, HALF));
    let trace = world.volatile_trace();
    assert_eq!(trace.len(), 3);
    for event in trace {
        assert!(
            matches!(
                &event.value,
                evocc_memory::ObservedValue::Representation { initialized, .. }
                    if initialized == &vec![false, false]
            ),
            "{event:?}"
        );
    }
    // Once every bit is initialized, the event is the raw bits, as for an
    // unmasked access.
    let (memory, world) = memory
        .volatile_store_integer_bits(&world, unit, HALF, 0xabcd, 0xf00f)
        .unwrap();
    let (value, world) = memory
        .volatile_load_integer_bits(&world, unit, HALF, 0xffff)
        .unwrap();
    assert_eq!(value, 0xabcd);
    assert!(memory.is_range_initialized(unit, HALF).unwrap());
    let last = world.volatile_trace().last().unwrap();
    assert_eq!(
        last.value,
        evocc_memory::ObservedValue::RawTargetBits(vec![0xcd, 0xab])
    );
}
