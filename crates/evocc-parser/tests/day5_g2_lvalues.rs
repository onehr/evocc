use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use evocc_chir::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirStmt, ExprFacts, FunctionDecl, IntLit, ObjectDecl,
    PlaceRoot, ScalarPlace, UnaryOp,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

const FILE_RAW: u32 = 79;

struct SourceCase {
    id: &'static str,
    source: &'static str,
}

const SIMPLE: &str = "int f(void) { int x = 0; x = 7; return x; }\n";
const COMPOUNDS: &str = concat!(
    "int f(void) { int x = 8; ",
    "x += 1; x -= 1; x *= 2; x /= 2; x %= 3; ",
    "x <<= 1; x >>= 1; x &= 7; x |= 8; x ^= 3; return x; }\n",
);
const PRE_INCREMENT: &str = "int f(void) { int x = 0; ++x; return x; }\n";
const PRE_DECREMENT: &str = "int f(void) { int x = 1; --x; return x; }\n";
const POST_INCREMENT: &str = "int f(void) { int x = 0; x++; return x; }\n";
const POST_DECREMENT: &str = "int f(void) { int x = 1; x--; return x; }\n";
const RIGHT_ASSOCIATIVE: &str = "int f(void) { int a = 0; int b = 0; a = b = 3; return a + b; }\n";

const INVALID_LITERAL: &str = "int f(void) { 1 = 2; return 0; }\n";
const INVALID_BINARY: &str = "int f(void) { int x = 0; (x + 1) = 2; return x; }\n";
const INVALID_CONST: &str = "int f(void) { const int x = 1; x = 2; return x; }\n";
const INVALID_ARRAY: &str = "int f(void) { int a[2]; a = 0; return 0; }\n";
const INVALID_FUNCTION: &str =
    "int callee(void) { return 0; } int f(void) { callee = 1; return 0; }\n";
const INVALID_RVALUE: &str = "int f(void) { int x = 0; (int)x = 2; return x; }\n";
const INVALID_PREFIX: &str = "int f(void) { ++1; return 0; }\n";
const INVALID_POSTFIX: &str = "int f(void) { const int x = 1; x++; return x; }\n";

const DAY7_DEREF: &str = "int f(int *p) { *p = 1; return 0; }\n";
const DAY7_INDEX: &str = "int f(void) { int a[2]; a[0] = 1; return 0; }\n";
const DAY7_MEMBER: &str = "struct S { int x; }; int f(void) { struct S s; s.x = 1; return 0; }\n";
const DAY7_PTR_MEMBER: &str = "struct S { int x; }; int f(struct S *p) { p->x = 1; return 0; }\n";
const AGGREGATE_MEMBER_ASSIGN: &str = concat!(
    "struct Inner { int v; }; struct Outer { struct Inner inner; }; ",
    "int f(void) { struct Outer o; struct Inner other; o.inner = other; return 0; }\n"
);
const DAY7_VOLATILE: &str = "int f(void) { volatile int x = 0; x = 1; return 0; }\n";
const DAY7_ATOMIC: &str = "int f(void) { _Atomic int x = 0; x = 1; return 0; }\n";
const DAY7_ALIAS: &str = "int f(void) { int x=5; int *p=&x; int *q=p; *q=9; return *p; }\n";
const DAY7_UNION_ASSIGN: &str = concat!(
    "union U { int x; }; int f(void) { ",
    "union U left; union U right; left = right; return 0; }\n",
);
const INVALID_STRUCT_COMPOUND: &str = concat!(
    "struct S { int x; }; int f(void) { ",
    "struct S left; struct S right; left += right; return 0; }\n",
);
const INVALID_POINTER_MULTIPLY: &str = "int f(int *p) { p *= 2; return 0; }\n";
const INVALID_POINTER_FLOAT_ADD: &str = "int f(int *p) { p += 1.5; return 0; }\n";
const INVALID_FLOAT_SHIFT: &str = "int f(void) { float x = 1.0; x <<= 1; return 0; }\n";
// L04 and D27 admit GNU ++ and -- on complex values; %= stays invalid for every complex type.
const INVALID_COMPLEX_UPDATE: &str = "int f(void) { _Complex int z = 0; z %= 2; return 0; }\n";
const DAY7_POINTER_ADD: &str = "int f(int *p) { p += 1; return 0; }\n";
const DAY7_POINTER_UPDATE: &str = "int f(int *p) { p++; return 0; }\n";
const SCALAR_CONVERSION_CONTROLS: &str = concat!(
    "int f(void) { int x = 0; double d = 1.5; float r = 1.0; ",
    "int *p; int *q; x = d; d = x; p = q; p = 0; ",
    "r += 2.0; r++; return x; }\n",
);
const INVALID_POINTER_ASSIGNMENT: &str = "int f(void) { int *p; long *q; p = q; return 0; }\n";
const DAY7_STRUCT_ASSIGN: &str = concat!(
    "struct S { int x; }; int f(void) { ",
    "struct S left; struct S right; left = right; return 0; }\n",
);
const GNU_FUNCTION_POINTER_UPDATE: &str = "int f(void) { int (*p)(void); p++; return 0; }\n";
const INVALID_INCOMPLETE_POINTER_UPDATE: &str =
    "struct S; int f(void) { struct S *p; p++; return 0; }\n";
