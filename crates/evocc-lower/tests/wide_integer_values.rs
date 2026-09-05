use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_evoir::{Inst, Module};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{EvoIRType, TargetDataLayout};

fn parse<'a>(source: &str, arena: &'a Arena) -> CHirUnit<'a> {
    let file = FileId::new(45_003);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect::<Vec<_>>();
    assert!(tokens.iter().all(|token| token.kind != TokenKind::Error));
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
    unit
}

fn lower(source: &str) -> Module {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("{} {error}", error.code()));
    evocc_evoir::validate_module(&module).unwrap();
    let text = evocc_evoir::render_module(&module);
    assert_eq!(
        evocc_evoir::parse_section9_module(text.as_bytes()).unwrap(),
        module
    );
    module
}

#[test]
fn products_shifts_and_call_boundaries_keep_all_128_bits() {
    let module = lower("typedef unsigned __int128 U; U identity(U x){return x;} unsigned long product(unsigned long a, unsigned long b, int s){U u=identity((U)a*b);return (unsigned long)(u>>s);}");
    let identity = module
        .functions
        .iter()
        .find(|f| f.name == "identity")
        .unwrap();
    assert_eq!(identity.params[0].ty, EvoIRType::Bits(128));
    assert_eq!(identity.ret_tys[0], EvoIRType::Bits(128));
    let instructions = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .collect::<Vec<_>>();
    assert!(instructions
        .iter()
        .any(|i| matches!(i, Inst::ZExt(value, _, _) if value.ty == EvoIRType::Bits(128))));
    assert!(instructions
        .iter()
        .any(|i| matches!(i, Inst::Mul(value, _, _) if value.ty == EvoIRType::Bits(128))));
    assert!(instructions
        .iter()
        .any(|i| matches!(i, Inst::LShr(value, _, _) if value.ty == EvoIRType::Bits(128))));
    assert!(instructions
        .iter()
        .any(|i| matches!(i, Inst::Trunc(value, _, _) if value.ty == EvoIRType::Bits(64))));
}

#[test]
fn unions_memory_and_assembly_halves_preserve_the_wide_value() {
    let module = lower("typedef unsigned __int128 U; union pair {U value; unsigned long halves[2];}; union pair copy_pair(union pair p){return p;} U exchange(U *p,U v){union pair a={.value=*p}; union pair b=copy_pair(a); __asm__ volatile(\"\" : \"+r\"(b.halves[0]), \"+r\"(b.halves[1])); *p=v;return b.value;}");
    let instructions = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .collect::<Vec<_>>();
    assert!(instructions
        .iter()
        .any(|i| matches!(i, Inst::Load { result, .. } if result.ty == EvoIRType::Bits(128))));
    assert!(instructions
        .iter()
        .any(|i| matches!(i, Inst::Store { value, .. } if value.ty == EvoIRType::Bits(128))));
    assert!(instructions.iter().any(|i| matches!(i, Inst::Copy { .. })));
}

#[test]
fn chir_executes_the_sealed_wide_reference_values() {
    for (body, expected) in [
        (
            "U u=(U)0xffffffffffffffffUL * 0xfffffffffffffffeUL; return (unsigned long)(u>>64);",
            0xfffffffffffffffd_i128,
        ),
        (
            "U u=(U)0x123456789abcdef0UL * 0xfedcba9876543210UL; return (unsigned long)(u>>37);",
            0x56bbeba1111b6c47_i128,
        ),
        (
            "U u=((U)1<<100)+7; return (unsigned long)(u>>64);",
            68719476736_i128,
        ),
        ("U u=((U)1<<100)+7; return (unsigned long)((u*3)>>100);", 3),
        ("U u=(((U)1<<64)|~0UL)+1; return (unsigned long)(u>>64);", 2),
        (
            "U u=(U)1<<64; return (u < (~(U)0>>64)) + 10*((U)1 < u);",
            10,
        ),
        (
            "U u=~(U)0; U mask=(u<<37)&(u>>11); return (unsigned long)(mask>>64);",
            0x1fffffffffffff_i128,
        ),
    ] {
        let source = format!("typedef unsigned __int128 U; unsigned long main(void){{{body}}}");
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        let main = unit
            .iter()
            .find_map(|d| match d {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        let result =
            interpret_scalar_function(&unit, main, InterpreterLimits::new(100_000)).unwrap();
        assert_eq!(
            result
                .return_value
                .unwrap()
                .mathematical_value(unit.types())
                .unwrap(),
            expected,
            "{body}"
        );
    }
}

#[test]
fn wide_division_and_remainder_keep_the_declared_refusal() {
    for op in ["/", "%"] {
        let source = format!("typedef unsigned __int128 U; U f(U a,U b){{return a {op} b;}}");
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        assert_eq!(
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
                .unwrap_err()
                .code(),
            "E3033"
        );
    }
}
