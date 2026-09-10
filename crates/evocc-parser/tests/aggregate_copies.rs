//! Record/union copies and flexible-array initialization at the C front end.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(10_201),
        "aggregate-copies.c".to_owned(),
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
    match parser.diagnostic().filter(|_| parser.had_error()) {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None if parser.had_error() => Err("parser failed without a diagnostic".to_owned()),
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
fn member_type_union_casts_are_values_with_checked_initializers() {
    accepts(include_str!("../../../tests/fixtures/union-member-casts.c"));
    accepts("union value{int member:32;};union value f(void){return (union value)7;}");
    accepts("union value{int member;};int f(void);_Static_assert(sizeof((union value)f())==4,\"unevaluated\");");
    rejects(
        "union value { int member; }; int f(void){((union value)1).member=2;return 0;}",
        "E2101",
    );
}

#[test]
fn static_union_member_casts_cannot_hide_a_nonconstant_initializer() {
    for source in [
        "union value{int member;};int f(void);union value saved=(union value)f();",
        "union value{int member;};int f(void);void g(void){static union value saved=(union value)f();}",
    ] {
        rejects(source, "E2208");
    }
}

#[test]
fn union_casts_require_a_compatible_member_not_an_assignment_conversion() {
    for source in [
        "union value{long member;};union value f(void){return (union value)1;}",
        "struct a{int member;};struct b{int member;};union value{struct a member;};union value f(struct b b){return (union value)b;}",
        "union value{char *member;};union value f(const char *p){return (union value)p;}",
        "union value{void *member;};union value f(void){return (union value)0;}",
        "union value{int member:3;};union value f(void){return (union value)7;}",
    ] {
        rejects(source, "E2942");
    }
}

#[test]
fn record_union_nested_and_array_members_are_assignable() {
    accepts(concat!(
        "struct Inner { int x; }; struct S { int a[2]; struct Inner inner; }; ",
        "union U { long wide; struct S record; }; ",
        "void records(struct S *p, struct S *q) { *p = *q; } ",
        "void unions(union U *p, union U *q) { *p = *q; }\n",
    ));
}

#[test]
fn aggregate_assignment_through_an_immediate_address_and_dereference_is_a_place() {
    accepts(concat!(
        "typedef struct { int raw_lock; } spinlock_t; ",
        "struct maple_tree { spinlock_t ma_lock; }; ",
        "void direct(struct maple_tree *tree) { ",
        "tree->ma_lock = (spinlock_t){ .raw_lock = 0 }; }\n",
    ));
    accepts(concat!(
        "typedef struct { int raw_lock; } spinlock_t; ",
        "struct maple_tree { spinlock_t ma_lock; }; ",
        "void addressed(struct maple_tree *tree) { ",
        "*(&tree->ma_lock) = (spinlock_t){ .raw_lock = 0 }; }\n",
    ));
}

#[test]
fn copy_initializers_accept_objects_calls_dereferences_globals_and_literals() {
    accepts(concat!(
        "struct S { int x; long y; }; struct S global; ",
        "struct S make(void) { struct S value = { 3, 4 }; return value; } ",
        "int f(struct S input, struct S *pointer) { ",
        "struct S a = input; struct S b = make(); struct S c = *pointer; ",
        "struct S d = global; struct S e = (struct S){ 5, 6 }; ",
        "return a.x + b.x + c.x + d.x + e.x; }\n",
    ));
}

#[test]
fn assignment_chains_retain_an_aggregate_expression_value() {
    accepts(concat!(
        "struct S { int x; int y; }; ",
        "int f(void) { struct S a = { 1, 2 }; struct S b = { 3, 4 }; ",
        "struct S c = { 5, 6 }; return (a = b = c).x + a.y; }\n",
    ));
}

#[test]
fn volatile_record_assignment_is_accepted_for_memberwise_access() {
    accepts(concat!(
        "struct S { int x; long y; }; ",
        "void f(volatile struct S *destination, volatile struct S *source) { ",
        "*destination = *source; }\n",
    ));
}

#[test]
fn flexible_array_initializers_are_file_scope_only() {
    accepts("struct T { int n; int d[]; }; struct T object = { 1, { 2, 3 } };\n");
    rejects(
        "struct T { int n; int d[]; }; void f(void) { struct T object = { 1, { 2, 3 } }; }\n",
        "E2919",
    );
}

#[test]
fn by_value_record_return_stays_accepted() {
    accepts(concat!(
        "struct S { int x; long y; }; ",
        "struct S identity(struct S value) { return value; }\n",
    ));
}

#[test]
fn atomic_updates_stay_refused_and_scalar_volatile_updates_stay_accepted() {
    rejects("void f(_Atomic int *pointer) { *pointer = 1; }\n", "E2702");
    accepts("void f(volatile int *pointer) { (*pointer)++; *pointer += 2; }\n");
}

#[test]
fn expanded_kernel_cmpxchg_declaration_parses_verbatim() {
    accepts(concat!(
        "typedef unsigned __int128 u128; typedef unsigned long u64; ",
        "union __u128_halves { u128 full; struct { u64 low, high; }; }; ",
        "static inline __attribute__((__gnu_inline__)) __attribute__((__unused__)) ",
        "__attribute__((__no_instrument_function__)) __attribute__((__always_inline__)) ",
        "u128 __ll_sc__cmpxchg128(volatile u128 *ptr, u128 old, u128 new) { ",
        "union __u128_halves r, o = { .full = (old) }, n = { .full = (new) }; ",
        "unsigned int tmp; asm volatile(\"// __cmpxchg128\\n\" ",
        "\"ldxp %[rl], %[rh], %[v]\\n\" ",
        "\"stxp %w[tmp], %[nl], %[nh], %[v]\\n\" ",
        ": [v] \"+Q\" (*(u128 *)ptr), [rl] \"=&r\" (r.low), ",
        "[rh] \"=&r\" (r.high), [tmp] \"=&r\" (tmp) ",
        ": [ol] \"r\" (o.low), [oh] \"r\" (o.high), ",
        "[nl] \"r\" (n.low), [nh] \"r\" (n.high) : \"cc\"); return r.full; }\n",
    ));
}
