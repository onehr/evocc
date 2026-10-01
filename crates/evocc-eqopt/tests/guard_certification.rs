//! The g0 slice is separate from P0 and preserves every obligation.
use evocc_eqopt::{optimize_scalar_module_with_policy, policy_by_name, ExtractionPolicy};
use evocc_evoir::{parse_section9_module, render_module, Inst};

fn input() -> evocc_evoir::Module {
    parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-size/guard-attribution.evo"
    ))
    .unwrap()
}

#[test]
fn guard_registry_replaces_only_the_proved_root() {
    let before = input();
    let policy = policy_by_name("guard-known-true").expect("approved guard registry policy");
    let outcome = optimize_scalar_module_with_policy(&before, &policy);
    assert_eq!(outcome.certificate.registry_name, "g0.ub.v1");
    assert_eq!(outcome.certificate.regions.len(), 1);
    let mut expected = before.clone();
    expected.functions[0].blocks[0].insts[1] = Inst::Const(
        evocc_evoir::Value {
            id: 4,
            ty: evocc_types::EvoIRType::Bits(1),
        },
        1,
    );
    assert_eq!(outcome.module, expected);
    assert_eq!(outcome.report.optimized_values, 1);
    assert_eq!(
        outcome.module.functions[0].effects,
        before.functions[0].effects
    );
}

#[test]
fn old_p0_does_not_admit_guard_roots() {
    let before = input();
    let outcome = optimize_scalar_module_with_policy(&before, &ExtractionPolicy::default());
    assert_eq!(render_module(&outcome.module), render_module(&before));
    assert!(outcome.certificate.regions.is_empty());
}
