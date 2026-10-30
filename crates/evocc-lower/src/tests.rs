//! Lowering unit tests.

use super::*;

#[test]
fn vector_declarator_checks_the_attribute_value_and_base_type() {
    use evocc_chir::{
        AppliedAttribute, Attribute, AttributeArgument, AttributePlacement, AttributeSyntax,
    };
    use evocc_types::{CanonicalType, IntRank, Qualifiers, VectorFamily};

    let mut types = TypeInterner::new();
    let scalar = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let other = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::UInt),
        qualifiers: Qualifiers::default(),
    });
    let vector = types.intern(CanonicalType {
        kind: TypeKind::Vector {
            element: scalar,
            lanes: 4,
            family: VectorFamily::Gnu,
        },
        qualifiers: Qualifiers::default(),
    });
    let span = SourceSpan::new(evocc_common::FileId::new(0), 0, 1, 1, 1);
    let mut attribute = Attribute {
        name: "vector_size".into(),
        written: "vector_size".into(),
        syntax: AttributeSyntax::Gnu,
        placement: AttributePlacement::DeclarationSpecifiers,
        arguments: vec![AttributeArgument {
            text: "16".into(),
            span,
            integer: Some(16),
        }],
        applied: Some(AppliedAttribute::VectorSize),
        span,
    };
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let matches = |attribute: &Attribute, base| {
        evocc_chir::vector::attribute_matches_base(
            &types,
            &target,
            std::slice::from_ref(attribute),
            vector,
            base,
        )
    };
    assert!(matches(&attribute, scalar));
    assert!(!matches(&attribute, other));
    assert!(!evocc_chir::vector::attribute_matches_base(
        &types,
        &target,
        &[],
        vector,
        scalar
    ));
    attribute.arguments[0].integer = Some(32);
    assert!(!matches(&attribute, scalar));
    attribute.arguments[0].integer = Some(16);
    attribute.applied = None;
    assert!(!matches(&attribute, scalar));
}

#[test]
fn array_completion_only_relaxes_the_initialized_outermost_bound() {
    use evocc_types::{CanonicalType, IntRank, Qualifiers};

    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let array = types.intern(CanonicalType {
        kind: TypeKind::Array {
            element: int,
            bound: ArrayBound::Fixed(4),
        },
        qualifiers: Qualifiers::default(),
    });
    let span = SourceSpan::new(evocc_common::FileId::new(0), 0, 1, 1, 1);
    let layer = |bound| WrittenDeclaratorLayer::Array {
        bound,
        parameter_qualifiers: Qualifiers::default(),
        static_bound: false,
        span,
    };
    let mut written = WrittenDeclarator {
        layers: vec![layer(ArrayBound::Unbounded)],
        span,
    };
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    assert!(written_declarator_matches(
        &types,
        &written,
        array,
        int,
        None,
        true,
        false,
        (&target, &[])
    ));
    assert!(!written_declarator_matches(
        &types,
        &written,
        array,
        int,
        None,
        false,
        false,
        (&target, &[])
    ));
    written.layers[0] = layer(ArrayBound::Fixed(3));
    assert!(!written_declarator_matches(
        &types,
        &written,
        array,
        int,
        None,
        true,
        false,
        (&target, &[])
    ));
    let nested = types.intern(CanonicalType {
        kind: TypeKind::Array {
            element: array,
            bound: ArrayBound::Fixed(2),
        },
        qualifiers: Qualifiers::default(),
    });
    written.layers = vec![layer(ArrayBound::Fixed(2)), layer(ArrayBound::Unbounded)];
    assert!(!written_declarator_matches(
        &types,
        &written,
        nested,
        int,
        None,
        true,
        false,
        (&target, &[])
    ));
}

#[test]
fn lower_error_is_stable_and_nonempty() {
    let error = LowerError::InvalidGeneratedSsa {
        message: "bad edge".into(),
    };
    assert_eq!(error.to_string(), "generated invalid EvoIR: bad edge");
}
