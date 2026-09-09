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

use evocc_chir::{render_unit_summary, CHirDecl, CHirUnit};
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
    let declared = unit
        .symbols()
        .symbol(symbol)
        .expect("declared")
        .declared_alignment();
    (layout.size_bytes, layout.align_bytes.max(declared))
}

#[test]
fn bracket_attributes_share_the_gnu_layout_and_placement_rules() {
    accepts(include_str!("../../../tests/fixtures/bracket-attributes.c"));
    assert_eq!(
        layout_of("[[gnu::aligned(32)]] int value;", "value"),
        (4, 32)
    );
    assert_eq!(
        layout_of(
            "typedef struct [[gnu::packed]] { char a; int b; } value;",
            "value"
        ),
        (5, 1)
    );
}

#[test]
fn bracket_namespaces_do_not_apply_unrelated_gnu_attributes() {
    for attribute in ["aligned(32)", "other::aligned(32)"] {
        assert_eq!(
            layout_of(&format!("[[{attribute}]] int value;"), "value"),
            (4, 4)
        );
    }
    assert_eq!(
        layout_of("[[__gnu__::__aligned__(32)]] int value;", "value"),
        (4, 32)
    );
    let arena = Arena::default();
    let unit = parse("[[gnu::unused, other::hint(1)]] int value;", &arena).unwrap();
    let attributes = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) => Some(&object.attributes),
            _ => None,
        })
        .unwrap();
    assert_eq!(attributes[0].written, "gnu::unused");
    assert_eq!(attributes[1].written, "other::hint");
    assert_eq!(layout_of("int [[other::hint]] value;", "value"), (4, 4));
    assert_eq!(
        layout_of("int value [[gnu::aligned(32)]];", "value"),
        (4, 32)
    );
    accepts("int value(void) [[gnu::aligned(32)]] { return 0; } _Static_assert(__alignof__(value)==4, \"function-type attribute ignored\");");
}

#[test]
fn bracket_type_alignment_conflicts_remain_refused() {
    for source in [
        "int value[4] [[gnu::aligned(32)]];",
        "struct S { int value[4] [[gnu::aligned(32)]]; };",
        "int f(int value[4] [[gnu::aligned(32)]]) { return 0; }",
        "int * [[gnu::aligned(32)]] value;",
    ] {
        rejects(source, "E2903");
    }
}

#[test]
fn malformed_bracket_attribute_lists_report_attribute_diagnostics() {
    for source in [
        "[[gnu::]] int value;",
        "[[gnu:aligned(4)]] int value;",
        "[[gnu::aligned(4)] int value;",
        "[[gnu::aligned(4), other::]] int value;",
        "[[gnu::aligned(3)]] int value;",
        "[[u8\"bad\"]] int value;",
        "[[gnu::u8\"bad\"]] int value;",
    ] {
        rejects(source, "E2903");
    }
}

#[test]
fn bracket_after_gnu_specifier_keeps_the_measured_ordering_refusal() {
    for source in [
        "__attribute__((unused)) [[gnu::aligned(32)]] static int value;",
        "static [[gnu::aligned(32)]] int value;",
        "int [[gnu::aligned(32)]] value;",
        "int a, [[gnu::unused]] b;",
        "struct S { char a; int b; } [[gnu::packed]];",
    ] {
        rejects(source, "E2903");
    }
    accepts("[[gnu::aligned(32)]] __attribute__((unused)) static int value;");
}

#[test]
fn integer_modes_select_the_measured_canonical_types() {
    for (mode, spelling, size) in [
        ("QI", "char", 1),
        ("HI", "short", 2),
        ("SI", "int", 4),
        ("DI", "long", 8),
        ("TI", "__int128", 16),
        ("byte", "char", 1),
        ("word", "long", 8),
        ("__QI__", "char", 1),
    ] {
        for sign in ["signed", "unsigned"] {
            let source = format!(
                "typedef {sign} int value __attribute__((mode({mode})));\n\
                 _Static_assert(sizeof(value) == {size}, \"size\");\n\
                 _Static_assert(_Alignof(value) == {size}, \"alignment\");\n\
                 _Static_assert(__builtin_types_compatible_p(value, {sign} {spelling}), \"rank\");"
            );
            accepts(&source);
        }
    }
}