const DAY7_POINTER_PREFIX: &str = "int f(int *p) { ++p; return 0; }\n";
const DAY7_POINTER_SUBTRACT: &str = "int f(int *p) { p -= 1; return 0; }\n";
const COMPLEX_ADD_CONTROL: &str = "int f(void) { double _Complex z = 0; z += 1.0; return 0; }\n";
const STRING_POINTER_CONTROL: &str = "int f(void) { char *p; p = \"x\"; return 0; }\n";
const FAILED_UPDATE_ROLLBACK: &str =
    "int keep; int bad(void) { int *p; p *= 2; return 0; } long after;\n";
const PREFIXED_STRING_CONTROLS: &str = concat!(
    "int f(void) { char *c; unsigned int *w; unsigned short *s; unsigned int *u32; void *v; ",
    "c = u8\"a\"; w = L\"b\"; s = u\"c\"; u32 = U\"d\"; v = U\"e\"; return 0; }\n",
);
const INVALID_WIDE_TO_CHAR: &str = "int f(void) { char *p; p = L\"x\"; return 0; }\n";
const INVALID_U16_TO_INT: &str = "int f(void) { int *p; p = u\"x\"; return 0; }\n";
const INVALID_U32_TO_U16: &str = "int f(void) { unsigned short *p; p = U\"x\"; return 0; }\n";
const INVALID_U8_TO_U32: &str = "int f(void) { unsigned int *p; p = u8\"x\"; return 0; }\n";
const STRING_EXPRESSION_CONTROLS: &str = concat!(
    "int f(int c) { char *p; char *q; unsigned int *w; int x = 0; ",
    "p = c ? \"a\" : 0; p = c ? 0 : \"b\"; ",
    "q = c ? \"c\" : p; q = c ? p : \"d\"; ",
    "w = c ? L\"e\" : 0; x = \"f\" != 0; return x; }\n",
);
const POINTER_QUALIFIER_JOIN_CONTROLS: &str = concat!(
    "int f(int c) { const int *out; int *a; const int *b; int x = 0; ",
    "out = c ? a : b; out = c ? b : a; ",
    "x = a == b; x = b == a; return x; }\n",
);

