use evocc_evoir::parse_section9_module;
use evocc_mir::{
    compile_module_for, lower_module_for, parse_module, render_module, MirLimits, MirValue, Target,
};
use evocc_types::{
    floating::{FloatBits, FpEnvironment, FpRounding, FpStatus},
    FloatKind,
};

fn value(bits: u128) -> MirValue {
    let mut repr = evocc_memory::ObjectRepresentation::uninitialized(16).unwrap();
    repr.write_integer(0, 16, Some(bits), evocc_memory::ByteOrder::Little)
        .unwrap();
    MirValue::Vector(repr)
}
fn unary(op: &str, result: &str, strict: bool) -> String {
    let body = if strict {
        format!("(%v3, %v4) = fp.strict.{op} %v2, %v0 : {result}\n    ret %v3, %v1, %v4")
    } else {
        format!("%v3 = fp.{op} %v0 : {result}\n    ret %v3, %v1, %v2")
    };
    format!("func @probe(%arg0: f128, %arg1: mem<all>, %arg2: world) -> ({result}, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv={strict}, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: f128, %v1: mem<all>, %v2: world):\n    {body}\n}}\n")
}

#[test]
fn quad_constants_helpers_and_machine_reconstruction_preserve_all_bits() {
    let ir = parse_section9_module(include_bytes!(
        "../../../tests/fixtures/floating-quad-exact.evoir"
    ))
    .unwrap();
    let mir = lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
    let text = render_module(&mir);
    assert!(text.contains("__addtf3"));
    assert_eq!(parse_module(&text).unwrap(), mir);
    let result =
        evocc_mir::interpret_module(&mir, "quad_exact", &[], MirLimits::new(1000)).unwrap();
    assert_eq!(
        result.results,
        vec![MirValue::Int(0x22), MirValue::Int(0xffff800000000000)]
    );
    let assembly = compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
    assert!(assembly.contains("bl __addtf3"));
    assert!(compile_module_for(Target::Darwin, &ir, &|_| true).is_err());
}

#[test]
fn fixed_helpers_and_ordinary_calls_do_not_erase_the_environment_boundary() {
    let text =
        include_str!("../../../tests/fixtures/floating-strict-world.evoir").replace("f64", "f128");
    let ir = parse_section9_module(text.as_bytes()).unwrap();
    let machine = lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
    let text = render_module(&machine);
    let fixed = parse_module(&text.replace("bl.strict_fp", "bl.fixed_fp")).unwrap();
    let ordinary = parse_module(&text.replace("bl.strict_fp", "bl")).unwrap();
    assert_ne!(fixed, ordinary);
    assert!(evocc_mir::validate_module(
        &parse_module(&text.replace("__addtf3", "unknown_helper")).unwrap()
    )
    .is_err());
    let arguments = [
        value(0x3fff0000000000000000000000000000),
        value(0x3f8e0000000000000000000000000000),
    ];
    let env = FpEnvironment {
        rounding: FpRounding::Upward,
        flags: FpStatus::DIV_BY_ZERO,
    };
    for m in [&machine, &ordinary] {
        let actual = evocc_mir::interp::interpret_module_with_fp_environment(
            m,
            "strict_add",
            &arguments,
            MirLimits::new(1000),
            env,
            &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
        .unwrap();
        assert_eq!(actual.results[0], value(0x3fff0000000000000000000000000001));
        assert_eq!(actual.world.fp_environment(), env.raised(FpStatus::INEXACT));
    }
    assert!(evocc_mir::interp::interpret_module_with_fp_environment(
        &fixed,
        "strict_add",
        &arguments,
        MirLimits::new(1000),
        env,
        &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
    )
    .is_err());
    let env = FpEnvironment {
        rounding: FpRounding::NearestEven,
        ..env
    };
    let actual = evocc_mir::interp::interpret_module_with_fp_environment(
        &fixed,
        "strict_add",
        &arguments,
        MirLimits::new(1000),
        env,
        &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
    )
    .unwrap();
    assert_eq!(actual.results[0], arguments[0]);
    assert_eq!(actual.world.fp_environment(), env);
}

#[test]
fn quad_conversion_domain_is_pure_and_raw_strict_conversion_is_total() {
    let inf = 0x7fff0000000000000000000000000000;
    let sign = 1u128 << 127;
    let mut inputs = vec![
        0,
        sign,
        1,
        sign | 1,
        inf,
        sign | inf,
        inf | 1,
        inf | (1u128 << 111) | 0x11,
    ];
    for n in [
        0, 1, 7, 8, 15, 16, 30, 31, 32, 62, 63, 64, 112, 113, 127, 128,
    ] {
        let bits = FloatBits::parse(FloatKind::F128, &format!("0x1p{n}"))
            .unwrap()
            .bits();
        for b in [bits - 1, bits, bits + 1] {
            inputs.extend([b, b | sign]);
        }
    }
    for signed in [false, true] {
        for width in [1, 8, 16, 31, 32, 33, 63, 64, 128] {
            let predicate = unary(
                &format!("to_int_defined.{}{width}", if signed { "s" } else { "u" }),
                "bits<1>",
                false,
            );
            let conversion = unary(
                if signed { "to_signed" } else { "to_unsigned" },
                &format!("bits<{width}>"),
                true,
            );
            let p = lower_module_for(
                Target::Linux,
                &parse_section9_module(predicate.as_bytes()).unwrap(),
                &|_| false,
            )
            .unwrap();
            let ir = parse_section9_module(conversion.as_bytes()).unwrap();
            let m = lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
            compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
            for rounding in [
                FpRounding::NearestEven,
                FpRounding::Upward,
                FpRounding::Downward,
                FpRounding::TowardZero,
            ] {
                let env = FpEnvironment {
                    rounding,
                    flags: FpStatus::DIV_BY_ZERO,
                };
                for &bits in &inputs {
                    let expected = FloatBits::from_bits(FloatKind::F128, bits)
                        .unwrap()
                        .to_integer_status(width, signed)
                        .unwrap();
                    let actual = evocc_mir::interp::interpret_module_with_fp_environment(
                        &p,
                        "probe",
                        &[value(bits)],
                        MirLimits::new(1000),
                        env,
                        &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
                    )
                    .unwrap();
                    assert_eq!(
                        actual.results,
                        vec![MirValue::Int(u64::from(expected.value.is_some()))],
                        "predicate s={signed} n={width} bits={bits:032x}"
                    );
                    assert_eq!(actual.world.fp_environment(), env);
                    let actual = evocc_mir::interp::interpret_module_with_fp_environment(
                        &m,
                        "probe",
                        &[value(bits)],
                        MirLimits::new(1000),
                        env,
                        &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
                    )
                    .unwrap();
                    let raw = expected.value.unwrap_or(0) & (u128::MAX >> (128 - width));
                    let values = if width == 128 {
                        vec![MirValue::Int(raw as u64), MirValue::Int((raw >> 64) as u64)]
                    } else {
                        vec![MirValue::Int(raw as u64)]
                    };
                    assert_eq!(
                        actual.results, values,
                        "conversion s={signed} n={width} bits={bits:032x}"
                    );
                    assert_eq!(actual.world.fp_environment(), env.raised(expected.status));
                }
            }
        }
    }
}
