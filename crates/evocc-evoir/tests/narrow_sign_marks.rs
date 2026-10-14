//! EvoIR 10.1.3, "Narrow integers across calls on Darwin" (revision 2.4.0).
//!
//! On `aarch64-apple-darwin` a parameter whose C type is a signed integer
//! narrower than 32 bits carries `sext`, in the function header, in every
//! `declare` and in every `fnptr<Sig>`. The mark is part of the signature for
//! equality and marks only a `bits<N>` with `1 < N < 32`. A caller extends
//! such an argument by its sign (EvoMIR Section 6.2); the interpreters read
//! nothing from the mark.

use evocc_evoir::{
    interpret_module, parse_section9_module, parse_target_module, render_module,
    render_target_module, InterpreterLimits, Module, ParamMark, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

const MARKED: &str = "\
declare @take(bits<8> sext, bits<16> sext, bits<8>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default

func @give(%arg0: bits<8> sext, %arg1: bits<16> sext, %arg2: bits<8>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<8>, %v1: bits<16>, %v2: bits<8>, %v3: mem<all>, %v4: world):
    %v5 = sext.bits<32> %v0
    ret %v5, %v3, %v4
}
";

#[test]
fn the_sign_mark_parses_renders_and_rides_in_the_signature() {
    let module = parse_section9_module(MARKED.as_bytes()).expect("marked text parses");
    assert_eq!(render_module(&module), MARKED);
    let marks = [
        ParamMark::SignExtend,
        ParamMark::SignExtend,
        ParamMark::None,
    ];
    assert_eq!(module.functions[0].param_marks[..3], marks);
    assert_eq!(module.declares[0].param_marks[..3], marks);
    // The interpreter executes the value, not the mark.
    let outcome = interpret_module(
        &module,
        "give",
        &[
            RuntimeValue::new(EvoIRType::Bits(8), 0xff),
            RuntimeValue::new(EvoIRType::Bits(16), 0x7fff),
            RuntimeValue::new(EvoIRType::Bits(8), 1),
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(64),
    )
    .expect("give runs");
    assert_eq!(
        outcome.return_values[0],
        RuntimeValue::new(EvoIRType::Bits(32), 0xffff_ffff)
    );
}

#[test]
fn the_sign_mark_marks_only_a_narrow_integer() {
    let declaration = |ty: &str| {
        format!("declare @f({ty} sext, mem<all>, world) -> (mem<all>, world) cc = target_default\n")
    };
    for ty in ["bits<8>", "bits<16>"] {
        parse_section9_module(declaration(ty).as_bytes())
            .unwrap_or_else(|error| panic!("{ty}: {error}"));
    }
    for ty in ["bits<1>", "bits<32>", "bits<64>", "ptr<generic>", "f32"] {
        assert!(
            parse_section9_module(declaration(ty).as_bytes()).is_err(),
            "a sign mark on {ty} must be refused"
        );
    }
}

#[test]
fn a_call_site_carries_no_sign_mark() {
    let text = "\
declare @vf(bits<32>, ..., mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default

func @call(%arg0: bits<8>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=true, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<8>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    (%v4, %v5, %v6) = call @vf(%v3, %v0 sext, %v1, %v2)
    ret %v4, %v5, %v6
}
";
    assert!(parse_section9_module(text.as_bytes()).is_err());
}

#[test]
fn darwin_text_names_its_target_and_round_trips() {
    let module = parse_section9_module(MARKED.as_bytes()).unwrap();
    let darwin = render_target_module(&module, "aarch64-apple-darwin");
    assert!(darwin.starts_with("target aarch64-apple-darwin\n\ndeclare @take("));
    assert_eq!(
        parse_target_module(darwin.as_bytes(), "aarch64-apple-darwin").unwrap(),
        module
    );
    // The Linux reader and a reader for another target refuse it.
    assert!(parse_section9_module(darwin.as_bytes()).is_err());
    assert!(parse_target_module(darwin.as_bytes(), "aarch64-unknown-linux-gnu").is_err());
    assert!(parse_target_module(MARKED.as_bytes(), "aarch64-apple-darwin").is_err());
    // The implicit target has no target line.
    assert_eq!(
        render_target_module(&module, "aarch64-unknown-linux-gnu"),
        MARKED
    );
    let empty = render_target_module(&Module::default(), "aarch64-apple-darwin");
    assert_eq!(empty, "target aarch64-apple-darwin\n");
    assert_eq!(
        parse_target_module(empty.as_bytes(), "aarch64-apple-darwin").unwrap(),
        Module::default()
    );
}
