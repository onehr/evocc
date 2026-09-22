use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::Lexer;
use evocc_parser::Parser;

fn accepts(source: &str) {
    let diagnostic = diagnostic(source);
    assert!(diagnostic.is_none(), "{diagnostic:?}");
}

fn diagnostic(source: &str) -> Option<evocc_parser::ParseDiagnostic> {
    let file = FileId::new(1);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut preprocessor = Preprocessor::new();
    let tokens = preprocessor
        .process(TokenStream::new(tokens), source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "{:?}",
        preprocessor.errors()
    );
    let arena = Arena::default();
    let mut parser = Parser::new(tokens, file, source);
    let _unit = parser.parse_translation_unit_in(&arena);
    parser.diagnostic().cloned()
}

#[test]
fn target_vectors_preserve_their_full_element_and_storage_types() {
    accepts(
        "_Static_assert(sizeof(__Float16x4_t) == 8, \"\");
         _Static_assert(sizeof(__Float16x8_t) == 16, \"\");
         _Static_assert(sizeof(__Float32x2_t) == 8, \"\");
         _Static_assert(sizeof(__Float64x1_t) == 8, \"\");
         _Static_assert(sizeof(__Bfloat16x4_t) == 8, \"\");
         _Static_assert(sizeof(__Bfloat16x8_t) == 16, \"\");
         _Static_assert(sizeof(__Poly128_t) == 16, \"\");
         _Static_assert(sizeof(__Poly8x16_t) == 16, \"\");
         _Static_assert(!__builtin_types_compatible_p(__Poly8_t, unsigned char), \"\");
         _Static_assert(!__builtin_types_compatible_p(__Poly8x16_t, __Uint8x16_t), \"\");
         extern __Float16x4_t declared_only(__Float16x4_t);",
    );
}

#[test]
fn the_target_header_pragma_declares_real_vector_tuple_records() {
    accepts(
        "#pragma GCC aarch64 \"arm_neon.h\"
         _Static_assert(sizeof(uint8x8x3_t) == 24, \"\");
         _Static_assert(__alignof__(uint8x8x3_t) == 8, \"\");
         _Static_assert(sizeof(uint64x2x4_t) == 64, \"\");
         _Static_assert(__alignof__(uint64x2x4_t) == 16, \"\");
         _Static_assert(sizeof(((uint8x8x3_t *)0)->val) == 24, \"\");
         _Static_assert(__builtin_types_compatible_p(
             __typeof__(((uint8x8x3_t *)0)->val[0]), __Uint8x8_t), \"\");
         _Static_assert(sizeof(float16x8x2_t) == 32, \"\");
         _Static_assert(sizeof(poly64x2x4_t) == 64, \"\");
         _Static_assert(sizeof(bfloat16x8x3_t) == 48, \"\");",
    );
}

#[test]
fn target_builtin_calls_have_the_recorded_prototypes_not_inferred_return_types() {
    accepts(
        "_Static_assert(__builtin_types_compatible_p(
             __typeof__(__builtin_aarch64_ld1v2di_us((const long *)0)),
             __Uint64x2_t), \"\");
         _Static_assert(__builtin_types_compatible_p(
             __typeof__(__builtin_aarch64_st1v2di_su((long *)0, (__Uint64x2_t){0, 0})),
             void), \"\");
         _Static_assert(__builtin_types_compatible_p(
             __typeof__(__builtin_aarch64_qtbl1v16qi_uuu(
                 (__Uint8x16_t){0}, (__Uint8x16_t){0})), __Uint8x16_t), \"\");",
    );
}

#[test]
fn polynomial_elements_follow_the_shared_scalar_initializer_path() {
    accepts(
        "__Poly64x1_t select_poly(__Uint64x1_t a, __Poly64x1_t b, __Poly64x1_t c) {
           return (__Poly64x1_t){__builtin_aarch64_simd_bsldi_pupp(a[0], b[0], c[0])};
         }
         __Poly8_t values[] = {1, 2, 3};
         _Static_assert(sizeof(values) == 3, \"\");
         struct holder { __Poly16_t x, y; } object = {1};",
    );
}

#[test]
fn unused_floating_vector_comparisons_have_integer_masks() {
    accepts(
        "typedef float pair __attribute__((vector_size(8)));
         typedef double wide_pair __attribute__((vector_size(16)));
         typedef int mask __attribute__((vector_size(8)));
         typedef long wide_mask __attribute__((vector_size(16)));
         mask compare(pair a, pair b) { return a == b; }
         wide_mask compare_wide(wide_pair a, wide_pair b) { return a < b; }
         _Static_assert(__builtin_types_compatible_p(
           __typeof__((pair){0, 0} == (pair){0, 0}), mask), \"\");",
    );
}

#[test]
fn floating_vector_broadcast_is_typed_without_permitting_lossy_operands() {
    let prefix = "typedef float v __attribute__((vector_size(8)));";
    for body in [
        "v f(v a,float b){return a+b;}",
        "v f(v a){return a+1.25;}",
        "__Int32x2_t f(v a){return (__Int32x2_t)(a==0);}",
    ] {
        accepts(&format!("{prefix}{body}"));
    }
    for body in [
        "v f(v a,int b){return a+b;}",
        "v f(v a,double b){return a+b;}",
        "v f(v a){return a+1.1;}",
        "v f(v a,v b){return a&b;}",
        "v f(v a){return ~a;}",
    ] {
        assert_eq!(
            diagnostic(&format!("{prefix}{body}")).expect(body).code,
            "E2102"
        );
    }
}

#[test]
fn pragma_injected_reinterpret_intrinsics_are_typed_and_scoped() {
    accepts(
        "#pragma GCC aarch64 \"arm_neon.h\"\n
        _Static_assert(__builtin_types_compatible_p(
          __typeof__(vreinterpret_u8_s8((__Int8x8_t){0})), __Uint8x8_t), \"\");
        _Static_assert(__builtin_types_compatible_p(
          __typeof__(vreinterpretq_p128_u8((__Uint8x16_t){0})), __Poly128_t), \"\");",
    );
    for source in [
        "void f(void){vreinterpret_u8_s8((__Int8x8_t){0});}",
        "#pragma GCC aarch64 \"arm_neon.h\"\nvoid f(void){vreinterpret_u8_u8((__Uint8x8_t){0});}",
    ] {
        assert_eq!(
            diagnostic(source).expect("undeclared intrinsic").code,
            "E2001"
        );
    }
}

#[test]
fn every_recorded_target_prototype_is_resolvable() {
    let mut source = String::from("#pragma GCC aarch64 \"arm_neon.h\"\n");
    let mut names = std::collections::HashSet::new();
    for (index, line) in include_str!("../../evocc-types/src/neon-signatures.tsv")
        .lines()
        .enumerate()
    {
        let name = line.split('\t').next().unwrap();
        assert!(names.insert(name), "duplicate builtin: {name}");
        if name.starts_with("__builtin_aarch64_") || name.starts_with("vreinterpret") {
            source.push_str(&format!("typedef __typeof__({name}) signature_{index};\n"));
        }
    }
    accepts(&source);
    for source in [
        "void f(void){ __builtin_aarch64_ld1v2di_us(); }",
        "void f(void){ __builtin_aarch64_st1v2di_su((long*)0, (__Uint8x8_t){0}); }",
    ] {
        assert!(diagnostic(source).is_some(), "bad call accepted: {source}");
    }
}
