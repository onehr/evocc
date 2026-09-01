use evocc_types::{
    ArrayBound, BitFieldSpec, CanonicalType, ComplexBase, EnumConstant, FloatKind,
    FunctionParameters, GnuInt128Signedness, IntRank, LayoutError, Qualifiers, RecordField,
    TagDefinition, TagKind, TagLayoutAttributes, TargetDataLayout, TypeInterner, TypeKind,
    TypeLayout,
};

fn plain(kind: TypeKind) -> CanonicalType {
    CanonicalType {
        kind,
        qualifiers: Qualifiers::default(),
    }
}

fn field(name: Option<&str>, ty: evocc_types::TypeId) -> RecordField {
    RecordField {
        layout: Default::default(),
        name: name.map(str::to_owned),
        ty,
        bit_field: None,
    }
}

fn bits(name: Option<&str>, ty: evocc_types::TypeId, width_bits: u16) -> RecordField {
    RecordField {
        layout: Default::default(),
        name: name.map(str::to_owned),
        ty,
        bit_field: Some(BitFieldSpec { width_bits }),
    }
}

fn complete_record(
    types: &mut TypeInterner,
    kind: TagKind,
    fields: Vec<RecordField>,
) -> (evocc_types::TagId, evocc_types::TypeId) {
    let tag = types.declare_tag(kind, None);
    let ty_kind = match kind {
        TagKind::Struct => TypeKind::Struct(tag),
        TagKind::Union => TypeKind::Union(tag),
        TagKind::Enum => panic!("use an enum helper"),
    };
    let ty = types.intern(plain(ty_kind));
    let definition = match kind {
        TagKind::Struct => TagDefinition::Struct(fields),
        TagKind::Union => TagDefinition::Union(fields),
        TagKind::Enum => unreachable!(),
    };
    types.complete_tag(tag, definition).unwrap();
    (tag, ty)
}

#[test]
fn aarch64_lp64_scalar_pointer_array_and_enum_layouts() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();

    let cases = [
        (TypeKind::Int(IntRank::Bool), TypeLayout::new(1, 1)),
        (TypeKind::Int(IntRank::Char), TypeLayout::new(1, 1)),
        (TypeKind::Int(IntRank::Short), TypeLayout::new(2, 2)),
        (TypeKind::Int(IntRank::Int), TypeLayout::new(4, 4)),
        (TypeKind::Int(IntRank::Long), TypeLayout::new(8, 8)),
        (TypeKind::Int(IntRank::LongLong), TypeLayout::new(8, 8)),
        (TypeKind::Float(FloatKind::F16), TypeLayout::new(2, 2)),
        (TypeKind::Float(FloatKind::F32), TypeLayout::new(4, 4)),
        (TypeKind::Float(FloatKind::F64), TypeLayout::new(8, 8)),
        (TypeKind::Float(FloatKind::F128), TypeLayout::new(16, 16)),
        (TypeKind::Complex(ComplexBase::CF64), TypeLayout::new(16, 8)),
    ];
    for (kind, expected) in cases {
        let ty = types.intern(plain(kind));
        assert_eq!(layout.layout_of(&types, ty), Ok(expected));
    }

    let incomplete = types.declare_tag(TagKind::Struct, Some("opaque".into()));
    let opaque = types.intern(plain(TypeKind::Struct(incomplete)));
    let pointer = types.intern(plain(TypeKind::Pointer(opaque)));
    assert_eq!(
        layout.layout_of(&types, pointer),
        Ok(TypeLayout::new(8, 8)),
        "pointer layout must not require a complete pointee"
    );

    let int = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let row = types.intern(plain(TypeKind::Array {
        element: int,
        bound: ArrayBound::Fixed(3),
    }));
    let matrix = types.intern(plain(TypeKind::Array {
        element: row,
        bound: ArrayBound::Fixed(2),
    }));
    assert_eq!(layout.layout_of(&types, matrix), Ok(TypeLayout::new(24, 4)));

    let enumeration = types.declare_tag(TagKind::Enum, Some("E".into()));
    let enumeration_ty = types.intern(plain(TypeKind::Enum(enumeration)));
    types
        .complete_tag(enumeration, TagDefinition::Enum(Vec::new()))
        .unwrap();
    assert_eq!(
        layout.layout_of(&types, enumeration_ty),
        Ok(TypeLayout::new(4, 4))
    );

    let wide_enum = types.declare_tag(TagKind::Enum, Some("Wide".into()));
    let wide_enum_ty = types.intern(plain(TypeKind::Enum(wide_enum)));
    types
        .complete_tag(
            wide_enum,
            TagDefinition::Enum(vec![
                EnumConstant {
                    name: "negative".into(),
                    value: -1,
                },
                EnumConstant {
                    name: "above_int".into(),
                    value: i128::from(i32::MAX) + 1,
                },
            ]),
        )
        .unwrap();
    assert_eq!(
        layout.layout_of(&types, wide_enum_ty),
        Ok(TypeLayout::new(8, 8)),
        "a mixed negative/above-INT_MAX enum fits neither int nor unsigned int"
    );
}

