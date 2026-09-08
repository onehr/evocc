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
fn expression_scopes_keep_their_static_objects_in_the_module() {
    for source in [
        "int f(void){if(({static int n=0; ++n;})) return 7;return 0;} int main(void){return f()!=7 || f()!=7;}",
        "int f(void){return ({static int n=0; ++n;});} int main(void){int a=f();int b=f();return a!=1 || b!=2;}",
        "int f(void){int x=({static int n=5; n++;});return x;} int main(void){int a=f();int b=f();return a!=5 || b!=6;}",
        "int main(void){int count=0;for(;({static int n=0; n++<3;});) ++count;return count!=3;}",
    ] { check(source, 0); }
}

#[test]
fn integer_mode_storage_and_call_values_agree_in_both_interpreters() {
    check(
        include_str!("../../../tests/fixtures/integer-mode-memory.c"),
        0,
    );
}

#[test]
fn packed_members_use_member_alignment_for_initializers_reads_and_updates() {
    for source in [
        "struct __attribute__((packed)) s { char pad; volatile long x; }; int main(void){struct s a={1, 5}; a.x+=2; ++a.x; return a.x!=8 || a.pad!=1;}",
        "struct inner {long x;}; struct __attribute__((packed)) outer {char pad; struct inner a[2];}; int main(void){struct outer o={1, {{5}, {7}}}; o.a[1].x++; return o.a[0].x!=5 || o.a[1].x!=8;}",
        "struct __attribute__((packed)) s {char pad;long x;}; int main(void){struct s a={.pad=1};return a.pad!=1 || a.x!=0;}",
    ] {
        check(source, 0);
    }
}

#[test]
fn ordinary_unaligned_pointer_dereferences_remain_runtime_errors() {
    let source = "int main(void){long a[2]={0,0};return *(long *)((char *)a+1);}";
    let arena = Arena::default();
    let file = FileId::new(45_011);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let main = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), "main")
        .unwrap()
        .unwrap();
    assert_eq!(
        interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
            .unwrap_err()
            .code(),
        "E3012"
    );
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let args = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap()
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
        .unwrap_err()
        .code(),
        "E3012"
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
fn void_return_expressions_execute_effects_without_a_data_result() {
    check(
        include_str!("../../../tests/fixtures/void-return-effects.c"),
        0,
    );
}

#[test]
fn void_return_does_not_execute_the_following_statement() {
    check("int n; void sink(void){n++;} void f(void){return sink();n=99;} int main(void){f();return n!=1;}", 0);
}

#[test]
fn discarded_void_conditionals_preserve_the_selected_effect() {
    check("int n; void sink(int x){n+=x;} int main(void){int c=0; c++ ? sink(99) : sink(3); c ? (void)(n+=5) : sink(99); return n!=8 || c!=1;}", 0);
}

#[test]
fn effect_only_returns_do_not_admit_mismatched_value_types() {
    for source in [
        "void f(void){return 1;} int main(void){f();return 0;}",
        "int f(void){return (void)1;} int main(void){return f();}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45_010);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        assert!(
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).is_err()
        );
        let main = unit
            .iter()
            .find_map(|decl| match decl {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        assert!(interp::interpret_scalar_function(
            &unit,
            main,
            interp::InterpreterLimits::new(10000)
        )
        .is_err());
    }
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
