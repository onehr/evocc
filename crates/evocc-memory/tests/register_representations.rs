//! The register forms of an object representation (EvoMIR 4.1, D29). The
//! EvoMIR interpreter keeps the bytes of a record in a general register with
//! the initializedness of each bit and their pointer fragments. A shift moves
//! whole bytes and fills with initialized zeros, an `orr` keeps the byte of
//! one operand where the other operand's byte is an initialized zero, and
//! bytes that are all initialized without a pointer fragment are an integer.

use evocc_memory::{
    AccessLayout, ByteOrder, InitialState, MemoryConfig, MemoryFaultKind, MemoryLimits,
    MemoryState, ObjectRepresentation, Pointer, PointerValue, WorldLimits, WorldState,
};
use evocc_types::TargetDataLayout;

const BYTE: AccessLayout = AccessLayout::new(1, 1);
const WORD: AccessLayout = AccessLayout::new(8, 1);

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

fn world() -> WorldState {
    WorldState::new(WorldLimits {
        max_volatile_events: 16,
    })
}

/// An uninitialized object of `size` bytes, aligned to eight.
fn object(memory: MemoryState, origin: u64, size: u64) -> (MemoryState, Pointer) {
    memory
        .allocate_static(
            origin,
            AccessLayout::new(size, 8),
            InitialState::Uninitialized,
        )
        .unwrap()
}

/// The eight bytes at `pointer`, as a transport load reads them.
fn read(memory: &MemoryState, pointer: Pointer) -> ObjectRepresentation {
    memory
        .load_representation(&world(), pointer, WORD, false)
        .unwrap()
        .0
}

/// `bytes` stored at a fresh object of `memory`.
fn stored(memory: MemoryState, bytes: &ObjectRepresentation) -> (MemoryState, Pointer) {
    let (memory, pointer) = object(memory, 9, 8);
    let (memory, _) = memory
        .store_representation(&world(), pointer, WORD, bytes, false)
        .unwrap();
    (memory, pointer)
}

fn integer(raw: u64) -> ObjectRepresentation {
    let mut bytes = ObjectRepresentation::uninitialized(8).unwrap();
    bytes
        .write_integer(0, 8, Some(u128::from(raw)), ByteOrder::Little)
        .unwrap();
    bytes
}

#[test]
fn a_shift_moves_whole_bytes_and_fills_with_initialized_zeros() {
    let bytes = integer(0x1234);
    assert_eq!(
        bytes.placed(8, 2).plain_integer(ByteOrder::Little),
        Some(0x1234_0000)
    );
    assert_eq!(
        bytes.placed(8, -1).plain_integer(ByteOrder::Little),
        Some(0x12)
    );
    // The low four bytes of the register, then zero above them.
    assert_eq!(
        integer(0x1122_3344_5566_7788)
            .placed(4, 0)
            .placed(8, 0)
            .plain_integer(ByteOrder::Little),
        Some(0x5566_7788)
    );
    // An uninitialized byte moves with the shift.
    let (memory, record) = object(memory(), 1, 8);
    let memory = memory.store_integer(record, BYTE, 7).unwrap();
    let bytes = read(&memory, record);
    assert_eq!(bytes.plain_integer(ByteOrder::Little), None);
    let (memory, copy) = stored(memory, &bytes.placed(8, 1).placed(8, -1));
    assert_eq!(memory.load_integer(copy, BYTE).unwrap(), 7);
    assert_eq!(
        memory
            .load_integer(copy.checked_offset(1).unwrap(), BYTE)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::UninitializedRead
    );
}

#[test]
fn an_orr_keeps_the_state_of_each_bit() {
    // Bits 0 to 2 of byte 0 hold 5; its other bits and bytes 1 to 7 are
    // uninitialized.
    let (memory, record) = object(memory(), 1, 8);
    let memory = memory
        .store_integer_bits(record, BYTE, 0b101, 0b111)
        .unwrap();
    let partial = read(&memory, record);
    // An initialized zero keeps the other operand's byte as it is.
    let kept = integer(0).or(&partial).unwrap();
    assert_eq!(kept, partial);
    assert_eq!(partial.or(&integer(0)).unwrap(), partial);
    // An initialized one bit is initialized whatever the other bit is.
    let combined = integer(0b1000).or(&partial).unwrap();
    let (memory, copy) = stored(memory, &combined);
    assert_eq!(
        memory.load_integer_bits(copy, BYTE, 0b1111).unwrap() & 0b1111,
        0b1101
    );
    assert_eq!(
        memory
            .load_integer_bits(copy, BYTE, 0b1_0000)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::UninitializedRead
    );
    // Two sizes do not combine.
    assert_eq!(integer(1).or(&integer(1).placed(4, 0)), None);
}

#[test]
fn a_pointer_fragment_moves_only_beside_initialized_zeros() {
    let (memory, target) = object(memory(), 1, 8);
    let (memory, slot) = object(memory, 2, 8);
    let memory = memory
        .store_pointer_value(slot, AccessLayout::new(8, 8), PointerValue::object(target))
        .unwrap();
    let pointer = read(&memory, slot);
    assert_eq!(pointer.plain_integer(ByteOrder::Little), None);
    let moved = integer(0).or(&pointer).unwrap();
    let (memory, copy) = stored(memory, &moved);
    assert_eq!(
        memory.load_pointer(copy, AccessLayout::new(8, 8)).unwrap(),
        target
    );
    // A fragment that meets a byte that is not an initialized zero does not
    // combine.
    assert_eq!(integer(1).or(&pointer), None);
}
