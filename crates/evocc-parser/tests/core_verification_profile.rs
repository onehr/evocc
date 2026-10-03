use evocc_chir::core_profile::{inspect_core, CoreRefusal};
use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> CHirUnit<'a> {
    let file = FileId::new(10_600);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    assert!(!tokens.iter().any(|t| t.kind == TokenKind::Error));
    let mut cpp = Preprocessor::new();
    let tokens = cpp.process(TokenStream::new(tokens), source).into_vec();
    assert!(cpp.errors().is_empty(), "{:?}", cpp.errors());
    let mut parser = Parser::new(tokens, file, source);
    parser.set_standard_builtins(false);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(parser.diagnostic().is_none(), "{:?}", parser.diagnostic());
    unit
}

#[test]
fn frozen_real_functions_and_control_population_are_in_profile() {
    for (source, entry, count) in [
        (
            include_str!("../../../tests/fixtures/verification/doom-fixedmul.c"),
            "FixedMul",
            1,
        ),
        (
            include_str!("../../../tests/fixtures/verification/musl-abs.c"),
            "abs",
            1,
        ),
        (
            include_str!("../../../tests/fixtures/verification/control.c"),
            "loop_join",
            1,
        ),
        (
            include_str!("../../../tests/fixtures/verification/control.c"),
            "short_circuit",
            1,
        ),
        (
            include_str!("../../../tests/fixtures/verification/pressure.c"),
            "pressure",
            2,
        ),
    ] {
        let arena = Arena::default();
        let unit = parse(source, &arena);
        let report = inspect_core(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            &[entry],
        )
        .unwrap_or_else(|e| panic!("{entry}: {e:?}"));
        assert_eq!(report.functions.len(), count);
    }
}

fn refused(source: &str, expected: CoreRefusal) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let error = inspect_core(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        &["f"],
    )
    .expect_err("unsupported source must not be silently projected away");
    assert_eq!(error.reason, expected, "{source}: {error:?}");
}

#[test]
fn types_memory_and_even_dead_unsupported_constructs_refuse() {
    for source in [
        "int f(int *p) { return *p; }",
        "double f(double x) { return x + 1; }",
        "int f(void) { volatile int x = 1; return x; }",
        "int f(void) { short x = 1; return x; }",
    ] {
        refused(source, CoreRefusal::Type);
    }
    refused(
        "int f(int x) { if (0) { asm(\"nop\"); } return x; }",
        CoreRefusal::Statement,
    );
    refused(
        "int f(int x) { return x << 1; }",
        CoreRefusal::SignedLeftShift,
    );
    refused(
        "int f(int x) { x <<= 1; return x; }",
        CoreRefusal::SignedLeftShift,
    );
    refused(
        "int g; int f(void) { return g; }",
        CoreRefusal::NonlocalObject,
    );
    refused(
        "int f(void) { int x; return x; }",
        CoreRefusal::Uninitialized,
    );
    refused(
        "int f(int x) { int y; if(x) y=1; return y; }",
        CoreRefusal::Uninitialized,
    );
    refused(
        "int f(int x) { int y; x && (y=1); return y; }",
        CoreRefusal::Uninitialized,
    );
    refused(
        "int f(int x) { int y; while(x) { y=1; x=0; } return y; }",
        CoreRefusal::Uninitialized,
    );
    refused(
        "int f(void) { int x = x; return x; }",
        CoreRefusal::Uninitialized,
    );
}

#[test]
fn complete_direct_call_closure_is_required() {
    refused(
        "int ext(int); int f(int x) { return ext(x); }",
        CoreRefusal::ExternalCall,
    );
    refused(
        "int f(int x) { return x ? f(x-1) : 0; }",
        CoreRefusal::Recursion,
    );
    refused(
        "int f(int); int g(int x) { return f(x); } int f(int x) { return g(x); }",
        CoreRefusal::Recursion,
    );
    refused("int f(int x, ...) { return x; }", CoreRefusal::Signature);
    refused("int f(x) int x; { return x; }", CoreRefusal::Signature);
}

#[test]
fn initialization_joins_and_lexical_shadowing_are_preserved() {
    let arena = Arena::default();
    let unit = parse(
        "int f(int x) { int y; if(x) y=1; else y=2; { int y=3; y++; } return y; }",
        &arena,
    );
    assert!(inspect_core(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        &["f"]
    )
    .is_ok());
}

#[test]
fn wrong_target_and_nonbaseline_source_options_refuse() {
    let arena = Arena::default();
    let mut unit = parse("int f(int x) { return x + 1; }", &arena);
    assert_eq!(
        inspect_core(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu().with_short_wchar(true),
            &["f"]
        )
        .unwrap_err()
        .reason,
        CoreRefusal::Profile
    );
    unit.set_signed_wrapping(true);
    assert_eq!(
        inspect_core(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            &["f"]
        )
        .unwrap_err()
        .reason,
        CoreRefusal::Profile
    );
}
