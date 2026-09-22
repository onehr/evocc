use evocc_memory::{
    AccessLayout, InitialState, MemoryConfig, MemoryFaultKind, MemoryLimits, MemoryState,
    ObjectOrigin, ThreadId,
};
use evocc_types::TargetDataLayout;

fn memory() -> MemoryState {
    MemoryState::new(
        MemoryConfig::for_target(
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            MemoryLimits {
                max_objects: 16,
                max_object_bytes: 64,
                max_total_bytes: 1024,
            },
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn thread_instances_escape_without_rebinding_and_end_with_their_owner() {
    let word = AccessLayout::new(4, 4);
    let before = memory();
    let (m, a, fresh) = before
        .allocate_thread_local(5, word, InitialState::Zeroed)
        .unwrap();
    assert!(fresh);
    let m = m
        .store_integer(a, word, 11)
        .unwrap()
        .begin_thread(ThreadId(1))
        .unwrap()
        .select_thread(ThreadId(1))
        .unwrap();
    let (m, b, fresh) = m
        .allocate_thread_local(5, word, InitialState::Zeroed)
        .unwrap();
    assert!(fresh);
    assert_ne!(a, b);
    assert_eq!(
        m.load_integer(a, word).unwrap(),
        11,
        "an escaped pointer still addresses its owner's instance"
    );
    assert_eq!(m.load_integer(b, word).unwrap(), 0);
    let m = m
        .store_integer(a, word, 12)
        .unwrap()
        .store_integer(b, word, 21)
        .unwrap();
    let (m, same_b, fresh) = m
        .allocate_thread_local(5, word, InitialState::Zeroed)
        .unwrap();
    assert!(!fresh);
    assert_eq!(same_b, b);
    assert_eq!(
        m.load_integer(b, word).unwrap(),
        21,
        "acquisition never reinitializes an instance"
    );
    let live = m.clone();
    let m = m.end_thread(ThreadId(0)).unwrap();
    assert_eq!(
        m.load_integer(a, word).unwrap_err().kind(),
        MemoryFaultKind::LifetimeEnded
    );
    assert_eq!(m.load_integer(b, word).unwrap(), 21);
    assert_eq!(
        live.load_integer(a, word).unwrap(),
        12,
        "snapshots retain their own lifetime state"
    );
    assert!(
        m.begin_thread(ThreadId(0)).is_err(),
        "logical identities cannot be recycled"
    );
    let (_, old_a, _) = before
        .allocate_thread_local(5, word, InitialState::Zeroed)
        .unwrap();
    assert_eq!(old_a, a);
}

#[test]
fn readonly_tls_remains_thread_owned_and_layout_is_stable() {
    let word = AccessLayout::new(4, 4);
    let (m, p, _) = memory()
        .allocate_thread_local(3, word, InitialState::Zeroed)
        .unwrap();
    let m = m.freeze_static(p).unwrap();
    assert_eq!(
        m.store_integer(p, word, 1).unwrap_err().kind(),
        MemoryFaultKind::ReadOnlyObject
    );
    assert!(m
        .allocate_thread_local(3, AccessLayout::new(8, 8), InitialState::Zeroed)
        .is_err());
    assert_eq!(
        m.end_thread(ThreadId(0))
            .unwrap()
            .load_integer(p, word)
            .unwrap_err()
            .kind(),
        MemoryFaultKind::LifetimeEnded
    );
    assert_eq!(
        m.object_info(p.object()).unwrap().origin,
        ObjectOrigin {
            site: 3,
            instance: 0,
            thread: Some(ThreadId(0))
        }
    );
}

#[test]
fn retired_thread_identity_is_not_reused_even_without_tls() {
    let m = memory()
        .begin_thread(ThreadId(8))
        .unwrap()
        .end_thread(ThreadId(8))
        .unwrap();
    assert!(m.begin_thread(ThreadId(8)).is_err());
}