#[test]
fn packed_enumerations_use_the_smallest_representable_integer_layout() {
    let cases = [
        (0, 255, TypeLayout::new(1, 1)),
        (0, 65_535, TypeLayout::new(2, 2)),
        (0, 65_536, TypeLayout::new(4, 4)),
        (0, i128::from(u32::MAX), TypeLayout::new(4, 4)),
        (-1, 6, TypeLayout::new(1, 1)),
        (-1, 200, TypeLayout::new(2, 2)),
    ];

    for (minimum, maximum, expected) in cases {
        let mut types = TypeInterner::new();
        let tag = types.declare_tag(TagKind::Enum, None);
        let ty = types.intern(plain(TypeKind::Enum(tag)));
        assert!(types.set_tag_layout(
            tag,
            TagLayoutAttributes {
                packed: true,
                ..TagLayoutAttributes::default()
            }
        ));
        types
            .complete_tag(
                tag,
                TagDefinition::Enum(vec![
                    EnumConstant {
                        name: "minimum".into(),
                        value: minimum,
                    },
                    EnumConstant {
                        name: "maximum".into(),
                        value: maximum,
                    },
                ]),
            )
            .unwrap();
        assert_eq!(
            TargetDataLayout::aarch64_unknown_linux_gnu().layout_of(&types, ty),
            Ok(expected),
            "enumerator range {minimum}..{maximum}"
        );
    }
}

#[test]
fn aarch64_gnu_int128_layout_preserves_signedness_identity() {
    let mut types = TypeInterner::new();
    let signed = types.intern(plain(TypeKind::GnuInt128(GnuInt128Signedness::Signed)));
    let signed_again = types.intern(plain(TypeKind::GnuInt128(GnuInt128Signedness::Signed)));
    let unsigned = types.intern(plain(TypeKind::GnuInt128(GnuInt128Signedness::Unsigned)));

    assert_eq!(signed, signed_again);
    assert_ne!(signed, unsigned);
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    assert_eq!(
        layout.layout_of(&types, signed),
        Ok(TypeLayout::new(16, 16))
    );
    assert_eq!(
        layout.layout_of(&types, unsigned),
        Ok(TypeLayout::new(16, 16))
    );
}

