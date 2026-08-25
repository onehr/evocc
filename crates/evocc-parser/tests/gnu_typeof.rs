//! `typeof` (specification 10.1.10): a type resolved by the front end.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> Result<CHirUnit<'a>, String> {
    let file = SourceFile::new(
        FileId::new(9_301),
        "gnu-typeof.c".to_owned(),
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

#[test]
fn typeof_of_a_type_name_is_that_type() {
    assert_eq!(layout_of("typeof(int) x;\n", "x"), (4, 4));
    assert_eq!(layout_of("__typeof__(long) x;\n", "x"), (8, 8));
    assert_eq!(layout_of("typeof(char *) x;\n", "x"), (8, 8));
}

#[test]
fn typeof_of_an_expression_is_its_type_before_lvalue_conversion() {
    assert_eq!(layout_of("int a[3]; typeof(a) b;\n", "b"), (12, 4));
    assert_eq!(
        layout_of("const int c = 1; typeof(c) d = 2;\n", "d"),
        (4, 4)
    );
    rejects(
        "const int c = 1; typeof(c) d = 2; void f(void) { d = 3; }\n",
        "E2101",
    );
}

#[test]
fn typeof_operand_is_not_evaluated_but_its_written_form_is_retained() {
    let arena = Arena::default();
    let unit = parse("int i; typeof(i++) j;\n", &arena).expect("typeof must parse");
    let retained = format!("{unit:#?}");
    assert!(retained.contains("Typeof"), "{retained}");
    assert!(retained.contains("Expression"), "{retained}");
    assert!(!retained.contains("Update"), "{retained}");
}

#[test]
fn typeof_nests_and_composes_with_declarators() {
    assert_eq!(layout_of("typeof(typeof(int)) x;\n", "x"), (4, 4));
    assert_eq!(layout_of("typeof(int) *p;\n", "p"), (8, 8));
    assert_eq!(layout_of("typeof(int) a[2];\n", "a"), (8, 4));
    accepts("void f(typeof(int) x, typeof(x) y);\n");
    accepts("struct s { typeof(int) m; };\n");
    accepts("int g(void) { return sizeof(typeof(int)); }\n");
    accepts("int h(void) { return (typeof(int))1; }\n");
}

#[test]
fn typeof_of_a_variably_modified_type_is_refused() {
    rejects("void f(int n) { typeof(int[n]) a; }\n", "E2904");
}

#[test]
fn the_kernel_read_once_line_parses() {
    accepts(concat!(
        "unsigned long f(unsigned long *addr) {\n",
        " return (*(const volatile typeof( _Generic((*(unsigned long *)addr), char: (char)0, ",
        "unsigned char: (unsigned char)0, signed char: (signed char)0, unsigned short: (unsigned short)0, ",
        "signed short: (signed short)0, unsigned int: (unsigned int)0, signed int: (signed int)0, ",
        "unsigned long: (unsigned long)0, signed long: (signed long)0, unsigned long long: (unsigned long long)0, ",
        "signed long long: (signed long long)0, default: (*(unsigned long *)addr))) *)&(*(unsigned long *)addr));\n",
        "}\n"
    ));
}
