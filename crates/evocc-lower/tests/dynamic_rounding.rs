use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module_with_fp_environment, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::{
    floating::{FpEnvironment, FpRounding, FpStatus},
    EvoIRType, Region, TargetDataLayout,
};

const SOURCE: &str = include_str!("../../../tests/fixtures/floating-dynamic-rounding.c");

#[test]
fn strict_wide_source_conversions_agree_in_both_interpreters() {
    let source = concat!(
        include_str!("../../../tests/fixtures/floating-wide-source.c"),
        r#"
typedef unsigned __int128 U;
union F { float f; unsigned u; }; union D { double d; unsigned long u; };
U probe_fs(void) { union F r={.f=wide_fs(((U)1<<64)+1)}; return r.u; }
U probe_fu(void) { union F r={.f=wide_fu(((U)1<<64)+1)}; return r.u; }
U probe_ds(void) { union D r={.d=wide_ds(((U)1<<64)+1)}; return r.u; }
U probe_du(void) { union D r={.d=wide_du(((U)1<<64)+1)}; return r.u; }
U probe_sf(void) { return wide_sf(-1.5f); }
U probe_uf(void) { return wide_uf(1.5f); }
U probe_sd(void) { return wide_sd(-1.5); }
U probe_ud(void) { return wide_ud(1.5); }
"#
    );
    let arena = Arena::default();
    let file = FileId::new(194109);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let mut unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    unit.set_dynamic_rounding(true);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(&unit, &target, optimize).unwrap();
        for rounding in [
            FpRounding::NearestEven,
            FpRounding::Upward,
            FpRounding::Downward,
            FpRounding::TowardZero,
        ] {
            let environment = FpEnvironment {
                rounding,
                flags: FpStatus::DIV_BY_ZERO,
            };
            for (entry, base, from_integer) in [
                ("probe_fs", 0x5f800000, true),
                ("probe_fu", 0x5f800000, true),
                ("probe_ds", 0x43f0000000000000, true),
                ("probe_du", 0x43f0000000000000, true),
                ("probe_sf", u128::MAX, false),
                ("probe_uf", 1, false),
                ("probe_sd", u128::MAX, false),
                ("probe_ud", 1, false),
            ] {
                let expected = base + u128::from(from_integer && rounding == FpRounding::Upward);
                let result = interpret_module_with_fp_environment(
                    &module,
                    entry,
                    &[
                        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                        RuntimeValue::new(EvoIRType::World, 0),
                    ],
                    &target,
                    InterpreterLimits::new(10000),
                    environment,
                )
                .unwrap();
                assert_eq!(
                    result.return_values[0].value(),
                    expected,
                    "IR {entry} {rounding:?}"
                );
                let symbol = unit
                    .symbols()
                    .symbols_in_source_order()
                    .iter()
                    .find(|s| s.spelling() == entry)
                    .unwrap()
                    .id();
                let chir = evocc_chir::interp::interpret_scalar_function_with_fp_environment(
                    &unit,
                    symbol,
                    &target,
                    evocc_chir::interp::InterpreterLimits::new(10000),
                    environment,
                )
                .unwrap();
                assert_eq!(
                    chir.return_value.unwrap().raw_bits,
                    expected,
                    "C-HIR {entry} {rounding:?}"
                );
                assert_eq!(chir.world.fp_environment(), result.world.fp_environment());
                assert_eq!(
                    chir.world.fp_environment().flags,
                    FpStatus::DIV_BY_ZERO.union(FpStatus::INEXACT)
                );
            }
        }
    }
}

#[test]
fn runtime_expressions_observe_rounding_but_static_bytes_do_not() {
    let arena = Arena::default();
    let file = FileId::new(194106);
    let mut lexer = Lexer::new(file, SOURCE);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, SOURCE);
    let mut unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    unit.set_dynamic_rounding(true);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(&unit, &target, optimize).unwrap();
        let text = render_module(&module);
        assert!(text.contains("fp.strict.add"), "{text}");
        assert!(text.contains("fp.strict.from_signed"), "{text}");
        for rounding in [
            FpRounding::NearestEven,
            FpRounding::Upward,
            FpRounding::Downward,
            FpRounding::TowardZero,
        ] {
            for (entry, base, runtime) in [
                ("runtime_constant", 0x3ff0000000000000, true),
                ("runtime_volatile", 0x3ff0000000000000, true),
                ("runtime_integer_conversion", 0x4340000000000000, true),
                ("runtime_narrowing", 0x4b800000, true),
                ("runtime_branch_conversion", 0x4340000000000000, true),
                ("runtime_condition", 0, true),
                ("translation_constant", 0x3ff0000000000000, false),
                ("quiet_nan_comparison", 0, false),
                ("signaling_nan_comparison", 0, false),
                ("ordered_nan_comparison", 0, false),
            ] {
                let result = interpret_module_with_fp_environment(
                    &module,
                    entry,
                    &[
                        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                        RuntimeValue::new(EvoIRType::World, 0),
                    ],
                    &target,
                    InterpreterLimits::new(10_000),
                    FpEnvironment {
                        rounding,
                        flags: FpStatus::DIV_BY_ZERO,
                    },
                )
                .unwrap();
                let increment = u128::from(runtime && rounding == FpRounding::Upward);
                assert_eq!(
                    result.return_values[0].value(),
                    base + increment,
                    "{entry} {rounding:?}"
                );
                let flags = FpStatus::DIV_BY_ZERO.union(if runtime {
                    FpStatus::INEXACT
                } else if matches!(entry, "signaling_nan_comparison" | "ordered_nan_comparison") {
                    FpStatus::INVALID
                } else {
                    FpStatus::NONE
                });
                assert_eq!(
                    result.world.fp_environment().flags,
                    flags,
                    "{entry} {rounding:?}"
                );
                let symbol = unit
                    .symbols()
                    .symbols_in_source_order()
                    .iter()
                    .find(|s| s.spelling() == entry)
                    .unwrap()
                    .id();
                let chir = evocc_chir::interp::interpret_scalar_function_with_fp_environment(
                    &unit,
                    symbol,
                    &target,
                    evocc_chir::interp::InterpreterLimits::new(10_000),
                    FpEnvironment {
                        rounding,
                        flags: FpStatus::DIV_BY_ZERO,
                    },
                )
                .unwrap();
                assert_eq!(
                    chir.return_value.unwrap().raw_bits,
                    base + increment,
                    "C-HIR {entry} {rounding:?}"
                );
                assert_eq!(
                    chir.world.fp_environment().flags,
                    flags,
                    "C-HIR {entry} {rounding:?}"
                );
            }
        }
    }
}
