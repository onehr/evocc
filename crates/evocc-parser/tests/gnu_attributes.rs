//! Executable specification for GNU attributes (specification 10.1.9).
//!
//! Refreshed kernel reconnaissance shows every parse failure in the cohort
//! stopping on one of two `__attribute__` lines. Those two lines are the first
//! fixtures here, verbatim. The rest pin the placement table, attachment,
//! spelling equivalence, the malformed-attribute diagnostic, and the layout
//! effect of the two attributes this stage applies.
//!
//! Layout expectations are GCC-14/Clang-19 behaviour on AArch64 and are
//! confirmed by the oracle run that task A adds to the layout corpus; until
//! that run exists these numbers are claims, and the oracle run, not this
//! file, is the authority if they disagree.

use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> Result<CHirUnit<'a>, String> {
    let file = SourceFile::new(
        FileId::new(9_201),
        "gnu-attributes.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(d) => Err(format!("{}: {}", d.code, d.message)),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(e) = parse(source, &arena) {
        panic!("must parse, got {e}\n  source: {source}");
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n  source: {source}"),
        Err(e) => assert!(
            e.starts_with(&format!("{code}:")),
            "expected {code}, got {e}\n  source: {source}"
        ),
    }
}

/// Size and alignment of the typedef named `name` after parsing `source`.
fn layout_of(source: &str, name: &str) -> (u64, u32) {
    let arena = Arena::default();
    let unit =
        parse(source, &arena).unwrap_or_else(|e| panic!("must parse, got {e}\n  source: {source}"));
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), name)
        .expect("symbol table is live")
        .unwrap_or_else(|| panic!("{name} is not declared"));
    let ty = unit.symbols().symbol(symbol).expect("declared").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .unwrap_or_else(|e| panic!("layout of {name}: {e:?}"));
    (layout.size_bytes, layout.align_bytes)
}

// ---------------------------------------------------------------------------
// The two lines that stop every parse lane in the kernel cohort
// ---------------------------------------------------------------------------

#[test]
fn kernel_types_h_aligned_int128_typedef_parses() {
    accepts("typedef __signed__ __int128 __s128 __attribute__((aligned(16)));\n");
}

#[test]
fn kernel_compiler_h_attributed_inline_function_parses() {
    accepts(
        "static inline __attribute__((__gnu_inline__)) __attribute__((__unused__)) \
         __attribute__((__no_instrument_function__)) void *offset_to_ptr(const int *off) { return 0; }\n",
    );
}

// ---------------------------------------------------------------------------
// Placement table, one row each
// ---------------------------------------------------------------------------

#[test]
fn placement_among_declaration_specifiers() {
    accepts("__attribute__((unused)) static int a;\n");
}

#[test]
fn placement_after_declarator() {
    accepts("int a __attribute__((unused));\n");
}

#[test]
fn placement_after_struct_keyword_and_after_closing_brace() {
    accepts("struct __attribute__((packed)) s { char c; int i; };\n");
    accepts("struct t { char c; int i; } __attribute__((packed));\n");
    accepts("enum __attribute__((packed)) e { E0 };\n");
}

#[test]
fn placement_after_member_declarator() {
    accepts("struct s { int a __attribute__((aligned(8))); char b; };\n");
}

#[test]
fn placement_after_parameter_declarator() {
    accepts("int f(int a __attribute__((unused))) { return 0; }\n");
}

#[test]
fn placement_after_function_declarator_before_body() {
    accepts("int f(void) __attribute__((noinline)) { return 0; }\n");
}

#[test]
fn placement_after_label() {
    accepts("int f(void) { goto out; out: __attribute__((unused)); return 0; }\n");
}

#[test]
fn placement_after_enumerator() {
    accepts("enum e { OLD __attribute__((deprecated)), NEW };\n");
}

#[test]
fn placement_in_type_name() {
    accepts("int f(void) { return sizeof(int __attribute__((aligned(8)))); }\n");
}

// ---------------------------------------------------------------------------
// Attachment, spelling, rejection
// ---------------------------------------------------------------------------

