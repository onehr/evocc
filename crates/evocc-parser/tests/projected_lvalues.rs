//! Executable specification for projected C lvalues.
//!
//! A projected lvalue designates a subobject reached from a root object by a
//! chain of member and subscript projections: `s.f`, `p->f`, `a[i]`, and any
//! composition of them. These are the lvalue forms that C code is actually
//! written in, and every one of them must agree with the C abstract machine on
//! three separate things: which object is designated, how many times each
//! subexpression is evaluated, and which qualifiers govern the access.
//!
//! Each test below pins one of those obligations against real C source parsed
//! and executed end to end, so a regression shows up as a wrong value or a
//! wrong diagnostic rather than as a silently different address computation.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_memory::{VolatileDirection, VolatileEvent};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 200_000;

/// Parse one self-contained translation unit, or return its first diagnostic
/// rendered as `CODE: message` so rejection tests can assert on it exactly.
fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_101),
        "projected-lvalues.c".to_owned(),
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

/// Execution always starts at `main`, never at whichever definition happens to
/// come first: a fixture that measures side effects declares its helper above
/// `main`, and entering the helper would quietly measure nothing.
fn entry(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.body.is_some() && function.name.spelling == "main" =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("every fixture defines main")
}

/// Execute `main` and return its value, or the diagnostic code it failed with.
fn run(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena)?;
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .map_err(|error| format!("value: {error}"))
}

fn volatile_trace(source: &str) -> Vec<VolatileEvent> {
    let arena = Arena::default();
    let unit = parse(source, &arena).expect("fixture parses");
    interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
        .expect("fixture executes")
        .world
        .volatile_trace()
        .to_vec()
}

/// Assert the program is rejected, and that it is rejected for the stated
/// reason. A test that only asserts "some error" cannot tell a real guard from
/// an unrelated parse failure.
fn expect_rejected(source: &str, code: &str) {
    match run(source) {
        Ok(value) => panic!("expected rejection with {code}, but the program returned {value}"),
        Err(actual) => assert!(
            actual.starts_with(&format!("{code}:")),
            "expected rejection with {code}, got {actual}"
        ),
    }
}

// ---------------------------------------------------------------------------
// Member projections
// ---------------------------------------------------------------------------

#[test]
fn struct_member_round_trips_through_memory() {
    assert_eq!(
        run("struct S { int a; int b; }; \
             int main(void) { struct S s; s.a = 3; s.b = 7; return s.a * 10 + s.b; }")
        .unwrap(),
        37
    );
}

#[test]
fn distinct_members_do_not_alias() {
    // The whole point of a field offset: writing `b` must leave `a` intact.
    assert_eq!(
        run("struct S { int a; int b; }; \
             int main(void) { struct S s; s.a = 1; s.b = 2; s.b = 9; return s.a; }")
        .unwrap(),
        1
    );
}

#[test]
fn union_members_share_one_storage_region() {
    // Union members all begin at offset zero, so a store through one member is
    // observable through another of the same width.
    assert_eq!(
        run("union U { int a; int b; }; \
             int main(void) { union U u; u.a = 5; return u.b; }")
        .unwrap(),
        5
    );
}

#[test]
fn nested_struct_members_compose_offsets() {
    assert_eq!(
        run(
            "struct Inner { int x; int y; }; struct Outer { int lead; struct Inner inner; }; \
             int main(void) { struct Outer o; o.lead = 1; o.inner.x = 2; o.inner.y = 3; \
                              return o.lead * 100 + o.inner.x * 10 + o.inner.y; }"
        )
        .unwrap(),
        123
    );
}

#[test]
fn arrow_member_projects_through_a_pointer_root() {
    // `p->f` is `(*p).f`: the root is the pointer expression, not an object.
    assert_eq!(
        run("struct S { int a; int b; }; \
             int main(void) { struct S s; struct S *p = &s; p->a = 4; p->b = 6; \
                              return s.a * 10 + s.b; }")
        .unwrap(),
        46
    );
}

// ---------------------------------------------------------------------------
// Subscript projections
// ---------------------------------------------------------------------------

#[test]
fn array_element_round_trips_through_memory() {
    assert_eq!(
        run("int main(void) { int a[3] = {0, 0, 0}; a[1] = 6; return a[1]; }").unwrap(),
        6
    );
}

#[test]
fn subscript_index_is_a_full_expression() {
    assert_eq!(
        run("int main(void) { int a[4] = {0, 0, 0, 0}; int i = 1; a[i + 2] = 8; return a[3]; }")
            .unwrap(),
        8
    );
}

#[test]
fn subscripting_a_pointer_reaches_the_pointed_to_array() {
    assert_eq!(
        run("int main(void) { int a[3] = {0, 0, 0}; int *p = a; p[2] = 9; return a[2]; }").unwrap(),
        9
    );
}

#[test]
fn subscript_operands_commute() {
    // C11 6.5.2.1p2: `E1[E2]` is identical to `(*((E1)+(E2)))`, so `2[a]` and
    // `a[2]` designate the same object. Rare in source, but it is the language.
    assert_eq!(
        run("int main(void) { int a[3] = {0, 0, 0}; 2[a] = 7; return a[2]; }").unwrap(),
        7
    );
}

#[test]
fn multidimensional_subscripts_compose() {
    assert_eq!(
        run(
            "int main(void) { int m[2][3] = {{0, 0, 0}, {0, 0, 0}}; m[1][2] = 8; \
             return m[1][2]; }"
        )
        .unwrap(),
        8
    );
}

