use evocc_memory::{WorldLimits, WorldState};
use evocc_types::floating::{FpRounding, FpStatus};

#[test]
fn fp_world_snapshots_do_not_leak_between_logical_threads_or_into_memory_events() {
    let original = WorldState::new(WorldLimits {
        max_volatile_events: 0,
    });
    let mut environment = original.fp_environment();
    environment.rounding = FpRounding::Upward;
    let up = original.with_fp_environment(environment.raised(FpStatus::INEXACT));
    environment.rounding = FpRounding::Downward;
    let down = original.with_fp_environment(environment.raised(FpStatus::INVALID));
    assert_eq!(original.fp_environment().rounding, FpRounding::NearestEven);
    assert_eq!(original.fp_environment().flags, FpStatus::NONE);
    assert_eq!(up.fp_environment().rounding, FpRounding::Upward);
    assert_eq!(up.fp_environment().flags, FpStatus::INEXACT);
    assert_eq!(down.fp_environment().flags, FpStatus::INVALID);
    assert!(up.volatile_trace().is_empty() && up.atomic_trace().is_empty());
    assert_ne!(up, down);
}
