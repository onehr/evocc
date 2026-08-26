use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use evocc_chir::{
    CHirDecl, CHirExpr, InitAction, InitPlan, ObjectDecl, ObjectInitializer, StringPrefix,
    SubobjectStep, WrittenInitializer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{ArrayBound, IntRank, TagDefinition, TagState, TypeId, TypeKind};

const FILE_RAW: u32 = 83;

struct SourceCase {
    id: &'static str,
    source: &'static str,
}

const SCALAR_UNBRACED: &str = "int scalar = (1 + 3);\n";
const SCALAR_BRACED: &str = "int scalar = { 3 };\n";
const SCALAR_NESTED: &str = "int scalar = { { 3 } };\n";
const SCALAR_EXCESS: &str = "int scalar = { 1, 2 };\n";
const ARRAY_PARTIAL: &str = "int values[3] = { 1, 2 };\n";
const ARRAY_INFERRED: &str = "int values[] = { 1, 2, 3 };\n";
const ARRAY_NESTED: &str = "int values[2][2] = { { 1 }, 2, 3 };\n";
const ARRAY_INFERRED_OUTER: &str = "int values[][2] = { 1, 2, 3 };\n";
const ARRAY_BRACE_ELISION: &str = "int values[2][2] = { 1, 2, 3 };\n";
const NESTED_ZERO_ORDER: &str = concat!(
    "struct Packet { int lanes[2]; int tail; }; ",
    "struct Packet packet = { { 1 }, 2 };\n",
);
const ARRAY_NESTED_EXCESS: &str = "int values[1][1] = { { 1, 2 } };\n";
const STRUCT_PARTIAL: &str = concat!(
    "struct Pair { unsigned : 3; int first; int second; }; ",
    "struct Pair pair = { 1 };\n",
);
const STRUCT_BRACE_ELISION: &str = concat!(
    "struct Packet { struct { int first; int second; }; int tail; }; ",
    "struct Packet packet = { 1, 2, 3 };\n",
);
const UNION_FIRST: &str = "union Word { int lanes[2]; long wide; }; union Word word = { 7, 8 };\n";
const UNION_EXCESS: &str = "union Word { int narrow; long wide; }; union Word word = { 7, 8 };\n";
const CHAR_FIXED: &str = "char text[4] = \"h\" \"i\";\n";
const CHAR_INFERRED: &str = "char text[] = \"hi\";\n";
const CHAR_EXACT_NO_TERMINATOR: &str = "char text[3] = \"abc\";\n";
const CHAR_ESCAPE: &str = concat!(
    "char text[4] = \"a\\0b\"; ",
    "char byte[1] = \"\\xFF\"; char inferred[] = \"\\xFF\";\n",
);
const CHAR_TOO_LONG: &str = "char text[2] = \"abc\";\n";
const TRAILING_COMMA: &str = "int values[2] = { 1, 2, };\n";
const FAILED_INITIALIZER_ROLLBACK: &str = "int keep; int bad[1] = { 1, 2 }; long after;\n";
const INVALID_INITIALIZER_TYPE: &str = "int *pointer = 1.5;\n";
const AGGREGATE_COPY_BOUNDARY: &str = concat!(
    "struct S { int member; }; struct S other; ",
    "struct S value = other;\n",
);
const DESIGNATED_BOUNDARY: &str = "struct S { int a; int b; }; struct S value = { .b = 2 };\n";
const COMPOUND_LITERAL_BOUNDARY: &str =
    "struct S { int a; }; int f(void) { return ((struct S){ 1 }).a; }\n";
const WIDE_STRING_BOUNDARY: &str = "char text[2] = L\"x\";\n";
const VLA_INITIALIZER_BOUNDARY: &str = "int f(int n) { int values[n] = { 1 }; return 0; }\n";
const FLEXIBLE_MEMBER_BOUNDARY: &str = concat!(
    "struct Packet { int count; int values[]; }; ",
    "struct Packet packet = { 1, { 2 } };\n",
);
const GNU_EMPTY_BRACES: &str = "int scalar = {}; int values[2] = {};\n";

const SOURCES: &[SourceCase] = &[
    SourceCase {
        id: "scalar.unbraced",
        source: SCALAR_UNBRACED,
    },
    SourceCase {
        id: "scalar.braced",
        source: SCALAR_BRACED,
    },
    SourceCase {
        id: "scalar.nested-braces",
        source: SCALAR_NESTED,
    },
    SourceCase {
        id: "scalar.excess",
        source: SCALAR_EXCESS,
    },
    SourceCase {
        id: "array.partial-zero-fill",
        source: ARRAY_PARTIAL,
    },
    SourceCase {
        id: "array.inferred-bound",
        source: ARRAY_INFERRED,
    },
    SourceCase {
        id: "array.multidimensional-nested",
        source: ARRAY_NESTED,
    },
    SourceCase {
        id: "array.multidimensional-inferred-outer",
        source: ARRAY_INFERRED_OUTER,
    },
    SourceCase {
        id: "array.multidimensional-brace-elision",
        source: ARRAY_BRACE_ELISION,
    },
    SourceCase {
        id: "aggregate.nested-zero-order",
        source: NESTED_ZERO_ORDER,
    },
    SourceCase {
        id: "array.nested-excess",
        source: ARRAY_NESTED_EXCESS,
    },
    SourceCase {
        id: "struct.partial-zero-fill",
        source: STRUCT_PARTIAL,
    },
    SourceCase {
        id: "struct.aggregate-brace-elision",
        source: STRUCT_BRACE_ELISION,
    },
    SourceCase {
        id: "union.first-member",
        source: UNION_FIRST,
    },
    SourceCase {
        id: "union.excess",
        source: UNION_EXCESS,
    },
    SourceCase {
        id: "string.fixed-char-array",
        source: CHAR_FIXED,
    },
    SourceCase {
        id: "string.inferred-char-array",
        source: CHAR_INFERRED,
    },
    SourceCase {
        id: "string.exact-array-without-terminator",
        source: CHAR_EXACT_NO_TERMINATOR,
    },
    SourceCase {
        id: "string.escape-unit-count",
        source: CHAR_ESCAPE,
    },
    SourceCase {
        id: "string.too-long",
        source: CHAR_TOO_LONG,
    },
    SourceCase {
        id: "list.trailing-comma",
        source: TRAILING_COMMA,
    },
    SourceCase {
        id: "transaction.failed-initializer-rollback",
        source: FAILED_INITIALIZER_ROLLBACK,
    },
    SourceCase {
        id: "diagnostic.invalid-initializer-type",
        source: INVALID_INITIALIZER_TYPE,
    },
    SourceCase {
        id: "boundary.day7-aggregate-copy",
        source: AGGREGATE_COPY_BOUNDARY,
    },
    SourceCase {
        id: "boundary.day9-designated",
        source: DESIGNATED_BOUNDARY,
    },
    SourceCase {
        id: "boundary.day9-compound-literal",
        source: COMPOUND_LITERAL_BOUNDARY,
    },
    SourceCase {
        id: "boundary.wide-string",
        source: WIDE_STRING_BOUNDARY,
    },
    SourceCase {
        id: "boundary.day9-vla-initializer",
        source: VLA_INITIALIZER_BOUNDARY,
    },
    SourceCase {
        id: "boundary.flexible-array-member",
        source: FLEXIBLE_MEMBER_BOUNDARY,
    },
    SourceCase {
        id: "gnu11.empty-braces-zero-fill",
        source: GNU_EMPTY_BRACES,
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
        "valid initializer source was rejected",
    );
    unit
}

fn object<'arena>(
    unit: &evocc_chir::CHirUnit<'arena>,
    spelling: &str,
) -> &'arena ObjectDecl<'arena> {
    unit.declarations()
        .iter()
        .copied()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == spelling => Some(object),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing object {spelling:?}"))
}

