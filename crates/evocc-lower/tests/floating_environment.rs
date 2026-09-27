use evocc_common::{Arena, FileId};
use evocc_evoir::{interpret_module, InterpreterLimits, RuntimeValue};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

#[test]
fn shared_target_fenv_library_returns_explicit_memory_and_world() {
    let source = include_str!("../../../tests/fixtures/floating-environment-library.c");
    let arena = Arena::default();
    let file = FileId::new(194107);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let mut unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    unit.set_dynamic_rounding(true);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(&unit, &target, optimize).unwrap();
        for entry in ["environment_probe", "environment_arithmetic"] {
            let ir = interpret_module(
                &module,
                entry,
                &[
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                    RuntimeValue::new(EvoIRType::World, 0),
                ],
                InterpreterLimits::new(200_000),
            )
            .unwrap();
            assert_eq!(
                ir.return_values[0].value(),
                0,
                "IR {entry} optimize={optimize}"
            );
            let symbol = unit
                .symbols()
                .symbols_in_source_order()
                .iter()
                .find(|s| s.spelling() == entry)
                .unwrap()
                .id();
            let chir = evocc_chir::interp::interpret_scalar_function_with_target(
                &unit,
                symbol,
                &target,
                evocc_chir::interp::InterpreterLimits::new(200_000),
            )
            .unwrap();
            assert_eq!(chir.return_value.unwrap().raw_bits, 0, "C-HIR {entry}");
            assert_eq!(ir.world.fp_environment(), chir.world.fp_environment());
            assert_eq!(ir.world.fp_environment(), Default::default());
        }
    }
}

#[test]
fn a_library_rounding_change_does_not_silently_change_the_fixed_profile() {
    let source="extern int fesetround(int); int main(void) { fesetround(0x400000); volatile double a=1.0,b=0x1p-53; return (int)(a+b); }";
    let arena = Arena::default();
    let file = FileId::new(194108);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let module = evocc_lower::lower_unit(&unit, &target).unwrap();
    let ir = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(1000),
    )
    .unwrap_err();
    assert_eq!(ir.code(), "E3064");
    let symbol = unit
        .symbols()
        .symbols_in_source_order()
        .iter()
        .find(|s| s.spelling() == "main")
        .unwrap()
        .id();
    let chir = evocc_chir::interp::interpret_scalar_function_with_target(
        &unit,
        symbol,
        &target,
        evocc_chir::interp::InterpreterLimits::new(1000),
    )
    .unwrap_err();
    assert_eq!(chir.code(), "E3064");
}
