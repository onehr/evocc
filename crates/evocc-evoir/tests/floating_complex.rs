//! Complex multiplication and division in EvoIR (3.13.5). Each part is its
//! own operation of four operands of one format: `fp.cmul.real`,
//! `fp.cmul.imag`, `fp.cdiv.real` and `fp.cdiv.imag`, with strict forms that
//! thread world. Their results are those of the target's runtime helper; the
//! interpreter executes the Linux libgcc model and refuses the Darwin
//! profile, whose compiler-rt sequence it does not model.

use evocc_evoir::{
    interpret_module_with_fp_environment, parse_section9_module, render_module, InterpreterLimits,
    RuntimeValue,
};
use evocc_types::{
    floating::{FpEnvironment, FpStatus},
    EvoIRType, Region, TargetDataLayout,
};

const HELPERS: &str = include_str!("../../../tests/fixtures/floating-complex-helpers.evoir");

fn run(
    function: &str,
    ty: EvoIRType,
    operands: [u128; 4],
    target: &TargetDataLayout,
) -> Result<(u128, u128, FpStatus), evocc_evoir::InterpreterError> {
    let module = parse_section9_module(HELPERS.as_bytes()).unwrap();
    let mut args = operands
        .iter()
        .map(|bits| RuntimeValue::wide(ty.clone(), *bits))
        .collect::<Vec<_>>();
    args.push(RuntimeValue::new(EvoIRType::Mem(Region::All), 0));
    args.push(RuntimeValue::new(EvoIRType::World, 0));
    let result = interpret_module_with_fp_environment(
        &module,
        function,
        &args,
        target,
        InterpreterLimits::new(100),
        FpEnvironment::default(),
    )?;
    Ok((
        result.return_values[0].value(),
        result.return_values[1].value(),
        result.world.fp_environment().flags,
    ))
}

#[test]
fn the_parts_round_trip_through_the_text() {
    let module = parse_section9_module(HELPERS.as_bytes()).unwrap();
    assert_eq!(render_module(&module), HELPERS);
}

#[test]
fn the_linux_profile_executes_the_libgcc_model() {
    let linux = TargetDataLayout::aarch64_unknown_linux_gnu();
    let one = 0x3ff0000000000000;
    let (real, imaginary, _) = run(
        "mul",
        EvoIRType::F64,
        [
            one,
            0x4000000000000000,
            0x4008000000000000,
            0x4010000000000000,
        ],
        &linux,
    )
    .unwrap();
    // (1 + 2i)(3 + 4i) = -5 + 10i.
    assert_eq!((real, imaginary), (0xc014000000000000, 0x4024000000000000));
    // (1 + 1i)/(0 + 0i): binary32 divides in binary64, so 0/0 raises only
    // invalid; binary128 takes Smith's path and also divides by zero.
    let (real, imaginary, flags) = run(
        "strict_div",
        EvoIRType::F32,
        [0x3f800000, 0x3f800000, 0, 0],
        &linux,
    )
    .unwrap();
    assert_eq!(
        (real, imaginary, flags),
        (0x7f800000, 0x7f800000, FpStatus::INVALID)
    );
    let quad_one = 0x3fff0000000000000000000000000000;
    let (real, imaginary, flags) = run(
        "strict_quad",
        EvoIRType::F128,
        [quad_one, quad_one, 0, 0],
        &linux,
    )
    .unwrap();
    let infinity = 0x7fff0000000000000000000000000000;
    assert_eq!(
        (real, imaginary, flags),
        (
            infinity,
            infinity,
            FpStatus::INVALID.union(FpStatus::DIV_BY_ZERO)
        )
    );
}

#[test]
fn the_darwin_profile_is_refused_by_the_interpreter() {
    let darwin = TargetDataLayout::aarch64_apple_darwin();
    let one = 0x3ff0000000000000;
    let error = run("mul", EvoIRType::F64, [one, one, one, one], &darwin).unwrap_err();
    assert_eq!(error.code(), "E3067", "{error}");
    assert_eq!(error.message(), "not-executed: complex-arithmetic");
}

#[test]
fn malformed_parts_are_rejected() {
    for invalid in [
        // Arity, format and result type.
        HELPERS.replace(
            "fp.cmul.real %v0, %v1, %v2, %v3 : f64",
            "fp.cmul.real %v0, %v1, %v2 : f64",
        ),
        HELPERS.replace(
            "fp.cmul.real %v0, %v1, %v2, %v3 : f64",
            "fp.cmul.real %v0, %v1, %v2, %v3, %v0 : f64",
        ),
        HELPERS.replace(
            "fp.cmul.imag %v0, %v1, %v2, %v3 : f64",
            "fp.cmul.imag %v0, %v1, %v2, %v3 : f32",
        ),
        HELPERS.replace(
            "fp.cmul.imag %v0, %v1, %v2, %v3 : f64",
            "fp.cmul.imag %v0, %v1, %v2, %v4 : f64",
        ),
        HELPERS.replace("fp.cmul.imag", "fp.cmul.both"),
        // A pure part has no world; a strict part needs the live world and
        // the fenv effect.
        HELPERS.replace(
            "%v7 = fp.cmul.imag %v0",
            "(%v7, %v8) = fp.cmul.imag %v5, %v0",
        ),
        HELPERS.replace(
            "fp.strict.cdiv.imag %v7, %v0, %v1, %v2, %v3 : f32",
            "fp.strict.cdiv.imag %v5, %v0, %v1, %v2, %v3 : f32",
        ),
        HELPERS.replacen("fenv=true", "fenv=false", 1),
        HELPERS.replace("ret %v6, %v8, %v4, %v9", "ret %v6, %v8, %v4, %v7"),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
