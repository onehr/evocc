use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(10_111),
        "statement-expressions.c".to_owned(),
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

#[test]
fn final_expression_yields_a_value_and_nesting_is_compositional() {
    accepts(
        "int value(void) { return ({ int t = 1; t + 1; }); }\n\
         int nested(void) { return ({ ({ 1; }) + ({ 2; }); }); }\n\
         void empty(void) { ({ }); }\n",
    );
}

#[test]
fn result_undergoes_lvalue_conversion_and_is_never_assignable() {
    accepts("int f(void) { int a[3]; return sizeof(({ a; })) == sizeof(int *); }\n");
    rejects(
        "int f(void) { int x = 0; ({ x; }) = 1; return x; }\n",
        "E2101",
    );
    rejects("int f(void) { int x = ({ int t; }); return x; }\n", "E2202");
}

#[test]
fn control_flow_may_leave_but_not_enter_the_expression() {
    accepts(
        "int breaks(void) { for (;;) { int v = ({ if (1) break; 3; }); return v; } return 7; }\n\
         int continues(void) { int i = 0; for (; i < 2; ++i) { ({ if (i == 0) continue; 0; }); } return i; }\n\
         int returns(void) { return ({ if (1) return 9; 3; }); }\n\
         int jumps(void) { return ({ goto out; 3; }); out: return 4; }\n",
    );

    let arena = Arena::default();
    assert!(
        parse(
            "int f(void) { goto inside; ({ inside: 1; }); return 0; }\n",
            &arena,
        )
        .is_err(),
        "jumping into a statement expression must be rejected"
    );
}

#[test]
fn file_scope_statement_expressions_report_e2905() {
    rejects("int g = ({ 1; });\n", "E2905");
    rejects("_Static_assert(({ 1; }), \"\");\n", "E2905");
    rejects("int values[({ 4; })];\n", "E2905");
    rejects("enum { E = ({ 1; }) };\n", "E2905");
}

#[test]
fn function_scope_unevaluated_contexts_accept_statement_expressions() {
    accepts(concat!(
        "struct holder { int member; }; ",
        "void check(struct holder *pointer) { ",
        "_Static_assert(__builtin_types_compatible_p(",
        "typeof(*({ const typeof(pointer) same = pointer; same; })), ",
        "struct holder), \"container type\"); ",
        "_Static_assert(sizeof(({ int side = 0; side++; })) == sizeof(int), \"size\"); ",
        "_Static_assert(_Alignof(({ int side = 0; side++; })) == _Alignof(int), \"align\"); ",
        "}\n",
    ));
}

#[test]
fn function_scope_value_contexts_report_their_own_constant_diagnostic() {
    rejects(
        "void check(void) { _Static_assert(({ 1; }), \"value\"); }\n",
        "E2912",
    );
    rejects("void check(void) { int values[({ 4; })]; }\n", "E2203");
}

#[test]
fn measured_kernel_shapes_parse_without_surface_rewriting() {
    accepts(
        "unsigned long bit(unsigned long nr) {\n\
           return 1UL & (({\n\
             union { typeof(_Generic(nr, unsigned long: nr, default: nr)) __val; char __c[1]; } __u;\n\
             __u.__val = 8;\n\
             __u.__val;\n\
           }) >> nr);\n\
         }\n\
         struct range { unsigned long start; };\n\
         void set(struct range *r, struct range *r1) {\n\
           r->start = ({ typeof(r1->start) x = r1->start; x + 1; });\n\
         }\n",
    );
}
