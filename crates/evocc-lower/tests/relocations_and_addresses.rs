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
                chir.unwrap_or_else(|error| panic!("{source}: {error:?}"))
                    .return_value
                    .unwrap()
                    .raw_bits,
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
fn zero_length_object_accesses_still_report_bounds_and_lifetime() {
    check("char a[0]; int main(void){return a[0];}", Err("E3011"), &[]);
    check(
        "int main(void){char a[0];a[0]=7;return 0;}",
        Err("E3011"),
        &[],
    );
    check(
        "char *f(void){char a[0];return a;}int main(void){return *f();}",
        Err("E3010"),
        &[],
    );
}

#[test]
fn absolute_static_addresses_encode_bits_without_inventing_symbols() {
    let source = include_str!("../../../tests/fixtures/absolute-static-addresses.c");
    check(source, Ok(0), &["relocs=[(0, @object, 0)]"]);
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    for global in &module.globals {
        if global.symbol != "symbolic" {
            assert!(global.relocs.is_empty(), "{}", global.symbol);
        }
    }
    let vram = module
        .globals
        .iter()
        .find(|global| global.symbol == "vram")
        .unwrap();
    assert_eq!(vram.bytes, 0xa000_u64.to_le_bytes());
}

#[test]
fn static_numeric_pointer_offsets_preserve_scaling_and_casts() {
    check(
        include_str!("../../../tests/fixtures/static-address-offsets.c"),
        Ok(0),
        &[],
    );
}

#[test]
fn static_place_addresses_do_not_read_through_their_numeric_base() {
    check(
        include_str!("../../../tests/fixtures/absolute-place-addresses.c"),
        Ok(0),
        &[],
    );
}

#[test]
fn integer_static_objects_can_carry_symbolic_address_relocations() {
    check(
        include_str!("../../../tests/fixtures/integer-address-relocations.c"),
        Ok(0),
        &["@object, 0", "@object, 8", "@object, 12", "@object, 4"],
    );
    check(
        "int object=7; static unsigned long address=(unsigned long)&object;
         int main(void){return *(int *)address;}",
        Ok(7),
        &["relocs=[(0, @object, 0, exposed)]"],
    );
    for (initializer, expected) in [
        ("&object", Err("E3013")),
        ("(int *)(unsigned long)&object", Ok(7)),
    ] {
        check(
            &format!(
                "int object=7; static int *p={initializer};
                 int main(void){{unsigned long address;
                 __builtin_memcpy(&address,&p,8);return *(int *)address;}}"
            ),
            expected,
            &[],
        );
    }
}

#[test]
fn absolute_static_addresses_do_not_invent_access_or_call_provenance() {
    check(
        "static int *p=(int *)0x12340; int main(void){return *p;}",
        Err("E3013"),
        &[],
    );
    check(
        "static int (*p)(void)=(int (*)(void))0x12340; int main(void){return p();}",
        Err("E3016"),
        &[],
    );
}

#[test]
fn ordered_pointer_comparisons_use_unsigned_addresses() {
    check(
        "int main(void) { int a[3]; int *p=a, *q=a+2; return (p<q) | ((p<=q)<<1) | ((q>p)<<2) | ((q>=p)<<3); }",
        Ok(15),
        &["ptr.to_int", "icmp.ult", "icmp.ule"],
    );
    check(
        "int main(void) { int a[3]; int *p=a+1; return (p<p) | ((p<=p)<<1) | ((p>p)<<2) | ((p>=p)<<3); }",
        Ok(10),
        &["ptr.to_int", "icmp.ult", "icmp.ule"],
    );
}

#[test]
fn static_addresses_relocate_objects_members_and_functions() {
    check("int a[4]={1,2,3,7}; int f(void){return 9;} struct s {char c; long n;} s={.n=11}; static int *p=&a[3]; static long *q=&s.n; static int (*r)(void)=(int (*)(void))(unsigned long)&f; int main(void){return *p+*q+r();}", Ok(27), &["relocs=[", "@a, 12", "@s, 8", "@f, 0"]);
    check(
        "extern int a; int *p=&a; int main(void){return *p;}",
        Err("E3040"),
        &["extern @a", "relocs=[(0, @a, 0)]"],
    );
}