#[test]
fn structs_unions_and_nested_records_include_internal_and_tail_padding() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let int_ty = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let long_ty = types.intern(plain(TypeKind::Int(IntRank::Long)));

    let (inner_tag, inner_ty) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![field(Some("tag"), char_ty), field(Some("value"), int_ty)],
    );
    assert_eq!(
        layout
            .record_layout(&types, inner_tag)
            .unwrap()
            .field_offsets(),
        vec![0, 4]
    );

    let (outer_tag, outer_ty) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![field(Some("head"), char_ty), field(Some("inner"), inner_ty)],
    );
    let outer = layout.record_layout(&types, outer_tag).unwrap();
    assert_eq!((outer.size_bytes, outer.align_bytes), (12, 4));
    assert_eq!(outer.field_offsets(), vec![0, 4]);
    assert_eq!(outer.tail_padding_bytes, 0);
    assert_eq!(
        layout.layout_of(&types, outer_ty),
        Ok(TypeLayout::new(12, 4))
    );

    let (union_tag, union_ty) = complete_record(
        &mut types,
        TagKind::Union,
        vec![field(Some("bytes"), outer_ty), field(Some("word"), long_ty)],
    );
    let union_layout = layout.record_layout(&types, union_tag).unwrap();
    assert_eq!((union_layout.size_bytes, union_layout.align_bytes), (16, 8));
    assert_eq!(union_layout.field_offsets(), vec![0, 0]);
    assert_eq!(union_layout.tail_padding_bytes, 4);
    assert_eq!(
        layout.layout_of(&types, union_ty),
        Ok(TypeLayout::new(16, 8))
    );

    let (tail_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![field(Some("word"), int_ty), field(Some("tail"), char_ty)],
    );
    let tail = layout.record_layout(&types, tail_tag).unwrap();
    assert_eq!((tail.size_bytes, tail.align_bytes), (8, 4));
    assert_eq!(tail.tail_padding_bytes, 3);
}

#[test]
fn a_final_unbounded_array_is_a_zero_sized_flexible_prefix() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let double_ty = types.intern(plain(TypeKind::Float(FloatKind::F64)));
    let flexible = types.intern(plain(TypeKind::Array {
        element: double_ty,
        bound: ArrayBound::Unbounded,
    }));

    let (tag, ty) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![
            field(Some("prefix"), char_ty),
            field(Some("data"), flexible),
        ],
    );
    let record = layout.record_layout(&types, tag).unwrap();
    assert_eq!((record.size_bytes, record.align_bytes), (8, 8));
    assert_eq!(record.field_offsets(), vec![0, 8]);
    assert_eq!(record.fields[1].layout, TypeLayout::new(0, 8));
    assert_eq!(record.flexible_array_padding_bytes, Some(7));
    assert_eq!(record.tail_padding_bytes, 0);
    assert_eq!(layout.layout_of(&types, ty), Ok(TypeLayout::new(8, 8)));
    assert_eq!(
        layout.layout_of(&types, flexible),
        Err(LayoutError::IncompleteArray(flexible))
    );
}

#[test]
fn zero_length_array_members_add_neither_extent_nor_natural_alignment() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let long_ty = types.intern(plain(TypeKind::Int(IntRank::Long)));
    let empty_long_array = types.intern(plain(TypeKind::Array {
        element: long_ty,
        bound: ArrayBound::Fixed(0),
    }));

    assert_eq!(
        layout.layout_of(&types, empty_long_array),
        Ok(TypeLayout::new(0, 8)),
        "the complete array type keeps its element alignment outside a record",
    );

    let (tag, ty) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![
            field(Some("head"), char_ty),
            field(Some("empty"), empty_long_array),
            field(Some("tail"), char_ty),
        ],
    );
    let record = layout.record_layout(&types, tag).unwrap();
    assert_eq!((record.size_bytes, record.align_bytes), (2, 1));
    assert_eq!(record.field_offsets(), vec![0, 1, 1]);
    assert_eq!(record.fields[1].layout, TypeLayout::new(0, 1));
    assert_eq!(record.flexible_array_padding_bytes, None);
    assert_eq!(layout.layout_of(&types, ty), Ok(TypeLayout::new(2, 1)));
}

#[test]
fn bit_fields_pack_from_the_current_bit_not_an_eagerly_aligned_unit() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let char_ty = types.intern(plain(TypeKind::Int(IntRank::Char)));
    let uint_ty = types.intern(plain(TypeKind::Int(IntRank::UInt)));

    let (tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![
            field(Some("head"), char_ty),
            bits(Some("a"), uint_ty, 3),
            bits(Some("b"), uint_ty, 5),
            field(Some("tail"), char_ty),
        ],
    );
    let record = layout.record_layout(&types, tag).unwrap();
    assert_eq!((record.size_bytes, record.align_bytes), (4, 4));
    assert_eq!(record.fields[0].byte_offset, 0);
    assert_eq!(record.fields[3].byte_offset, 2);

    let a = record.fields[1].bit_field.unwrap();
    assert_eq!(
        (
            a.storage_byte_offset,
            a.storage_size_bytes,
            a.bit_offset,
            a.width_bits
        ),
        (0, 4, 8, 3)
    );
    let b = record.fields[2].bit_field.unwrap();
    assert_eq!(
        (b.storage_byte_offset, b.bit_offset, b.width_bits),
        (0, 11, 5)
    );
}

