use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> CHirUnit<'a> {
    let file = FileId::new(45_005);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    unit
}

fn lowered(source: &str, expected: u128, forms: &[&str], execute_ir: bool) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("specified builtin lowers");
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
    let result = evocc_chir::interp::interpret_scalar_function(
        &unit,
        main,
        evocc_chir::interp::InterpreterLimits::new(100_000),
    )
    .unwrap();
    assert_eq!(result.return_value.unwrap().raw_bits, expected);
    if execute_ir {
        let result = evocc_evoir::interpret_module(
            &module,
            "main",
            &module
                .functions
                .iter()
                .find(|f| f.name == "main")
                .unwrap()
                .params
                .iter()
                .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
                .collect::<Vec<_>>(),
            evocc_evoir::InterpreterLimits::new(100_000),
        )
        .unwrap();
        assert_eq!(u128::from(result.return_values[0].raw_bits), expected);
    }
}

#[test]
fn file_assembly_is_preserved_and_not_executed() {
    lowered(
        "asm(\".globl named\\n\"); int main(void){return 7;}",
        7,
        &["asm \".globl named\\x0a\""],
        true,
    );
}

#[test]
fn checked_arithmetic_uses_exact_width_and_round_trip_representability() {
    for (source, expected) in [
        ("int main(void){unsigned long r; return __builtin_add_overflow(-1,0,&r);}", 1),
        ("long main(void){long r; int o=__builtin_mul_overflow(70000L,70000L,&r); return o ? 0 : r;}", 4900000000),
        ("int main(void){unsigned long r; return __builtin_mul_overflow(~0ul,2,&r);}", 1),
        ("int main(void){unsigned char r; int o=__builtin_add_overflow(200,100,&r); return 100*o+r;}", 144),
        ("long main(void){long r; int o=__builtin_sub_overflow(4000000000u,-1,&r); return o ? 0 : r;}", 4000000001),
    ] { lowered(source, expected, &["icmp.ne", "store"], true); }
}

#[test]
fn unsigned_checked_products_use_the_full_unsigned_exact_width() {
    for (arguments, expected) in [("70000ul,70000ul", 0), ("~0ul,2ul", 1), ("~0ul,~0ul", 1)] {
        lowered(&format!("int product(unsigned long a,unsigned long b,unsigned long *r){{return __builtin_mul_overflow(a,b,r);}} int main(void){{unsigned long r; return product({arguments},&r);}}"), expected, &["mul.bits<128>", "zext.bits<128>", "trunc.bits<64>"], true);
    }
    lowered(
        "int main(void){long r; return __builtin_mul_overflow(~0ul,~0ul,&r);}",
        1,
        &["mul.bits<128>", "icmp.slt"],
        true,
    );
    lowered("int main(void){long r; return __builtin_mul_overflow((unsigned short)65535,(unsigned short)65535,&r) || r!=4294836225L;}", 0, &["mul.bits<32>", "zext.bits<64>"], true);
}

#[test]
fn checked_products_beyond_ir_width_still_have_defined_oracle_values() {
    lowered(
        "int main(void){unsigned long r; int o=__builtin_mul_overflow(~0ul,-2L,&r); return o && r==2;}",
        1,
        &["mul.bits<128>", "store"],
        true,
    );
}

#[test]
fn wide_checked_arithmetic_preserves_wrapped_results_and_overflow_flags() {
    lowered(
        include_str!("../../../tests/fixtures/wide-checked-arithmetic.c"),
        0,
        &["add.bits<128>", "sub.bits<128>", "mul.bits<128>", "store"],
        true,
    );
}

#[test]
fn checked_products_do_not_introduce_division_runtime_dependencies() {
    let arena = Arena::default();
    let unit = parse(
        include_str!("../../../tests/fixtures/wide-checked-arithmetic.c"),
        &arena,
    );
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    for inst in module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
    {
        assert!(
            !matches!(inst, evocc_evoir::Inst::UDiv(..) | evocc_evoir::Inst::SDiv(..)),
            "checked multiplication introduced division and a freestanding runtime dependency: {inst:?}"
        );
    }
}