// ---------------------------------------------------------------------------
// Mixed chains
// ---------------------------------------------------------------------------

#[test]
fn subscript_through_a_member_composes_with_field_offset() {
    assert_eq!(
        run("struct S { int lead; int values[3]; }; \
             int main(void) { struct S s; s.lead = 1; s.values[2] = 5; \
                              return s.lead * 10 + s.values[2]; }")
        .unwrap(),
        15
    );
}

#[test]
fn member_of_an_array_element_composes_with_element_stride() {
    assert_eq!(
        run("struct S { int a; int b; }; \
             int main(void) { struct S xs[2]; xs[1].a = 3; xs[1].b = 4; \
                              return xs[1].a * 10 + xs[1].b; }")
        .unwrap(),
        34
    );
}

#[test]
fn a_linked_structure_is_traversable_through_stored_pointers() {
    assert_eq!(
        run("struct Node { int value; struct Node *next; }; \
             int main(void) { struct Node a; struct Node b; struct Node c; \
                              a.value = 4; b.value = 5; c.value = 6; \
                              a.next = &b; b.next = &c; c.next = 0; \
                              struct Node *p = &a; int sum = 0; \
                              while (p) { sum += p->value; p = p->next; } return sum; }")
        .unwrap(),
        15
    );
}

// ---------------------------------------------------------------------------
// Evaluation count
// ---------------------------------------------------------------------------

#[test]
fn a_subscript_index_is_evaluated_exactly_once_in_a_simple_assignment() {
    // If the place were re-evaluated to compute the address a second time, the
    // observable call count would be 2 and the value would still look right.
    // Only counting the side effect catches that.
    assert_eq!(
        run("int calls; int idx(void) { calls = calls + 1; return 1; } \
             int main(void) { int a[3] = {0, 0, 0}; a[idx()] = 7; \
                              return calls * 100 + a[1]; }")
        .unwrap(),
        107
    );
}

#[test]
fn a_compound_assignment_evaluates_its_lvalue_exactly_once() {
    // C11 6.5.16.2p3: `E1 op= E2` behaves as `E1 = E1 op (E2)` except that the
    // lvalue E1 is evaluated only once. A naive desugaring evaluates `idx()`
    // twice and reports 2 here.
    assert_eq!(
        run("int calls; int idx(void) { calls = calls + 1; return 1; } \
             int main(void) { int a[3] = {0, 10, 0}; a[idx()] += 5; \
                              return calls * 100 + a[1]; }")
        .unwrap(),
        115
    );
}

#[test]
fn an_increment_evaluates_its_lvalue_exactly_once() {
    assert_eq!(
        run("int calls; int idx(void) { calls = calls + 1; return 1; } \
             int main(void) { int a[3] = {0, 10, 0}; a[idx()]++; \
                              return calls * 100 + a[1]; }")
        .unwrap(),
        111
    );
}

// ---------------------------------------------------------------------------
// Qualifiers
// ---------------------------------------------------------------------------

#[test]
fn a_const_array_element_is_not_a_modifiable_lvalue() {
    expect_rejected(
        "int main(void) { const int a[3] = {1, 2, 3}; a[0] = 5; return a[0]; }",
        "E2101",
    );
}

#[test]
fn const_propagates_from_a_member_to_its_projection() {
    // C11 6.5.2.3: a member of a const-qualified structure is const-qualified,
    // and so is a member declared const in an otherwise unqualified structure.
    expect_rejected(
        "struct S { const int c; int m; }; \
         int main(void) { struct S s; s.c = 1; return s.m; }",
        "E2101",
    );
}

#[test]
fn volatile_array_elements_are_observable_accesses_in_order() {
    let trace = volatile_trace(
        "int main(void) { volatile int a[2]; a[0] = 11; a[1] = 22; \
                          int first = a[0]; return first; }",
    );
    let directions: Vec<VolatileDirection> = trace.iter().map(|event| event.direction).collect();
    assert_eq!(
        directions,
        vec![
            VolatileDirection::Store,
            VolatileDirection::Store,
            VolatileDirection::Load
        ],
        "every volatile subobject access must appear in the world trace, in order"
    );
    // The two stores hit distinct subobjects; the load must hit the first one.
    assert_ne!(trace[0].byte_offset, trace[1].byte_offset);
    assert_eq!(trace[0].byte_offset, trace[2].byte_offset);
}

// ---------------------------------------------------------------------------
// Fail-closed boundaries
// ---------------------------------------------------------------------------

#[test]
fn a_member_of_an_incomplete_record_cannot_be_projected() {
    expect_rejected(
        "struct Opaque; \
         int main(void) { struct Opaque *p; return p->field; }",
        "E2102",
    );
}

#[test]
fn a_member_name_that_does_not_exist_is_rejected() {
    expect_rejected(
        "struct S { int a; }; int main(void) { struct S s; s.a = 1; return s.missing; }",
        "E2102",
    );
}

#[test]
fn a_dot_projection_requires_a_record_operand() {
    expect_rejected("int main(void) { int x = 1; return x.a; }", "E2102");
}

#[test]
fn subscripting_a_non_pointer_is_rejected() {
    expect_rejected("int main(void) { int x = 1; return x[0]; }", "E2102");
}

#[test]
fn a_non_integer_subscript_is_rejected() {
    expect_rejected(
        "int main(void) { int a[2] = {0, 0}; int *p = a; return a[p]; }",
        "E2102",
    );
}