#[test]
fn bit_fields_cross_units_obey_zero_width_barriers_and_mix_base_types() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let uint_ty = types.intern(plain(TypeKind::Int(IntRank::UInt)));
    let ulong_ty = types.intern(plain(TypeKind::Int(IntRank::ULong)));

    let (cross_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![bits(Some("a"), uint_ty, 31), bits(Some("b"), uint_ty, 2)],
    );
    let cross = layout.record_layout(&types, cross_tag).unwrap();
    assert_eq!((cross.size_bytes, cross.align_bytes), (8, 4));
    assert_eq!(cross.fields[0].bit_field.unwrap().bit_offset, 0);
    assert_eq!(cross.fields[1].bit_field.unwrap().storage_byte_offset, 4);

    let (barrier_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![
            bits(Some("a"), uint_ty, 3),
            bits(None, uint_ty, 0),
            bits(Some("b"), uint_ty, 4),
        ],
    );
    let barrier = layout.record_layout(&types, barrier_tag).unwrap();
    assert_eq!((barrier.size_bytes, barrier.align_bytes), (8, 4));
    assert_eq!(barrier.fields[1].bit_field.unwrap().storage_byte_offset, 4);
    assert_eq!(barrier.fields[2].bit_field.unwrap().storage_byte_offset, 4);

    let (mixed_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![
            bits(Some("a"), uint_ty, 4),
            bits(Some("b"), ulong_ty, 5),
            bits(Some("c"), uint_ty, 6),
        ],
    );
    let mixed = layout.record_layout(&types, mixed_tag).unwrap();
    assert_eq!((mixed.size_bytes, mixed.align_bytes), (8, 8));
    assert_eq!(mixed.fields[0].bit_field.unwrap().bit_offset, 0);
    assert_eq!(mixed.fields[1].bit_field.unwrap().bit_offset, 4);
    assert_eq!(mixed.fields[2].bit_field.unwrap().bit_offset, 9);

    let (unnamed_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![
            bits(Some("a"), uint_ty, 3),
            bits(None, uint_ty, 5),
            bits(Some("b"), uint_ty, 4),
        ],
    );
    let unnamed = layout.record_layout(&types, unnamed_tag).unwrap();
    assert_eq!((unnamed.size_bytes, unnamed.align_bytes), (4, 4));
    assert_eq!(unnamed.fields[1].bit_field.unwrap().bit_offset, 3);
    assert_eq!(unnamed.fields[2].bit_field.unwrap().bit_offset, 8);

    let (zero_only_tag, _) =
        complete_record(&mut types, TagKind::Struct, vec![bits(None, uint_ty, 0)]);
    let zero_only = layout.record_layout(&types, zero_only_tag).unwrap();
    assert_eq!(
        (zero_only.size_bytes, zero_only.align_bytes),
        (0, 4),
        "AAPCS64 makes even an anonymous zero-width container contribute alignment"
    );

    let (union_tag, _) = complete_record(
        &mut types,
        TagKind::Union,
        vec![
            bits(Some("bits"), ulong_ty, 5),
            field(Some("word"), uint_ty),
        ],
    );
    let union_layout = layout.record_layout(&types, union_tag).unwrap();
    assert_eq!((union_layout.size_bytes, union_layout.align_bytes), (8, 8));
    assert_eq!(union_layout.field_offsets(), vec![0, 0]);
    assert_eq!(union_layout.fields[0].bit_field.unwrap().bit_offset, 0);
}

