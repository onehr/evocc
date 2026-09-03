use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_201);
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
            "array lvalue must decay, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

#[test]
fn fixed_array_member_decays_before_a_following_arrow() {
    accepts(concat!(
        "struct kvec { unsigned long iov_len; }; ",
        "struct xb { struct kvec head[1]; unsigned long len; }; ",
        "int probe(struct xb *buf) { return buf->len != buf->head->iov_len; }\n",
    ));
}

#[test]
fn flexible_and_zero_length_array_members_decay() {
    accepts(concat!(
        "struct nh { int fam; }; struct fi { int flags; struct nh fib6_nh[]; }; ",
        "int flexible(const struct fi *f) { return f->fib6_nh->fam; } ",
        "struct z { int n; char buf[0]; }; char *zero(struct z *p) { return p->buf; }\n",
    ));
}

#[test]
fn string_literals_and_array_of_array_elements_decay() {
    accepts(concat!(
        "char letter(int i) { return \"RSDTZW\"[i]; } ",
        "int *row(int values[2][3], int i) { return values[i]; }\n",
    ));
}

#[test]
fn compound_literal_arrays_decay() {
    accepts("int *values(void) { return (int[2]){ 1, 2 }; }\n");
}

#[test]
fn array_lvalues_decay_inside_conditional_arms() {
    accepts("int f(void) { return (0 ? \"\" : 0)[0]; }\n");
    accepts(concat!(
        "int *choose(int condition, int (*array)[4]) { ",
        "return condition ? *array : (int *)0; }\n",
    ));
}

#[test]
fn kfifo_get_conditional_array_idiom_is_a_projectable_place() {
    accepts(concat!(
        "struct fifo { unsigned int in; unsigned int out; int *type; int buf[4]; }; ",
        "int kfifo_get_shape(struct fifo *fifo, int *value) { ",
        "*value = (0 ? ((typeof(fifo->type))(void *)0) : fifo->buf)[fifo->out & 3]; ",
        "return 1; }\n",
    ));
}

#[test]
fn kfifo_put_conditional_array_idiom_is_a_projectable_place() {
    accepts(concat!(
        "struct fifo { unsigned int in; unsigned int out; int *type; int buf[4]; }; ",
        "int kfifo_put_shape(struct fifo *fifo, int value) { ",
        "(0 ? ((typeof(fifo->type))(void *)0) : fifo->buf)[fifo->in & 3] = value; ",
        "return 1; }\n",
    ));
}
