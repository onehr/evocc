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
    // The wide IR executor is delivered separately; these fixtures pin C-HIR
    // values and the actual lowering, without pretending to execute it here.
    for (source, expected) in [
        ("int main(void){unsigned long r; return __builtin_add_overflow(-1,0,&r);}", 1),
        ("long main(void){long r; int o=__builtin_mul_overflow(70000L,70000L,&r); return o ? 0 : r;}", 4900000000),
        ("int main(void){unsigned long r; return __builtin_mul_overflow(~0ul,2,&r);}", 1),
        ("int main(void){unsigned char r; int o=__builtin_add_overflow(200,100,&r); return 100*o+r;}", 144),
        ("long main(void){long r; int o=__builtin_sub_overflow(4000000000u,-1,&r); return o ? 0 : r;}", 4000000001),
    ] { lowered(source, expected, &["icmp.ne", "store"], false); }
}

#[test]
fn unsigned_checked_products_use_the_full_unsigned_exact_width() {
    for (arguments, expected) in [("70000ul,70000ul", 0), ("~0ul,2ul", 1), ("~0ul,~0ul", 1)] {
        lowered(&format!("int product(unsigned long a,unsigned long b,unsigned long *r){{return __builtin_mul_overflow(a,b,r);}} int main(void){{unsigned long r; return product({arguments},&r);}}"), expected, &["mul.bits<128>", "zext.bits<128>", "trunc.bits<64>"], false);
    }
    lowered("int main(void){__uint128_t r; int o=__builtin_mul_overflow(~0ul,~0ul,&r); return o || (r>>64)!=18446744073709551614ul;}", 0, &["mul.bits<128>"], false);
    lowered(
        "int main(void){__int128_t r; return __builtin_mul_overflow(~0ul,~0ul,&r);}",
        1,
        &["mul.bits<128>", "icmp.slt"],
        false,
    );
    lowered("int main(void){long r; return __builtin_mul_overflow((unsigned short)65535,(unsigned short)65535,&r) || r!=4294836225L;}", 0, &["mul.bits<32>", "zext.bits<64>"], true);
}

#[test]
fn checked_products_beyond_ir_width_still_have_defined_oracle_values() {
    let source = "int main(void){__uint128_t r; int o=__builtin_mul_overflow(~(__uint128_t)0,2u,&r); return o && (r>>64)==~0ul && (unsigned long)r==~1ul;}";
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let main = unit
        .iter()
        .find_map(|d| match d {
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
        .unwrap()
        .return_value
        .unwrap()
        .raw_bits,
        1
    );
    assert_eq!(
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .unwrap_err()
            .code(),
        "E3044"
    );
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