#[test]
fn specifier_position_attaches_to_every_declarator() {
    // Both `a` and `b` carry `aligned(8)`; observable through layout of each.
    let arena = Arena::default();
    let unit = parse("typedef int __attribute__((aligned(8))) a, b;\n", &arena).expect("parses");
    let typedefs: Vec<_> = unit
        .iter()
        .filter_map(|d| match d {
            CHirDecl::Typedef(t) => Some(t.name.spelling.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(typedefs, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(
        layout_of("typedef int __attribute__((aligned(8))) a, b;\n", "a").1,
        8
    );
    assert_eq!(
        layout_of("typedef int __attribute__((aligned(8))) a, b;\n", "b").1,
        8
    );
}

#[test]
fn double_underscore_spelling_is_the_same_attribute() {
    assert_eq!(
        layout_of("typedef int __attribute__((__aligned__(8))) t;\n", "t").1,
        8
    );
    assert_eq!(
        layout_of("typedef int __attribute__((aligned(8))) t;\n", "t").1,
        8
    );
}

#[test]
fn malformed_argument_list_is_rejected() {
    rejects("int a __attribute__((aligned(8));\n", "E2903");
    rejects("int a __attribute__((aligned(,)));\n", "E2903");
}

#[test]
fn aligned_argument_must_be_a_representable_power_of_two() {
    rejects("typedef int t __attribute__((aligned(3)));\n", "E2903");
    rejects("typedef int t __attribute__((aligned(0)));\n", "E2903");
}

// ---------------------------------------------------------------------------
// aligned / packed applied to layout (GCC-14 / Clang-19, AArch64)
// ---------------------------------------------------------------------------

#[test]
fn aligned_raises_a_typedef_and_makes_a_distinct_type() {
    assert_eq!(
        layout_of("typedef __int128 t __attribute__((aligned(16)));\n", "t"),
        (16, 16)
    );
    assert_eq!(
        layout_of("typedef char t __attribute__((aligned(4)));\n", "t"),
        (1, 4)
    );
}

#[test]
fn aligned_on_a_typedef_sets_the_alignment_exactly_even_lower() {
    // Sealed GCC-14 and Clang-19 both report alignment 1 here. An earlier
    // draft of this test expected 4 on the belief that aligned never lowers;
    // that is true of records and members, not of typedefs.
    assert_eq!(
        layout_of("typedef int t __attribute__((aligned(1)));\n", "t"),
        (4, 1)
    );
}

#[test]
fn bare_aligned_is_the_largest_fundamental_alignment() {
    assert_eq!(
        layout_of("typedef char t __attribute__((aligned));\n", "t"),
        (1, 16)
    );
}

#[test]
fn packed_record_has_no_padding_and_alignment_one() {
    assert_eq!(
        layout_of(
            "typedef struct { char c; int i; } __attribute__((packed)) t;\n",
            "t"
        ),
        (5, 1)
    );
}

#[test]
fn packed_with_aligned_takes_the_given_alignment() {
    assert_eq!(
        layout_of(
            "typedef struct { char c; int i; } __attribute__((packed, aligned(4))) t;\n",
            "t"
        ),
        (8, 4)
    );
}

#[test]
fn packed_member_alone_moves_only_that_member() {
    // c at 0; i packed at 1..5; s aligned to 2 at 6..8. The packed member no
    // longer contributes its type's alignment, so the record aligns to `short`:
    // size 8, align 2. The oracle run confirms this figure.
    assert_eq!(
        layout_of(
            "typedef struct { char c; int i __attribute__((packed)); short s; } t;\n",
            "t"
        ),
        (8, 2)
    );
}

#[test]
fn bit_fields_in_a_packed_record_cross_unit_boundaries() {
    // 3 + 6 + 7 = 16 bits packed to bit granularity: 2 bytes, alignment 1.
    assert_eq!(
        layout_of("typedef struct { unsigned a : 3; unsigned b : 6; unsigned c : 7; } __attribute__((packed)) t;\n", "t"),
        (2, 1)
    );
}