fn initializer<'unit, 'arena>(
    object: &'unit ObjectDecl<'arena>,
) -> &'unit ObjectInitializer<'arena> {
    object.initializer.as_ref().expect("object initializer")
}

fn assert_joined_plan<'unit, 'arena>(
    unit: &'unit evocc_chir::CHirUnit<'arena>,
    object: &'unit ObjectDecl<'arena>,
) -> &'unit InitPlan<'arena> {
    let symbol = unit.symbols().symbol(object.symbol).unwrap();
    assert_eq!(symbol.ty(), object.ty, "SymbolId/TypeId join drifted");
    let plan = initializer(object).plan();
    assert_eq!(plan.target_type(), object.ty);
    plan.validate(unit.types()).unwrap();
    plan
}

fn expression<'arena>(written: &WrittenInitializer<'arena>) -> &'arena CHirExpr<'arena> {
    match written {
        WrittenInitializer::Expression(expression) => expression,
        WrittenInitializer::List { .. } => panic!("expected written expression initializer"),
    }
}

fn list<'unit, 'arena>(
    written: &'unit WrittenInitializer<'arena>,
) -> &'unit [WrittenInitializer<'arena>] {
    match written {
        WrittenInitializer::List { elements, .. } => elements,
        WrittenInitializer::Expression(_) => panic!("expected written brace-list initializer"),
    }
}

