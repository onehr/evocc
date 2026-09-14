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
fn literal_comparisons_resolve_without_library_bindings() {
    assert_pairs(
        &[
            (
                "int f(void){return __builtin_strcmp(\"a\",\"b\");}",
                "int f(void){return -1;}",
            ),
            (
                "int f(void){return __builtin_strcmp(\"a\\0z\",\"a\\0x\");}",
                "int f(void){return 0;}",
            ),
            (
                "unsigned long f(void){return __builtin_strlen(\"a\\0tail\");}",
                "unsigned long f(void){return 1UL;}",
            ),
        ],
        2200,
    );
}

#[test]
fn type_classification_is_resolved_without_values_or_calls_in_evoir() {
    assert_pairs(
        &[
            (
                "int f(int x){return __builtin_classify_type(x++);}",
                "int f(int x){return 1;}",
            ),
            (
                "int f(int *p){return __builtin_classify_type(p);}",
                "int f(int *p){return 5;}",
            ),
            (
                "int f(void){return __builtin_classify_type(*(double *)0);}",
                "int f(void){return 8;}",
            ),
            (
                "int f(void){return __builtin_classify_type((struct S{int i;}){0});}",
                "int f(void){return 12;}",
            ),
            (
                "int f(void){return __builtin_classify_type(\"x\");}",
                "int f(void){return 5;}",
            ),
            (
                "int f(void){return __builtin_classify_type(*(int (*)(void))0);}",
                "int f(void){return 5;}",
            ),
        ],
        2100,
    );
}

#[test]
fn bracket_attributes_resolve_through_the_existing_attribute_semantics() {
    assert_pairs(&[
        ("[[gnu::aligned(32)]] int x; int f(void){return x;}",
         "__attribute__((aligned(32))) int x; int f(void){return x;}"),
        ("struct [[gnu::packed]] S{char a; int b;}; int f(struct S *p){return p->b;}",
         "struct __attribute__((packed)) S{char a; int b;}; int f(struct S *p){return p->b;}"),
        ("[[other::weak, other::alias(\"missing\")]] static int x=3; int f(void){return x;}",
         "static int x=3; int f(void){return x;}"),
        ("int f(void) [[gnu::aligned(32)]] {return 0;}", "int f(void){return 0;}"),
    ], 2000);
}

#[test]
fn unused_target_header_prototypes_do_not_require_floating_execution() {
    for spelling in [
        "_Float16",
        "_Float32",
        "_Float64",
        "_Float128",
        "_Float32x",
        "_Float64x",
        "__fp16",
        "__Float32x4_t",
        "__Float64x2_t",
        "__SVFloat32_t",
        "__SVFloat64_t",
        "__SVBool_t",
    ] {
        assert_pairs(
            &[(
                &format!("typedef {spelling} header_type; extern header_type unused(header_type); int main(void){{return 0;}}"),
                "int main(void){return 0;}",
            )],
            1900,
        );
    }
}

