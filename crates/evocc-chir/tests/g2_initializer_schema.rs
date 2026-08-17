use evocc_chir::{
    CHirExpr, InitAction, InitPlan, InitPlanError, IntBase, IntLit, IntSuffix, StringLit,
    StringPrefix, SubobjectPath, SubobjectStep, WrittenInitializer,
};
use evocc_common::{FileId, SourceSpan};
use evocc_types::{
    ArrayBound, CanonicalType, IntRank, Qualifiers, RecordField, TagDefinition, TagKind,
    TypeInterner, TypeKind,
};

fn span(start: u32) -> SourceSpan {
    SourceSpan::new(FileId::new(42), start, start + 1, 1, start + 1)
}

fn plain(kind: TypeKind) -> CanonicalType {
    CanonicalType {
        kind,
        qualifiers: Qualifiers::default(),
    }
}

fn literal(value: u64, at: u32) -> CHirExpr<'static> {
    CHirExpr::Lit(
        IntLit {
            value,
            suffix: IntSuffix::None,
            base: IntBase::Dec,
        },
        span(at),
    )
}

#[test]
fn written_initializer_preserves_expression_and_brace_structure() {
    let first = literal(1, 1);
    let second = literal(2, 2);
    let nested = WrittenInitializer::List {
        elements: vec![WrittenInitializer::Expression(&second)],
        span: span(4),
    };
    let written = WrittenInitializer::List {
        elements: vec![WrittenInitializer::Expression(&first), nested],
        span: span(5),
    };

    assert_eq!(written.span(), span(5));
    let elements = written.elements().expect("outer brace list");
    assert_eq!(elements.len(), 2);
    assert!(matches!(elements[0], WrittenInitializer::Expression(_)));
    assert_eq!(elements[1].span(), span(4));
}

#[test]
fn normalized_plan_resolves_paths_and_preserves_action_order() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let char_array = types.intern(plain(TypeKind::Array {
        element: char_ty,
        bound: ArrayBound::Fixed(4),
    }));
    let tag = types.declare_tag(TagKind::Struct, Some("Pair".to_owned()));
    let pair = types.intern(plain(TypeKind::Struct(tag)));
    types
        .complete_tag(
            tag,
            TagDefinition::Struct(vec![
                RecordField {
                    name: Some("count".to_owned()),
                    ty: int,
                    bit_field: None,
                },
                RecordField {
                    name: Some("name".to_owned()),
                    ty: char_array,
                    bit_field: None,
                },
            ]),
        )
        .unwrap();

    let value = literal(7, 1);
    let count = SubobjectPath::root().with_field(0);
    let name = SubobjectPath::root().with_field(1);
    let plan = InitPlan::new(
        pair,
        vec![
            InitAction::store_scalar(count.clone(), int, &value, span(1)),
            InitAction::store_string(
                name.clone(),
                char_array,
                StringLit {
                    value: "abc".to_owned(),
                    prefix: StringPrefix::None,
                },
                span(2),
            ),
        ],
    );

    plan.validate(&types).unwrap();
    assert_eq!(plan.actions()[0].path(), &count);
    assert_eq!(plan.actions()[1].path(), &name);
    assert_eq!(name.steps(), &[SubobjectStep::Field { field_index: 1 }]);
}

#[test]
fn plan_validation_rejects_overlap_type_mismatch_and_bad_paths() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let array = types.intern(plain(TypeKind::Array {
        element: int,
        bound: ArrayBound::Fixed(2),
    }));
    let value = literal(1, 1);

    let duplicate = SubobjectPath::root().with_index(0);
    let plan = InitPlan::new(
        array,
        vec![
            InitAction::store_scalar(duplicate.clone(), int, &value, span(1)),
            InitAction::zero_scalar(duplicate.clone(), int),
        ],
    );
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::OverlappingActions {
            first_action: 0,
            second_action: 1,
            first_path: duplicate.clone(),
            second_path: duplicate,
        })
    );

    let out_of_bounds = SubobjectPath::root().with_index(2);
    let plan = InitPlan::new(
        array,
        vec![InitAction::zero_scalar(out_of_bounds.clone(), int)],
    );
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::ArrayIndexOutOfBounds {
            action: 0,
            path: out_of_bounds,
            index: 2,
            bound: 2,
        })
    );

    let plan = InitPlan::new(
        array,
        vec![InitAction::store_scalar(
            SubobjectPath::root().with_index(0),
            array,
            &value,
            span(1),
        )],
    );
    assert!(matches!(
        plan.validate(&types),
        Err(InitPlanError::DestinationTypeMismatch { action: 0, .. })
    ));
}

