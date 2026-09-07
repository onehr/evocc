use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_206);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
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
            "library builtin must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

const SIZE_T: &str = "typedef unsigned long size_t; ";

#[test]
fn conflicting_termination_spellings_fail_closed() {
    for name in ["__builtin_exit", "__builtin__Exit"] {
        let arena = Arena::default();
        assert_eq!(
            parse(&format!("void f(void){{{name}(0);}}"), &arena)
                .unwrap_err()
                .code,
            "E2933"
        );
    }
}

#[test]
fn memory_builtins_carry_their_library_prototypes() {
    accepts(&format!(
        "{SIZE_T} void *copy(void *d, const void *s, size_t n) {{ __builtin_memset(d, 0, n); return __builtin_memcpy(d, s, n); }} int compare(const void *a, const void *b, size_t n) {{ return __builtin_memcmp(a, b, n); }}\n"
    ));
}

#[test]
fn string_builtins_do_not_require_visible_library_declarations() {
    accepts(&format!(
        "{SIZE_T} int compare(const char *a, const char *b, size_t n) {{ return __builtin_strcmp(a, b) + __builtin_strncmp(a, b, n) + (int)__builtin_strlen(a); }}\n"
    ));
}

#[test]
fn alloca_and_library_result_types_match_the_builtin_prototypes() {
    accepts(&format!(
        "{SIZE_T} int probe(void) {{ _Static_assert(__builtin_types_compatible_p(typeof(__builtin_memset((void *)0, 0, 0)), void *), \"memset\"); _Static_assert(__builtin_types_compatible_p(typeof(__builtin_strlen(\"\")), size_t), \"strlen\"); _Static_assert(__builtin_types_compatible_p(typeof(__builtin_alloca(1)), void *), \"alloca\"); return 0; }}\n"
    ));
}

#[test]
fn ctype_builtins_take_and_return_int() {
    accepts(concat!(
        "int probe(int value) { ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_isdigit(value)), int), \"isdigit\"); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_isalpha(value)), int), \"isalpha\"); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_isspace(value)), int), \"isspace\"); ",
        "return __builtin_toupper(value) + __builtin_tolower(value); }\n",
    ));
}

#[test]
fn incompatible_visible_library_declaration_is_e2927() {
    let source = concat!(
        "typedef unsigned long size_t; int strlen(int); ",
        "size_t probe(char *text) { return __builtin_strlen(text); }\n",
    );
    let arena = Arena::default();
    let error = parse(source, &arena).expect_err("incompatible declaration must fail");
    assert_eq!(error.code, "E2927");
    assert_eq!(error.message, "builtin-library-declaration-incompatible");
}

#[test]
fn memmove_uses_its_library_prototype_without_a_visible_declaration() {
    accepts(&format!(
        "{SIZE_T} void *move(void *destination, const void *source, size_t count) {{ _Static_assert(__builtin_types_compatible_p(typeof(__builtin_memmove(destination, source, count)), void *), \"memmove\"); return __builtin_memmove(destination, source, count); }}\n"
    ));
}

#[test]
fn prefetch_accepts_one_to_three_operands_and_has_void_type() {
    accepts(concat!(
        "void probe(const void *address) { ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_prefetch(address)), void), \"one\"); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_prefetch(address, 0)), void), \"two\"); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_prefetch(address, 1, 3)), void), \"three\"); ",
        "__builtin_prefetch(address); __builtin_prefetch(address, 0); ",
        "__builtin_prefetch(address, 1, 3); }\n",
    ));
}

#[test]
fn extract_return_address_has_the_measured_void_pointer_type() {
    accepts(concat!(
        "void *probe(void *address) { ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_extract_return_addr(address)), void *), \"return address\"); ",
        "return __builtin_extract_return_addr(address); }\n",
    ));
}

#[test]
fn synthesized_library_prototypes_are_unique_and_in_no_lexical_scope() {
    let arena = Arena::default();
    let unit = parse("void f(void *p) { __builtin_memset(p, 0, 1); } void g(void *p) { __builtin_memset(p, 0, 2); }", &arena).unwrap();
    let symbols: Vec<_> = unit
        .symbols()
        .symbols_in_source_order()
        .iter()
        .filter(|symbol| symbol.spelling() == "memset")
        .collect();
    assert_eq!(symbols.len(), 1);
    assert!(unit
        .symbols()
        .scopes_in_source_order()
        .iter()
        .all(|scope| !scope.symbols().contains(&symbols[0].id())));
    evocc_chir::AllocationSiteManifest::derive(&unit).unwrap();
    let speculative = parse("void f(void *p) { typeof(__builtin_memset(p, 0, 1)) q = __builtin_memset(p, 0, 2); (void)q; }", &arena).unwrap();
    evocc_chir::AllocationSiteManifest::derive(&speculative).unwrap();
}

#[test]
fn library_builtins_do_not_declare_or_shadow_ordinary_identifiers() {
    accepts("void f(void *p) { __builtin_memset(p, 0, 1); } static int memset = 3; int g(void) { return memset; } void h(void *p) { __builtin_memset(p, 0, 2); }");
    let arena = Arena::default();
    let error = parse(
        "void f(void *p) { __builtin_memset(p, 0, 1); memset(p, 0, 1); }",
        &arena,
    )
    .unwrap_err();
    assert_eq!(error.code, "E2001");
}
