use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_program, interpret_program_with_site, render_module, InterpreterLimits,
};
use evocc_lex::Lexer;
use evocc_lower::{lower_unit_with_locations, lower_unit_with_optimization};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn origins_do_not_change_ir_or_interpreter_errors() {
    for optimize in [false, true] {
        for source in [
            "int main(void) { return 2 / 0; }",
            "int f(int n) { return 1 / n; } int main(void) { return f(0); }",
            "int main(void) { int n=0; while(n<4) n++; return n; }",
            "int main(void) { int n; return n; }",
            "int main(void) { int a[2]; return a[3]; }",
        ] {
            let arena = Arena::default();
            let file = FileId::new(1);
            let mut lexer = Lexer::new(file, source);
            let tokens = std::iter::from_fn(|| lexer.next()).collect();
            let mut parser = Parser::new(tokens, file, source);
            let unit = parser.parse_translation_unit_in(&arena);
            assert!(!parser.had_error(), "{source}");
            let target = TargetDataLayout::aarch64_unknown_linux_gnu();
            let plain = lower_unit_with_optimization(&unit, &target, optimize).unwrap();
            let (located, locations) =
                lower_unit_with_locations(&unit, &target, optimize, |_, module, locations| {
                    (module, locations)
                })
                .unwrap();
            assert_eq!(plain, located);
            assert_eq!(render_module(&plain), render_module(&located));
            let args = plain
                .functions
                .iter()
                .find(|f| f.name == "main")
                .unwrap()
                .params
                .iter()
                .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
                .collect::<Vec<_>>();
            let expected = interpret_program(&plain, &args, InterpreterLimits::new(10_000));
            let mut site = None;
            let actual = interpret_program_with_site(
                &located,
                &args,
                InterpreterLimits::new(10_000),
                &mut site,
            );
            match (expected, actual) {
                (Err(expected), Err(actual)) => {
                    assert_eq!(expected, actual);
                    let site = site.expect("an executed failure must name its instruction");
                    let function = &located.functions[site.function];
                    assert!(locations.declaration(&function.name).is_some());
                }
                (Ok(expected), Ok(actual)) => {
                    assert_eq!(expected.return_values, actual.return_values);
                    assert_eq!(expected.steps, actual.steps);
                }
                results => panic!("execution changed: {results:?}"),
            }
        }
    }
}
