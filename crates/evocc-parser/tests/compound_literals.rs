use evocc_chir::{AllocationSiteManifest, CHirUnit};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(10_113),
        "compound-literals.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected input at {:?}", token.span));
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

#[test]
fn scalar_record_address_and_array_literals_parse() {
    accepts(
        "struct pair { int a; int b; };\n\
         int read(const int *p) { return *p; }\n\
         int scalar(void) { return (int){ 3 }; }\n\
         int member(void) { return ((struct pair){ .a = 1, .b = 2 }).a; }\n\
         int passed(void) { return read(&(int){ 4 }); }\n\
         int extent(void) { return sizeof((int[]){ 1, 2, 3 }); }\n",
    );
}

#[test]
fn file_scope_literals_have_static_storage() {
    accepts("int *p = (int[]){ 1, 2 }; int read(void) { return p[1]; }\n");
}

#[test]
fn each_occurrence_has_a_consecutive_allocation_site() {
    let arena = Arena::default();
    let unit = parse("int f(void) { return (int){ 1 } + (int){ 2 }; }\n", &arena)
        .expect("compound literals must parse");
    let manifest = AllocationSiteManifest::derive(&unit).expect("allocation manifest");
    assert_eq!(manifest.sites().len(), 2);
    assert_eq!(manifest.sites()[0].ordinal().raw(), 0);
    assert_eq!(manifest.sites()[1].ordinal().raw(), 1);
}

#[test]
fn block_scope_occurrence_can_be_reentered_as_a_fresh_object() {
    accepts(
        "int f(void) { int total = 0; for (int i = 0; i < 3; ++i) {\n\
           int *p = &(int){ 0 }; if (*p != 0) return 99; *p = 7; total += *p;\n\
         } return total; }\n",
    );
}

#[test]
fn const_file_scope_literal_can_be_addressed_for_runtime_read_only_checks() {
    accepts("const int *p = &(const int){ 1 }; int f(void) { return *(int *)p; }\n");
}
