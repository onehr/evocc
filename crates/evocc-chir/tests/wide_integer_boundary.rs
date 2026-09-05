//! Execution boundary for front-end-complete 128-bit integer values.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_131);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "fixture must parse: {:?}\nsource: {source}",
        parser.diagnostic()
    );
    unit
}

fn main_symbol(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.name.spelling == "main" && function.body.is_some() =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main")
}

#[test]
fn chir_executes_wide_products_and_shifts() {
    let arena = Arena::default();
    let unit = parse(
        concat!(
            "int main(void) { ",
            "__uint128_t u = ((__uint128_t)1 << 100) + 7; ",
            "unsigned long high = (unsigned long)(u >> 64); ",
            "unsigned long multiplied = (unsigned long)((u * 3) >> 100); ",
            "return high == 68719476736UL && multiplied == 3; }\n",
        ),
        &arena,
    );
    let outcome =
        interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000))
            .unwrap();
    assert_eq!(outcome.return_value.unwrap().raw_bits, 1);
}

#[test]
fn wide_returns_memory_and_calls_do_not_truncate() {
    for (body, expected) in [
        ("return ((U)1<<100)+7;", (1_u128<<100)+7),
        ("return ~(U)0;", u128::MAX),
        ("volatile U x=((U)1<<100)+7; U *p=(U *)&x; return pass(*p);", (1_u128<<100)+7),
        ("union P {U u; unsigned long halves[2];}; union P a={.u=((U)1<<100)+7}; union P b=a; return b.u;", (1_u128<<100)+7),
        ("U x=1; for(int i=0;i<100;i++) x*=2; return x|7;", (1_u128<<100)+7),
    ] {
        let source = format!("typedef unsigned __int128 U; U pass(U x){{return x;}} U main(void){{{body}}}");
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        let outcome = interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000)).unwrap();
        let value = outcome.return_value.unwrap();
        assert_eq!(value.raw_bits, expected, "{source}");
        if expected == u128::MAX {
            assert!(value.mathematical_value(unit.types()).is_err());
        }
    }
}

#[test]
fn wide_division_refusal_covers_values_and_compound_stores() {
    for body in [
        "return x/3;",
        "return x%3;",
        "x/=3; return x;",
        "x%=3; return x;",
        "U *p=&x; *p/=3; return x;",
        "U *p=&x; *p%=3; return x;",
    ] {
        let source = format!("typedef unsigned __int128 U; U main(void){{U x=(U)1<<100; {body}}}");
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        let error =
            interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000))
                .unwrap_err();
        assert_eq!(error.code(), "E3033", "{source}: {error}");
    }
}

#[test]
fn wide_runtime_obligations_remain_typed_failures() {
    for (expression, code) in [
        ("((__int128)1<<126)*2", "E3004"),
        ("(__int128)1<<127", "E3006"),
        ("(__int128)-1<<1", "E3006"),
        ("(__uint128_t)1<<128", "E3005"),
        ("(__uint128_t)1<<(~(__uint128_t)0)", "E3005"),
    ] {
        let source = format!("int main(void){{ return (int)({expression}); }}");
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        let error =
            interpret_scalar_function(&unit, main_symbol(&unit), InterpreterLimits::new(100_000))
                .unwrap_err();
        assert_eq!(error.code(), code, "{source}: {error}");
    }
}
