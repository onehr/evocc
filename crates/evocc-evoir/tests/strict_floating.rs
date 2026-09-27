use evocc_evoir::{
    interpret_module_with_fp_environment, parse_section9_module, render_module, InterpreterLimits,
    RuntimeValue,
};
use evocc_types::{
    floating::{FpEnvironment, FpRounding, FpStatus},
    EvoIRType, Region, TargetDataLayout,
};

const ADD: &str = include_str!("../../../tests/fixtures/floating-strict-world.evoir");

#[test]
fn strict_world_roundtrips_and_observes_caller_rounding_and_prior_flags() {
    let module = parse_section9_module(ADD.as_bytes()).unwrap();
    assert_eq!(render_module(&module), ADD);
    for (rounding, expected) in [
        (FpRounding::NearestEven, 0x3ff0000000000000),
        (FpRounding::Upward, 0x3ff0000000000001),
        (FpRounding::Downward, 0x3ff0000000000000),
        (FpRounding::TowardZero, 0x3ff0000000000000),
    ] {
        let args = [
            RuntimeValue::new(EvoIRType::F64, 0x3ff0000000000000),
            RuntimeValue::new(EvoIRType::F64, 0x3ca0000000000000),
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ];
        let result = interpret_module_with_fp_environment(
            &module,
            "strict_add",
            &args,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            InterpreterLimits::new(100),
            FpEnvironment {
                rounding,
                flags: FpStatus::DIV_BY_ZERO,
            },
        )
        .unwrap();
        assert_eq!(result.return_values[0].value(), expected);
        assert_eq!(result.world.fp_environment().rounding, rounding);
        assert_eq!(
            result.world.fp_environment().flags,
            FpStatus::DIV_BY_ZERO.union(FpStatus::INEXACT)
        );
        assert!(result.world.volatile_trace().is_empty());
        assert!(result.world.atomic_trace().is_empty());
    }
}

#[test]
fn strict_world_cannot_be_dropped_duplicated_or_mislabeled_pure() {
    for invalid in [
        ADD.replace("fenv=true", "fenv=false"),
        ADD.replace(", fenv=true", ""),
        ADD.replace("ret %v4, %v2, %v5", "ret %v4, %v2, %v3"),
        ADD.replace("fp.strict.add %v3", "fp.strict.add %v2"),
        ADD.replace("fp.strict.add", "fp.strict.neg"),
        ADD.replace(
            "fp.strict.add %v3, %v0, %v1 : f64",
            "fp.strict.oeq %v3, %v0, %v1 : f64",
        ),
        ADD.replace(
            "    ret %v4",
            "    (%v6, %v7) = fp.strict.add %v3, %v0, %v1 : f64\n    ret %v4",
        ),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn transitive_strict_effect_is_required_in_the_callers_summary() {
    let caller = "\nfunc @caller(%arg0: f64, %arg1: f64, %arg2: mem<all>, %arg3: world) -> (f64, mem<all>, world) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=true, may_trap=false, may_unwind=false }\n{\n^entry(%v0: f64, %v1: f64, %v2: mem<all>, %v3: world):\n    (%v4, %v5, %v6) = call @strict_add(%v0, %v1, %v2, %v3)\n    ret %v4, %v5, %v6\n}\n";
    let complete = format!("{ADD}{caller}");
    assert!(parse_section9_module(complete.as_bytes()).is_ok());
    assert!(parse_section9_module(
        format!("{ADD}{}", caller.replace("fenv=true", "fenv=false")).as_bytes()
    )
    .is_err());
}
