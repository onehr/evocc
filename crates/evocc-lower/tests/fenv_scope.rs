use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_evoir::{interpret_module, render_module, InterpreterLimits, RuntimeValue};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

fn tokens(source: &str, file: FileId) -> Vec<evocc_lex::Token> {
    let mut lexer = Lexer::new(file, source);
    let input = std::iter::from_fn(|| lexer.next()).collect();
    let mut cpp = Preprocessor::new();
    cpp.current_file = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/floating-fenv-scope.c"
    )
    .into();
    let output = cpp.process(TokenStream::new(input), source).into_vec();
    assert!(cpp.errors().is_empty(), "{:?}", cpp.errors());
    output
}

#[test]
fn lexical_fenv_survives_inlining_scopes_and_gotos() {
    let source = include_str!("../../../tests/fixtures/floating-fenv-scope.c");
    let arena = Arena::default();
    let file = FileId::new(194110);
    let mut parser = Parser::new(tokens(source, file), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    assert!(unit.runtime_floating_environment());
    assert!(!unit.dynamic_rounding());
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(&unit, &target, optimize).unwrap();
        let text = render_module(&module);
        assert!(text.contains("fp.strict.add") && text.contains("fp.add"));
        for entry in [
            "scope_probe",
            "scope_goto",
            "scope_boundaries",
            "scope_computed",
            "environment_probe",
            "environment_arithmetic",
            "scope_discarded",
            "scope_expression",
        ] {
            let ir = interpret_module(
                &module,
                entry,
                &[
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                    RuntimeValue::new(EvoIRType::World, 0),
                ],
                InterpreterLimits::new(200000),
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
                evocc_chir::interp::InterpreterLimits::new(200000),
            )
            .unwrap();
            assert_eq!(chir.return_value.unwrap().raw_bits, 0, "C-HIR {entry}");
            assert_eq!(chir.world.fp_environment(), ir.world.fp_environment());
        }
    }
}

#[test]
fn fenv_pragmas_are_not_accepted_after_block_items() {
    for source in [
        "int f(void){int x;\n#pragma STDC FENV_ACCESS ON\nreturn 0;}",
        "int f(void){; _Pragma(\"STDC FENV_ACCESS OFF\") return 0;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(194111);
        let mut parser = Parser::new(tokens(source, file), file, source);
        let _ = parser.parse_translation_unit_in(&arena);
        assert_eq!(parser.diagnostic().unwrap().code, "E3065");
    }
}