#[test]
fn checked_product_limb_carries_preserve_full_width_results() {
    lowered(
        "typedef unsigned __int128 U;
         int product(U a,U b,U *p){return __builtin_mul_overflow(a,b,p);}
         int main(void){U b=(U)1<<64,r;
           if(product(b-1,b-1,&r)||r!=~(U)0-2*b+2)return 1;
           if(product(b,b-1,&r)||r!=~(U)0-b+1)return 2;
           if(product(b+1,b-1,&r)||r!=~(U)0)return 3;
           if(product(b,b,&r)!=1||r!=0)return 4;
           if(product(b+1,b+1,&r)!=1||r!=2*b+1)return 5;
           if(product((U)1<<127,0,&r)||r!=0)return 6;
           return 0;}",
        0,
        &["mul.bits<128>"],
        true,
    );
}

#[test]
fn wide_checked_boundaries_include_cancellation_and_conditional_operands() {
    for body in [
        "return __builtin_add_overflow(imin, (U)1<<127, &u) || u!=0;",
        "return __builtin_sub_overflow((U)0, imin, &u) || u!=((U)1<<127);",
        "return !__builtin_sub_overflow((U)0, imin, &i) || i!=imin;",
        "return !__builtin_add_overflow(imin, imin, &i) || i!=0;",
        "return !__builtin_sub_overflow(umax, -(I)1, &u) || u!=0;",
        "return __builtin_mul_overflow(umax, (U)0, &u) || u!=0;",
        "return __builtin_mul_overflow(-(I)1, (U)1<<127, &i) || i!=imin;",
        "return !__builtin_mul_overflow(imin, (I)1, &u) || u!=((U)1<<127);",
        "return !__builtin_add_overflow(umax, imax, &i) || i!=imax-1;",
        "unsigned char c; return __builtin_add_overflow((U)255, (U)0, &c) || c!=255;",
        "signed char c; return __builtin_add_overflow((I)-128, (U)0, &c) || c!=-128;",
        "U values[2]={1,umax}; int p=0,c=1; int o=__builtin_add_overflow(values[p++], c?umax:0, p++?&u:&u); return p!=2 || !o || u!=0;",
        "volatile U v=99; int o=__builtin_add_overflow(umax,(U)1,&v); return !o || v!=0;",
    ] {
        lowered(
            &format!("typedef unsigned __int128 U; typedef __int128 I; int main(void){{U umax=~(U)0,u; I imax=(I)(umax>>1),imin=-imax-1,i; {body}}}"),
            0,
            &["store"],
            true,
        );
    }
}

#[test]
fn bit_builtins_lower_to_total_operations_and_explicit_zero_obligations() {
    lowered("int f(unsigned x){return __builtin_clz(x)+__builtin_ctz(x)+__builtin_popcount(x);} int main(void){return f(8);}", 32, &["ctlz.bits<32>", "cttz.bits<32>", "ctpop.bits<32>", "ub.assume.bit_builtin_zero_operand"], true);
    lowered(
        "int f(unsigned x){return __builtin_ffs(x);} int main(void){return f(0)+f(16);}",
        5,
        &["cttz.bits<32>", "cond_br"],
        true,
    );
    lowered("unsigned f(unsigned x){return __builtin_bswap32(x);} unsigned main(void){return f(0x11223344);}", 0x44332211, &["bswap.bits<32>"], true);
}

#[test]
fn zero_bit_builtin_operands_keep_the_typed_runtime_failure() {
    let arena = Arena::default();
    let unit = parse(
        "int f(unsigned x){return __builtin_clz(x);} int main(void){return f(0);}",
        &arena,
    );
    let main = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        evocc_chir::interp::interpret_scalar_function(
            &unit,
            main,
            evocc_chir::interp::InterpreterLimits::new(10000)
        )
        .unwrap_err()
        .code(),
        "E3022"
    );
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
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
        .unwrap_err()
        .code(),
        "E3022"
    );
}

#[test]
fn checked_operands_survive_control_flow_and_unsigned_conversion() {
    lowered("int main(void){int n=1,r; int o=__builtin_add_overflow(n++, n ? 2 : 3, n++ ? &r : &r); return n*100+r*10+o;}", 330, &["add.bits<64>", "cond_br"], true);
    lowered(
        "int main(void){unsigned long r; return __builtin_add_overflow(-1,0,&r);}",
        1,
        &["icmp.slt", "or.bits<1>"],
        true,
    );
}
