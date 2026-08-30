use evocc_types::{
    ArrayBound, CanonicalType, EnumConstant, FunctionParameters, IntRank, Qualifiers, RecordField,
    TagDefinition, TagId, TagKind, TagLayoutAttributes, TagState, TypeId, TypeInterner,
    TypeInternerTransactionError, TypeKind,
};

fn plain(kind: TypeKind) -> CanonicalType {
    CanonicalType {
        kind,
        qualifiers: Qualifiers::default(),
    }
}

fn constant(kind: TypeKind) -> CanonicalType {
    CanonicalType {
        kind,
        qualifiers: Qualifiers {
            is_const: true,
            ..Qualifiers::default()
        },
    }
}

fn pointer(pointee: TypeId) -> CanonicalType {
    plain(TypeKind::Pointer(pointee))
}

fn array(element: TypeId, bound: u64) -> CanonicalType {
    plain(TypeKind::Array {
        element,
        bound: ArrayBound::Fixed(bound),
    })
}

fn function(return_type: TypeId, parameters: FunctionParameters) -> CanonicalType {
    // Parameter names and source spans deliberately have no place in this key.
    plain(TypeKind::Function {
        return_type,
        parameters,
    })
}

#[test]
fn structurally_equal_types_collapse_to_one_public_id() {
    let mut types = TypeInterner::new();
    assert!(types.is_empty());

    let int_a: TypeId = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let int_b = types.intern(plain(TypeKind::Int(IntRank::Int)));
    assert_eq!(int_a, int_b, "equal scalars must collapse");

    let pointer_a = types.intern(pointer(int_a));
    let pointer_b = types.intern(pointer(int_b));
    assert_eq!(
        pointer_a, pointer_b,
        "equal child IDs must collapse pointers"
    );

    let array_a = types.intern(array(pointer_a, 8));
    let array_b = types.intern(array(pointer_b, 8));
    assert_eq!(array_a, array_b, "equal arrays must collapse");

    let params = || FunctionParameters::Prototype {
        types: vec![pointer_a, array_a],
        variadic: false,
    };
    let function_a = types.intern(function(int_a, params()));
    let function_b = types.intern(function(int_b, params()));
    assert_eq!(function_a, function_b, "equal functions must collapse");

    assert_eq!(types.len(), 4, "duplicates must not grow the interner");
    assert_eq!(
        types.get(function_a),
        Some(&function(int_a, params())),
        "TypeId must resolve to its canonical public key"
    );
}

#[test]
fn every_canonical_difference_remains_distinct() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let long = types.intern(plain(TypeKind::Int(IntRank::Long)));
    let const_int = types.intern(constant(TypeKind::Int(IntRank::Int)));

    let pointer_to_int = types.intern(pointer(int));
    let const_pointer_to_int = types.intern(constant(TypeKind::Pointer(int)));
    let pointer_to_const_int = types.intern(pointer(const_int));

    let bound_4 = types.intern(array(int, 4));
    let bound_5 = types.intern(array(int, 5));

    let unspecified = types.intern(function(int, FunctionParameters::Unspecified));
    let empty_prototype = types.intern(function(
        int,
        FunctionParameters::Prototype {
            types: vec![],
            variadic: false,
        },
    ));
    let fixed = types.intern(function(
        int,
        FunctionParameters::Prototype {
            types: vec![int],
            variadic: false,
        },
    ));
    let variadic = types.intern(function(
        int,
        FunctionParameters::Prototype {
            types: vec![int],
            variadic: true,
        },
    ));
    let long_parameter = types.intern(function(
        int,
        FunctionParameters::Prototype {
            types: vec![long],
            variadic: false,
        },
    ));
    let long_return = types.intern(function(
        long,
        FunctionParameters::Prototype {
            types: vec![int],
            variadic: false,
        },
    ));

    let first_tag = types.declare_tag(TagKind::Struct, Some("Node".into()));
    let second_tag = types.declare_tag(TagKind::Struct, Some("Node".into()));
    let first_struct = types.intern(plain(TypeKind::Struct(first_tag)));
    let second_struct = types.intern(plain(TypeKind::Struct(second_tag)));

    let differences = [
        ("scalar qualifier", int, const_int),
        (
            "pointer qualifier layer",
            const_pointer_to_int,
            pointer_to_const_int,
        ),
        ("array bound", bound_4, bound_5),
        ("unspecified versus prototype", unspecified, empty_prototype),
        ("prototype variadic bit", fixed, variadic),
        ("parameter type", fixed, long_parameter),
        ("return type", fixed, long_return),
        ("nominal tag identity", first_struct, second_struct),
    ];

    for (case, left, right) in differences {
        assert_ne!(left, right, "{case} must participate in canonical identity");
    }
    assert_ne!(pointer_to_int, const_pointer_to_int);
}

