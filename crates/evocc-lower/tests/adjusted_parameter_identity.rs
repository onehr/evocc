use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::CHirDecl;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{AddressSpace, EvoIRType, TargetDataLayout};

#[test]
fn array_parameter_spelling_matches_its_adjusted_pointer_identity() {
    for source in [
        "int f(int a[4]) { return a[0]; } int main(void) { int a[4]={7}; return f(a); }",
        "int f(int a[]) { return a[0]; } int main(void) { int a[4]={7}; return f(a); }",
        "int f(int a[][2]) { return a[1][0]; } int main(void) { int a[2][2]={{1,2},{7,4}}; return f(a); }",
        "typedef int A[4]; int f(A a) { return a[0]; } int main(void) { A a={7}; return f(a); }",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45_002);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect::<Vec<_>>();
        assert!(tokens.iter().all(|token| token.kind != TokenKind::Error));
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
        .unwrap_or_else(|error| panic!("{source}: {} {error}", error.code()));
        let f = module.functions.iter().find(|function| function.name == "f").unwrap();
        assert_eq!(f.params[0].ty, EvoIRType::Ptr(AddressSpace::Generic));
        let main_symbol = unit.iter().find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "main" => Some(function.symbol),
            _ => None,
        }).unwrap();
        let chir = interpret_scalar_function(&unit, main_symbol, InterpreterLimits::new(100_000))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(chir.return_value.unwrap().raw_bits, 7);
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        let arguments = main.params.iter().map(|parameter| {
            evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0)
        }).collect::<Vec<_>>();
        let result = evocc_evoir::interpret_module(
            &module, "main", &arguments, evocc_evoir::InterpreterLimits::new(100_000),
        ).unwrap();
        assert_eq!(result.return_values[0].raw_bits, 7);
    }
}