fn array_type(unit: &evocc_chir::CHirUnit<'_>, ty: TypeId) -> (TypeId, ArrayBound) {
    match &unit.types().get(ty).expect("known type").kind {
        TypeKind::Array { element, bound } => (*element, bound.clone()),
        other => panic!("expected array type, got {other:?}"),
    }
}

fn assert_int(unit: &evocc_chir::CHirUnit<'_>, ty: TypeId, rank: IntRank) {
    assert!(matches!(
        unit.types().get(ty).map(|item| &item.kind),
        Some(TypeKind::Int(actual)) if *actual == rank
    ));
}

fn assert_store<'arena>(
    source: &str,
    action: &'arena InitAction<'arena>,
    expected_path: &[SubobjectStep],
    destination: TypeId,
    value_text: &str,
) -> &'arena CHirExpr<'arena> {
    let InitAction::StoreScalar {
        path,
        destination_type,
        value,
        span: action_span,
    } = action
    else {
        panic!("expected scalar store, got {action:?}");
    };
    assert_eq!(path.steps(), expected_path);
    assert_eq!(*destination_type, destination);
    assert_eq!(span_text(source, value.span()), value_text);
    assert_eq!(*action_span, value.span());
    value
}

fn assert_zero(action: &InitAction<'_>, expected_path: &[SubobjectStep], destination: TypeId) {
    let InitAction::ZeroScalar {
        path,
        destination_type,
    } = action
    else {
        panic!("expected scalar zero-fill, got {action:?}");
    };
    assert_eq!(path.steps(), expected_path);
    assert_eq!(*destination_type, destination);
    assert_eq!(action.span(), None, "synthetic zero-fill acquired a span");
}

fn assert_string(
    source: &str,
    action: &InitAction<'_>,
    destination: TypeId,
    literal_text: &str,
    semantic_value: &str,
) {
    let InitAction::StoreString {
        path,
        destination_type,
        literal,
        span: action_span,
    } = action
    else {
        panic!("expected string store, got {action:?}");
    };
    assert!(path.is_root());
    assert_eq!(*destination_type, destination);
    assert_eq!(literal.prefix, StringPrefix::None);
    assert_eq!(literal.value, semantic_value);
    assert_eq!(*action_span, span(source, literal_text, 0));
}

fn assert_diagnostic(
    source: &str,
    target: &str,
    occurrence: usize,
    code: &str,
    message: &str,
) -> evocc_chir::CHirUnit<'static> {
    let arena = Box::leak(Box::new(Arena::default()));
    let (unit, diagnostic) = parse(source, arena);
    let diagnostic = diagnostic.expect("invalid initializer must return one diagnostic");
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.message, message);
    assert_eq!(diagnostic.span, span(source, target, occurrence));
    unit
}

#[test]
fn source_topology_is_frozen_and_byte_unique() {
    assert_eq!(SOURCES.len(), 30, "S4 source denominator changed");
    let ids = SOURCES.iter().map(|case| case.id).collect::<HashSet<_>>();
    let bytes = SOURCES
        .iter()
        .map(|case| case.source.as_bytes())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), SOURCES.len(), "duplicate S4 source id");
    assert_eq!(bytes.len(), SOURCES.len(), "duplicate S4 source bytes");
    assert!(SOURCES.iter().all(|case| case.source.ends_with('\n')));
}