#[test]
fn insertion_order_is_deterministic() {
    fn trace() -> Vec<u32> {
        let mut types = TypeInterner::default();
        let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
        let pointer = types.intern(pointer(int));
        let array = types.intern(array(pointer, 3));
        let function = types.intern(function(
            int,
            FunctionParameters::Prototype {
                types: vec![array],
                variadic: false,
            },
        ));
        [int, pointer, array, function]
            .into_iter()
            .map(TypeId::raw)
            .collect()
    }

    let first = trace();
    let second = trace();
    assert_eq!(
        first, second,
        "equal insertion traces must allocate equal IDs"
    );
    assert!(first.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn nominal_tags_keep_identity_when_completed_and_allow_recursion() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));

    let structure: TagId = types.declare_tag(TagKind::Struct, Some("Node".into()));
    let union = types.declare_tag(TagKind::Union, Some("Payload".into()));
    let enumeration = types.declare_tag(TagKind::Enum, Some("Mode".into()));
    assert_ne!(structure, union);
    assert_ne!(union, enumeration);
    assert!(matches!(
        &types.tag(structure).unwrap().state,
        TagState::Incomplete
    ));

    let incomplete = types.intern(plain(TypeKind::Struct(structure)));
    let recursive_pointer = types.intern(pointer(incomplete));
    types
        .complete_tag(
            structure,
            TagDefinition::Struct(vec![RecordField {
                layout: Default::default(),
                name: Some("next".into()),
                ty: recursive_pointer,
                bit_field: None,
            }]),
        )
        .unwrap();
    let complete = types.intern(plain(TypeKind::Struct(structure)));
    assert_eq!(incomplete, complete, "completion must preserve TypeId");

    let union_before = types.intern(plain(TypeKind::Union(union)));
    types
        .complete_tag(
            union,
            TagDefinition::Union(vec![RecordField {
                layout: Default::default(),
                name: Some("word".into()),
                ty: int,
                bit_field: None,
            }]),
        )
        .unwrap();
    assert_eq!(union_before, types.intern(plain(TypeKind::Union(union))));

    let enum_before = types.intern(plain(TypeKind::Enum(enumeration)));
    types
        .complete_tag(
            enumeration,
            TagDefinition::Enum(vec![EnumConstant {
                name: "Ready".into(),
                value: 1,
            }]),
        )
        .unwrap();
    assert_eq!(
        enum_before,
        types.intern(plain(TypeKind::Enum(enumeration)))
    );
    assert!(matches!(
        &types.tag(structure).unwrap().state,
        TagState::Complete(TagDefinition::Struct(_))
    ));
}

#[test]
fn speculative_type_transactions_are_lifo_and_restore_types_and_tags() {
    let mut types = TypeInterner::new();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let outer = types.begin_transaction();
    let tag = types.declare_tag(TagKind::Struct, Some("temporary".into()));
    let record = types.intern(plain(TypeKind::Struct(tag)));
    assert!(types.set_tag_layout(
        tag,
        TagLayoutAttributes {
            packed: true,
            aligned: Some(8),
            max_align: Some(2),
        }
    ));
    types
        .complete_tag(
            tag,
            TagDefinition::Struct(vec![RecordField {
                name: Some("value".into()),
                ty: int,
                layout: Default::default(),
                bit_field: None,
            }]),
        )
        .unwrap();

    let inner = types.begin_transaction();
    let pointer = types.intern(pointer(record));
    assert_eq!(
        types.commit_transaction(&outer),
        Err(TypeInternerTransactionError)
    );
    types.commit_transaction(&inner).unwrap();
    types.rollback_transaction(&outer).unwrap();

    assert_eq!(types.len(), 1);
    assert_eq!(types.get(record), None);
    assert_eq!(types.get(pointer), None);
    assert_eq!(types.tag(tag), None);
    let replacement_tag = types.declare_tag(TagKind::Struct, Some("replacement".into()));
    assert_eq!(replacement_tag, tag, "rolled-back tag IDs are reusable");
    let replacement_record = types.intern(plain(TypeKind::Struct(replacement_tag)));
    assert_eq!(
        replacement_record, record,
        "rolled-back type IDs are reusable"
    );

    let foreign = TypeInterner::new().begin_transaction();
    assert_eq!(
        types.rollback_transaction(&foreign),
        Err(TypeInternerTransactionError)
    );
}

#[test]
fn committed_type_transactions_persist_and_tokens_are_one_shot() {
    let mut types = TypeInterner::new();
    let transaction = types.begin_transaction();
    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    types.commit_transaction(&transaction).unwrap();

    assert_eq!(types.get(int), Some(&plain(TypeKind::Int(IntRank::Int))));
    assert_eq!(
        types.rollback_transaction(&transaction),
        Err(TypeInternerTransactionError)
    );
}
