//! The half storage formats in EvoIR (3.13.6): `f16` (binary16) and `bf16`
//! (bfloat16). A half value is an exact 16-bit representation; the only
//! numeric operations on it are conversions, which round once. The fixture
//! is the lowering of small C conversions, strict forms included, and one
//! function of half constants.

use evocc_evoir::{
    interpret_module_with_fp_environment, parse_section9_module, render_module, InterpreterLimits,
    RuntimeValue,
};
use evocc_types::{
    floating::{FpEnvironment, FpRounding, FpStatus},
    EvoIRType, Region, TargetDataLayout,
};

const MODULE: &str = include_str!("../../../tests/fixtures/floating-half-conversions.evoir");

fn run(
    text: &str,
    function: &str,
    argument: Option<(EvoIRType, u128)>,
    rounding: FpRounding,
) -> Result<(Vec<u128>, FpStatus), String> {
    let module = parse_section9_module(text.as_bytes())?;
    let mut args = argument
        .map(|(ty, bits)| RuntimeValue::wide(ty, bits))
        .into_iter()
        .collect::<Vec<_>>();
    args.push(RuntimeValue::new(EvoIRType::Mem(Region::All), 0));
    args.push(RuntimeValue::new(EvoIRType::World, 0));
    let result = interpret_module_with_fp_environment(
        &module,
        function,
        &args,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        InterpreterLimits::new(100),
        FpEnvironment {
            rounding,
            flags: FpStatus::default(),
        },
    )
    .map_err(|error| error.to_string())?;
    let values = result
        .return_values
        .iter()
        .filter(|value| !matches!(value.ty, EvoIRType::Mem(_) | EvoIRType::World))
        .map(RuntimeValue::value)
        .collect();
    Ok((values, result.world.fp_environment().flags))
}

fn nearest(function: &str, ty: EvoIRType, bits: u128) -> u128 {
    run(MODULE, function, Some((ty, bits)), FpRounding::NearestEven)
        .unwrap()
        .0[0]
}

#[test]
fn the_half_forms_round_trip_through_the_text() {
    let module = parse_section9_module(MODULE.as_bytes()).unwrap();
    assert_eq!(render_module(&module), MODULE);
}

#[test]
fn conversions_round_once() {
    // A signaling NaN widens to a quiet NaN with its payload.
    assert_eq!(nearest("widen16", EvoIRType::F16, 0x7d01), 0x7fe02000);
    assert_eq!(nearest("widenb", EvoIRType::BF16, 0xff81), 0xffc10000);
    assert_eq!(nearest("widen16", EvoIRType::F16, 0x0001), 0x33800000);
    // Ties round to even; the value above a tie rounds up.
    assert_eq!(nearest("narrow16", EvoIRType::F32, 0x3f801000), 0x3c00);
    assert_eq!(nearest("narrow16", EvoIRType::F32, 0x3f803000), 0x3c02);
    assert_eq!(nearest("narrowb", EvoIRType::F32, 0x3f808000), 0x3f80);
    assert_eq!(nearest("narrowb", EvoIRType::F32, 0x3f808001), 0x3f81);
    // Overflow and NaN.
    assert_eq!(nearest("narrow16", EvoIRType::F32, 0x477ff000), 0x7c00);
    assert_eq!(nearest("narrowb", EvoIRType::F32, 0xffc12345), 0xffc1);
    // 1 + 2^-8 + 2^-30 is above the bfloat16 tie; a rounding through
    // binary32 would lose the 2^-30 and round down.
    assert_eq!(
        nearest("narrowb64", EvoIRType::F64, 0x3ff0100000400000),
        0x3f81
    );
    assert_eq!(nearest("from_int", EvoIRType::Bits(32), 65520), 0x7c00);
    assert_eq!(nearest("from_int", EvoIRType::Bits(32), 2049), 0x6800);
    assert_eq!(
        nearest("from_uint", EvoIRType::Bits(32), 0x01010001),
        0x4b81
    );
    // A conversion between the half formats keeps the leading payload bits.
    assert_eq!(nearest("cross", EvoIRType::F16, 0x7e11), 0x7fc2);
    assert_eq!(nearest("cross", EvoIRType::F16, 0x3c01), 0x3f80);
}

