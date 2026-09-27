use evocc_evoir::{
    interpret_module, interpret_module_with_fp_environment, parse_section9_module, render_module,
    InterpreterLimits, RuntimeValue,
};
use evocc_types::{
    floating::{FpEnvironment, FpRounding, FpStatus},
    EvoIRType, Region, TargetDataLayout,
};

const EXACT: &str = include_str!("../../../tests/fixtures/floating-quad-exact.evoir");

#[test]
fn quad_bits_have_no_binary64_intermediate() {
    let module = parse_section9_module(EXACT.as_bytes()).unwrap();
    assert_eq!(render_module(&module), EXACT);
    let result = interpret_module(&module, "quad_exact", &[], InterpreterLimits::new(100)).unwrap();
    assert_eq!(
        result.return_values[0].value(),
        0xffff8000000000000000000000000022
    );
    for text in [
        EXACT.replace(
            "0x7fff8000000000000000000000000011",
            "0x07fff8000000000000000000000000011",
        ),
        EXACT.replace("const.f128 0x7fff", "const.f64 0x7fff"),
        EXACT.replace("fp.to_bits %v2 : bits<128>", "fp.to_bits %v2 : bits<64>"),
        EXACT.replace("fp.add %v0, %v1 : f128", "fp.add %v0, %v1 : f64"),
    ] {
        assert!(parse_section9_module(text.as_bytes()).is_err(), "{text}");
    }
}

#[test]
fn quad_strict_world_and_hfa_metadata_are_not_erased() {
    let source =
        include_str!("../../../tests/fixtures/floating-strict-world.evoir").replace("f64", "f128");
    let module = parse_section9_module(source.as_bytes()).unwrap();
    for (rounding, increment) in [
        (FpRounding::NearestEven, 0),
        (FpRounding::Upward, 1),
        (FpRounding::Downward, 0),
        (FpRounding::TowardZero, 0),
    ] {
        let one = 0x3fff0000000000000000000000000000;
        let result = interpret_module_with_fp_environment(
            &module,
            "strict_add",
            &[
                RuntimeValue::wide(EvoIRType::F128, one),
                RuntimeValue::wide(EvoIRType::F128, 0x3f8e0000000000000000000000000000),
                RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            InterpreterLimits::new(100),
            FpEnvironment {
                rounding,
                flags: FpStatus::DIV_BY_ZERO,
            },
        )
        .unwrap();
        assert_eq!(result.return_values[0].value(), one + increment);
        assert_eq!(
            result.world.fp_environment().flags,
            FpStatus::DIV_BY_ZERO.union(FpStatus::INEXACT)
        );
    }
    let hfa = EXACT
        .replace(
            "@quad_exact()",
            "@quad_exact(%arg0: ptr<generic> byval(32, 16, hfa=2xf128, abi_align=16))",
        )
        .replace("^entry()", "^entry(%v4: ptr<generic>)");
    assert_eq!(
        render_module(&parse_section9_module(hfa.as_bytes()).unwrap()),
        hfa
    );
    assert!(parse_section9_module(hfa.replace(", abi_align=16", "").as_bytes()).is_err());
}