const SOURCES: &[SourceCase] = &[
    SourceCase {
        id: "assignment.simple",
        source: SIMPLE,
    },
    SourceCase {
        id: "assignment.all-compound-operators",
        source: COMPOUNDS,
    },
    SourceCase {
        id: "update.pre-increment",
        source: PRE_INCREMENT,
    },
    SourceCase {
        id: "update.pre-decrement",
        source: PRE_DECREMENT,
    },
    SourceCase {
        id: "update.post-increment",
        source: POST_INCREMENT,
    },
    SourceCase {
        id: "update.post-decrement",
        source: POST_DECREMENT,
    },
    SourceCase {
        id: "assignment.right-associative",
        source: RIGHT_ASSOCIATIVE,
    },
    SourceCase {
        id: "invalid.literal-target",
        source: INVALID_LITERAL,
    },
    SourceCase {
        id: "invalid.binary-target",
        source: INVALID_BINARY,
    },
    SourceCase {
        id: "invalid.const-target",
        source: INVALID_CONST,
    },
    SourceCase {
        id: "invalid.array-target",
        source: INVALID_ARRAY,
    },
    SourceCase {
        id: "invalid.function-designator-target",
        source: INVALID_FUNCTION,
    },
    SourceCase {
        id: "invalid.rvalue-target",
        source: INVALID_RVALUE,
    },
    SourceCase {
        id: "invalid.prefix-literal",
        source: INVALID_PREFIX,
    },
    SourceCase {
        id: "invalid.postfix-const",
        source: INVALID_POSTFIX,
    },
    SourceCase {
        id: "boundary.day7-deref",
        source: DAY7_DEREF,
    },
    SourceCase {
        id: "boundary.day7-index",
        source: DAY7_INDEX,
    },
    SourceCase {
        id: "boundary.day7-member",
        source: DAY7_MEMBER,
    },
    SourceCase {
        id: "boundary.day7-pointer-member",
        source: DAY7_PTR_MEMBER,
    },
    SourceCase {
        id: "boundary.day7-volatile",
        source: DAY7_VOLATILE,
    },
    SourceCase {
        id: "boundary.day7-atomic",
        source: DAY7_ATOMIC,
    },
    SourceCase {
        id: "boundary.day7-union-assignment",
        source: DAY7_UNION_ASSIGN,
    },
    SourceCase {
        id: "invalid.struct-compound-assignment",
        source: INVALID_STRUCT_COMPOUND,
    },
    SourceCase {
        id: "invalid.pointer-multiply",
        source: INVALID_POINTER_MULTIPLY,
    },
    SourceCase {
        id: "invalid.pointer-plus-float",
        source: INVALID_POINTER_FLOAT_ADD,
    },
    SourceCase {
        id: "invalid.float-shift",
        source: INVALID_FLOAT_SHIFT,
    },
    SourceCase {
        id: "invalid.complex-update",
        source: INVALID_COMPLEX_UPDATE,
    },
    SourceCase {
        id: "boundary.day7-pointer-add",
        source: DAY7_POINTER_ADD,
    },
    SourceCase {
        id: "boundary.day7-pointer-update",
        source: DAY7_POINTER_UPDATE,
    },
    SourceCase {
        id: "assignment.scalar-conversion-controls",
        source: SCALAR_CONVERSION_CONTROLS,
    },
    SourceCase {
        id: "invalid.incompatible-pointer-assignment",
        source: INVALID_POINTER_ASSIGNMENT,
    },
    SourceCase {
        id: "boundary.day7-struct-assignment",
        source: DAY7_STRUCT_ASSIGN,
    },
    SourceCase {
        id: "gnu.function-pointer-update",
        source: GNU_FUNCTION_POINTER_UPDATE,
    },
    SourceCase {
        id: "invalid.incomplete-pointer-update",
        source: INVALID_INCOMPLETE_POINTER_UPDATE,
    },
    SourceCase {
        id: "boundary.day7-pointer-prefix-update",
        source: DAY7_POINTER_PREFIX,
    },
    SourceCase {
        id: "boundary.day7-pointer-subtract",
        source: DAY7_POINTER_SUBTRACT,
    },
    SourceCase {
        id: "assignment.complex-add-control",
        source: COMPLEX_ADD_CONTROL,
    },
    SourceCase {
        id: "assignment.string-pointer-control",
        source: STRING_POINTER_CONTROL,
    },
    SourceCase {
        id: "diagnostic.failed-update-rollback",
        source: FAILED_UPDATE_ROLLBACK,
    },
    SourceCase {
        id: "assignment.prefixed-string-controls",
        source: PREFIXED_STRING_CONTROLS,
    },
    SourceCase {
        id: "invalid.wide-string-to-char-pointer",
        source: INVALID_WIDE_TO_CHAR,
    },
    SourceCase {
        id: "invalid.u16-string-to-int-pointer",
        source: INVALID_U16_TO_INT,
    },
    SourceCase {
        id: "invalid.u32-string-to-u16-pointer",
        source: INVALID_U32_TO_U16,
    },
    SourceCase {
        id: "invalid.u8-string-to-u32-pointer",
        source: INVALID_U8_TO_U32,
    },
    SourceCase {
        id: "assignment.string-expression-controls",
        source: STRING_EXPRESSION_CONTROLS,
    },
    SourceCase {
        id: "assignment.pointer-qualifier-join-controls",
        source: POINTER_QUALIFIER_JOIN_CONTROLS,
    },
];

fn file() -> FileId {
    FileId::new(FILE_RAW)
}

fn span(source: &str, text: &str, occurrence: usize) -> SourceSpan {
    let (start, matched) = source
        .match_indices(text)
        .nth(occurrence)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {text:?}"));
    SourceSpan::new(
        file(),
        start as u32,
        (start + matched.len()) as u32,
        1,
        start as u32 + 1,
    )
}

fn span_text(source: &str, actual: SourceSpan) -> &str {
    assert_eq!(actual.file, file());
    &source[actual.byte_start as usize..actual.byte_end as usize]
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_owned(),
            Err(_) => "non-string panic payload".to_owned(),
        },
    }
}