#[test]
fn strict_conversions_follow_the_rounding_direction_and_raise_status() {
    let tie = Some((EvoIRType::F32, 0x3f808000));
    for (rounding, bits) in [
        (FpRounding::NearestEven, 0x3f80),
        (FpRounding::Upward, 0x3f81),
        (FpRounding::Downward, 0x3f80),
        (FpRounding::TowardZero, 0x3f80),
    ] {
        let (values, flags) = run(MODULE, "strict_narrowb", tie.clone(), rounding).unwrap();
        assert_eq!(
            (values[0], flags),
            (bits, FpStatus::INEXACT),
            "{rounding:?}"
        );
    }
    // Tininess before rounding: the least bfloat16 normal is reached from
    // below with underflow.
    let (values, flags) = run(
        MODULE,
        "strict_narrowb",
        Some((EvoIRType::F32, 0x007fc000)),
        FpRounding::NearestEven,
    )
    .unwrap();
    assert_eq!(values[0], 0x0080);
    assert_eq!(flags, FpStatus::INEXACT.union(FpStatus::UNDERFLOW));
    let (values, flags) = run(
        MODULE,
        "strict_narrow16",
        Some((EvoIRType::F64, 0x40effe0000000000)),
        FpRounding::NearestEven,
    )
    .unwrap();
    assert_eq!(values[0], 0x7c00);
    assert_eq!(flags, FpStatus::INEXACT.union(FpStatus::OVERFLOW));
    let (values, flags) = run(
        MODULE,
        "strict_widenb",
        Some((EvoIRType::BF16, 0x7f81)),
        FpRounding::NearestEven,
    )
    .unwrap();
    assert_eq!((values[0], flags), (0x7fc10000, FpStatus::INVALID));
}

#[test]
fn half_constants_keep_every_bit() {
    let (values, flags) = run(MODULE, "signaling", None, FpRounding::NearestEven).unwrap();
    assert_eq!(values, [0x7d01, 0xff81, 0x7d01]);
    assert_eq!(flags, FpStatus::default());
}

/// One-field mutations: each changes the meaning of the module, so the
/// checker either rejects it or computes other bits.
#[test]
fn one_field_mutations_are_rejected_or_change_the_result() {
    for invalid in [
        // Only conversions and representation moves take a half format.
        MODULE.replace(
            "%v3 = fp.convert %v0 : f32\n    ret %v3, %v1, %v2\n}\n\nfunc @widenb",
            "%v3 = fp.neg %v0 : f32\n    ret %v3, %v1, %v2\n}\n\nfunc @widenb",
        ),
        // A half constant has four hexadecimal digits of 16 bits.
        MODULE.replace("const.f16 0x7d01", "const.f16 0x17d01"),
        // A representation move keeps the width.
        MODULE.replace("fp.to_bits %v2 : bits<16>", "fp.to_bits %v2 : bits<32>"),
        // The result type names the format: bfloat16 is not binary16.
        MODULE.replace(
            "(f16, bf16, bits<16>, mem<all>, world)",
            "(bf16, bf16, bits<16>, mem<all>, world)",
        ),
        // A strict conversion threads the world.
        MODULE.replace(
            "(%v3, %v4) = fp.strict.convert %v2, %v0 : bf16",
            "(%v3, %v4) = fp.strict.convert %v1, %v0 : bf16",
        ),
        // A conversion to the other half format disagrees with the result.
        MODULE.replace(
            "(%v3, %v4) = fp.strict.convert %v2, %v0 : bf16",
            "(%v3, %v4) = fp.strict.convert %v2, %v0 : f16",
        ),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    // The same bits read as the other format, or another constant, are
    // other values.
    let mutated = MODULE.replace(
        "func @widen16(%arg0: f16, %arg1: mem<all>, %arg2: world) -> (f32, mem<all>, world) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: f16,",
        "func @widen16(%arg0: bf16, %arg1: mem<all>, %arg2: world) -> (f32, mem<all>, world) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bf16,",
    );
    assert_ne!(mutated, MODULE);
    let (values, _) = run(
        &mutated,
        "widen16",
        Some((EvoIRType::BF16, 0x3c00)),
        FpRounding::NearestEven,
    )
    .unwrap();
    assert_eq!(values[0], 0x3c000000);
    let mutated = MODULE.replace("const.f16 0x7d01", "const.f16 0x7d02");
    let (values, _) = run(&mutated, "signaling", None, FpRounding::NearestEven).unwrap();
    assert_eq!(values[0], 0x7d02);
}

/// A record of binary16 members is an HFA; a bfloat16 base has no HFA
/// mark, because the C lowering refuses such a record (E3049).
#[test]
fn binary16_hfa_marks_parse_and_bfloat16_marks_do_not() {
    let declaration = |base: &str| {
        format!(
            "declare @f(ptr<generic> byval(6, 2, hfa=3x{base}), mem<all>, world) -> (mem<all>, world) cc = target_default\n"
        )
    };
    let module = parse_section9_module(declaration("f16").as_bytes()).unwrap();
    assert_eq!(render_module(&module), declaration("f16"));
    for base in ["bf16", "f32"] {
        assert!(
            parse_section9_module(declaration(base).as_bytes()).is_err(),
            "{base}"
        );
    }
}
