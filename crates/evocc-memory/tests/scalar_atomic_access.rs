use evocc_memory::{
    AccessLayout, AtomicOrder, AtomicValue, InitialState, MemoryConfig, MemoryFaultKind,
    MemoryLimits, MemoryState, PointerValue, WorldLimits, WorldState,
};
use evocc_types::TargetDataLayout;

#[test]
fn atomic_events_preserve_snapshots_provenance_and_volatile_ordering() {
    let memory = MemoryState::new(
        MemoryConfig::for_target(
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            MemoryLimits {
                max_objects: 4,
                max_object_bytes: 16,
                max_total_bytes: 64,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let word = AccessLayout::new(4, 4);
    let pointer = AccessLayout::new(8, 8);
    let (memory, object) = memory
        .allocate_static(1, word, InitialState::Zeroed)
        .unwrap();
    let (memory, slot) = memory
        .allocate_static(2, pointer, InitialState::Uninitialized)
        .unwrap();
    let before = memory.clone();
    let world = WorldState::new(WorldLimits {
        max_volatile_events: 3,
    });
    let (memory, stored) = memory
        .atomic_store(
            &world,
            slot,
            pointer,
            AtomicOrder::Release,
            true,
            AtomicValue::Pointer(PointerValue::Object(object)),
        )
        .unwrap();
    assert!(world.atomic_trace().is_empty());
    assert!(
        stored.volatile_trace().is_empty(),
        "volatile atomic is one event, not two"
    );
    assert_eq!(
        before.load_pointer_value(slot, pointer).unwrap_err().kind(),
        MemoryFaultKind::UninitializedRead
    );
    let (_, ordinary) = memory.volatile_load_integer(&stored, object, word).unwrap();
    let (loaded, final_world) = memory
        .atomic_load(&ordinary, slot, pointer, AtomicOrder::Acquire, false, true)
        .unwrap();
    assert_eq!(loaded, AtomicValue::Pointer(PointerValue::Object(object)));
    assert_eq!(final_world.atomic_trace()[0].sequence, 0);
    assert_eq!(final_world.volatile_trace()[0].sequence, 1);
    assert_eq!(final_world.atomic_trace()[1].sequence, 2);
    assert!(final_world.atomic_trace()[0].volatile);
    assert!(!final_world.atomic_trace()[1].volatile);
    assert_eq!(final_world.atomic_trace()[0].order, AtomicOrder::Release);
    assert_eq!(final_world.atomic_trace()[1].order, AtomicOrder::Acquire);
    assert_eq!(
        final_world.atomic_trace()[0].value,
        final_world.atomic_trace()[1].value
    );
    assert_eq!(
        memory
            .atomic_load(
                &final_world,
                slot,
                pointer,
                AtomicOrder::Relaxed,
                false,
                true
            )
            .unwrap_err()
            .kind(),
        MemoryFaultKind::VolatileEventLimitExceeded
    );
}
