use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn check(source: &str, expected: u64) {
    let arena = Arena::default();
    let file = FileId::new(45_009);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("{source}: {error}"));
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
        u128::from(expected)
    );
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
        expected
    );
    let text = evocc_evoir::render_module(&module);
    assert_eq!(
        evocc_evoir::render_module(&evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()),
        text
    );
}

#[test]
fn static_compound_literals_have_storage_and_copy_their_initializer() {
    check("struct s {int n;}; struct s x=(struct s){.n=7}; int *p=(int[]){2,5}; int main(void){return x.n+p[1];}", 12);
}

#[test]
fn discarded_pointer_expressions_keep_their_side_effects() {
    check(
        "int main(void){int a[2]={2,7}; int *p=a; (void)p++; return *p;}",
        7,
    );
}

#[test]
fn block_scope_external_function_addresses_use_the_unit_symbol() {
    check("int main(void){typedef int fn(void); extern fn f; fn *p=f; return p();} int f(void){return 7;}", 7);
}