#[test]
fn integer_modes_apply_to_each_declared_entity_without_losing_qualifiers() {
    accepts(
        "typedef unsigned int __attribute__((mode(QI))) a, b;\n\
         typedef const unsigned int c __attribute__((mode(__QI__)));\n\
         struct s { unsigned int x __attribute__((mode(QI)));\n\
                    unsigned int y __attribute__((mode(HI))); };\n\
         _Static_assert(sizeof(a) == 1 && sizeof(b) == 1 && sizeof(c) == 1, \"typedef\");\n\
         _Static_assert(sizeof(struct s) == 4 && _Alignof(struct s) == 2, \"record\");\n\
         _Static_assert(__builtin_offsetof(struct s, y) == 2, \"member\");\n\
         unsigned int x __attribute__((mode(HI)));\n\
         _Static_assert(sizeof(x) == 2, \"object\");\n\
         int f(unsigned int x __attribute__((mode(QI)))) {\n\
             _Static_assert(sizeof(x) == 1, \"parameter\"); return x; }",
    );
    rejects(
        "typedef const int c __attribute__((mode(QI))); int f(void){c x=0; x=1; return x;}",
        "E2101",
    );
}

#[test]
fn unsupported_and_conflicting_integer_modes_do_not_silently_keep_int() {
    for source in [
        "typedef int t __attribute__((mode(unknown)));",
        "typedef int t __attribute__((mode()));",
        "typedef int t __attribute__((mode(QI, HI)));",
        "typedef float t __attribute__((mode(QI)));",
        "typedef _Bool t __attribute__((mode(QI)));",
        "enum e { A, B }; typedef enum e t __attribute__((mode(QI)));",
        "enum e { A, B }; struct s { enum e x __attribute__((mode(QI))); };",
        "int f(void){return sizeof(unsigned int __attribute__((mode(QI))));}",
    ] {
        rejects(source, "E2903");
    }
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

/// An attribute written after the comma, before the next declarator,
/// attaches to that declarator alone (specification 10.1.9).
#[test]
fn an_attribute_before_a_later_declarator_attaches_to_it_alone() {
    let source = "int a, __attribute__((aligned(8))) b;\n";
    assert_eq!(layout_of(source, "a"), (4, 4));
    assert_eq!(layout_of(source, "b"), (4, 8));
}

/// The kernel writes `aligned(sizeof(void *))`: the argument is an integer
/// constant expression, not only a literal.
#[test]
fn aligned_accepts_an_integer_constant_expression() {
    assert_eq!(
        layout_of(
            "struct s { char c; } __attribute__((aligned(sizeof(void *)))) v;\n",
            "v"
        ),
        (8, 8)
    );
    assert_eq!(
        layout_of("int __attribute__((aligned(1 << 4))) w;\n", "w"),
        (4, 16)
    );
    assert_eq!(
        layout_of("int __attribute__((aligned((2 + 2) * 2))) x;\n", "x"),
        (4, 8)
    );
}

/// Inside a record, an attribute written between the specifiers and the
/// first declarator sits among the declaration specifiers, as it does at
/// file scope (`__u64 __attribute__((aligned(8))) excl_prog_hash;`).
#[test]
fn a_member_attribute_after_the_specifiers_applies_to_the_member() {
    assert_eq!(
        layout_of(
            "struct s { char c; int __attribute__((aligned(8))) n; } v;\n",
            "v"
        ),
        (16, 8)
    );
    assert_eq!(
        layout_of(
            "struct s { char c; int __attribute__((aligned(8))) n, m; } v;\n",
            "v"
        ),
        (24, 8)
    );
}

// ---------------------------------------------------------------------------
// Position x context matrix: every attribute position the specification
// names, in every declaration context, parses and reaches the C-HIR dump.
// ---------------------------------------------------------------------------

#[test]
fn every_attribute_position_parses_in_every_declaration_context() {
    let a = "__attribute__((unused))";
    let sources = [
        // file scope: before specifiers, after specifiers, before a later declarator, after declarators
        format!("{a} int f0;\n"),
        format!("int {a} f1;\n"),
        format!("int f2, {a} f3;\n"),
        format!("int f4 {a}, f5 {a};\n"),
        // block scope
        format!("void g(void) {{ {a} int b0; int {a} b1; int b2, {a} b3; int b4 {a}, b5 {a}; }}\n"),
        // record members
        format!("struct m {{ {a} int m0; int {a} m1; int m2, {a} m3; int m4 {a}, m5 {a}; }};\n"),
        // bit-field members, attribute after the width
        format!("struct w {{ int w0 : 3 {a}, w1 : 2 {a}; }};\n"),
        // parameters
        format!("void p({a} int p0, int {a} p1, int p2 {a});\n"),
        // enumerators and tags
        format!("enum {a} e {{ E0 {a}, E1 {a} = 2 }} {a};\n"),
        // function definition, before the body
        format!("void d(void) {a} {{ }}\n"),
    ];
    for source in &sources {
        accepts(source);
    }
}

#[test]
fn single_underscore_attribute_spelling_works_in_every_attribute_position() {
    let attribute = "__attribute((unused))";
    let sources = [
        format!("{attribute} int f0;\n"),
        format!("int {attribute} f1;\n"),
        format!("int f2, {attribute} f3;\n"),
        format!("int * {attribute} pointer;\n"),
        format!("struct {attribute} s {{ char c; int i; }};\n"),
        format!("struct t {{ char c; int i; }} {attribute};\n"),
        format!("struct m {{ int field {attribute}; }};\n"),
        format!("int parameter(int value {attribute});\n"),
        format!("int definition(void) {attribute} {{ return 0; }}\n"),
        format!("int label(void) {{ out: {attribute}; return 0; }}\n"),
        format!("int statement(void) {{ {attribute}; return 0; }}\n"),
        format!("enum e {{ E0 {attribute}, E1 }};\n"),
        format!("int type_name(void) {{ return sizeof(int {attribute}); }}\n"),
    ];
    let failures = sources
        .iter()
        .filter_map(|source| {
            parse(source, &Arena::default())
                .err()
                .map(|error| (source, error))
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "single-underscore alias failures: {failures:#?}"
    );

    assert_eq!(
        layout_of(
            "typedef struct { char c; int i; } __attribute((packed)) alias_layout;\n",
            "alias_layout",
        ),
        (5, 1),
    );
}

#[test]
fn statement_attributes_attach_only_to_null_statements_and_remain_observable() {
    let arena = Arena::default();
    let source = concat!(
        "int f(int value) { ",
        "if (value) __attribute__((unused)); ",
        "switch (value) { case 0: __attribute__((__fallthrough__)); default: return value; } ",
        "}\n",
    );
    let unit = parse(source, &arena).expect("attributes before null statements must parse");
    let inventory = unit.attribute_inventory();
    assert_eq!(inventory.preserved_not_applied.get("unused"), Some(&1));
    assert_eq!(inventory.preserved_not_applied.get("fallthrough"), Some(&1));
    let summary = render_unit_summary(&unit);
    assert_eq!(
        summary.matches("placement=null-statement").count(),
        2,
        "{summary}"
    );
}

#[test]
fn statement_attributes_before_non_null_statements_are_e2924() {
    for source in [
        "int f(void) { __attribute__((unused)) return 0; }\n",
        "int f(int x) { __attribute__((unused)) x++; return x; }\n",
        "int f(void) { __attribute__((unused)) { return 0; } }\n",
    ] {
        rejects(source, "E2924");
    }
}
