use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn agrees(source: &str) {
    let arena = Arena::default();
    let file = FileId::new(45023);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let arguments = function
        .params
        .iter()
        .map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0))
        .collect::<Vec<_>>();
    let execution = evocc_evoir::interpret_module(
        &module,
        "main",
        &arguments,
        evocc_evoir::InterpreterLimits::new(10000),
    )
    .unwrap();
    assert_eq!(execution.return_values[0].raw_bits, 0, "EvoIR: {source}");
    let execution =
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000))
            .unwrap();
    assert_eq!(
        execution.return_value.unwrap().raw_bits,
        0,
        "C-HIR: {source}"
    );
}

#[test]
fn a_pointer_return_value_can_be_stored_through_another_pointer() {
    agrees("int n=7; int *get(void){return &n;} int main(void){int *p=0;int **q=&p;*q=get();return *p!=7;}");
}

#[test]
fn assignment_values_are_operands_with_their_effects_preserved() {
    agrees("int main(void){int a=0;int b=0;int r=(a=3)+(b=4);return r!=7 || a!=3 || b!=4;}");
    agrees("int a;int main(void){int b=2;int r=(a=3)*(b+=4);return r!=18 || a!=3 || b!=6;}");
}

#[test]
fn dereferenced_function_pointers_remain_function_designators() {
    agrees("int f(int n){return n+1;} int main(void){int (*p)(int)=f;return (*p)(6)!=7;}");
}

#[test]
fn a_conditional_array_index_is_evaluated_once() {
    agrees("int main(void){int a[2]={3,7};int n=0;return a[n++ ? 0 : 1]!=7 || n!=1;}");
}

#[test]
fn a_local_bit_field_record_is_initialized_and_updated() {
    agrees("struct S{unsigned a:3;unsigned b:5;};int main(void){struct S s={2,7};s.a+=1;return s.a!=3 || s.b!=7;}");
}