#[test]
fn used_target_header_values_are_not_silently_lowered_as_integer_or_scalar_aliases() {
    for spelling in [
        "float",
        "double",
        "long double",
        "_Float32",
        "__Float32x4_t",
        "__SVFloat32_t",
    ] {
        let source = format!("{spelling} identity({spelling} x){{return x;}}");
        let arena = Arena::default();
        let file = FileId::new(11_900);
        let mut lexer = Lexer::new(file, &source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            assert_ne!(token.kind, TokenKind::Error);
            tokens.push(token);
        }
        let mut parser = Parser::new(tokens, file, &source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let error = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect_err("header admission must not invent execution support");
        assert_eq!(error.code(), "E3101", "{source}: {error}");
    }
}

#[test]
fn digit_builtin_resolves_to_ordinary_integer_arithmetic() {
    assert_pairs(
        &[
            (
                "int f(int x){return __builtin_isdigit(x);}",
                "int f(int x){return ((unsigned int)(int)x-48u)<=9u;}",
            ),
            (
                "int f(int *p){return __builtin_isdigit((*p)++);}",
                "int f(int *p){return ((unsigned int)(int)((*p)++)-48u)<=9u;}",
            ),
        ],
        1800,
    );
}

#[test]
fn integer_mode_attributes_resolve_to_canonical_types_before_lowering() {
    assert_pairs(
        &[
            (
                "typedef unsigned int t __attribute__((mode(QI))); t f(t x){return x+1;}",
                "typedef unsigned char t; t f(t x){return x+1;}",
            ),
            (
                "typedef unsigned int t __attribute__((mode(DI))); t f(t x){return x+1;}",
                "typedef unsigned long t; t f(t x){return x+1;}",
            ),
        ],
        1700,
    );
}

#[test]
fn ignored_type_name_attributes_do_not_change_lowered_function_pointer_calls() {
    assert_pairs(
        &[
            (
                "int f(void *p){return ((__attribute__((noinline)) int (*)(void))p)();}",
                "int f(void *p){return ((int (*)(void))p)();}",
            ),
            (
                "int f(void *p){return ((int (__attribute__((noinline)) *)(void))p)();}",
                "int f(void *p){return ((int (*)(void))p)();}",
            ),
        ],
        1600,
    );
}

#[test]
fn formatted_and_string_builtins_use_external_function_prototypes() {
    assert_pairs(
        &[
            (
                "int f(char *d,const char *fmt,unsigned char x){return __builtin_sprintf(d,fmt,x);}",
                "int sprintf(char *,const char *,...); int f(char *d,const char *fmt,unsigned char x){return sprintf(d,fmt,x);}",
            ),
            (
                "int f(char *d,unsigned long n,const char *fmt,short x){return __builtin_snprintf(d,n,fmt,x);}",
                "int snprintf(char *,unsigned long,const char *,...); int f(char *d,unsigned long n,const char *fmt,short x){return snprintf(d,n,fmt,x);}",
            ),
            (
                "char *f(const char *s,int c){return __builtin_strchr(s,c);}",
                "char *strchr(const char *,int); char *f(const char *s,int c){return strchr(s,c);}",
            ),
            (
                "char *f(char *d,const char *s,unsigned long n){return __builtin_strncpy(d,s,n);}",
                "char *strncpy(char *,const char *,unsigned long); char *f(char *d,const char *s,unsigned long n){return strncpy(d,s,n);}",
            ),
        ],
        1500,
    );
}

#[test]
fn hosted_library_builtins_use_external_function_prototypes() {
    assert_pairs(
        &[
            (
                "int f(const char *fmt,unsigned char x){return __builtin_printf(fmt,x);}",
                "int printf(const char *,...); int f(const char *fmt,unsigned char x){return printf(fmt,x);}",
            ),
            (
                "void *f(unsigned long n){return __builtin_malloc(n);}",
                "void *malloc(unsigned long); void *f(unsigned long n){return malloc(n);}",
            ),
            (
                "char *f(char *d,const char *s){return __builtin_strcpy(d,s);}",
                "char *strcpy(char *,const char *); char *f(char *d,const char *s){return strcpy(d,s);}",
            ),
        ],
        1400,
    );
}

#[test]
fn hosted_library_builtins_keep_argument_and_declaration_checks() {
    for (source, code) in [
        ("int f(void){return __builtin_printf();}", "E2909"),
        ("void *f(void){return __builtin_malloc(1,2);}", "E2909"),
        ("char *f(void){return __builtin_strcpy(1,2);}", "E2913"),
        ("int f(void){return __builtin_puts(\"x\");}", "E2935"),
        (
            "long printf(const char *,...); int f(void){return __builtin_printf(\"x\");}",
            "E2927",
        ),
    ] {
        let arena = Arena::default();
        let file = FileId::new(10899);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let _ = parser.parse_translation_unit_in(&arena);
        assert_eq!(
            parser.diagnostic().map(|diagnostic| diagnostic.code),
            Some(code),
            "{source}"
        );
    }
}

#[test]
fn termination_builtins_are_typed_ordinary_library_calls() {
    assert_pairs(
        &[(
            "void f(void){__builtin_abort();}",
            "void abort(void);void f(void){abort();}",
        )],
        1300,
    );
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
        (
            "int f(int i) { const int c = i; __typeof_unqual__(c) j = c + 1; return j; }\n",
            "int f(int i) { const int c = i; int j = c + 1; return j; }\n",
        ),
        (
            "int declared(int x) { return x; } extern typeof(declared) declared; int f(int x) { return declared(x); }\n",
            "int declared(int x) { return x; } extern int declared(int); int f(int x) { return declared(x); }\n",
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
fn dynamic_offsetof_resolves_to_byte_identical_runtime_arithmetic() {
    assert_pairs(
        &[(
            "struct s { int head; int arr[4]; }; unsigned long f(int i) { return __builtin_offsetof(struct s, arr[i]); }\n",
            "struct s { int head; int arr[4]; }; unsigned long f(int i) { return 4ul + (unsigned long)i * 4ul; }\n",
        )],
        103,
    );
}

#[test]
fn builtin_object_size_resolves_to_proven_or_unknown_constants() {
    assert_pairs(
        &[
            (
                "unsigned long f(int *p) { return __builtin_object_size(p, 0); }\n",
                "unsigned long f(int *p) { return 18446744073709551615ul; }\n",
            ),
            (
                "unsigned long f(int *p) { return __builtin_object_size(p, 2); }\n",
                "unsigned long f(int *p) { return 0ul; }\n",
            ),
            (
                "struct s { char text[16]; }; unsigned long f(struct s *p) { return __builtin_object_size(p->text, 1); }\n",
                "struct s { char text[16]; }; unsigned long f(struct s *p) { return 16ul; }\n",
            ),
        ],
        104,
    );
}

#[test]
fn attribute_queries_resolve_before_lowering() {
    assert_pairs(
        &[(
            concat!(
                "struct s { char text[4] __attribute__((nonstring)); }; ",
                "int f(void) { struct s value; return ",
                "__builtin_has_attribute(value.text, nonstring); }\n",
            ),
            concat!(
                "struct s { char text[4] __attribute__((nonstring)); }; ",
                "int f(void) { struct s value; return (_Bool)1; }\n",
            ),
        )],
        106,
    );
}

#[test]
fn builtin_strlen_resolves_to_constants_or_ordinary_calls() {
    assert_pairs(
        &[
            (
                "typedef unsigned long size_t; size_t f(void) { return __builtin_strlen(\"abc\"); }\n",
                "typedef unsigned long size_t; size_t f(void) { return 3ul; }\n",
            ),
            (
                "typedef unsigned long size_t; size_t strlen(const char *s) { return 5ul; } size_t f(const char *s) { return __builtin_strlen(s); }\n",
                "typedef unsigned long size_t; size_t strlen(const char *s) { return 5ul; } size_t f(const char *s) { return strlen(s); }\n",
            ),
        ],
        107,
    );
}

#[test]
fn undeclared_library_builtin_lowers_like_an_explicit_external_prototype() {
    assert_pairs(
        &[(
            "void f(void *p) { __builtin_memset(p, 0, 1); } void *memset(void *p, int value, unsigned long size) { return p; }",
            "extern void *memset(void *, int, unsigned long); void f(void *p) { memset(p, 0, 1); } void *memset(void *p, int value, unsigned long size) { return p; }",
        )],
        200,
    );
}

#[test]
fn attributed_null_statements_do_not_reach_evoir() {
    assert_pairs(
        &[(
            "int f(int x) { if (x) __attribute__((fallthrough)); return x; }\n",
            "int f(int x) { if (x) ; return x; }\n",
        )],
        105,
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
