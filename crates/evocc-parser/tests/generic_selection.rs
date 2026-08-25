//! `_Generic` (specification 10.1.10): selection resolved by the front end.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> Result<CHirUnit<'a>, String> {
    let file = SourceFile::new(
        FileId::new(9_302),
        "generic-selection.c".to_owned(),
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

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\n  source: {source}");
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n  source: {source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\n  source: {source}"
        ),
    }
}

fn layout_of(source: &str, name: &str) -> (u64, u32) {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| panic!("must parse, got {error}"));
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), name)
        .expect("symbol table is live")
        .unwrap_or_else(|| panic!("{name} is not declared"));
    let ty = unit.symbols().symbol(symbol).expect("declared").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .unwrap_or_else(|error| panic!("layout of {name}: {error:?}"));
    (layout.size_bytes, layout.align_bytes)
}

fn constant(expression: &str) -> u64 {
    layout_of(&format!("char a[{expression}];\n"), "a").0
}

#[test]
fn generic_selects_the_association_compatible_with_the_controlling_type() {
    assert_eq!(constant("_Generic(1, int: 10, long: 20, default: 30)"), 10);
    assert_eq!(constant("_Generic(1L, int: 10, long: 20, default: 30)"), 20);
    assert_eq!(
        constant("_Generic(1.0, int: 10, long: 20, default: 30)"),
        30
    );
}

#[test]
fn controlling_type_is_taken_after_lvalue_conversion() {
    accepts("const int c = 0; int x = _Generic(c, int: 1);\n");
    accepts("int a[3]; int y = _Generic(a, int *: 1);\n");
    accepts("void g(void); int z = _Generic(g, void (*)(void): 1);\n");
    rejects("int a[3]; int y = _Generic(a, int[3]: 1);\n", "E2908");
}

#[test]
fn unselected_branches_are_parsed_but_not_retained_or_evaluated() {
    accepts("int f(void *p) { return _Generic(1, int: 1, default: *(int *)p); }\n");
    let arena = Arena::default();
    let unit = parse(
        "int f(void) { int i = 0; return _Generic(1, int: 1, default: i++); }\n",
        &arena,
    )
    .expect("generic selection must parse");
    let retained = format!("{unit:#?}");
    assert!(retained.contains("GenericSelectionRecord"), "{retained}");
    assert!(!retained.contains("Update"), "{retained}");
}

#[test]
fn generic_is_a_constant_expression_when_its_selection_is() {
    accepts("int a[_Generic(1, int: 4, default: 8)];\n");
    assert_eq!(
        layout_of("int a[_Generic(1, int: 4, default: 8)];\n", "a"),
        (16, 4)
    );
}

#[test]
fn duplicate_or_missing_associations_are_refused() {
    rejects("int x = _Generic(1, int: 1, signed int: 2);\n", "E2907");
    rejects("int x = _Generic(1, default: 1, default: 2);\n", "E2907");
    rejects("int x = _Generic(1, long: 1);\n", "E2908");
}

#[test]
fn generic_result_is_the_selected_expression_including_lvalues() {
    accepts("int f(void) { int i = 0; _Generic(i, int: i, default: i) = 3; return i; }\n");
}
