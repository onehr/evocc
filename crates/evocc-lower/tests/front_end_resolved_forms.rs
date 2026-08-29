use evocc_chir::AllocationSiteManifest;
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_function, parse_section9_module, render_module, validate_module, InterpreterLimits,
    Module,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn lower(source: &str, ordinal: u32) -> Module {
    let arena = Arena::default();
    let file = FileId::new(9_400 + ordinal);
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
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("lowering case {ordinal}: {error}"));
    validate_module(&module).unwrap_or_else(|error| panic!("validation case {ordinal}: {error}"));
    module
}

fn assert_pairs(pairs: &[(&str, &str)], ordinal_base: u32) {
    for (offset, (written, spelled_out)) in pairs.iter().enumerate() {
        let ordinal = ordinal_base + u32::try_from(offset).expect("small fixture index");
        let written = render_module(&lower(written, ordinal));
        let spelled_out = render_module(&lower(spelled_out, ordinal));
        assert_eq!(
            written, spelled_out,
            "front-end-only form leaked into EvoIR pair {ordinal}"
        );
    }
}

#[test]
fn front_end_resolved_forms_leave_byte_identical_evoir() {
    let pairs = [
        (
            "int f(int i) { typeof(i) j = i + 1; return j; }\n",
            "int f(int i) { int j = i + 1; return j; }\n",
        ),
        (
            "int f(int i) { return _Generic(i, int: i * 2, default: 0); }\n",
            "int f(int i) { return i * 2; }\n",
        ),
        (
            "long f(void) { long x = 7; long *p = &x; return *(const volatile typeof(_Generic(*p, long: (long)0, default: *p)) *)p; }\n",
            "long f(void) { long x = 7; long *p = &x; return *(const volatile long *)p; }\n",
        ),
        (
            "long f(int i) { return __builtin_expect(i + 1, i++); }\n",
            "long f(int i) { return (long)(i + 1); }\n",
        ),
        (
            "int f(int i) { return __builtin_constant_p(i) + __builtin_constant_p(7); }\n",
            "int f(int i) { return 0 + 1; }\n",
        ),
        (
            "unsigned long f(void) { int i = 0; return __alignof__(i++); }\n",
            "unsigned long f(void) { int i = 0; return (unsigned long)4; }\n",
        ),
        (
            "unsigned long f(void) { int i = 0; return sizeof(i++); }\n",
            "unsigned long f(void) { int i = 0; return (unsigned long)4; }\n",
        ),
        (
            "typedef int function_type(void); unsigned long f(void) { return sizeof(void) + sizeof(function_type); }\n",
            "typedef int function_type(void); unsigned long f(void) { return (unsigned long)1 + (unsigned long)1; }\n",
        ),
        (
            "unsigned long f(void) { return sizeof(*(8 ? ((void *)((long)(1) * 0l)) : (int *)8)); }\n",
            "unsigned long f(void) { return (unsigned long)4; }\n",
        ),
        (
            "unsigned long f(int value) { return sizeof(*(8 ? ((void *)((long)(value) * 0l)) : (int *)8)); }\n",
            "unsigned long f(int value) { return (unsigned long)1; }\n",
        ),
    ];

    assert_pairs(&pairs, 0);
}

#[test]
fn selection_type_and_offset_builtins_resolve_before_lowering() {
    assert_pairs(
        &[
            (
                "int f(int x) { return __builtin_choose_expr(1, x + 1, x + 2); }\n",
                "int f(int x) { return x + 1; }\n",
            ),
            (
                "int f(void) { return __builtin_types_compatible_p(const int, int); }\n",
                "int f(void) { return 1; }\n",
            ),
            (
                "struct s { char c; int i; }; unsigned long f(void) { return __builtin_offsetof(struct s, i); }\n",
                "struct s { char c; int i; }; unsigned long f(void) { return 4ul; }\n",
            ),
        ],
        100,
    );
}

#[test]
fn constant_bit_builtins_resolve_before_lowering() {
    assert_pairs(
        &[(
            "int f(void) { return __builtin_ffs(8) + __builtin_popcount(7u); }\n",
            "int f(void) { return 4 + 3; }\n",
        )],
        110,
    );
}

#[test]
fn unreachable_lowers_to_a_round_trippable_typed_trap() {
    let module = lower("void f(void) { __builtin_unreachable(); }\n", 111);
    let rendered = render_module(&module);
    assert!(rendered.contains("may_trap=true"), "{rendered}");
    assert!(rendered.contains("trap.unreachable_reached"), "{rendered}");

    let reparsed = parse_section9_module(rendered.as_bytes()).expect("Section 9 round trip");
    assert_eq!(reparsed, module);
    let error = interpret_function(&reparsed.functions[0], &[], InterpreterLimits::new(4))
        .expect_err("reaching the report terminator must fail");
    assert_eq!(error.code(), "E3024");
    assert_eq!(error.message(), "unreachable-reached");
}

#[test]
fn inferred_declarations_and_elvis_resolve_before_lowering() {
    assert_pairs(
        &[
            (
                "int f(int x) { __auto_type value = x; return value; }\n",
                "int f(int x) { int value = x; return value; }\n",
            ),
            (
                "int f(int a, int b) { return a ?: b; }\n",
                "int f(int a, int b) { int temporary = a; return temporary ? temporary : b; }\n",
            ),
        ],
        120,
    );
}

#[test]
fn case_ranges_and_local_labels_resolve_before_lowering() {
    assert_pairs(
        &[
            (
                "int f(int x) { switch (x) { case 1 ... 3: return 1; default: return 0; } }\n",
                "int f(int x) { switch (x) { case 1: case 2: case 3: return 1; default: return 0; } }\n",
            ),
            (
                "int f(void) { __label__ out; goto out; out: return 1; }\n",
                "int f(void) { goto out; out: return 1; }\n",
            ),
        ],
        130,
    );
}
