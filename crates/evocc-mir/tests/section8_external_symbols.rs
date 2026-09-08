//! EvoMIR Section 8, "External symbols" (revision 0.23).
//!
//! Calls retain the platform's symbol spelling. Undefined object addresses
//! use the GOT so the same object file links both statically and in a PIE.

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// One unit that calls a function it does not define and reads an object it
/// does not define.
const UNIT: &str = r#"extern @table_value size=4 align=4

declare @helper(bits<32>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default

func @caller(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @helper(%v0, %v1, %v2)
    %v6 = global_addr @table_value : ptr<generic>
    %v7 = access_in_bounds %v4, %v6 [size=4, align=4]
    (%v8, %v9, %v10) = load %v7, %v5, %v6 : bits<32>
    %v12 = add.bits<32> %v3, %v8
    ret %v12, %v9, %v10
}
"#;

/// A call to a `declare` whose narrow argument the platform rule extends.
const NARROW: &str = r#"declare @take(bits<8>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default

func @pass(%arg0: bits<8>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }
{
^entry(%v0: bits<8>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @take(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}
"#;

fn emitted(target: Target, text: &str) -> String {
    let module = parse_section9_module(text.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    }
}

#[test]
fn an_undefined_object_uses_the_got_and_calls_keep_their_spelling_on_darwin() {
    let text = emitted(Target::Darwin, UNIT);
    assert!(
        text.contains("bl _helper"),
        "the call names the undefined symbol with the platform prefix:\n{text}"
    );
    assert!(
        text.contains("_table_value@GOTPAGE") && text.contains("_table_value@GOTPAGEOFF"),
        "the object address is loaded through the GOT:\n{text}"
    );
    assert!(
        !text.contains("_helper:"),
        "an undefined symbol has no definition:\n{text}"
    );
    assert!(
        !text.contains("_table_value:"),
        "an undefined object has no definition:\n{text}"
    );
    assert!(
        !text.contains(".globl _helper") && !text.contains(".globl _table_value"),
        "an undefined symbol is not declared global:\n{text}"
    );
}

#[test]
fn an_undefined_object_uses_the_got_and_calls_keep_their_spelling_on_linux() {
    let text = emitted(Target::Linux, UNIT);
    assert!(
        text.contains("bl helper"),
        "the call names the undefined symbol:\n{text}"
    );
    assert!(
        text.contains(":got:table_value") && text.contains("#:got_lo12:table_value"),
        "the object address is loaded through the GOT:\n{text}"
    );
    assert!(
        !text.contains("helper:") && !text.contains("table_value:"),
        "an undefined symbol has no definition:\n{text}"
    );
}

#[test]
fn a_call_to_a_declaration_is_cross_boundary() {
    // Section 6: "Across the host boundary the platform rule governs instead:
    // a caller extends an argument narrower than 32 bits to 32 bits". Section
    // 8 makes every call to a `declare` such a call.
    let text = emitted(Target::Darwin, NARROW);
    assert!(
        text.contains("uxtb"),
        "the narrow argument is extended before the call:\n{text}"
    );
}
