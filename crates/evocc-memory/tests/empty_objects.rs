use evocc_memory::{
    AccessLayout, InitialState, MemoryConfig, MemoryFaultKind, MemoryLimits, MemoryState,
};
use evocc_types::TargetDataLayout;

fn memory() -> MemoryState {
    let limits = MemoryLimits {
        max_objects: 4,
        max_object_bytes: 0,
        max_total_bytes: 0,
    };
    MemoryState::new(
        MemoryConfig::for_target(&TargetDataLayout::aarch64_unknown_linux_gnu(), limits).unwrap(),
    )
    .unwrap()
}

#[test]
fn empty_objects_have_identity_but_no_accessible_bytes() {
    let zero = AccessLayout::new(0, 16);
    let (memory, first) = memory()
        .allocate_static(10, zero, InitialState::Zeroed)
        .unwrap();
    let (memory, second) = memory
        .allocate_static(20, zero, InitialState::Zeroed)
        .unwrap();
    assert_ne!(first.object(), second.object());
    assert_eq!(memory.object_info(first.object()).unwrap().layout, zero);
    memory.check_pointer(first).unwrap();
    memory.copy_bytes(second, first, zero).unwrap();
    let error = memory
        .load_integer(first, AccessLayout::new(1, 1))
        .unwrap_err();
    assert_eq!(error.kind(), MemoryFaultKind::OutOfBounds);
    assert_eq!(
        memory
            .store_integer(first, AccessLayout::new(1, 1), 7)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::OutOfBounds
    );
    assert_eq!(
        memory
            .allocate_static(30, AccessLayout::new(0, 0), InitialState::Zeroed)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::InvalidAlignment
    );
}

#[test]
fn empty_automatic_objects_still_obey_lifetime() {
    let zero = AccessLayout::new(0, 1);
    let (memory, destination) = memory()
        .allocate_static(10, zero, InitialState::Zeroed)
        .unwrap();
    let (memory, frame, scope) = memory.begin_frame().unwrap();
    let (memory, local) = memory.allocate_automatic(frame, scope, 20, zero).unwrap();
    memory.copy_bytes(destination, local, zero).unwrap();
    let ended = memory.end_frame(frame).unwrap();
    assert_eq!(
        ended.check_pointer(local).unwrap_err().kind(),
        MemoryFaultKind::LifetimeEnded
    );
    assert_eq!(
        ended
            .copy_bytes(destination, local, zero)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::LifetimeEnded
    );
}
