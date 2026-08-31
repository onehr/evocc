//! Null pointer and static address constant rules from specification 10.1.19.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_124);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!(
            "must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    let error = parse(source, &arena).expect_err("fixture must be rejected");
    assert_eq!(error.code, code, "wrong diagnostic: {error:?}");
}

#[test]
fn void_pointer_spelled_null_converts_to_a_function_pointer() {
    accepts("struct c { void (*fn)(int); }; void f(struct c *call) { call->fn = ((void *)0); }\n");
}

#[test]
fn null_pointer_constants_are_admitted_in_every_conversion_context() {
    accepts(concat!(
        "typedef void callback(int); struct holder { callback *fn; }; ",
        "void consume(callback *fn) { (void)fn; } ",
        "callback *return_null(void) { return (void *)0; } ",
        "int check(struct holder *holder) { ",
        "callback *local = (void *)0; ",
        "holder->fn = (void *)0; consume((void *)0); ",
        "return local == (void *)0 && return_null() == (void *)0; }\n",
    ));
}

#[test]
fn the_measured_delayed_call_line_is_preserved_verbatim() {
    accepts(concat!(
        "struct delayed_call { void (*fn)(void *); void *arg; };\n",
        "static inline __attribute__((__gnu_inline__)) __attribute__((__unused__)) ",
        "__attribute__((__no_instrument_function__)) void clear_delayed_call",
        "(struct delayed_call *call)\n{\n call->fn = ((void *)0);\n}\n",
    ));
}

#[test]
fn nonzero_and_runtime_zero_void_pointers_do_not_become_null_constants() {
    rejects(
        "struct c { void (*fn)(int); }; void f(struct c *call) { call->fn = (void *)1; }\n",
        "E2102",
    );
    rejects(
        "struct c { void (*fn)(int); }; void f(struct c *call, int zero) { call->fn = (void *)zero; }\n",
        "E2102",
    );
}

#[test]
fn an_address_constant_survives_an_integer_round_trip() {
    accepts(
        "typedef unsigned long uintptr_t; int k(char *s); static void *p = (void *)(uintptr_t)&k;\n",
    );
}

#[test]
fn the_measured_export_symbol_line_is_preserved_verbatim() {
    accepts(concat!(
        "typedef unsigned long uintptr_t; ",
        "int kstrtoull(const char *s, unsigned int base, unsigned long long *res); ",
        "extern typeof(kstrtoull) kstrtoull; static void * ",
        "__attribute__((__used__)) __attribute__((__section__(\".discard.addressable\"))) ",
        "__UNIQUE_ID___addressable_kstrtoull279 = (void *)(uintptr_t)&kstrtoull; ",
        "asm(\".section \\\".export_symbol\\\",\\\"a\\\" ; __export_symbol_kstrtoull: ; ",
        ".asciz \\\"\\\" ; .ascii \\\"\\\" \\\"\\\\0\\\" ; .balign 8 ; .quad kstrtoull ; .previous\");\n",
    ));
}