#[test]
fn static_address_conditions_recognize_typed_null_pointer_values() {
    for condition in ["f == (fn_type)0", "(fn_type)(void *)0 == f"] {
        check(
            &format!("typedef int (*fn_type)(void);int f(void){{return 7;}}int g(void){{return 11;}}static fn_type p=({condition})?g:f;int main(void){{return p();}}"),
            Ok(7),
            &["relocs=[(0, @f, 0)]"],
        );
    }
    check(
        "typedef int (*fn_type)(void);int f(void){return 7;}static const void *data=(f==(fn_type)((void *)0))?f:f;int main(void){return ((fn_type)data)();}",
        Ok(7),
        &["relocs=[(0, @f, 0)]"],
    );
    check(
        "int a=7,b=11;static int *p=(&a!=(int *)0)?&a:&b;int main(void){return *p;}",
        Ok(7),
        &["relocs=[(0, @a, 0)]"],
    );
}

#[test]
fn function_relocations_travel_through_object_pointer_storage() {
    check(
        "int f(void){return 7;} static void *p=(void *)(unsigned long)&f; int main(void){return p!=0;}",
        Ok(1),
        &["relocs=[(0, @f, 0, exposed)]"],
    );
    check(
        "int f(void){return 7;} static void *p=(void *)(unsigned long)&f; int main(void){return ((int (*)(void))(unsigned long)p)();}",
        Ok(7),
        &["ptr.to_int", "ptr.from_int.fnptr"],
    );
}

#[test]
fn address_of_a_function_uses_its_call_signature_not_object_storage() {
    check(
        "int f(void){return 7;} int main(void){int (*p)(void)=&f; return p();}",
        Ok(7),
        &["func_addr @f"],
    );
    check(
        "int f(); void *address(void){return &f;} int f(){return 7;} int main(void){return ((int (*)(void))address())();}",
        Ok(7),
        &["func_addr @f", "ptr.to_int"],
    );
}

#[test]
fn object_aliases_share_storage_and_relocation_provenance() {
    check(
        "int a=7; extern int b __attribute__((alias(\"a\"))); static int *p=&b; int main(void){b=9; return (a==9) | ((&a==&b)<<1) | ((*p==9)<<2);}",
        Ok(7),
        &["alias @b = @a"],
    );
}

#[test]
fn function_aliases_share_callable_identity_and_keep_internal_targets() {
    check(
        "static int value(void){return 7;} int alternative(void) __attribute__((alias(\"value\"))); int main(void){return alternative() + (alternative==value);}",
        Ok(8),
        &["alias @alternative = @value", "visibility=internal"],
    );
    check(
        "int value(void){return 7;} int a(void) __attribute__((alias(\"value\"))); int b(void) __attribute__((alias(\"a\"))); int main(void){return b()+(a==b);}",
        Ok(8),
        &["alias @a = @value", "alias @b = @value"],
    );
    check(
        "static inline __attribute__((always_inline)) int value(void){return 7;} int a(void) __attribute__((alias(\"value\"))); int main(void){return a();}",
        Ok(7),
        &["alias @a = @value", "func @value"],
    );
    check(
        "int install(void){extern int value(void);return 0;} int value(void){return 7;} int alternative(void) __attribute__((alias(\"value\"))); int main(void){return alternative();}",
        Ok(7),
        &["alias @alternative = @value"],
    );
}

#[test]
fn weak_marks_accumulate_over_declarations_without_changing_execution() {
    check(
        "int fallback(void) __attribute__((weak)); int fallback(void){return 3;} extern int value __attribute__((weak)); int value=7; int main(void){return fallback()+value;}",
        Ok(10),
        &["linkage=weak"],
    );
    check(
        "int value=7; int main(void){extern int value __attribute__((weak)); return value;}",
        Ok(7),
        &["linkage=weak"],
    );
    check(
        "extern int value __attribute__((weak)); int main(void){return value;}",
        Err("E3040"),
        &["linkage=weak"],
    );
    check(
        "int fallback(void) __attribute__((weak)); int main(void){return fallback();}",
        Err("E3039"),
        &["linkage=weak"],
    );
}

