//! Section 10: every EvoIR form this revision does not lower is refused at the
//! instruction with its code, and the function then produces no EvoMIR.

use evocc_evoir::parse_section9_module;
use evocc_mir::lower_module;

fn codes(text: &str) -> Vec<&'static str> {
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let refusals = lower_module(&module).expect_err("this module is not lowerable");
    let mut codes: Vec<&'static str> = refusals.iter().map(|refusal| refusal.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

const WIDE_INTEGER: &str = "\
func @main(%arg0: bits<96>, %arg1: mem<all>, %arg2: world) -> (bits<96>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<96>, %v1: mem<all>, %v2: world):
    %v3 = add.bits<96> %v0, %v0
    ret %v3, %v1, %v2
}
";

#[test]
fn a_non_pair_ninety_six_bit_integer_is_e4003() {
    assert_eq!(codes(WIDE_INTEGER), vec!["E4003"]);
}

const PREDICATE: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = divisor_nonzero %v0
    ret %v3, %v1, %v2
}
";

#[test]
fn every_ub_predicate_now_has_a_row_and_is_not_refused() {
    // Revision 0.2 gives each predicate a lowering row, and they "lower
    // whether or not a `ub.assume` consumes them".
    let module = parse_section9_module(PREDICATE.as_bytes()).expect("canonical EvoIR");
    assert!(lower_module(&module).is_ok());
}

const EIGHT_RESULTS: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    ret %v0, %v0, %v0, %v0, %v0, %v0, %v0, %v0, %v1, %v2
}
";

const NINE_RESULTS: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    ret %v0, %v0, %v0, %v0, %v0, %v0, %v0, %v0, %v0, %v1, %v2
}
";

#[test]
fn eight_ordinary_results_are_accepted() {
    // Section 6: eight results go in x0 to x7.
    let module = parse_section9_module(EIGHT_RESULTS.as_bytes()).expect("canonical EvoIR");
    assert!(lower_module(&module).is_ok());
}

#[test]
fn a_ninth_ordinary_result_is_e4013() {
    assert_eq!(codes(NINE_RESULTS), vec!["E4013"]);
}

const NINE_ARGUMENTS: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>, %arg3: bits<32>, %arg4: bits<32>, %arg5: bits<32>, %arg6: bits<32>, %arg7: bits<32>, %arg8: bits<32>, %arg9: mem<all>, %arg10: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>, %v3: bits<32>, %v4: bits<32>, %v5: bits<32>, %v6: bits<32>, %v7: bits<32>, %v8: bits<32>, %v9: mem<all>, %v10: world):
    ret %v0, %v9, %v10
}
";

#[test]
fn a_ninth_integer_argument_is_e4010() {
    assert_eq!(codes(NINE_ARGUMENTS), vec!["E4010"]);
}

const FLOATING_POINT: &str = "\
func @main(%arg0: f64, %arg1: mem<all>, %arg2: world) -> (f64, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: f64, %v1: mem<all>, %v2: world):
    ret %v0, %v1, %v2
}
";

#[test]
fn supported_scalar_floating_identity_uses_the_simd_bank() {
    let module = parse_section9_module(FLOATING_POINT.as_bytes()).unwrap();
    let module = evocc_mir::lower_module(&module).unwrap();
    assert_eq!(
        module.functions[0].blocks[0].args[0].class,
        evocc_mir::RegClass::Simd64
    );
}

const POINTER_IN_STATIC_DATA: &str = "\
global @table origin=0 kind=file visibility=external size=8 align=8 bytes=0000000000000000 initialized=11111111

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    ret %v0, %v1, %v2
}
";

#[test]
fn ordinary_scalar_static_data_is_not_refused() {
    // Plain bytes lower. Pointer relocations also lower under the current
    // profile; their separate checks live in section8_relocations.rs.
    let module = parse_section9_module(POINTER_IN_STATIC_DATA.as_bytes()).expect("canonical EvoIR");
    assert!(lower_module(&module).is_ok());
}

#[test]
fn a_refusal_names_the_instruction_it_was_reported_at() {
    let module = parse_section9_module(WIDE_INTEGER.as_bytes()).expect("canonical EvoIR");
    let refusals = lower_module(&module).expect_err("refused");
    assert_eq!(refusals[0].function, "main");
    assert_eq!(refusals[0].name, "not-lowered: wide-integer");
}
