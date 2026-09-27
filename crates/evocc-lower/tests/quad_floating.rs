use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_evoir::{interpret_module, render_module, InterpreterLimits, RuntimeValue};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

#[test]
fn quad_source_interpreters_and_exact_text() {
    let source = include_str!("../../../tests/fixtures/floating-quad-core.c");
    check(
        source,
        &[
            "quad_core",
            "quad_nan",
            "quad_dynamic",
            "quad_varargs",
            "quad_uninitialized",
            "quad_invalid_conversion",
        ],
    );
    // Exercise constant folding as well as a unit containing observable fenv.
    check(
        source.split("extern int fesetround").next().unwrap(),
        &["quad_core", "quad_nan"],
    );
}

fn check(source: &str, entries: &[&str]) {
    let arena = Arena::default();
    let file = FileId::new(194128);
    let mut lexer = Lexer::new(file, source);
    let input = std::iter::from_fn(|| lexer.next()).collect();
    let mut cpp = Preprocessor::new();
    let tokens = cpp.process(TokenStream::new(input), source).into_vec();
    assert!(cpp.errors().is_empty(), "{:?}", cpp.errors());
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(&unit, &target, optimize).unwrap();
        let text = render_module(&module);
        assert!(text.contains("f128"));
        assert_eq!(
            text.contains("fp.strict.add"),
            source.contains("FENV_ACCESS ON")
        );
        let reparsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
        assert_eq!(text, render_module(&reparsed));
        for &entry in entries {
            let ir = interpret_module(
                &reparsed,
                entry,
                &[
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                    RuntimeValue::new(EvoIRType::World, 0),
                ],
                InterpreterLimits::new(200000),
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
                evocc_chir::interp::InterpreterLimits::new(200000),
            );
            if matches!(entry, "quad_uninitialized" | "quad_invalid_conversion") {
                let code = if entry == "quad_uninitialized" {
                    "E3001"
                } else {
                    "E3063"
                };
                assert_eq!(ir.unwrap_err().code(), code);
                assert_eq!(chir.unwrap_err().code(), code);
            } else {
                let ir = ir.unwrap();
                let chir = chir.unwrap();
                assert_eq!(ir.return_values[0].value(), 0, "IR {entry} O2={optimize}");
                assert_eq!(chir.return_value.unwrap().raw_bits, 0, "C-HIR {entry}");
                assert_eq!(chir.world.fp_environment(), ir.world.fp_environment());
            }
        }
    }
}