#[test]
fn invalid_symbol_linkage_is_rejected_by_both_consumers() {
    for (source, code) in [
        ("static int x __attribute__((weak)); int main(void){return 0;}", "E3060"),
        ("static int f(void) __attribute__((weak)); int main(void){return 0;}", "E3060"),
        ("extern int x __attribute__((alias(\"absent\"))); int main(void){return 0;}", "E3061"),
        ("extern int x; extern int a __attribute__((alias(\"x\"))); int main(void){return 0;}", "E3061"),
        ("int a(void) __attribute__((alias(\"b\"))); int b(void) __attribute__((alias(\"a\"))); int main(void){return 0;}", "E3061"),
    ] {
        let arena = Arena::default();
        let unit = parse(source, &arena);
        let main = unit.iter().find_map(|d| match d { CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol), _ => None }).unwrap();
        assert_eq!(evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap_err().code(), code, "{source}");
        assert_eq!(evocc_chir::interp::interpret_scalar_function(&unit, main, evocc_chir::interp::InterpreterLimits::new(1000)).unwrap_err().code(), code, "{source}");
    }
}

#[test]
fn address_of_a_dereferenced_function_pointer_preserves_the_pointer() {
    check(
        "typedef int (*fn)(void); int calls; int f(void){return 7;} fn get(void){calls++;return f;} int main(void){fn p=&*get(); return p()+calls;}",
        Ok(8),
        &["func_addr @f", "call @get"],
    );
    check(
        "typedef int (*fn)(void); int calls; int f(void){return 7;} fn get(void){calls++;return f;} int main(void){fn p=*get(); return p()+calls;}",
        Ok(8),
        &["func_addr @f", "call @get"],
    );
    check(
        "int main(void){int (*p)(void)=0; return &*p==p;}",
        Ok(1),
        &["fnptr"],
    );
}

#[test]
fn pointer_integer_round_trip_retains_live_exposed_provenance() {
    check(
        "int main(void){int x=4; int *p=(int *)(unsigned long)&x; return p ? 1 : 0;}",
        Ok(1),
        &["ptr.from_int"],
    );
    check(
        "int main(void){int x=4; int *p=&x; int *q=(int *)(unsigned long)p; return p==q && *q==4;}",
        Ok(1),
        &["ptr.to_int", "ptr.from_int"],
    );
    check("int f(void){return 7;} int main(void){int (*p)(void)=f; int (*q)(void)=(int (*)(void))(unsigned long)p; return p==q && q()==7;}", Ok(1), &["ptr.to_int", "ptr.from_int.fnptr"]);
    check("int main(void){int a[3]={2,5,9}; unsigned long n=(unsigned long)&a[0]; int *p=(int *)(n+sizeof(int)); return *p;}", Ok(5), &["ptr.to_int", "ptr.from_int.ptr<generic>"]);
    check("int main(void){int n=-1; unsigned u=0xffffffffu; return (unsigned long)(void *)n == ~0ul && (unsigned long)(void *)u == 0xfffffffful;}", Ok(1), &["sext.bits<64>", "zext.bits<64>", "ptr.from_int"]);
    check("int main(void){int x; int *p=&x; return (unsigned short)p == (unsigned short)(unsigned long)p;}", Ok(1), &["ptr.to_int", "trunc.bits<16>"]);
}

#[test]
fn static_typed_null_addresses_are_representable() {
    check(
        "struct s {int n;}; struct d {const struct s *p;}; static struct d d={(const struct s *)0}; int main(void){return d.p==0;}",
        Ok(1),
        &[],
    );
}