#[test]
fn string_actions_require_character_arrays() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let int_array = types.intern(plain(TypeKind::Array {
        element: int,
        bound: ArrayBound::Fixed(4),
    }));
    let plan = InitPlan::new(
        int_array,
        vec![InitAction::store_string(
            SubobjectPath::root(),
            int_array,
            StringLit {
                value: "abc".to_owned(),
                prefix: StringPrefix::None,
            },
            span(1),
        )],
    );

    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::StringDestinationIsNotCharacterArray {
            action: 0,
            path: SubobjectPath::root(),
            destination: int_array,
        })
    );
}

#[test]
fn validation_rejects_prefix_overlap_wrong_step_and_oversized_string() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let short_string = types.intern(plain(TypeKind::Array {
        element: char_ty,
        bound: ArrayBound::Fixed(2),
    }));
    let root = SubobjectPath::root();
    let element = root.clone().with_index(0);
    let plan = InitPlan::new(
        short_string,
        vec![
            InitAction::store_string(
                root.clone(),
                short_string,
                StringLit {
                    value: "a".to_owned(),
                    prefix: StringPrefix::None,
                },
                span(1),
            ),
            InitAction::zero_scalar(element.clone(), char_ty),
        ],
    );
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::OverlappingActions {
            first_action: 0,
            second_action: 1,
            first_path: root,
            second_path: element,
        })
    );

    let bad_step = SubobjectPath::root().with_index(0);
    let plan = InitPlan::new(int, vec![InitAction::zero_scalar(bad_step.clone(), int)]);
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::IndexStepOnNonArray {
            action: 0,
            path: bad_step,
            step: 0,
            ty: int,
        })
    );

    let plan = InitPlan::new(
        short_string,
        vec![InitAction::store_string(
            SubobjectPath::root(),
            short_string,
            StringLit {
                value: "abc".to_owned(),
                prefix: StringPrefix::None,
            },
            span(2),
        )],
    );
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::StringInitializerTooLong {
            action: 0,
            path: SubobjectPath::root(),
            units: 3,
            bound: 2,
        })
    );
}

#[test]
fn validation_rejects_union_sibling_writes_and_unsupported_wide_strings() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let char_array = types.intern(plain(TypeKind::Array {
        element: char_ty,
        bound: ArrayBound::Fixed(4),
    }));
    let tag = types.declare_tag(TagKind::Union, Some("Either".to_owned()));
    let union_ty = types.intern(plain(TypeKind::Union(tag)));
    types
        .complete_tag(
            tag,
            TagDefinition::Union(vec![
                RecordField {
                    name: Some("left".to_owned()),
                    ty: int,
                    bit_field: None,
                },
                RecordField {
                    name: Some("right".to_owned()),
                    ty: int,
                    bit_field: None,
                },
            ]),
        )
        .unwrap();
    let first_value = literal(1, 1);
    let second_value = literal(2, 2);
    let left = SubobjectPath::root().with_field(0);
    let right = SubobjectPath::root().with_field(1);
    let plan = InitPlan::new(
        union_ty,
        vec![
            InitAction::store_scalar(left.clone(), int, &first_value, span(1)),
            InitAction::store_scalar(right.clone(), int, &second_value, span(2)),
        ],
    );
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::OverlappingActions {
            first_action: 0,
            second_action: 1,
            first_path: left,
            second_path: right,
        })
    );

    let plan = InitPlan::new(
        char_array,
        vec![InitAction::store_string(
            SubobjectPath::root(),
            char_array,
            StringLit {
                value: "abc".to_owned(),
                prefix: StringPrefix::L,
            },
            span(3),
        )],
    );
    assert_eq!(
        plan.validate(&types),
        Err(InitPlanError::UnsupportedStringPrefix {
            action: 0,
            path: SubobjectPath::root(),
            prefix: StringPrefix::L,
        })
    );
}