#[test]
fn scalar_initializers_preserve_every_brace_and_share_the_plan_expression() {
    for (source, outer_text, inner_text, depth) in [
        (SCALAR_UNBRACED, "(1 + 3)", "(1 + 3)", 0),
        (SCALAR_BRACED, "{ 3 }", "3", 1),
        (SCALAR_NESTED, "{ { 3 } }", "3", 2),
    ] {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        let object = object(&unit, "scalar");
        assert_int(&unit, object.ty, IntRank::Int);
        let initializer = initializer(object);
        assert_eq!(initializer.written().span(), span(source, outer_text, 0));

        let written_expression = match depth {
            0 => expression(initializer.written()),
            1 => {
                let outer = list(initializer.written());
                assert_eq!(outer.len(), 1);
                expression(&outer[0])
            }
            2 => {
                let outer = list(initializer.written());
                assert_eq!(outer.len(), 1);
                assert_eq!(outer[0].span(), span(source, "{ 3 }", 0));
                let inner = list(&outer[0]);
                assert_eq!(inner.len(), 1);
                expression(&inner[0])
            }
            _ => unreachable!(),
        };
        assert_eq!(written_expression.span(), span(source, inner_text, 0));

        let plan = assert_joined_plan(&unit, object);
        assert_eq!(plan.actions().len(), 1);
        let planned_expression =
            assert_store(source, &plan.actions()[0], &[], object.ty, inner_text);
        assert!(
            std::ptr::eq(written_expression, planned_expression),
            "written tree and normalized plan cloned or diverged",
        );
    }
}

#[test]
fn arrays_infer_bounds_and_emit_depth_first_store_then_zero_paths() {
    let arena = Arena::default();
    let unit = parse_valid(ARRAY_PARTIAL, &arena);
    let values = object(&unit, "values");
    let (int, bound) = array_type(&unit, values.ty);
    assert_eq!(bound, ArrayBound::Fixed(3));
    assert_int(&unit, int, IntRank::Int);
    let plan = assert_joined_plan(&unit, values);
    assert_eq!(plan.actions().len(), 3);
    assert_store(
        ARRAY_PARTIAL,
        &plan.actions()[0],
        &[SubobjectStep::Index { index: 0 }],
        int,
        "1",
    );
    assert_store(
        ARRAY_PARTIAL,
        &plan.actions()[1],
        &[SubobjectStep::Index { index: 1 }],
        int,
        "2",
    );
    assert_zero(
        &plan.actions()[2],
        &[SubobjectStep::Index { index: 2 }],
        int,
    );

    let arena = Arena::default();
    let unit = parse_valid(ARRAY_INFERRED, &arena);
    let values = object(&unit, "values");
    let (int, bound) = array_type(&unit, values.ty);
    assert_eq!(bound, ArrayBound::Fixed(3));
    assert_eq!(
        unit.symbols().symbol(values.symbol).unwrap().ty(),
        values.ty
    );
    let plan = assert_joined_plan(&unit, values);
    assert_eq!(plan.actions().len(), 3);
    for (index, value) in ["1", "2", "3"].iter().enumerate() {
        assert_store(
            ARRAY_INFERRED,
            &plan.actions()[index],
            &[SubobjectStep::Index {
                index: index as u64,
            }],
            int,
            value,
        );
    }
}

fn assert_two_by_two(
    source: &str,
    unit: &evocc_chir::CHirUnit<'_>,
    expected_outer_bound: ArrayBound,
    expected_values: [Option<&str>; 4],
) {
    let object = object(unit, "values");
    let (inner_array, outer_bound) = array_type(unit, object.ty);
    let (int, inner_bound) = array_type(unit, inner_array);
    assert_eq!(outer_bound, expected_outer_bound);
    assert_eq!(inner_bound, ArrayBound::Fixed(2));
    let plan = assert_joined_plan(unit, object);
    assert_eq!(plan.actions().len(), 4);
    for (action_index, value) in expected_values.into_iter().enumerate() {
        let outer = (action_index / 2) as u64;
        let inner = (action_index % 2) as u64;
        let path = [
            SubobjectStep::Index { index: outer },
            SubobjectStep::Index { index: inner },
        ];
        if let Some(value) = value {
            assert_store(source, &plan.actions()[action_index], &path, int, value);
        } else {
            assert_zero(&plan.actions()[action_index], &path, int);
        }
    }
}

#[test]
fn multidimensional_nested_elided_and_inferred_forms_have_one_canonical_plan() {
    for (source, values) in [
        (ARRAY_NESTED, [Some("1"), None, Some("2"), Some("3")]),
        (ARRAY_BRACE_ELISION, [Some("1"), Some("2"), Some("3"), None]),
        (
            ARRAY_INFERRED_OUTER,
            [Some("1"), Some("2"), Some("3"), None],
        ),
    ] {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        assert_two_by_two(source, &unit, ArrayBound::Fixed(2), values);
    }
}