#[test]
fn static_flexible_array_member_addresses_are_representable() {
    check(
        "struct font {unsigned h[4]; const unsigned char data[];} __attribute__((packed)); static const struct font f={{0,0,2,0},{7,9}}; const unsigned char *p=f.data; int main(void){return p[1];}",
        Ok(9),
        &["@f, 16"],
    );
}

#[test]
fn object_and_function_pointers_convert_through_integer_forms() {
    check(
        "int f(void){return 7;} int main(void){return ((int (*)(void))((1ul<<63)|16ul))();}",
        Ok(7),
        &["ptr.from_int.fnptr"],
    );
    check("typedef int (*callback)(void); int f(void){return 7;} callback recover(void *p){return p;} int main(void){void *p=f; return recover(p)();}", Ok(7), &["ptr.to_int", "ptr.from_int.fnptr"]);
    check("typedef int (*callback)(void); int main(void){int object=0; callback p=(callback)(void *)&object; return p();}", Err("E3016"), &["ptr.to_int", "ptr.from_int.fnptr"]);
}

#[test]
fn pointer_differences_lower_to_signed_element_counts() {
    check(
        "long f(int *a,int *b){return a-b;} int main(void){int a[4]; return f(a+3,a);}",
        Ok(3),
        &["ptr.to_int", "sub.bits<64>", "sdiv.bits<64>"],
    );
    check(
        "long f(void *a,void *b){return a-b;} int main(void){char a[11]; return f(a,a+10)==-10;}",
        Ok(1),
        &["ptr.to_int", "sub.bits<64>", "sdiv.bits<64>"],
    );
}

#[test]
fn assignment_addresses_and_loaded_values_survive_rhs_branches() {
    for source in [
        "int main(void){int n=3; int *p=&n; *p=n?5:6; return n;}",
        "int main(void){int n=3; int *p=&n; *p+=n?2:6; return n;}",
        "int n=3; int main(void){n=n?5:6; return n;}",
        "int n=3; int main(void){n+=n?2:6; return n;}",
    ] {
        check(source, Ok(5), &["cond_br", "store"]);
    }
}

#[test]
fn initializer_addresses_survive_branches_and_later_actions() {
    for source in [
        "int main(void){int c=1; int v=c?5:6; int *p=&v; return *p;}",
        "struct S{int a,b;}; int main(void){int c=1; struct S s={c?2:3,c?3:4}; return s.a+s.b;}",
        "struct S{int a,b;}; int main(void){int c=1; struct S *p=&(struct S){c?2:3,c?3:4}; return p->a+p->b;}",
    ] {
        check(source, Ok(5), &["cond_br", "store"]);
    }
}

#[test]
fn addresses_of_aggregate_subobjects_use_the_resolved_place() {
    for source in [
        "struct S{int v;}; int main(void){struct S a[2]={{3},{7}}; struct S *p=&a[1]; return p->v;}",
        "struct S{int v;}; struct O{struct S s;}; int main(void){struct O o={{7}}; struct S *p=&o.s; return p->v;}",
        "struct S{int v;}; struct O{struct S s;}; int main(void){struct O o={{7}}; struct O *q=&o; struct S *p=&q->s; return p->v;}",
        "struct S{int v;}; int main(void){int n=6; struct S *p=&(struct S){++n}; return p->v+n-7;}",
    ] {
        check(source, Ok(7), &["load", "store"]);
    }
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

#[test]
fn array_members_decay_to_addresses_at_call_sites() {
    for source in [
        "int first(const unsigned char *p){return *p;} struct S{unsigned char magic[2];}; int main(void){struct S s={{7,8}}; struct S *p=&s; return first(p->magic);}",
        "int first(const unsigned char *p){return *p;} struct S{unsigned char magic[2];}; int main(void){struct S s={{7,8}}; return first(s.magic);}",
        "int first(const int *p){return *p;} int main(void){int a[2][2]={{7,8},{9,10}}; return first(a[0]);}",
    ] {
        check(source, Ok(7), &["load", "call"]);
    }
}