fn parse<'arena>(
    source: &str,
    arena: &'arena Arena,
) -> (evocc_chir::CHirUnit<'arena>, Option<ParseDiagnostic>) {
    let mut lexer = Lexer::new(file(), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mut parser = Parser::new(tokens, file(), source);
        let unit = parser.parse_translation_unit_in(arena);
        (unit, parser.diagnostic().cloned())
    })) {
        Ok(result) => result,
        Err(payload) => panic!(
            "parser panicked instead of returning CHIR/diagnostic for {source:?}: {}",
            panic_text(payload)
        ),
    }
}

fn parse_valid<'arena>(source: &str, arena: &'arena Arena) -> evocc_chir::CHirUnit<'arena> {
    let (unit, diagnostic) = parse(source, arena);
    assert_eq!(
        diagnostic
            .as_ref()
            .map(|item| (item.code, item.message.as_str(), item.span)),
        None,
        "valid direct-scalar update source was rejected",
    );
    unit
}

fn function<'arena>(
    unit: &evocc_chir::CHirUnit<'arena>,
    index: usize,
) -> &'arena FunctionDecl<'arena> {
    match unit.declarations()[index] {
        CHirDecl::Function(declaration) => declaration,
        other => panic!("declaration {index} is not a function: {other:?}"),
    }
}

fn items<'arena>(function: &'arena FunctionDecl<'arena>) -> &'arena [CHirBlockItem<'arena>] {
    match function.body.expect("function body") {
        CHirStmt::Compound(items, _, _) => items,
        other => panic!("expected compound body, got {other:?}"),
    }
}

fn object<'arena>(item: &CHirBlockItem<'arena>) -> &'arena ObjectDecl<'arena> {
    match item {
        CHirBlockItem::Declaration(CHirDecl::Object(declaration)) => declaration,
        other => panic!("expected object declaration, got {other:?}"),
    }
}

fn expression<'arena>(item: &CHirBlockItem<'arena>) -> &'arena CHirExpr<'arena> {
    match item {
        CHirBlockItem::Statement(CHirStmt::Expr(expression, _)) => expression,
        other => panic!("expected expression statement, got {other:?}"),
    }
}

fn occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn assert_direct_scalar_node(
    unit: &evocc_chir::CHirUnit<'_>,
    declaration: &ObjectDecl<'_>,
    expression: &CHirExpr<'_>,
    source: &str,
    expected_text: &str,
    node_name: &str,
    operator_name: &str,
) {
    assert_eq!(span_text(source, expression.span()), expected_text);

    let place = ScalarPlace::bind(unit.symbols(), unit.types(), declaration.symbol)
        .expect("declared scalar must bind one direct place");
    assert_eq!(place.symbol(), declaration.symbol);
    assert_eq!(place.ty(), declaration.ty);

    // CHIR derives Debug directly from the semantic node. Keeping this projection
    // in the contract lets the same tests compile against the pre-S3 tuple node,
    // where they fail at runtime, while still checking the post-S3 node's fields.
    let node = format!("{expression:?}");
    assert!(
        node.starts_with(node_name),
        "expected {node_name} node, got {node}",
    );
    assert!(
        node.contains(&format!("op: {operator_name}")),
        "operator was not retained in {node}",
    );
    assert!(
        node.contains(&format!("place: {place:?}")),
        "node place does not join declared SymbolId/TypeId: {node}",
    );
    assert_eq!(
        occurrences(&node, &format!("spelling: {:?}", declaration.name.spelling)),
        1,
        "target expression must be retained exactly once: {node}",
    );
    let value_facts = ExprFacts::value(declaration.ty);
    assert!(
        node.contains(&format!("facts: {value_facts:?}")),
        "assignment/update result must be a value of target type: {node}",
    );
}

fn assert_diagnostic(source: &str, target: &str, occurrence: usize, code: &str, message: &str) {
    let arena = Arena::default();
    let (_, diagnostic) = parse(source, &arena);
    let diagnostic = diagnostic.expect("invalid update must return one diagnostic");
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.message, message);
    assert_eq!(diagnostic.span, span(source, target, occurrence));
}

#[test]
fn source_topology_is_frozen_and_byte_unique() {
    assert_eq!(SOURCES.len(), 46, "S3 source denominator changed");
    let ids = SOURCES.iter().map(|case| case.id).collect::<HashSet<_>>();
    let bytes = SOURCES
        .iter()
        .map(|case| case.source.as_bytes())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), SOURCES.len(), "duplicate S3 source id");
    assert_eq!(bytes.len(), SOURCES.len(), "duplicate S3 source bytes");
    assert!(SOURCES.iter().all(|case| case.source.ends_with('\n')));
}

