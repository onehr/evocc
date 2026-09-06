use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, TargetDataLayout};

#[test]
fn compatible_parameter_qualifiers_do_not_change_function_identity() {
    for source in [
        "typedef int (*fn)(int); int f(const fn callback); int f(fn callback){return callback(6);}",
        "int f(int *restrict p); int f(int *p){return *p;}",
        "int f(const int n); int f(int n){return n;}",
        "int f(int *p); int f(int *const p){return *p;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45_008);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(module.functions.len(), 1);
        evocc_evoir::validate_module(&module).unwrap();
    }
}

#[test]
fn function_parameter_spelling_matches_its_adjusted_pointer_identity() {
    for parameter in ["fn a", "int a(int)", "int (*a)(int)"] {
        let source = format!("typedef int fn(int); int f({parameter}){{return a(6);}} int add(int x){{return x+1;}} int main(void){{return f(add);}}");
        let arena = Arena::default();
        let file = FileId::new(45_007);
        let mut lexer = Lexer::new(file, &source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, &source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        assert!(matches!(
            module
                .functions
                .iter()
                .find(|f| f.name == "f")
                .unwrap()
                .params[0]
                .ty,
            EvoIRType::FnPtr { .. }
        ));
        let main = unit
            .iter()
            .find_map(|decl| match decl {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
                .unwrap()
                .return_value
                .unwrap()
                .raw_bits,
            7
        );
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let arguments = main
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        assert_eq!(
            evocc_evoir::interpret_module(
                &module,
                "main",
                &arguments,
                evocc_evoir::InterpreterLimits::new(10000)
            )
            .unwrap()
            .return_values[0]
                .raw_bits,
            7
        );
    }
}
