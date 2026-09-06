use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn short_circuit_assignments_reach_the_selected_successor() {
    for source in [
        "int main(void){int x;int y=0;if(y || (x=5)) return x!=5;return 1;}",
        "int f(int y){int x=0;if(y || (x=5)) return x;return -1;} int main(void){return f(0)!=5 || f(1)!=0;}",
        "int f(int y){int x=0;if(y && (x=5)) return x;return x;} int main(void){return f(1)!=5 || f(0)!=0;}",
        "int main(void){int x=0; int y=0; if (y || ((x=5) && (x+=2))) return x!=7; return 1;}",
        "int main(void){int x=0;int y=0;int b=(y || (x=5));return x!=5 || b!=1;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45022);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let symbol = unit.iter().find_map(|d| match d {CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol), _ => None}).unwrap();
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let main = module.functions.iter().find(|f| f.name=="main").unwrap();
        let args = main.params.iter().map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(),0)).collect::<Vec<_>>();
        assert_eq!(evocc_evoir::interpret_module(&module,"main",&args,evocc_evoir::InterpreterLimits::new(10000)).unwrap().return_values[0].raw_bits, 0, "EvoIR: {source}\n{}",evocc_evoir::render_module(&module));
        assert_eq!(interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000)).unwrap().return_value.unwrap().raw_bits, 0, "C-HIR: {source}");
    }
}
