//! S4 attribution is not a certificate-producing rewrite.
use evocc_eqopt::guard_size_attribution;

#[test]
fn retained_true_guard_is_identified_without_mutating_ir_or_admitting_the_unknown_guard() {
    let m = evocc_evoir::parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-size/guard-attribution.evo"
    ))
    .unwrap();
    evocc_evoir::validate_module(&m).unwrap();
    let before = evocc_evoir::render_module(&m);
    let facts = guard_size_attribution(&m.functions[0]);
    assert_eq!(facts.true_predicates.into_iter().collect::<Vec<_>>(), [4]);
    assert_eq!(facts.total_predicates, 2);
    assert_eq!(facts.removable_assumptions, 1);
    assert_eq!(evocc_evoir::render_module(&m), before);
}

#[test]
fn a_poisoned_operand_cannot_discharge_even_add_zero() {
    let m = evocc_evoir::parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-size/guard-poison.evo"
    ))
    .unwrap();
    evocc_evoir::validate_module(&m).unwrap();
    let facts = guard_size_attribution(&m.functions[0]);
    assert!(facts.true_predicates.is_empty());
    assert_eq!(facts.removable_assumptions, 0);
}
