use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> CHirUnit<'a> {
    let file = FileId::new(45_006);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    unit
}

fn check(source: &str, expected: Result<u64, &str>, forms: &[&str]) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("address conversion and symbolic initialization lower");
    let text = evocc_evoir::render_module(&module);
    for form in forms {
        assert!(text.contains(form), "missing {form}: {text}");
    }
    assert_eq!(
        evocc_evoir::render_module(&evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()),
        text
    );
    let main = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    let chir = evocc_chir::interp::interpret_scalar_function(
        &unit,
        main,
        evocc_chir::interp::InterpreterLimits::new(100_000),
    );
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let arguments = function
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let ir = evocc_evoir::interpret_module(
        &module,
        "main",
        &arguments,
        evocc_evoir::InterpreterLimits::new(100_000),
    );
    match expected {
        Ok(value) => {
            assert_eq!(
                chir.unwrap().return_value.unwrap().raw_bits,
                u128::from(value)
            );
            assert_eq!(ir.unwrap().return_values[0].raw_bits, value);
        }
        Err(code) => {
            assert_eq!(chir.unwrap_err().code(), code);
            assert_eq!(ir.unwrap_err().code(), code);
        }
    }
}

#[test]
fn static_addresses_relocate_objects_members_and_functions() {
    check("int a[4]={1,2,3,7}; int f(void){return 9;} struct s {char c; long n;} s={.n=11}; static int *p=&a[3]; static long *q=&s.n; static int (*r)(void)=(int (*)(void))(unsigned long)&f; int main(void){return *p+*q+r();}", Ok(27), &["relocs=[", "@a, 12", "@s, 8", "@f, 0"]);
}

#[test]
fn pointer_integer_round_trip_retains_live_exposed_provenance() {
    check("int main(void){int a[3]={2,5,9}; unsigned long n=(unsigned long)&a[0]; int *p=(int *)(n+sizeof(int)); return *p;}", Ok(5), &["ptr.to_int", "ptr.from_int.ptr<generic>"]);
    check("int main(void){int n=-1; unsigned u=0xffffffffu; return (unsigned long)(void *)n == ~0ul && (unsigned long)(void *)u == 0xfffffffful;}", Ok(1), &["sext.bits<64>", "zext.bits<64>", "ptr.from_int"]);
    check("int main(void){int x; int *p=&x; return (unsigned short)p == (unsigned short)(unsigned long)p;}", Ok(1), &["ptr.to_int", "trunc.bits<16>"]);
}

#[test]
fn integer_addresses_resolve_only_live_exposed_storage() {
    check(
        "int main(void){unsigned long n=1; return *(int *)n;}",
        Err("E3013"),
        &["ptr.from_int"],
    );
    check("unsigned long f(void){int x=9; return (unsigned long)&x;} int main(void){return *(int *)f();}", Err("E3013"), &["ptr.to_int", "ptr.from_int"]);
    check(
        "int main(void){int *p=(int *)0; return p==0;}",
        Ok(1),
        &["ptr.null"],
    );
}
