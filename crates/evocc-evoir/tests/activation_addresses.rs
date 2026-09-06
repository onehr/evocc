//! EvoIR Section 3.7, revision 1.9.72: the two activation addresses.
//!
//! "`return_address` yields the address the current activation will return to,
//! with code provenance like `block_addr`: equal to itself within one
//! activation, comparable, storable, `ptr.to_int`-able, and `E3013` when
//! dereferenced. `frame_address` yields the address of the current activation's
//! frame record ... with stack provenance of an object no load or store may
//! touch (`E3013`). Both are pure, consume no tokens, are not in the Section
//! 8.5 allowlist, and the interpreters give each activation a fresh value for
//! each."
//!
//! The facts below are exactly those: the shapes of Section 9 round trip, the
//! result type is the form's own `ptr<generic>`, two readings inside one
//! activation agree, two activations disagree, both addresses are non-zero and
//! unequal to each other, and a load through either is `E3013`.

use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

/// Both forms, with the pure uses Section 3.7 allows: comparison and
/// `ptr.to_int`.
const ROUND_TRIP: &str = "\
func @f(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = return_address : ptr<generic>
    %v3 = frame_address : ptr<generic>
    %v4 = ptr.to_int %v2
    %v5 = ptr.to_int %v3
    %v6 = add.bits<64> %v4, %v5
    ret %v6, %v0, %v1
}
";

/// One activation reads each address twice and a second activation reads its
/// own return address, so the freshness rule has something to disagree with.
const FRESHNESS: &str = "\
func @leaf(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = return_address : ptr<generic>
    %v3 = ptr.to_int %v2
    ret %v3, %v0, %v1
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<64>, bits<64>, bits<64>, bits<64>, bits<64>, bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = return_address : ptr<generic>
    %v3 = return_address : ptr<generic>
    %v4 = frame_address : ptr<generic>
    %v5 = frame_address : ptr<generic>
    %v6 = ptr.to_int %v2
    %v7 = ptr.to_int %v3
    %v8 = ptr.to_int %v4
    %v9 = ptr.to_int %v5
    %v10 = icmp.eq %v6, %v7
    %v11 = icmp.eq %v8, %v9
    %v12 = zext.bits<64> %v10
    %v13 = zext.bits<64> %v11
    (%v14, %v15, %v16) = call @leaf(%v0, %v1)
    (%v17, %v18, %v19) = call @leaf(%v15, %v16)
    ret %v12, %v13, %v6, %v8, %v14, %v17, %v18, %v19
}
";

fn interpret(text: &str) -> Vec<u64> {
    let module = parse_section9_module(text.as_bytes()).expect("the unit parses");
    let arguments = vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
        .expect("the execution is defined")
        .return_values
        .iter()
        .filter(|value| matches!(value.ty, EvoIRType::Bits(_)))
        .map(|value| value.raw_bits)
        .collect()
}

#[test]
fn both_forms_round_trip_through_section9() {
    let module = parse_section9_module(ROUND_TRIP.as_bytes()).expect("the unit parses");
    assert_eq!(render_module(&module), ROUND_TRIP);
}

#[test]
fn the_result_type_is_the_forms_own_generic_pointer() {
    for invalid in [
        ROUND_TRIP.replace(
            "return_address : ptr<generic>",
            "return_address : ptr<stack>",
        ),
        ROUND_TRIP.replace(
            "frame_address : ptr<generic>",
            "frame_address : ptr<global>",
        ),
        ROUND_TRIP.replace("return_address : ptr<generic>", "return_address : bits<64>"),
        ROUND_TRIP.replace("frame_address : ptr<generic>", "frame_address : bits<64>"),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "only ptr<generic> is the form's result type: {invalid}"
        );
    }
}

#[test]
fn neither_form_takes_an_operand() {
    for invalid in [
        ROUND_TRIP.replace(
            "return_address : ptr<generic>",
            "return_address %v0 : ptr<generic>",
        ),
        ROUND_TRIP.replace(
            "frame_address : ptr<generic>",
            "frame_address ^entry : ptr<generic>",
        ),
        ROUND_TRIP.replace("return_address : ptr<generic>", "return_address"),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "the form has no operand and no token: {invalid}"
        );
    }
}

#[test]
fn one_activation_agrees_with_itself_and_two_activations_do_not() {
    let results = interpret(FRESHNESS);
    let [return_twice, frame_twice, return_address, frame_address, leaf_first, leaf_second] =
        results.as_slice()
    else {
        panic!("six ordinary results, got {results:?}");
    };
    assert_eq!(
        *return_twice, 1,
        "two return_address in one activation agree"
    );
    assert_eq!(*frame_twice, 1, "two frame_address in one activation agree");
    assert_ne!(*return_address, 0, "the return address names an address");
    assert_ne!(*frame_address, 0, "the frame address names an address");
    assert_ne!(
        return_address, frame_address,
        "the two addresses of one activation are distinct"
    );
    assert_ne!(
        leaf_first, leaf_second,
        "each activation of the callee gets its own fresh value"
    );
    for callee in [leaf_first, leaf_second] {
        assert_ne!(
            callee, return_address,
            "and none of them is the caller's own"
        );
    }
}

/// A load through either address, which Section 3.7 makes `E3013`.
///
/// Section 3.8 makes every load consume the `access_in_bounds` token of its own
/// address, and Section 3.7 refuses that guard through a code or activation
/// address for the same reason it refuses the load: the value names no object.
fn dereference(mnemonic: &str) -> String {
    format!(
        "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects {{ reads={{all}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = {mnemonic} : ptr<generic>
    %v3 = access_in_bounds %v0, %v2 [size=4, align=4]
    (%v4, %v5, %v6) = load %v3, %v1, %v2 : bits<32>
    ret %v4, %v5, %v6
}}
"
    )
}

#[test]
fn a_memory_access_through_either_address_is_e3013() {
    for mnemonic in ["return_address", "frame_address"] {
        let text = dereference(mnemonic);
        let module = parse_section9_module(text.as_bytes()).expect("the unit parses");
        let arguments = vec![
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ];
        let error = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
            .expect_err("no load or store may touch an activation address");
        assert_eq!(
            error.code(),
            "E3013",
            "a dereference of {mnemonic} is E3013, not {error:?}"
        );
    }
}
