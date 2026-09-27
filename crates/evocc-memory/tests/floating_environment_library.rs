use evocc_memory::{
    AccessLayout, FenvRuntime, InitialState, LibraryValue, MemoryConfig, MemoryFaultKind,
    MemoryLimits, MemoryState, Pointer, WorldLimits, WorldState,
};
use evocc_types::{
    floating::{FpEnvironment, FpRounding, FpStatus},
    TargetDataLayout,
};

fn states() -> (MemoryState, WorldState) {
    (
        MemoryState::new(
            MemoryConfig::for_target(
                &TargetDataLayout::aarch64_unknown_linux_gnu(),
                MemoryLimits {
                    max_objects: 8,
                    max_object_bytes: 64,
                    max_total_bytes: 128,
                },
            )
            .unwrap(),
        )
        .unwrap(),
        WorldState::new(WorldLimits {
            max_volatile_events: 0,
        }),
    )
}

#[test]
fn runtime_raise_policy_is_explicit_and_world_snapshots_are_independent() {
    let (memory, world) = states();
    for bits in 0..32 {
        for (runtime, expected) in [
            (FenvRuntime::Aarch64Musl, bits),
            (
                FenvRuntime::Aarch64Glibc,
                bits | if bits & 12 != 0 { 16 } else { 0 },
            ),
        ] {
            let (_, next, _) = memory
                .call_library_with_world(
                    &world,
                    runtime,
                    "feraiseexcept",
                    &[LibraryValue::Integer(bits)],
                )
                .unwrap();
            assert_eq!(u64::from(next.fp_environment().flags.bits()), expected);
            assert_eq!(world.fp_environment(), FpEnvironment::default());
            assert!(next.atomic_trace().is_empty() && next.volatile_trace().is_empty());
        }
    }
    let (_, up, _) = memory
        .call_library_with_world(
            &world,
            FenvRuntime::Aarch64Glibc,
            "fesetround",
            &[LibraryValue::Integer(0x400000)],
        )
        .unwrap();
    let (_, down, _) = memory
        .call_library_with_world(
            &world,
            FenvRuntime::Aarch64Glibc,
            "fesetround",
            &[LibraryValue::Integer(0x800000)],
        )
        .unwrap();
    assert_eq!(up.fp_environment().rounding, FpRounding::Upward);
    assert_eq!(down.fp_environment().rounding, FpRounding::Downward);
    assert_eq!(
        up.check_fixed_fp_environment()
            .unwrap_err()
            .runtime_diagnostic()
            .unwrap()
            .code,
        "E3064"
    );
}

#[test]
fn fenv_memory_keeps_lifetime_initializedness_and_control_boundaries() {
    let (memory, world) = states();
    let (memory, address) = memory
        .allocate_static(1, AccessLayout::new(16, 4), InitialState::Uninitialized)
        .unwrap();
    let get = |memory: &MemoryState, name: &str, address: Pointer| {
        memory.call_library_with_world(
            &world,
            FenvRuntime::Aarch64Glibc,
            name,
            &[LibraryValue::Pointer(address)],
        )
    };
    assert_eq!(
        get(&memory, "fesetenv", address).unwrap_err().kind(),
        MemoryFaultKind::UninitializedRead
    );
    let (saved, _, _) = get(&memory, "fegetenv", address).unwrap();
    assert_eq!(
        saved
            .load_integer(address, AccessLayout::new(8, 4))
            .unwrap(),
        0
    );
    for bad in [1, 1 << 8, 1 << 24, 1 << 25, 1 << 63] {
        let forged = saved
            .store_integer(address, AccessLayout::new(8, 4), bad)
            .unwrap();
        assert_eq!(
            get(&forged, "fesetenv", address)
                .unwrap_err()
                .runtime_diagnostic()
                .unwrap()
                .code,
            "E3064"
        );
    }
    assert_eq!(
        get(&saved, "fegetenv", address.checked_offset(1).unwrap())
            .unwrap_err()
            .kind(),
        MemoryFaultKind::Misaligned
    );
    let (_, reset, _) = get(&saved, "fesetenv", Pointer::from_address(u64::MAX)).unwrap();
    assert_eq!(reset.fp_environment(), FpEnvironment::default());
    assert_eq!(
        get(&saved, "fesetenv", Pointer::NULL).unwrap_err().kind(),
        MemoryFaultKind::NullDereference
    );
    let with_flags = world.with_fp_environment(FpEnvironment::default().raised(FpStatus::OVERFLOW));
    let (_, updated, _) = saved
        .call_library_with_world(
            &with_flags,
            FenvRuntime::Aarch64Glibc,
            "feupdateenv",
            &[LibraryValue::Pointer(address)],
        )
        .unwrap();
    assert_eq!(
        updated.fp_environment().flags,
        FpStatus::OVERFLOW.union(FpStatus::INEXACT)
    );
}