#[test]
fn simple_assignment_retains_one_target_place_span_and_value_facts() {
    let arena = Arena::default();
    let unit = parse_valid(SIMPLE, &arena);
    let body = items(function(&unit, 0));
    let declaration = object(&body[0]);
    assert_direct_scalar_node(
        &unit,
        declaration,
        expression(&body[1]),
        SIMPLE,
        "x = 7",
        "Assign {",
        "Simple",
    );
}

#[test]
fn every_compound_assignment_token_retains_its_distinct_operator() {
    let expected = [
        ("x += 1", "Add"),
        ("x -= 1", "Subtract"),
        ("x *= 2", "Multiply"),
        ("x /= 2", "Divide"),
        ("x %= 3", "Remainder"),
        ("x <<= 1", "ShiftLeft"),
        ("x >>= 1", "ShiftRight"),
        ("x &= 7", "BitAnd"),
        ("x |= 8", "BitOr"),
        ("x ^= 3", "BitXor"),
    ];
    let arena = Arena::default();
    let unit = parse_valid(COMPOUNDS, &arena);
    let body = items(function(&unit, 0));
    let declaration = object(&body[0]);
    assert_eq!(body.len(), expected.len() + 2, "compound denominator");
    for ((text, operator), item) in expected.iter().zip(&body[1..=expected.len()]) {
        assert_direct_scalar_node(
            &unit,
            declaration,
            expression(item),
            COMPOUNDS,
            text,
            "Assign {",
            operator,
        );
    }
}

#[test]
fn all_four_increment_decrement_forms_retain_form_and_one_operand() {
    let cases = [
        (PRE_INCREMENT, "++x", "PreIncrement"),
        (PRE_DECREMENT, "--x", "PreDecrement"),
        (POST_INCREMENT, "x++", "PostIncrement"),
        (POST_DECREMENT, "x--", "PostDecrement"),
    ];
    for (source, text, operator) in cases {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        let body = items(function(&unit, 0));
        assert_direct_scalar_node(
            &unit,
            object(&body[0]),
            expression(&body[1]),
            source,
            text,
            "Update {",
            operator,
        );
    }
}

#[test]
fn assignment_is_right_associative_without_target_duplication() {
    let arena = Arena::default();
    let unit = parse_valid(RIGHT_ASSOCIATIVE, &arena);
    let body = items(function(&unit, 0));
    let root = expression(&body[2]);
    assert_eq!(span_text(RIGHT_ASSOCIATIVE, root.span()), "a = b = 3");
    let debug = format!("{root:?}");
    assert_eq!(occurrences(&debug, "Assign {"), 2, "{debug}");
    assert_eq!(occurrences(&debug, "op: Simple"), 2, "{debug}");
    assert_eq!(occurrences(&debug, "spelling: \"a\""), 1, "{debug}");
    assert_eq!(occurrences(&debug, "spelling: \"b\""), 1, "{debug}");

    let a = object(&body[0]);
    let b = object(&body[1]);
    let a_place = ScalarPlace::bind(unit.symbols(), unit.types(), a.symbol).unwrap();
    let b_place = ScalarPlace::bind(unit.symbols(), unit.types(), b.symbol).unwrap();
    let a_marker = format!("place: {a_place:?}");
    let b_marker = format!("place: {b_place:?}");
    let a_position = debug.find(&a_marker).expect("outer a place");
    let b_position = debug.find(&b_marker).expect("nested b place");
    assert!(
        a_position < b_position,
        "assignment tree is not right-nested: {debug}"
    );
}

#[test]
fn non_modifiable_targets_report_exact_e2101_without_panic() {
    let cases = [
        (INVALID_LITERAL, "1", 0),
        (INVALID_BINARY, "(x + 1)", 0),
        (INVALID_CONST, "x", 1),
        (INVALID_ARRAY, "a", 1),
        (INVALID_FUNCTION, "callee", 1),
        (INVALID_RVALUE, "(int)x", 0),
        (INVALID_PREFIX, "1", 0),
        (INVALID_POSTFIX, "x", 1),
    ];
    for (source, target, occurrence) in cases {
        assert_diagnostic(
            source,
            target,
            occurrence,
            "E2101",
            "expected-modifiable-lvalue",
        );
    }
}

#[test]
fn record_and_union_assignments_are_accepted() {
    for source in [DAY7_UNION_ASSIGN, DAY7_STRUCT_ASSIGN] {
        let arena = Arena::default();
        assert!(!parse_valid(source, &arena).is_empty());
    }
}

