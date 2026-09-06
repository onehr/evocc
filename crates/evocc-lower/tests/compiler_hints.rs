use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

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
fn dynamic_stack_allocation_is_refused_before_linking() {
    for (source, refused) in [
        (
            "void *f(unsigned long n){return __builtin_alloca(n);}",
            true,
        ),
        ("void f(void){__builtin_alloca(16);}", true),
        (
            "void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
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
        let result = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu());
        if refused {
            let error = result.expect_err("alloca must not become an unresolved library call");
            assert_eq!(error.code(), "E3046");
            assert_eq!(error.to_string(), "not-lowered: dynamic-stack-allocation");
        } else {
            result.expect("an ordinary function with this name is not a builtin");
        }
    }
}
