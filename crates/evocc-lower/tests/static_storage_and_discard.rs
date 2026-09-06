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
fn static_local_updates_persist_across_calls() {
    check(
        "int f(void){ static int c = 5; c++; return c; } int main(void){ f(); return f(); }",
        7,
    );
    check(
        "int f(void){ static int c = 5; c += 1; return c; } int main(void){ f(); return f(); }",
        7,
    );
}

#[test]
fn conditional_updates_preserve_the_selected_side_effect() {
    check(
        "extern void abort(void); int main(void){ int x = 3, y = 2; if ((x < y ? x++ : y++) != 2) abort(); if (x != 3 || y != 3) abort(); return 0; }",
        0,
    );
    check(
        "int main(void){int x=2,y=3; int old=x<y?x++:y++; return old*100+x*10+y;}",
        233,
    );
    check(
        "int main(void){int x=3,y=2; int now=x<y?(x+=2):(y+=2); return now*100+x*10+y;}",
        434,
    );
    check(
        "int x=3,y=2; int main(void){int old=x<y?x++:y++; return old*100+x*10+y;}",
        233,
    );
    check(
        "int x=3,y=2; int main(void){if((x<y?x++:y++)!=2)return 1; return x==3 && y==3 ? 0 : 2;}",
        0,
    );
    check(
        "int main(void){int x=3,y=2; int *p=&x; int old=*p<y?x++:y++; return old*100+x*10+y;}",
        233,
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
fn address_taken_scalar_parameters_own_frame_local_storage() {
    for (source, expected) in [
        ("int f(int x){int *p=&x; return *p;} int main(void){return f(7);}", 7),
        ("int f(int x){if(x<0)return 0; return *(&x);} int main(void){return f(7);}", 7),
        ("int f(int *p){int **q=&p; return **q;} int main(void){int x=7; return f(&x);}", 7),
        ("int f(int x){int *p=&x; if(x)return f(x-1)+*p; return *p;} int main(void){return f(3);}", 6),
    ] {
        check(source, expected);
    }
}

#[test]
fn block_scope_external_function_addresses_use_the_unit_symbol() {
    check("int main(void){typedef int fn(void); extern fn f; fn *p=f; return p();} int f(void){return 7;}", 7);
}

#[test]
fn control_flow_before_a_declaration_does_not_require_its_future_address() {
    for (source, expected) in [
        ("int f(int x){if(x<0)return 0; struct s{int n;} value={x}; return value.n;} int main(void){return f(7);}", 7),
        ("int f(int x){if(x<0 || x>9)return 0; int a[2]={x,2}; return a[0]+a[1];} int main(void){return f(7);}", 9),
        ("int f(int x){while(x>7)x--; int value=x; int *p=&value; return *p;} int main(void){return f(9);}", 7),
    ] {
        check(source, expected);
    }
}