fn record_field_types(unit: &evocc_chir::CHirUnit<'_>, ty: TypeId) -> Vec<TypeId> {
    let tag = match unit.types().get(ty).map(|item| &item.kind) {
        Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
        other => panic!("expected record type, got {other:?}"),
    };
    match &unit.types().tag(tag).expect("known tag").state {
        TagState::Complete(TagDefinition::Struct(fields) | TagDefinition::Union(fields)) => {
            fields.iter().map(|field| field.ty).collect()
        }
        other => panic!("expected complete record, got {other:?}"),
    }
}

#[test]
fn struct_union_and_nested_zero_fill_use_declared_field_order() {
    let arena = Arena::default();
    let unit = parse_valid(STRUCT_PARTIAL, &arena);
    let pair = object(&unit, "pair");
    let fields = record_field_types(&unit, pair.ty);
    assert_eq!(
        fields.len(),
        3,
        "unnamed bit-field must retain its physical slot"
    );
    let plan = assert_joined_plan(&unit, pair);
    assert_eq!(plan.actions().len(), 2);
    assert_store(
        STRUCT_PARTIAL,
        &plan.actions()[0],
        &[SubobjectStep::Field { field_index: 1 }],
        fields[1],
        "1",
    );
    assert_zero(
        &plan.actions()[1],
        &[SubobjectStep::Field { field_index: 2 }],
        fields[2],
    );

    let arena = Arena::default();
    let unit = parse_valid(UNION_FIRST, &arena);
    let word = object(&unit, "word");
    let fields = record_field_types(&unit, word.ty);
    let (int, bound) = array_type(&unit, fields[0]);
    assert_eq!(bound, ArrayBound::Fixed(2));
    let plan = assert_joined_plan(&unit, word);
    assert_eq!(
        plan.actions().len(),
        2,
        "inactive union fields must not be zeroed"
    );
    assert_store(
        UNION_FIRST,
        &plan.actions()[0],
        &[
            SubobjectStep::Field { field_index: 0 },
            SubobjectStep::Index { index: 0 },
        ],
        int,
        "7",
    );
    assert_store(
        UNION_FIRST,
        &plan.actions()[1],
        &[
            SubobjectStep::Field { field_index: 0 },
            SubobjectStep::Index { index: 1 },
        ],
        int,
        "8",
    );

    let arena = Arena::default();
    let unit = parse_valid(NESTED_ZERO_ORDER, &arena);
    let packet = object(&unit, "packet");
    let fields = record_field_types(&unit, packet.ty);
    let (int, bound) = array_type(&unit, fields[0]);
    assert_eq!(bound, ArrayBound::Fixed(2));
    let plan = assert_joined_plan(&unit, packet);
    assert_eq!(plan.actions().len(), 3);
    assert_store(
        NESTED_ZERO_ORDER,
        &plan.actions()[0],
        &[
            SubobjectStep::Field { field_index: 0 },
            SubobjectStep::Index { index: 0 },
        ],
        int,
        "1",
    );
    assert_zero(
        &plan.actions()[1],
        &[
            SubobjectStep::Field { field_index: 0 },
            SubobjectStep::Index { index: 1 },
        ],
        int,
    );
    assert_store(
        NESTED_ZERO_ORDER,
        &plan.actions()[2],
        &[SubobjectStep::Field { field_index: 1 }],
        fields[1],
        "2",
    );
}

#[test]
fn record_brace_elision_descends_before_advancing_to_the_next_field() {
    let arena = Arena::default();
    let unit = parse_valid(STRUCT_BRACE_ELISION, &arena);
    let object = object(&unit, "packet");
    let fields = record_field_types(&unit, object.ty);
    let nested_fields = record_field_types(&unit, fields[0]);
    assert_eq!((fields.len(), nested_fields.len()), (2, 2));
    let plan = assert_joined_plan(&unit, object);
    assert_eq!(plan.actions().len(), 3);
    for (index, (path, destination, value)) in [
        (
            vec![
                SubobjectStep::Field { field_index: 0 },
                SubobjectStep::Field { field_index: 0 },
            ],
            nested_fields[0],
            "1",
        ),
        (
            vec![
                SubobjectStep::Field { field_index: 0 },
                SubobjectStep::Field { field_index: 1 },
            ],
            nested_fields[1],
            "2",
        ),
        (
            vec![SubobjectStep::Field { field_index: 1 }],
            fields[1],
            "3",
        ),
    ]
    .iter()
    .enumerate()
    {
        assert_store(
            STRUCT_BRACE_ELISION,
            &plan.actions()[index],
            path,
            *destination,
            value,
        );
    }
}

