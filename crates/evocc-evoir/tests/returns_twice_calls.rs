//! EvoIR 3.10, "Calls that return twice" (revision 2.12.0).
//!
//! A direct call can carry the mark `call.returns_twice`: the callee can
//! resume the call a second time, as `setjmp` does after a `longjmp`. The
//! parser records the mark and the renderer writes it back. An indirect call
//! cannot carry it. The call takes the top effect summary, and validation
//! compares the recorded summary of its caller with that. EvoIR never
//! executes a second return, so the interpreter runs the call once.

use evocc_evoir::{
    interpret_module, parse_pre_fenv_module, parse_section9_module, render_module, Inst,
    InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

const TOP: &str = "    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, fenv=true, may_trap=true, may_unwind=true }\n";
const EMPTY: &str = "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n";

/// An external `setjmp` and two callers: one call returns twice, one is
/// ordinary.
fn external_unit() -> String {
    format!(
        "\
declare @setjmp(ptr<generic>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default

func @twice(%arg0: ptr<generic>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
{TOP}{{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call.returns_twice @setjmp(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}}

func @once(%arg0: ptr<generic>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
{TOP}{{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @setjmp(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}}
"
    )
}

/// A module function with an empty summary and a caller with `caller`
/// as its recorded summary, which calls it with `call`.
fn defined_unit(call: &str, caller: &str) -> String {
    format!(
        "\
func @callee(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
{EMPTY}{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 7
    ret %v2, %v0, %v1
}}

func @caller(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
{caller}{{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = {call} @callee(%v0, %v1)
    ret %v2, %v3, %v4
}}
"
    )
}

fn marks(text: &str) -> Vec<bool> {
    parse_section9_module(text.as_bytes())
        .expect("the unit parses")
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.insts)
        .filter_map(|inst| match inst {
            Inst::Call { returns_twice, .. } => Some(*returns_twice),
            _ => None,
        })
        .collect()
}

#[test]
fn the_mark_is_recorded_and_written_back() {
    let text = external_unit();
    assert_eq!(marks(&text), [true, false]);
    let module = parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(
        render_module(&module),
        text,
        "the round trip is the identity"
    );
}

#[test]
fn only_a_direct_call_spells_the_mark() {
    let text = external_unit();
    for (from, to) in [
        // An indirect call is an ordinary call.
        (
            "    (%v3, %v4, %v5) = call.returns_twice @setjmp(%v0, %v1, %v2)\n",
            "    %v6 = func_addr @setjmp : fnptr<(ptr<generic>, mem<all>, world)->(bits<32>, mem<all>, world)>\n    (%v3, %v4, %v5) = call.returns_twice %v6(%v0, %v1, %v2)\n",
        ),
        (
            "call.returns_twice @setjmp(",
            "call.returns_twice setjmp(",
        ),
        ("call.returns_twice @setjmp(", "call.returnstwice @setjmp("),
        ("call.returns_twice @setjmp(", "call.returns_twice2 @setjmp("),
        ("call.returns_twice @setjmp(", "call .returns_twice @setjmp("),
        ("call.returns_twice @setjmp(", "call.returns_twice  @setjmp("),
        ("call.returns_twice @setjmp(", "call @setjmp.returns_twice("),
    ] {
        assert_eq!(text.matches(from).count(), 1, "{from}");
        let changed = text.replacen(from, to, 1);
        let error = parse_section9_module(changed.as_bytes())
            .expect_err(&format!("'{to}' is not a canonical returns-twice call"));
        if to.contains("%v6(") {
            assert!(error.contains("must name its callee as @symbol"), "{error}");
        }
    }
}

#[test]
fn a_returns_twice_call_takes_the_top_summary() {
    // An ordinary call of an effect-free module function adds no effect.
    assert!(parse_section9_module(defined_unit("call", EMPTY).as_bytes()).is_ok());
    assert!(parse_section9_module(defined_unit("call", TOP).as_bytes()).is_err());
    // A second return resumes the caller after code that the call does not
    // bound, so the caller's summary is the top summary.
    let error = parse_section9_module(defined_unit("call.returns_twice", EMPTY).as_bytes())
        .expect_err("a weaker recorded summary is refused");
    assert!(error.contains("recorded effect summary"), "{error}");
    let text = defined_unit("call.returns_twice", TOP);
    assert_eq!(marks(&text), [true]);
}

#[test]
fn the_interpreter_runs_the_call_once() {
    let module = parse_section9_module(defined_unit("call.returns_twice", TOP).as_bytes()).unwrap();
    let outcome = interpret_module(
        &module,
        "caller",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(1000),
    )
    .expect("the module runs");
    assert_eq!(outcome.return_values[0].raw_bits, 7);
}

#[test]
fn the_pre_fenv_text_has_no_returns_twice_call() {
    let legacy = defined_unit("call.returns_twice", TOP)
        .replace(", fenv=true", "")
        .replace(", fenv=false", "");
    let error = parse_pre_fenv_module(legacy.as_bytes()).expect_err("the archived grammar");
    assert!(error.contains("returns-twice"), "{error}");
}
