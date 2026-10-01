//! Use the existing C-HIR/EvoIR bridge, without changing either interpreter.
use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_evoir::{interpret_module, InterpreterLimits, Module, RuntimeValue};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_passes::{
    observation_difference, run_pipeline, ExecutionInput, IdentityPass, ValidationError,
};
use evocc_randgate::{DefinedRun, Observation};
use evocc_types::TargetDataLayout;

fn with_source(source: &str, check: impl FnOnce(Module, Vec<RuntimeValue>, Observation)) {
    let arena = Arena::default();
    let file = FileId::new(60200);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let arguments = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap()
        .params
        .iter()
        .map(|p| RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let chir =
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(100_000));
    let evo = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000));
    let observation = match (chir, evo) {
        (Ok(chir), Ok(evo)) => {
            assert_eq!(
                chir.return_value.unwrap().raw_bits,
                evo.return_values[0].value()
            );
            assert_eq!(chir.world.volatile_trace(), evo.world.volatile_trace());
            Observation::Defined(DefinedRun::from(&evo))
        }
        (Err(chir), Err(evo)) => {
            assert_eq!(chir.code(), evo.code());
            Observation::Undefined {
                code: evo.code(),
                detail: evo.message(),
            }
        }
        other => panic!("C-HIR/EvoIR mismatch: {other:?}"),
    };
    check(module, arguments, observation);
}

#[test]
fn source_bridge_compares_return_diagnostics_memory_and_loop_steps() {
    for source in [
        "int main(void){unsigned x=3;for(unsigned i=0;i<12;++i)x=(x*5+i)^17;return (int)(x&127);}",
        "int state=7;int add(int x){state+=x;return state;}int main(void){return add(5);}",
        "int main(void){int x;return x;}",
        "int main(void){int x=0;return 7/x;}",
    ] {
        with_source(source, |module, arguments, expected| {
            let input = ExecutionInput::main("source-bridge", arguments.clone(), 100_000);
            let output = run_pipeline(&module, &[&IdentityPass], &[input]).unwrap();
            let actual = evocc_randgate::observe(&output.module, &arguments, 100_000);
            assert_eq!(observation_difference(&expected, &actual), None);
            assert_eq!(output.module, module);
        });
    }
}

#[test]
fn event_changes_are_compared_but_volatile_passes_still_fail_closed() {
    with_source(
        "volatile int state;int main(void){state=7;return 0;}",
        |module, arguments, before| {
            let mut after = before.clone();
            let Observation::Defined(run) = &mut after else {
                panic!()
            };
            assert!(!run.volatile.is_empty());
            run.volatile.clear();
            assert_eq!(observation_difference(&before, &after), Some("events"));
            assert!(matches!(
                run_pipeline(
                    &module,
                    &[&IdentityPass],
                    &[ExecutionInput::main("volatile", arguments, 100_000)]
                ),
                Err(ValidationError::Ineligible(_))
            ));
        },
    );
}

#[test]
fn unmodeled_external_calls_are_never_validated() {
    let arena = Arena::default();
    let source = "extern int opaque(void);int main(void){return opaque();}";
    let file = FileId::new(60201);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    assert!(matches!(
        run_pipeline(
            &module,
            &[&IdentityPass],
            &[ExecutionInput::main("external", vec![], 100_000)]
        ),
        Err(ValidationError::Ineligible(_))
    ));
}