#[test]
fn scalar_assignment_conversions_and_real_updates_are_retained() {
    let arena = Arena::default();
    let unit = parse_valid(SCALAR_CONVERSION_CONTROLS, &arena);
    let body = items(function(&unit, 0));
    assert_eq!(body.len(), 12, "scalar conversion control denominator");
    let x = object(&body[0]);
    let d = object(&body[1]);
    let r = object(&body[2]);
    let p = object(&body[3]);

    for (item, declaration, text, node, op) in [
        (&body[5], x, "x = d", "Assign {", "Simple"),
        (&body[6], d, "d = x", "Assign {", "Simple"),
        (&body[7], p, "p = q", "Assign {", "Simple"),
        (&body[8], p, "p = 0", "Assign {", "Simple"),
        (&body[9], r, "r += 2.0", "Assign {", "Add"),
        (&body[10], r, "r++", "Update {", "PostIncrement"),
    ] {
        assert_direct_scalar_node(
            &unit,
            declaration,
            expression(item),
            SCALAR_CONVERSION_CONTROLS,
            text,
            node,
            op,
        );
    }
}

/// A floating complex value has two parts, so its update assigns the
/// complete object through one memory place, as integer complex does (L04).
#[test]
fn complex_addition_assignment_updates_one_complex_object() {
    let arena = Arena::default();
    let unit = parse_valid(COMPLEX_ADD_CONTROL, &arena);
    let body = items(function(&unit, 0));
    let declaration = object(&body[0]);
    let update = expression(&body[1]);
    assert_eq!(span_text(COMPLEX_ADD_CONTROL, update.span()), "z += 1.0");
    let CHirExpr::MemoryAssign {
        op, place, facts, ..
    } = update
    else {
        panic!("expected a complex memory update, got {update:?}");
    };
    assert_eq!(format!("{op:?}"), "Add");
    assert!(matches!(
        place.root(),
        PlaceRoot::Object { symbol } if *symbol == declaration.symbol
    ));
    assert!(place.projections().is_empty());
    assert_eq!(place.value_ty(), declaration.ty);
    assert_eq!(facts.ty(), declaration.ty);
}

#[test]
fn ordinary_string_literal_converts_to_a_compatible_char_pointer() {
    let arena = Arena::default();
    let unit = parse_valid(STRING_POINTER_CONTROL, &arena);
    let body = items(function(&unit, 0));
    assert_direct_scalar_node(
        &unit,
        object(&body[0]),
        expression(&body[1]),
        STRING_POINTER_CONTROL,
        "p = \"x\"",
        "Assign {",
        "Simple",
    );
}

#[test]
fn prefixed_string_literals_retain_prefix_and_element_pointer_type() {
    let arena = Arena::default();
    let unit = parse_valid(PREFIXED_STRING_CONTROLS, &arena);
    let body = items(function(&unit, 0));
    assert_eq!(body.len(), 11, "prefixed string control denominator");
    for (item, declaration, text, prefix) in [
        (&body[5], object(&body[0]), "c = u8\"a\"", "U8"),
        (&body[6], object(&body[1]), "w = L\"b\"", "L"),
        (&body[7], object(&body[2]), "s = u\"c\"", "U16"),
        (&body[8], object(&body[3]), "u32 = U\"d\"", "U"),
        (&body[9], object(&body[4]), "v = U\"e\"", "U"),
    ] {
        let expression = expression(item);
        assert_direct_scalar_node(
            &unit,
            declaration,
            expression,
            PREFIXED_STRING_CONTROLS,
            text,
            "Assign {",
            "Simple",
        );
        let debug = format!("{expression:?}");
        assert!(
            debug.contains(&format!("prefix: {prefix}")),
            "string prefix was not retained: {debug}",
        );
    }
}

#[test]
fn mismatched_prefixed_string_pointer_assignments_are_e2102() {
    for (source, target) in [
        (INVALID_WIDE_TO_CHAR, "p = L\"x\""),
        (INVALID_U16_TO_INT, "p = u\"x\""),
        (INVALID_U32_TO_U16, "p = U\"x\""),
        (INVALID_U8_TO_U32, "p = u8\"x\""),
    ] {
        assert_diagnostic(source, target, 0, "E2102", "invalid-update-operands");
    }
}