#[test]
fn invalid_and_non_object_layouts_fail_closed_with_typed_errors() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let void_ty = types.intern(plain(TypeKind::Void));
    let int_ty = types.intern(plain(TypeKind::Int(IntRank::Int)));
    let pointer_ty = types.intern(plain(TypeKind::Pointer(int_ty)));
    let function_ty = types.intern(plain(TypeKind::Function {
        return_type: int_ty,
        parameters: FunctionParameters::Prototype {
            types: Vec::new(),
            variadic: false,
        },
    }));
    let vla = types.intern(plain(TypeKind::Array {
        element: int_ty,
        bound: ArrayBound::Variable,
    }));
    assert_eq!(
        layout.layout_of(&types, void_ty),
        Err(LayoutError::NonObjectType(void_ty))
    );
    assert_eq!(
        layout.layout_of(&types, function_ty),
        Err(LayoutError::NonObjectType(function_ty))
    );
    assert_eq!(
        layout.layout_of(&types, vla),
        Err(LayoutError::VariableLengthArray(vla))
    );

    let incomplete = types.declare_tag(TagKind::Struct, None);
    let incomplete_ty = types.intern(plain(TypeKind::Struct(incomplete)));
    assert_eq!(
        layout.layout_of(&types, incomplete_ty),
        Err(LayoutError::IncompleteTag(incomplete))
    );

    let recursive = types.declare_tag(TagKind::Struct, Some("recursive".into()));
    let recursive_ty = types.intern(plain(TypeKind::Struct(recursive)));
    types
        .complete_tag(
            recursive,
            TagDefinition::Struct(vec![field(Some("self"), recursive_ty)]),
        )
        .unwrap();
    assert_eq!(
        layout.layout_of(&types, recursive_ty),
        Err(LayoutError::RecursiveByValue(recursive))
    );

    let (wide_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![bits(Some("wide"), int_ty, 33)],
    );
    assert_eq!(
        layout.record_layout(&types, wide_tag),
        Err(LayoutError::BitFieldWidthExceedsType {
            tag: wide_tag,
            field_index: 0,
            width_bits: 33,
            type_bits: 32,
        })
    );

    let (named_zero_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![bits(Some("named"), int_ty, 0)],
    );
    assert_eq!(
        layout.record_layout(&types, named_zero_tag),
        Err(LayoutError::NamedZeroWidthBitField {
            tag: named_zero_tag,
            field_index: 0,
        })
    );

    let (bad_base_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![bits(Some("pointer"), pointer_ty, 1)],
    );
    assert_eq!(
        layout.record_layout(&types, bad_base_tag),
        Err(LayoutError::InvalidBitFieldBase {
            tag: bad_base_tag,
            field_index: 0,
            ty: pointer_ty,
        })
    );

    let flexible = types.intern(plain(TypeKind::Array {
        element: int_ty,
        bound: ArrayBound::Unbounded,
    }));
    let (nonfinal_tag, _) = complete_record(
        &mut types,
        TagKind::Struct,
        vec![field(Some("data"), flexible), field(Some("tail"), int_ty)],
    );
    assert_eq!(
        layout.record_layout(&types, nonfinal_tag),
        Err(LayoutError::FlexibleArrayNotLast {
            tag: nonfinal_tag,
            field_index: 0,
        })
    );
    let (union_flexible_tag, _) = complete_record(
        &mut types,
        TagKind::Union,
        vec![field(Some("data"), flexible)],
    );
    assert_eq!(
        layout.record_layout(&types, union_flexible_tag),
        Err(LayoutError::FlexibleArrayInUnion {
            tag: union_flexible_tag,
            field_index: 0,
        })
    );
}

#[test]
fn arrays_and_record_rounding_report_overflow_instead_of_wrapping() {
    let mut types = TypeInterner::new();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    let long_double = types.intern(plain(TypeKind::Float(FloatKind::F128)));
    let impossible = types.intern(plain(TypeKind::Array {
        element: long_double,
        bound: ArrayBound::Fixed(u64::MAX),
    }));
    assert_eq!(
        layout.layout_of(&types, impossible),
        Err(LayoutError::ArithmeticOverflow)
    );
}
