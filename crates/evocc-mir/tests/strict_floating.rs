use evocc_evoir::parse_section9_module;
use evocc_mir::{
    compile_module_for, lower_module, parse_module, render_module, MirLimits, MirValue, Target,
};
use evocc_types::floating::{FpEnvironment, FpRounding, FpStatus};

const ADD: &str = include_str!("../../../tests/fixtures/floating-strict-world.evoir");

fn value(bits: u128) -> MirValue {
    let mut representation = evocc_memory::ObjectRepresentation::uninitialized(8).unwrap();
    representation
        .write_integer(0, 8, Some(bits), evocc_memory::ByteOrder::Little)
        .unwrap();
    MirValue::Vector(representation)
}

#[test]
fn machine_strictness_survives_text_execution_and_allocation() {
    let ir = parse_section9_module(ADD.as_bytes()).unwrap();
    let machine = lower_module(&ir).unwrap();
    let text = render_module(&machine);
    assert!(text.contains("fp.strict.add.f64"));
    assert_eq!(parse_module(&text).unwrap(), machine);
    let result = evocc_mir::interp::interpret_module_with_fp_environment(
        &machine,
        "strict_add",
        &[value(0x3ff0000000000000), value(0x3ca0000000000000)],
        MirLimits::new(100),
        FpEnvironment {
            rounding: FpRounding::Upward,
            flags: FpStatus::DIV_BY_ZERO,
        },
    )
    .unwrap();
    assert_eq!(result.results[0], value(0x3ff0000000000001));
    assert_eq!(
        result.world.fp_environment().flags,
        FpStatus::DIV_BY_ZERO.union(FpStatus::INEXACT)
    );
    let assembly = compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
    assert!(assembly.contains("bic x16, x16, #0xc00000"));
    assert!(assembly.contains("fadd "));
    assert!(!assembly.contains("msr fpcr") && !assembly.contains("msr fpsr"));
}

#[test]
fn explicit_fma_and_strict_ordered_comparisons_select_distinct_instructions() {
    let compare = ADD
        .replace("-> (f64, mem<all>, world)", "-> (bits<1>, mem<all>, world)")
        .replace(
            "fp.strict.add %v3, %v0, %v1 : f64",
            "fp.strict.olt %v3, %v0, %v1 : bits<1>",
        );
    let ir = parse_section9_module(compare.as_bytes()).unwrap();
    assert!(compile_module_for(Target::Linux, &ir, &|_| true)
        .unwrap()
        .contains("fcmpe "));
    let fused = ADD.replace(
        "fp.strict.add %v3, %v0, %v1",
        "fp.strict.fma %v3, %v0, %v1, %v0",
    );
    let ir = parse_section9_module(fused.as_bytes()).unwrap();
    assert!(compile_module_for(Target::Linux, &ir, &|_| true)
        .unwrap()
        .contains("fmadd "));
}

#[test]
fn strict_conversion_keeps_semantic_integer_width_and_pure_guard_status() {
    let ir = parse_section9_module(include_bytes!(
        "../../../tests/fixtures/floating-strict-conversions.evoir"
    ))
    .unwrap();
    let machine = lower_module(&ir).unwrap();
    let text = render_module(&machine);
    assert!(text.contains("fp.strict.to_signed.i8.f64"));
    assert_eq!(parse_module(&text).unwrap(), machine);
    for (input, expected, flags) in [
        (0x405fe00000000000, 127, FpStatus::INEXACT),
        (0x4060000000000000, 0, FpStatus::INVALID),
        (0xc060100000000000, 128, FpStatus::INEXACT),
        (0x7ff0000000000011, 0, FpStatus::INVALID),
    ] {
        let environment = FpEnvironment {
            rounding: FpRounding::Upward,
            flags: FpStatus::DIV_BY_ZERO,
        };
        let result = evocc_mir::interp::interpret_module_with_fp_environment(
            &machine,
            "strict_trunc",
            &[value(input)],
            MirLimits::new(100),
            environment,
        )
        .unwrap();
        assert_eq!(result.results[0], MirValue::Int(expected));
        assert_eq!(
            result.world.fp_environment().flags,
            flags.union(FpStatus::DIV_BY_ZERO)
        );
        let guard = evocc_mir::interp::interpret_module_with_fp_environment(
            &machine,
            "defined_only",
            &[value(input)],
            MirLimits::new(100),
            environment,
        )
        .unwrap();
        assert_eq!(guard.world.fp_environment(), environment);
    }
    let assembly = compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
    assert!(assembly.contains("and x16, x16, #0x7fffffffffffffff"));
    assert!(assembly.contains("fdiv d31, d31, d31"));
}

#[test]
fn dynamic_rounding_admission_is_not_reused_as_a_fixed_environment_check() {
    let strict_first = ADD.replace(
        "    ret %v4,",
        "    %v6 = fp.add %v4, %v1 : f64\n    ret %v6,",
    );
    let ir = parse_section9_module(strict_first.as_bytes()).unwrap();
    let assembly = compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
    assert_eq!(assembly.matches("mrs x16, fpcr").count(), 2);
    assert_eq!(assembly.matches("bic x16, x16, #0xc00000").count(), 1);
    let fixed_first = ADD.replace(
        "    (%v4, %v5) = fp.strict.add %v3, %v0, %v1 : f64",
        "    %v6 = fp.add %v0, %v1 : f64\n    (%v4, %v5) = fp.strict.add %v3, %v6, %v1 : f64",
    );
    let ir = parse_section9_module(fixed_first.as_bytes()).unwrap();
    let assembly = compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
    assert_eq!(assembly.matches("mrs x16, fpcr").count(), 1);
    assert!(!assembly.contains("bic x16, x16, #0xc00000"));
}