#[test]
fn string_pointer_conditional_and_comparison_joins_are_typed() {
    let arena = Arena::default();
    let unit = parse_valid(STRING_EXPRESSION_CONTROLS, &arena);
    let body = items(function(&unit, 0));
    assert_eq!(body.len(), 11, "string expression control denominator");
    let p = object(&body[0]);
    let q = object(&body[1]);
    let w = object(&body[2]);
    let x = object(&body[3]);
    for (item, declaration, text) in [
        (&body[4], p, "p = c ? \"a\" : 0"),
        (&body[5], p, "p = c ? 0 : \"b\""),
        (&body[6], q, "q = c ? \"c\" : p"),
        (&body[7], q, "q = c ? p : \"d\""),
        (&body[8], w, "w = c ? L\"e\" : 0"),
        (&body[9], x, "x = \"f\" != 0"),
    ] {
        assert_direct_scalar_node(
            &unit,
            declaration,
            expression(item),
            STRING_EXPRESSION_CONTROLS,
            text,
            "Assign {",
            "Simple",
        );
    }
    assert!(
        format!("{:?}", expression(&body[8])).contains("prefix: L"),
        "wide conditional arm lost its prefix",
    );
}

#[test]
fn pointer_qualifier_conditional_and_comparison_joins_are_order_independent() {
    let arena = Arena::default();
    let unit = parse_valid(POINTER_QUALIFIER_JOIN_CONTROLS, &arena);
    let body = items(function(&unit, 0));
    assert_eq!(body.len(), 9, "pointer qualifier join control denominator");
    let out = object(&body[0]);
    let x = object(&body[3]);
    for (item, declaration, text) in [
        (&body[4], out, "out = c ? a : b"),
        (&body[5], out, "out = c ? b : a"),
        (&body[6], x, "x = a == b"),
        (&body[7], x, "x = b == a"),
    ] {
        assert_direct_scalar_node(
            &unit,
            declaration,
            expression(item),
            POINTER_QUALIFIER_JOIN_CONTROLS,
            text,
            "Assign {",
            "Simple",
        );
    }
}

#[test]
fn invalid_update_operands_report_exact_e2102() {
    for (source, target) in [
        (INVALID_POINTER_MULTIPLY, "p *= 2"),
        (INVALID_POINTER_FLOAT_ADD, "p += 1.5"),
        (INVALID_FLOAT_SHIFT, "x <<= 1"),
        (INVALID_COMPLEX_UPDATE, "z %= 2"),
        (INVALID_POINTER_ASSIGNMENT, "p = q"),
        (INVALID_STRUCT_COMPOUND, "left += right"),
        (INVALID_INCOMPLETE_POINTER_UPDATE, "p++"),
    ] {
        assert_diagnostic(source, target, 0, "E2102", "invalid-update-operands");
    }
}

#[test]
fn failed_update_rolls_back_function_scope_symbols_and_types() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse(FAILED_UPDATE_ROLLBACK, &arena);
    let diagnostic = diagnostic.expect("failed update must diagnose");
    assert_eq!(diagnostic.code, "E2102");
    assert_eq!(diagnostic.message, "invalid-update-operands");
    assert_eq!(diagnostic.span, span(FAILED_UPDATE_ROLLBACK, "p *= 2", 0));

    assert_eq!(unit.declarations().len(), 2, "failed function leaked CHIR");
    let CHirDecl::Object(keep) = unit.declarations()[0] else {
        panic!("expected keep object");
    };
    let CHirDecl::Object(after) = unit.declarations()[1] else {
        panic!("expected after object");
    };
    assert_eq!(
        (keep.name.spelling.as_str(), keep.symbol.raw()),
        ("keep", 0)
    );
    assert_eq!(
        (after.name.spelling.as_str(), after.symbol.raw()),
        ("after", 1)
    );
    assert_eq!(keep.ty.raw(), 0, "first canonical int identity");
    assert_eq!(
        after.ty.raw(),
        1,
        "failed function types were not rolled back"
    );
    assert_eq!(
        unit.types().len(),
        2,
        "failed function leaked canonical types"
    );

    let symbols = unit.symbols();
    assert_eq!(symbols.scopes_in_source_order().len(), 1, "scope leak");
    assert_eq!(
        symbols
            .symbols_in_source_order()
            .iter()
            .map(|symbol| (symbol.id().raw(), symbol.spelling()))
            .collect::<Vec<_>>(),
        [(0, "keep"), (1, "after")],
    );
    let file_scope = symbols.file_scope();
    assert_eq!(
        symbols.resolve(file_scope, "keep").unwrap(),
        Some(keep.symbol)
    );
    assert_eq!(
        symbols.resolve(file_scope, "after").unwrap(),
        Some(after.symbol)
    );
    assert_eq!(symbols.resolve(file_scope, "bad").unwrap(), None);
    assert_eq!(symbols.resolve(file_scope, "p").unwrap(), None);
}

