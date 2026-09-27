//! EvoIR 2.1, "Function linkage" (revision 1.9.70).
//!
//! "A function carries the same distinction, written after the calling
//! convention ... `visibility=internal` is written for a function whose C
//! definition has storage class `static` (including `static inline`); the
//! field is omitted for every other function, and its absence means external,
//! so every module text written before this revision is unchanged."
//!
//! The field is a property of the object, so the parser records it, the
//! renderer writes it back and the interpreter, which executes a module whole,
//! is indifferent to it.

use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, GlobalVisibility, InterpreterLimits,
    RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

/// One internal helper and one external function that calls it.
const UNIT: &str = "\
func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = add.bits<32> %v0, %v3
    ret %v4, %v1, %v2
}

func @caller(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @helper(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}
";

#[test]
fn the_field_is_recorded_and_written_back() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let helper = &module.functions[0];
    let caller = &module.functions[1];
    assert_eq!(helper.name, "helper");
    assert_eq!(helper.visibility, GlobalVisibility::Internal);
    assert_eq!(caller.name, "caller");
    assert_eq!(
        caller.visibility,
        GlobalVisibility::External,
        "an omitted field means external"
    );
    assert_eq!(
        render_module(&module),
        UNIT,
        "the round trip is the identity"
    );
}

#[test]
fn an_external_function_omits_the_field() {
    let external = UNIT.replace(
        " cc = target_default visibility=internal",
        " cc = target_default",
    );
    let module = parse_section9_module(external.as_bytes()).expect("the unit parses");
    assert!(
        module
            .functions
            .iter()
            .all(|function| function.visibility == GlobalVisibility::External),
        "every function of a text written before the revision is external"
    );
    let rendered = render_module(&module);
    assert!(
        !rendered.contains("visibility="),
        "and the canonical text of one is unchanged:\n{rendered}"
    );
    assert_eq!(rendered, external);
}

#[test]
fn noncanonical_visibility_spellings_are_refused() {
    for spelling in [
        " cc = target_default visibility=external",
        " cc = target_default visibility=Internal",
        " cc = target_default visibility = internal",
        " cc = target_default  visibility=internal",
        " cc = target_default visibility=internal extra",
    ] {
        let text = UNIT.replace(" cc = target_default visibility=internal", spelling);
        assert!(
            parse_section9_module(text.as_bytes()).is_err(),
            "'{spelling}' is not a canonical function header"
        );
    }
}

#[test]
fn the_interpreter_is_indifferent_to_the_field() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    for entry in ["caller", "helper"] {
        let result = interpret_module(
            &module,
            entry,
            &[
                RuntimeValue::new(EvoIRType::Bits(32), 41),
                RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            InterpreterLimits::new(1000),
        )
        .expect("the module runs");
        assert_eq!(result.return_values[0].raw_bits, 42);
    }
}