#[test]
fn character_array_strings_count_units_infer_bounds_and_keep_exact_source_spans() {
    for (source, expected_bound, source_literal, semantic_value) in [
        (CHAR_FIXED, 4, "\"h\" \"i\"", "hi"),
        (CHAR_INFERRED, 3, "\"hi\"", "hi"),
        (CHAR_EXACT_NO_TERMINATOR, 3, "\"abc\"", "abc"),
        (CHAR_ESCAPE, 4, "\"a\\0b\"", "a\\0b"),
    ] {
        let arena = Arena::default();
        let unit = parse_valid(source, &arena);
        let object = object(&unit, "text");
        let (character, bound) = array_type(&unit, object.ty);
        assert_eq!(bound, ArrayBound::Fixed(expected_bound));
        assert_int(&unit, character, IntRank::Char);
        let initializer = initializer(object);
        let written_expression = expression(initializer.written());
        assert_eq!(written_expression.span(), span(source, source_literal, 0));
        let CHirExpr::StringLit(written_literal, _) = written_expression else {
            panic!("written initializer lost its string expression");
        };
        assert_eq!(written_literal.value, semantic_value);
        assert_eq!(written_literal.prefix, StringPrefix::None);

        let plan = assert_joined_plan(&unit, object);
        assert_eq!(plan.actions().len(), 1);
        assert_string(
            source,
            &plan.actions()[0],
            object.ty,
            source_literal,
            semantic_value,
        );
    }

    let arena = Arena::default();
    let unit = parse_valid(CHAR_ESCAPE, &arena);
    for (name, bound, literal_occurrence) in [("byte", 1, 0), ("inferred", 2, 1)] {
        let object = object(&unit, name);
        let (character, actual_bound) = array_type(&unit, object.ty);
        assert_eq!(actual_bound, ArrayBound::Fixed(bound));
        assert_int(&unit, character, IntRank::Char);
        let initializer = initializer(object);
        let written_expression = expression(initializer.written());
        assert_eq!(
            written_expression.span(),
            span(CHAR_ESCAPE, "\"\\xFF\"", literal_occurrence),
        );
        let CHirExpr::StringLit(literal, _) = written_expression else {
            panic!("hex escape did not remain a string expression");
        };
        assert_eq!(literal.value, "\\xFF");
        let plan = assert_joined_plan(&unit, object);
        assert_eq!(plan.actions().len(), 1);
        let InitAction::StoreString {
            literal,
            span: action_span,
            ..
        } = &plan.actions()[0]
        else {
            panic!("hex string did not normalize to one StoreString");
        };
        assert_eq!(literal.value, "\\xFF");
        assert_eq!(
            *action_span,
            span(CHAR_ESCAPE, "\"\\xFF\"", literal_occurrence),
        );
    }
}

#[test]
fn gnu11_empty_braces_recursively_zero_fill_without_fabricated_written_elements() {
    let arena = Arena::default();
    let unit = parse_valid(GNU_EMPTY_BRACES, &arena);

    let scalar = object(&unit, "scalar");
    assert_int(&unit, scalar.ty, IntRank::Int);
    let scalar_initializer = initializer(scalar);
    assert_eq!(
        scalar_initializer.written().span(),
        span(GNU_EMPTY_BRACES, "{}", 0)
    );
    assert!(list(scalar_initializer.written()).is_empty());
    let scalar_plan = assert_joined_plan(&unit, scalar);
    assert_eq!(scalar_plan.actions().len(), 1);
    assert_zero(&scalar_plan.actions()[0], &[], scalar.ty);

    let values = object(&unit, "values");
    let (int, bound) = array_type(&unit, values.ty);
    assert_eq!(bound, ArrayBound::Fixed(2));
    let values_initializer = initializer(values);
    assert_eq!(
        values_initializer.written().span(),
        span(GNU_EMPTY_BRACES, "{}", 1)
    );
    assert!(list(values_initializer.written()).is_empty());
    let values_plan = assert_joined_plan(&unit, values);
    assert_eq!(values_plan.actions().len(), 2);
    assert_zero(
        &values_plan.actions()[0],
        &[SubobjectStep::Index { index: 0 }],
        int,
    );
    assert_zero(
        &values_plan.actions()[1],
        &[SubobjectStep::Index { index: 1 }],
        int,
    );
}