#[test]
fn valid_pointer_updates_produce_typed_chir() {
    for source in [
        DAY7_POINTER_ADD,
        DAY7_POINTER_UPDATE,
        DAY7_POINTER_PREFIX,
        DAY7_POINTER_SUBTRACT,
        GNU_FUNCTION_POINTER_UPDATE,
    ] {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        assert_eq!(unit.declarations().len(), 1);
    }
}

#[test]
fn aggregate_member_assignment_is_accepted() {
    let arena = Arena::default();
    assert!(!parse_valid(AGGREGATE_MEMBER_ASSIGN, &arena).is_empty());
}

/// The forms the boundary used to cover are executable now, and stay that way.
#[test]
fn scalar_subscript_and_member_lvalues_are_accepted() {
    let arena = Arena::default();
    for source in [DAY7_INDEX, DAY7_MEMBER, DAY7_PTR_MEMBER] {
        let unit = parse_valid(source, &arena);
        assert!(
            !unit.is_empty(),
            "a scalar projected lvalue must parse: {source:?}"
        );
    }
}

#[test]
fn dereference_assignment_uses_one_typed_memory_place() {
    let arena = Arena::default();
    let unit = parse_valid(DAY7_ALIAS, &arena);
    let body = items(function(&unit, 0));
    assert_eq!(body.len(), 5, "alias program body topology");

    let assignment = expression(&body[3]);
    let CHirExpr::MemoryAssign {
        place,
        target_span,
        value,
        facts,
        ..
    } = assignment
    else {
        panic!("expected indirect assignment, got {assignment:?}");
    };
    let PlaceRoot::PointerExpression {
        pointer,
        pointer_type,
    } = place.root()
    else {
        panic!("dereference must use a pointer-expression root");
    };
    let q = object(&body[2]);
    let CHirExpr::Ident {
        symbol,
        span: pointer_span,
        ..
    } = pointer
    else {
        panic!("memory-place root must retain the original q subnode");
    };
    assert_eq!(*symbol, q.symbol);
    assert_eq!(span_text(DAY7_ALIAS, *pointer_span), "q");
    assert_eq!(span_text(DAY7_ALIAS, *target_span), "*q");
    assert_eq!(pointer_span.byte_start, target_span.byte_start + 1);
    assert_eq!(pointer_span.byte_end, target_span.byte_end);
    let sources = place.source_expressions().collect::<Vec<_>>();
    assert_eq!(sources.len(), 1, "pointer root must be evaluated once");
    assert!(std::ptr::eq(sources[0], *pointer));
    assert_eq!(pointer_type.pointee(), place.value_ty());
    assert!(place.projections().is_empty());
    assert!(place.is_modifiable());
    assert!(place.is_addressable());
    assert!(!place.is_bit_field());
    assert_eq!(facts.ty(), place.value_ty());
    assert!(matches!(value, CHirExpr::Lit(IntLit { value: 9, .. }, _)));

    let CHirBlockItem::Statement(CHirStmt::Return(Some(returned), _)) = &body[4] else {
        panic!("expected return statement");
    };
    assert!(matches!(returned, CHirExpr::Unary(UnaryOp::Deref, _, _)));
}

#[test]
fn volatile_scalar_update_is_an_accepted_day7_contract() {
    let arena = Arena::default();
    let unit = parse_valid(DAY7_VOLATILE, &arena);
    let body = items(function(&unit, 0));
    assert_eq!(body.len(), 3, "volatile body topology");

    let declaration = object(&body[0]);
    let canonical = unit
        .types()
        .get(declaration.ty)
        .expect("volatile declaration type");
    assert!(canonical.qualifiers.is_volatile);
    assert!(!canonical.qualifiers.is_atomic);
    assert_eq!(
        span_text(DAY7_VOLATILE, expression(&body[1]).span()),
        "x = 1"
    );
}

#[test]
fn atomic_scalar_assignment_is_one_memory_place_and_floating_updates_stay_refused() {
    // L11a: an `_Atomic` scalar target is read and written through one
    // memory place, which every consumer lowers to atomic accesses.
    let arena = Arena::default();
    let unit = parse_valid(DAY7_ATOMIC, &arena);
    let body = items(function(&unit, 0));
    let CHirExpr::MemoryAssign { place, .. } = expression(&body[1]) else {
        panic!("an atomic target is a memory place");
    };
    assert!(place.access_qualifiers().is_atomic);
    assert_eq!(span_text(DAY7_ATOMIC, expression(&body[1]).span()), "x = 1");
    assert_diagnostic(
        "int f(void) { _Atomic double x = 0; x += 1; return 0; }\n",
        "x",
        1,
        "E2702",
        "atomic-update-unsupported",
    );
}
