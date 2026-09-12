//! Indirect transfer retains ordinary local SSA and alternative effect edges.

use evocc_evoir::{
    interpret_function, interpret_module, parse_section9_module, render_module, validate_module,
    InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

const COUNTER: &str = include_str!("../../../tests/fixtures/computed-counter.evoir");

#[test]
fn indirect_edges_round_trip_and_bind_the_selected_objects_and_tokens() {
    let module = parse_section9_module(COUNTER.as_bytes()).expect("indirect edge syntax");
    validate_module(&module).unwrap();
    assert_eq!(render_module(&module), COUNTER);
    let args = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    let limits = InterpreterLimits::new(1_000);
    assert_eq!(
        interpret_module(&module, "computed_counter", &args, limits)
            .unwrap()
            .return_values[0]
            .raw_bits,
        3
    );
    assert_eq!(
        interpret_function(&module.functions[0], &args, limits)
            .unwrap()
            .return_values[0]
            .raw_bits,
        3
    );
}

#[test]
fn indirect_edges_do_not_weaken_local_use_def_or_token_binding() {
    parse_section9_module(COUNTER.as_bytes()).expect("positive control must parse first");
    for (from, to) in [
        ("^b1(%v26, %v27, %v28)", "^b1(%v2, %v27, %v28)"),
        ("^b1(%v26, %v27, %v28)", "^b1(%v26, %v27)"),
        ("^b1(%v26, %v27, %v28)", "^b1(%v27, %v26, %v28)"),
        ("^b1(%v26, %v27, %v28)", "^b1(%v26, %v27, %v27)"),
        (", ^b3(%v26, %v27, %v28)", ""),
        ("^b3(%v26, %v27, %v28)]", "^b1(%v26, %v27, %v28)]"),
        ("br.indirect %v25", "br.indirect %v27"),
    ] {
        let invalid = COUNTER.replace(from, to);
        assert_ne!(invalid, COUNTER, "mutation must change its input");
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn a_null_indirect_target_is_a_branch_error_not_a_call_error() {
    let source = COUNTER.replace(
        "%v24 = block_addr ^b3 : ptr<generic>",
        "%v24 = ptr.null : ptr<generic>",
    );
    // Retain the second address-taken destination independently of the bad
    // runtime target. The failure must not be an incomplete-CFG diagnostic.
    let source = source.replace(
        "    br.indirect %v25",
        "    %v36 = block_addr ^b3 : ptr<generic>\n    br.indirect %v25",
    );
    let module = parse_section9_module(source.as_bytes()).expect("well-formed null branch");
    let args = [
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    assert_eq!(
        interpret_module(
            &module,
            "computed_counter",
            &args,
            InterpreterLimits::new(1_000)
        )
        .unwrap_err()
        .code(),
        "E3048"
    );
}
