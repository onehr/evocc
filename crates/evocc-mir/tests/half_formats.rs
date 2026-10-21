//! The half formats in EvoMIR (Sections 2.6, 4.1 and 6.3). A half value is
//! its 16 bits in a `gpr32` register and crosses a call in the `h` view of a
//! vector register. Binary16 converts with FCVT; bfloat16 widens by a shift
//! and narrows by an integer rounding sequence. The fixture is the EvoIR
//! module of the EvoIR half-format test.

use evocc_evoir::parse_section9_module;
use evocc_mir::{
    compile_module, interpret_module, lower_module, parse_module, render_module, MInst, MirLimits,
    MirValue, RegClass,
};

const MODULE: &str = include_str!("../../../tests/fixtures/floating-half-conversions.evoir");

/// The low 64 bits of a general or SIMD register value.
fn low_bits(value: &MirValue) -> u64 {
    match value {
        MirValue::Int(bits) => *bits,
        MirValue::Vector(representation) => representation
            .bytes()
            .iter()
            .take(8)
            .rev()
            .fold(0, |bits, byte| bits << 8 | u64::from(*byte)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn half_conversions_lower_and_round_trip_through_the_text() {
    let ir = parse_section9_module(MODULE.as_bytes()).unwrap();
    let machine = lower_module(&ir).unwrap();
    let text = render_module(&machine);
    assert_eq!(parse_module(&text).unwrap(), machine);
    for form in [
        "fp.convert.f16",
        "fp.convert.bf16",
        "fp.strict.convert.bf16",
    ] {
        assert!(text.contains(form), "{form}\n{text}");
    }
    // A half conversion names its half format; the other side is the class
    // of the other register. A half value in a SIMD class is malformed.
    let mut forged = machine.clone();
    let mut changed = false;
    for function in &mut forged.functions {
        for block in &mut function.blocks {
            for inst in &mut block.insts {
                if let MInst::Floating { kind, dst, .. } = inst {
                    if kind.is_half() && dst.class == RegClass::Gpr32 {
                        dst.class = RegClass::Simd64;
                        changed = true;
                    }
                }
            }
        }
    }
    assert!(changed);
    assert!(evocc_mir::validate_module(&forged).is_err());
}

/// The machine interpreter gives the bits of one IEEE rounding. A half
/// result is the low 16 bits of its register.
#[test]
fn the_machine_interpreter_rounds_each_conversion_once() {
    let ir = parse_section9_module(MODULE.as_bytes()).unwrap();
    let machine = lower_module(&ir).unwrap();
    for (function, input, expected, half) in [
        ("widen16", 0x7d01, 0x7fe02000, false),
        ("widen16", 0x0001, 0x33800000, false),
        ("widenb", 0xff81, 0xffc10000, false),
        ("narrow16", 0x3f801000, 0x3c00, true),
        ("narrow16", 0x3f803000, 0x3c02, true),
        ("narrow16", 0x477ff000, 0x7c00, true),
        ("narrowb", 0x3f808000, 0x3f80, true),
        ("narrowb", 0x3f808001, 0x3f81, true),
        ("narrowb", 0xffc12345, 0xffc1, true),
        ("narrowb64", 0x3ff0100000400000, 0x3f81, true),
        ("from_int", 65520, 0x7c00, true),
        ("from_int", 2049, 0x6800, true),
        ("from_uint", 0x01010001, 0x4b81, true),
        ("cross", 0x7e11, 0x7fc2, true),
        ("cross", 0x3c01, 0x3f80, true),
    ] {
        let out = interpret_module(
            &machine,
            function,
            &[MirValue::Int(input)],
            MirLimits::new(1000),
        )
        .unwrap_or_else(|error| panic!("{function} {input:#x}: {error:?}"));
        let bits = low_bits(&out.results[0]);
        let bits = if half {
            bits & 0xffff
        } else {
            bits & 0xffff_ffff
        };
        assert_eq!(bits, expected, "{function} {input:#x}");
    }
    let out = interpret_module(&machine, "signaling", &[], MirLimits::new(1000)).unwrap();
    let low = out
        .results
        .iter()
        .take(3)
        .map(|value| low_bits(value) & 0xffff)
        .collect::<Vec<_>>();
    assert_eq!(low, [0x7d01, 0xff81, 0x7d01]);
}

/// Native code converts binary16 through the `h` view of v31, quiets a
/// bfloat16 signaling NaN with FMAX, narrows a binary64 value with FCVTXN,
/// and reads the strict rounding direction from FPCR.
#[test]
fn emission_uses_the_h_view_and_the_bfloat16_sequences() {
    let ir = parse_section9_module(MODULE.as_bytes()).unwrap();
    let assembly = compile_module(&ir).unwrap();
    for fragment in [
        "fcvt s0, h31",
        "fcvt h31, s0",
        "fmax s0, s0, s0",
        "fcvtxn s31, d0",
        "mov w17, #0x7fff",
        "mrs x17, fpcr",
        "msr fpsr, x17",
    ] {
        assert!(assembly.contains(fragment), "{fragment}\n{assembly}");
    }
}