#[test]
fn trailing_comma_is_syntax_only_and_does_not_create_an_initializer_element() {
    let arena = Arena::default();
    let unit = parse_valid(TRAILING_COMMA, &arena);
    let object = object(&unit, "values");
    let initializer = initializer(object);
    let elements = list(initializer.written());
    assert_eq!(
        initializer.written().span(),
        span(TRAILING_COMMA, "{ 1, 2, }", 0)
    );
    assert_eq!(elements.len(), 2);
    assert_eq!(
        expression(&elements[0]).span(),
        span(TRAILING_COMMA, "1", 0)
    );
    assert_eq!(
        expression(&elements[1]).span(),
        span(TRAILING_COMMA, "2", 1)
    );
    assert_eq!(assert_joined_plan(&unit, object).actions().len(), 2);
}

#[test]
fn excess_and_string_overflow_report_exact_semantic_diagnostics() {
    for (source, target, occurrence) in [
        (SCALAR_EXCESS, "2", 0),
        (ARRAY_NESTED_EXCESS, "2", 0),
        (UNION_EXCESS, "8", 0),
        (FLEXIBLE_MEMBER_BOUNDARY, "{ 2 }", 0),
    ] {
        assert_diagnostic(source, target, occurrence, "E2201", "excess-initializer");
    }
    assert_diagnostic(
        CHAR_TOO_LONG,
        "\"abc\"",
        0,
        "E2204",
        "string-initializer-too-long: 3 > 2",
    );
}

#[test]
fn failed_initializer_rolls_back_its_declaration_without_ghost_symbols() {
    let unit = assert_diagnostic(
        FAILED_INITIALIZER_ROLLBACK,
        "2",
        0,
        "E2201",
        "excess-initializer",
    );
    let declarations = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) => Some(object.name.spelling.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations, ["keep", "after"]);
    assert_eq!(
        unit.symbols()
            .symbols_in_source_order()
            .iter()
            .map(|symbol| symbol.spelling())
            .collect::<Vec<_>>(),
        ["keep", "after"],
    );
    assert_eq!(
        unit.symbols().resolve(unit.symbols().file_scope(), "bad"),
        Ok(None),
    );

    let control_arena = Arena::default();
    let control = parse_valid("int keep; long after;\n", &control_arena);
    for spelling in ["keep", "after"] {
        let recovered = object(&unit, spelling);
        let uninterrupted = object(&control, spelling);
        assert_eq!(
            (recovered.symbol, recovered.ty),
            (uninterrupted.symbol, uninterrupted.ty),
            "failed initializer changed later SymbolId/TypeId allocation for {spelling}",
        );
    }
    assert_eq!(
        unit.types().len(),
        control.types().len(),
        "failed initializer leaked a canonical type",
    );
}

#[test]
fn unsupported_initializer_forms_keep_their_stable_boundaries() {
    assert_diagnostic(
        INVALID_INITIALIZER_TYPE,
        "1.5",
        0,
        "E2202",
        "invalid-initializer-type",
    );
    assert_diagnostic(
        AGGREGATE_COPY_BOUNDARY,
        "other",
        1,
        "E2705",
        "not-covered-day7: aggregate-copy-initializer",
    );
    assert_diagnostic(
        WIDE_STRING_BOUNDARY,
        "L\"x\"",
        0,
        "E2205",
        "not-covered-day5: wide-string-initializer",
    );
    assert_diagnostic(
        VLA_INITIALIZER_BOUNDARY,
        "{ 1 }",
        0,
        "E2203",
        "variable-length-object-cannot-be-initialized",
    );
}

#[test]
fn designated_initializers_are_covered_forms() {
    let designated_arena = Arena::default();
    parse_valid(DESIGNATED_BOUNDARY, &designated_arena);
}

#[test]
fn compound_literals_are_covered_forms() {
    let compound_arena = Arena::default();
    parse_valid(COMPOUND_LITERAL_BOUNDARY, &compound_arena);
}
