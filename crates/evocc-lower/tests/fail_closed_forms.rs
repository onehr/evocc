//! Fail-closed lowering for front-end-complete, IR-deferred forms.

use evocc_chir::AllocationSiteManifest;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn assert_lowering_error(source: &str, ordinal: u32, code: &str, message: &str) {
    let arena = Arena::default();
    let file = FileId::new(9_610 + ordinal);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(
            token.kind,
            TokenKind::Error,
            "lexer rejected case {ordinal}"
        );
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(
        !parser.had_error(),
        "parser rejected case {ordinal}: {:?}",
        parser.diagnostic()
    );
    AllocationSiteManifest::derive(&unit)
        .unwrap_or_else(|error| panic!("allocation manifest case {ordinal}: {error:?}"));
    let error = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect_err("lowering must fail closed");
    assert_eq!(error.code(), code, "wrong lowering diagnostic: {error}");
    assert_eq!(error.to_string(), message, "wrong lowering message");
}

#[test]
fn checked_arithmetic_fails_closed_with_only_e3020() {
    assert_lowering_error(
        "int f(int a) { int r; return __builtin_mul_overflow(a, a, &r); }\n",
        0,
        "E3020",
        "not-lowered: checked-arithmetic",
    );
}

#[test]
fn extended_asm_fails_closed_with_only_e3021() {
    assert_lowering_error(
        "void f(void) { asm volatile(\"nop\" ::: \"memory\"); }\n",
        1,
        "E3021",
        "not-lowered: asm",
    );
}

#[test]
fn asm_goto_fails_closed_with_only_e3021() {
    assert_lowering_error(
        "int h(void) { asm goto(\"b %l[o]\" : : : : o); return 0; o: return 1; }\n",
        2,
        "E3021",
        "not-lowered: asm",
    );
}

#[test]
fn runtime_bit_builtins_fail_closed_with_only_e3023() {
    assert_lowering_error(
        "int f(unsigned value) { return __builtin_clz(value); }\n",
        3,
        "E3023",
        "not-lowered: bit-builtin",
    );
}

#[test]
fn return_address_fails_closed_with_only_e3025() {
    assert_lowering_error(
        "void *f(void) { return __builtin_return_address(0); }\n",
        4,
        "E3025",
        "not-lowered: return-address",
    );
}

#[test]
fn variable_arguments_fail_closed_with_only_e3030() {
    for (ordinal, source) in [
        (
            5,
            "int main(void) { __builtin_va_list ap; __builtin_va_end(ap); return 0; }\n",
        ),
        (
            6,
            "int main(void) { __builtin_va_list ap; return 0; }\n",
        ),
        (
            7,
            "int f(int last, ...) { __builtin_va_list ap; __builtin_va_start(ap, last); return 0; }\n",
        ),
    ] {
        assert_lowering_error(
            source,
            ordinal,
            "E3030",
            "not-lowered: varargs",
        );
    }
}
