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
    if evocc_chir::inline::has_inline_definitions(&unit) {
        let rewritten = evocc_chir::inline::substitute(
            &unit,
            &arena,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
        .unwrap();
        let execution = interp::interpret_scalar_function(
            &rewritten,
            symbol,
            interp::InterpreterLimits::new(10000),
        )
        .unwrap();
        assert_eq!(
            execution.return_value.unwrap().raw_bits,
            0,
            "substituted C-HIR: {source}"
        );
    }
}

#[test]
fn a_pointer_return_value_can_be_stored_through_another_pointer() {
    agrees("int n=7; int *get(void){return &n;} int main(void){int *p=0;int **q=&p;*q=get();return *p!=7;}");
}

#[test]
fn aggregate_array_elements_can_be_passed_and_copied() {
    agrees("struct S{int n;};struct S a[2]={{3},{7}};int f(struct S s){return s.n;}int main(void){struct S b=a[1];return f(a[0])!=3 || b.n!=7;}");
}

#[test]
fn pointer_equality_accepts_integer_signedness_counterparts() {
    agrees("int main(void){short x=0;short *p=&x;unsigned short *q=(unsigned short *)&x;int r=p!=q;return r || !(p==q);}");
}

#[test]
fn assignment_values_are_operands_with_their_effects_preserved() {
    agrees("int main(void){int a=0;int b=0;int r=(a=3)+(b=4);return r!=7 || a!=3 || b!=4;}");
    agrees("int a;int main(void){int b=2;int r=(a=3)*(b+=4);return r!=18 || a!=3 || b!=6;}");
}

#[test]
fn a_selected_short_circuit_operand_defines_a_previously_uninitialized_local() {
    agrees("int f(int y){int x;if(y||(x=5))return x;return 0;}int main(void){return f(0)!=5;}");
    agrees("int f(int y){int x=9;if(y||(x=5))return x;return 0;}int main(void){return f(1)!=9 || f(0)!=5;}");
}

#[test]
fn inline_calls_preserve_parameters_returns_labels_and_static_identity() {
    agrees("static __attribute__((always_inline)) int f(int x){static int n=3; ++n; if(x) return n+x; return n;} int main(void){return f(2)!=6 || f(0)!=5;}");
    agrees("static __attribute__((always_inline)) int f(int x){if(x)goto done;x=7;done:return x;} int main(void){int x=0;int a=f(x++);int b=f(x++);return a!=7 || b!=1 || x!=2;}");
    agrees("static __attribute__((always_inline)) int g(int x){return x+1;} static __attribute__((always_inline)) int f(int x){return g(x)*2;} int main(void){return f(3)!=8;}");
    agrees("static __attribute__((always_inline)) void f(int *p){*p+=3;return;} int main(void){int x=2;f(&x);return x!=5;}");
    agrees("struct S{int n;};static __attribute__((always_inline)) struct S f(struct S s){s.n++;return s;}int main(void){struct S s={3};struct S b=f(s);return b.n!=4 || s.n!=3;}");
    agrees("static __attribute__((always_inline)) int f(void){return __func__[0];}int main(void){return f()!='f';}");
    agrees("static __attribute__((always_inline)) int f(void){static int n=2;return ++n;}int (*p)(void)=f;int main(void){return f()!=3 || p()!=4;}");
    agrees("static __attribute__((always_inline)) int f(int n,...){return n;}int main(void){return f(3,4)!=3;}");
    agrees("int n=3;int g(void){return n;}static __attribute__((always_inline)) int f(void){extern int n;extern int g(void);return n+g();}int main(void){int n=9;return f()!=6 || n!=9;}");
}

#[test]
fn dereferenced_function_pointers_remain_function_designators() {
    agrees("int f(int n){return n+1;} int main(void){int (*p)(int)=f;return (*p)(6)!=7;}");
}

#[test]
fn empty_parameter_lists_in_definitions_have_no_parameters() {
    agrees("int f(){return 7;} int main(){return f()!=7;}");
    agrees("int count;void f(){++count;}int main(){f();return count!=1;}");
}

#[test]
fn inline_rewriting_preserves_existing_statement_expression_values() {
    agrees("static __attribute__((always_inline)) int *get(int *p){return p;} int main(void){int x=7;return *({get(&x);})!=7;}");
    agrees("static __attribute__((always_inline)) int f(int n){typedef int T;struct S{T x;};struct S s={n};return s.x;} int main(void){return f(3)+f(4)!=7;}");
    agrees("static __attribute__((always_inline)) int f(int n){return n;} int main(void){int x=({int n=7;f(n);});return x!=7;}");
}

#[test]
fn a_conditional_array_index_is_evaluated_once() {
    agrees("int main(void){int a[2]={3,7};int n=0;return a[n++ ? 0 : 1]!=7 || n!=1;}");
}

#[test]
fn a_local_bit_field_record_is_initialized_and_updated() {
    agrees("struct S{unsigned a:3;unsigned b:5;};int main(void){struct S s={2,7};s.a+=1;return s.a!=3 || s.b!=7;}");
}

#[test]
fn bit_fields_preserve_neighbors_sign_and_cross_byte_values() {
    for source in [
        "struct S{signed a:4;unsigned b:12;};int main(void){struct S s={-3,1000};int old=s.a++;return old!=-3 || s.a!=-2 || s.b!=1000;}",
        "struct __attribute__((packed)) S{unsigned a:9;unsigned b:7;};int main(void){struct S s={511,73};s.a=0;return s.a!=0 || s.b!=73 || sizeof(s)!=2;}",
        "struct S{char head;unsigned b:3;};int main(void){struct S s={42,5};s.b++;return s.head!=42 || s.b!=6;}",
        "struct S{unsigned a:3;unsigned b:5;};struct S s={2,7};int main(void){return ++s.a!=3 || s.b!=7;}",
        "struct S{unsigned long a:60;unsigned long b:4;};int main(void){struct S s={0xfffffffffffffffUL,9};s.a=3;return s.a!=3 || s.b!=9;}",
        "struct S{unsigned a:3;};int main(void){struct S s={1};return !((s.a-2)<0);}",
    ] { agrees(source); }
}
