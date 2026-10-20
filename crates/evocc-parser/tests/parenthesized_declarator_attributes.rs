//! GNU attributes after the `(` of a parenthesized declarator (EvoIR 10.1.9,
//! D19), as in libxml's
//! `typedef void *(__attribute__((alloc_size(1))) *xmlMallocFunc)(size_t)`.
//! GCC 14 and Apple clang accept this position in every declarator and
//! refuse an attribute after the inner declarator or a bracketed one there.
//! The attribute attaches to the declared entity and is preserved. An
//! attribute that has an effect in this profile is E2903 there, because the
//! two compilers attach it to different entities, and an attribute that the
//! front end refuses elsewhere keeps its code.

use evocc_chir::{render_unit_summary, CHirUnit};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_511),
        "parenthesized-declarator-attributes.c".to_owned(),
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
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
}

fn dump(source: &str) -> String {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| panic!("{source}: {error}"));
    render_unit_summary(&unit)
}

fn error(source: &str) -> String {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("{source}: must be refused"),
        Err(error) => error,
    }
}

const PRESERVED: &str = "placement=parenthesized-declarator preserved-not-applied";

/// Every named position that both reference compilers accept.
#[test]
fn every_named_position_is_accepted_and_preserved() {
    for (source, count) in [
        (
            "typedef void *(__attribute__((alloc_size(1))) *M)(unsigned long);",
            1,
        ),
        ("int (__attribute__((unused)) x);", 1),
        ("void (__attribute__((unused)) *f)(void);", 1),
        ("int ((__attribute__((unused)) x));", 1),
        ("int (__attribute__((unused)) (x));", 1),
        ("void g(int (__attribute__((unused)) x));", 1),
        ("struct S { int (__attribute__((unused)) m); };", 1),
        ("int (__attribute__((unused)) a)[3];", 1),
        ("int (__attribute__((unused)) f)(void) { return 0; }", 1),
        (
            "void (__attribute__((unused)) __attribute__((deprecated)) *f)(int);",
            2,
        ),
        ("void (*(__attribute__((unused)) g))(void);", 1),
        (
            "int (__attribute__((unused)) x), (__attribute__((unused)) y);",
            2,
        ),
        (
            "void *(__attribute__((__alloc_size__(1), __nonnull__)) *p)(unsigned long);",
            2,
        ),
    ] {
        let output = dump(source);
        assert_eq!(
            output.matches(PRESERVED).count(),
            count,
            "{source}\n{output}"
        );
    }
}

/// The libxml form keeps its type: a pointer to a function.
#[test]
fn the_libxml_typedef_names_a_function_pointer() {
    let output = dump(concat!(
        "typedef void *(__attribute__((alloc_size(1))) *xmlMallocFunc)(unsigned long size);\n",
        "static void *grab(unsigned long size) { return (void *)size; }\n",
        "xmlMallocFunc allocate = grab;\n",
        "_Static_assert(sizeof(xmlMallocFunc) == sizeof(void (*)(void)), \"pointer\");\n",
        "_Static_assert(__builtin_types_compatible_p(xmlMallocFunc, void *(*)(unsigned long)), \"type\");\n",
    ));
    assert!(output.contains("typedef xmlMallocFunc"), "{output}");
    assert!(
        output.contains(&format!("attribute alloc_size(1) {PRESERVED}")),
        "{output}"
    );
}

/// An abstract declarator keeps the type-name meaning of this position.
#[test]
fn an_abstract_declarator_keeps_the_type_name_position() {
    for source in [
        "int s = sizeof(int (__attribute__((unused)) *));",
        "void g(void (__attribute__((unused)) *)(int));",
    ] {
        let output = dump(source);
        assert!(
            output.contains("attribute unused() placement=type-name preserved-not-applied"),
            "{source}\n{output}"
        );
    }
}

/// An attribute with an effect is E2903 here; a refused one keeps its code.
#[test]
fn attributes_with_an_effect_or_a_refusal_are_diagnosed() {
    for (source, code) in [
        ("int (__attribute__((aligned(16))) x);", "E2903"),
        ("int (__attribute__((aligned(16))) *p);", "E2903"),
        ("int (__attribute__((weak)) x);", "E2903"),
        ("int (__attribute__((section(\"data\"))) x);", "E2903"),
        ("void (__attribute__((noinline)) f)(void);", "E2903"),
        ("void (__attribute__((always_inline)) f)(void);", "E2903"),
        ("typedef int (__attribute__((vector_size(16))) v);", "E2903"),
        ("typedef int (__attribute__((mode(DI))) wide);", "E2903"),
        ("void (__attribute__((naked)) *fp)(void);", "E2952"),
        (
            "void (__attribute__((aarch64_vector_pcs)) f)(void);",
            "E2952",
        ),
        (
            "int (__attribute__((scalar_storage_order(\"big-endian\"))) x);",
            "E2940",
        ),
    ] {
        let error = error(source);
        assert!(error.starts_with(&format!("{code}:")), "{source}: {error}");
    }
}

/// Both reference compilers refuse these positions.
#[test]
fn positions_that_the_reference_compilers_refuse_stay_errors() {
    for source in [
        "int (x __attribute__((unused)));",
        "void (*f __attribute__((unused)))(void);",
        "int ([[gnu::unused]] x);",
    ] {
        let _ = error(source);
    }
}
