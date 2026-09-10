use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn literal_string_comparison_does_not_bind_a_same_named_local_function() {
    let source = include_str!("../../../tests/fixtures/literal-string-comparison.c");
    for builtins in [true, false] {
        let arena = Arena::default();
        let file = FileId::new(45019);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        parser.set_standard_builtins(builtins);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let main = unit
            .iter()
            .find_map(|decl| match decl {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        let chir =
            interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
                .unwrap();
        assert_eq!(chir.return_value.unwrap().raw_bits, 0);
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        assert!(!evocc_evoir::render_module(&module).contains("call @strcmp"));
        let f = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = f
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        let ir = evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000),
        )
        .unwrap();
        assert_eq!(ir.return_values[0].raw_bits, 0);
    }
}

#[test]
fn nonliteral_builtin_comparison_with_local_binding_is_refused() {
    let source = "static int strcmp(const char *a,const char *b){return -123;} int f(const char *a,const char *b){return __builtin_strcmp(a,b);}";
    let arena = Arena::default();
    let file = FileId::new(45020);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error(), "conflicting local binding was accepted");
    assert_eq!(parser.diagnostic().unwrap().code, "E2946");
}

#[test]
fn prefetch_keeps_address_evaluation_without_an_external_call() {
    let source = "int n; int *address(void){n++;return &n;} int main(void){__builtin_prefetch(address()); __builtin_prefetch(address(),0); __builtin_prefetch(address(),1,3); return n-3;}";
    let arena = Arena::default();
    let file = FileId::new(45012);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let text = evocc_evoir::render_module(&module);
    assert!(!text.contains("call @__builtin_prefetch"), "{text}");
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let args = main
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    assert_eq!(
        evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000)
        )
        .unwrap()
        .return_values[0]
            .raw_bits,
        0
    );
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000))
            .unwrap()
            .return_value
            .unwrap()
            .raw_bits,
        0
    );
}

#[test]
fn dynamic_stack_allocation_never_becomes_an_external_call() {
    for (source, builtin) in [
        (
            "void *f(unsigned long n){return __builtin_alloca(n);}",
            true,
        ),
        ("void f(void){__builtin_alloca(16);}", true),
        (
            "static void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
            false,
        ),
    ] {
        let arena = Arena::default();
        let file = FileId::new(45013);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect("dynamic storage lowers without a library fallback");
        let instructions = module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts);
        assert_eq!(
            instructions
                .clone()
                .any(|i| matches!(i, evocc_evoir::Inst::DynamicAlloca { .. })),
            builtin
        );
        assert_eq!(
            instructions
                .clone()
                .any(|i| matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "alloca")),
            !builtin
        );
    }
}

#[test]
fn ordinary_allocation_calls_follow_the_builtin_option_and_prototype() {
    for (prefix, legacy, builtins) in [
        ("", true, true),
        ("extern void *alloca(unsigned long);", false, true),
        ("extern void *alloca(unsigned long);", false, false),
    ] {
        let source = format!("{prefix} int main(void) {{ unsigned long n=19; unsigned char *p=alloca(n); p[0]=17; p[18]=29; return p[0]+p[18]-46; }}");
        let arena = Arena::default();
        let file = FileId::new(45014);
        let mut lexer = Lexer::new(file, &source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, &source);
        parser.set_legacy_declarations(legacy);
        parser.set_standard_builtins(builtins);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let text = evocc_evoir::render_module(&module);
        assert_eq!(text.contains("alloca.dynamic"), builtins, "{text}");
        assert_eq!(text.contains("call @alloca"), !builtins, "{text}");
        if builtins {
            let main = unit
                .iter()
                .find_map(|d| match d {
                    CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                interp::interpret_scalar_function(
                    &unit,
                    main,
                    interp::InterpreterLimits::new(10000)
                )
                .unwrap()
                .return_value
                .unwrap()
                .raw_bits,
                0
            );
            let function = module.functions.iter().find(|f| f.name == "main").unwrap();
            let arguments = function
                .params
                .iter()
                .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
                .collect::<Vec<_>>();
            let result = evocc_evoir::interpret_module(
                &module,
                "main",
                &arguments,
                evocc_evoir::InterpreterLimits::new(10000),
            )
            .unwrap();
            assert_eq!(result.return_values[0].raw_bits, 0);
        }
    }
}

#[test]
fn conflicting_ordinary_allocation_bindings_are_explicitly_refused() {
    for source in [
        "void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
        "extern void *alloca(unsigned long) asm(\"replacement\"); void *f(void){return alloca(16);}",
        "extern void *alloca(unsigned long); void *f(void){return alloca(16);} void *alloca(unsigned long n){return 0;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45015);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert!(parser.had_error(), "conflicting builtin binding was accepted: {source}");
        assert_eq!(parser.diagnostic().unwrap().code, "E2944");
    }
}

#[test]
fn disabled_allocation_recognition_preserves_written_functions() {
    for source in [
        "void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
        "extern void *alloca(unsigned long) asm(\"replacement\"); void *f(void){return alloca(16);}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45016);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        parser.set_standard_builtins(false);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .unwrap();
        let text = evocc_evoir::render_module(&module);
        assert!(!text.contains("alloca.dynamic"), "{text}");
        assert!(text.contains("call @alloca") || text.contains("call @replacement"), "{text}");
    }
}

#[test]
fn explicit_allocation_is_not_disabled_by_the_ordinary_name_option() {
    let source = "void *f(unsigned long n){return __builtin_alloca(n);}";
    let arena = Arena::default();
    let file = FileId::new(45017);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    parser.set_standard_builtins(false);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    assert!(evocc_evoir::render_module(&module).contains("alloca.dynamic"));
}

#[test]
fn ordinary_allocation_needs_a_declaration_outside_legacy_mode() {
    let source = "void *f(unsigned long n){return alloca(n);}";
    let arena = Arena::default();
    let file = FileId::new(45018);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error());
    assert_eq!(parser.diagnostic().unwrap().code, "E2001");
}
