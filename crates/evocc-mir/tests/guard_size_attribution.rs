//! A diagnostic lowering does not change source IR or production lowering.
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn production_omits_only_a_direct_canonical_true_guard() {
    let source = include_str!("../../../tests/fixtures/eqsat-size/guard-attribution.evo");
    for (replacement, expected) in [("const.bits<1> 1", 1), ("const.bits<1> 0", 2)] {
        let m = evocc_evoir::parse_section9_module(
            source
                .replace("signed_add_defined %v0, %v3", replacement)
                .as_bytes(),
        )
        .unwrap();
        evocc_evoir::validate_module(&m).unwrap();
        let ir = evocc_evoir::render_module(&m);
        let mir = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &m, &|_| false).unwrap();
        let count = mir.functions[0]
            .blocks
            .iter()
            .filter(|b| {
                matches!(
                    b.term,
                    evocc_mir::MTerm::Tb { .. } | evocc_mir::MTerm::BCond { .. }
                )
            })
            .count();
        assert_eq!(count, expected);
        assert_eq!(
            evocc_evoir::render_module(&m),
            ir,
            "MIR may not delete the IR obligation"
        );
    }
}

#[test]
fn diagnostic_retains_other_predicate_consumers_and_the_unproved_guard() {
    let m = evocc_evoir::parse_section9_module(include_bytes!(
        "../../../tests/fixtures/eqsat-size/guard-attribution.evo"
    ))
    .unwrap();
    evocc_evoir::validate_module(&m).unwrap();
    let before = evocc_evoir::render_module(&m);
    let facts = BTreeMap::from([("main".to_owned(), BTreeSet::from([4]))]);
    let ordinary = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &m, &|_| false).unwrap();
    let diagnostic = evocc_mir::lower::lower_module_for_guard_attribution(
        evocc_mir::Target::Linux,
        &m,
        &|_| false,
        &facts,
    )
    .unwrap();
    evocc_mir::validate_module(&diagnostic).unwrap();
    assert_eq!(evocc_evoir::render_module(&m), before);
    let guards = |m: &evocc_mir::MModule| {
        m.functions[0]
            .blocks
            .iter()
            .filter(|b| {
                matches!(
                    b.term,
                    evocc_mir::MTerm::Tb { .. } | evocc_mir::MTerm::BCond { .. }
                )
            })
            .count()
    };
    assert_eq!(guards(&ordinary), 2);
    assert_eq!(guards(&diagnostic), 1);
    assert!(diagnostic.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, evocc_mir::MInst::MovImm { imm: 1, .. })));
    assert_eq!(
        ordinary,
        evocc_mir::lower_module_for(evocc_mir::Target::Linux, &m, &|_| false).unwrap()
    );
}
