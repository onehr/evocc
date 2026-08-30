use evocc_chir::{
    AssignmentOp, ExprCategory, ExprFacts, ExprFactsError, ScalarPlace, ScalarPlaceError,
    ScopeKind, StorageClass, SymbolData, SymbolKind, SymbolTable, SymbolTableError,
};
use evocc_common::{FileId, SourceSpan};
use evocc_types::{
    ArrayBound, CanonicalType, FunctionParameters, IntRank, Qualifiers, TagKind, TypeInterner,
    TypeKind,
};

fn span(start: u32) -> SourceSpan {
    SourceSpan::new(FileId::new(41), start, start + 1, 1, start + 1)
}

fn int_type(types: &mut TypeInterner) -> evocc_types::TypeId {
    types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    })
}

#[test]
fn symbol_ids_scopes_and_iteration_follow_exact_source_order() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();

    let outer = symbols
        .declare(
            file,
            "value",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1),
        )
        .unwrap();
    let block = symbols
        .create_scope(file, ScopeKind::Block, Some(span(2)))
        .unwrap();
    assert_eq!(symbols.resolve(block, "value").unwrap(), Some(outer));

    let inner = symbols
        .declare(
            block,
            "value",
            SymbolData::Object {
                storage: StorageClass::Auto,
            },
            int,
            span(3),
        )
        .unwrap();
    let parameter = symbols
        .declare(
            block,
            "argument",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            int,
            span(4),
        )
        .unwrap();

    assert_eq!((file.raw(), block.raw()), (0, 1));
    assert_eq!((outer.raw(), inner.raw(), parameter.raw()), (0, 1, 2));
    assert_eq!(symbols.resolve(block, "value").unwrap(), Some(inner));
    assert_eq!(symbols.resolve(file, "argument").unwrap(), None);
    assert_eq!(
        symbols
            .symbols_in_source_order()
            .iter()
            .map(|symbol| symbol.spelling())
            .collect::<Vec<_>>(),
        ["value", "value", "argument"]
    );
    assert_eq!(symbols.scope_symbols(file).unwrap(), &[outer]);
    assert_eq!(symbols.scope_symbols(block).unwrap(), &[inner, parameter]);
}

#[test]
fn duplicate_declaration_fails_transactionally() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let first = symbols
        .declare(
            file,
            "same",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1),
        )
        .unwrap();

    assert_eq!(
        symbols.declare(file, "same", SymbolData::Typedef, int, span(2)),
        Err(SymbolTableError::DuplicateInScope {
            scope: file,
            existing: first,
            spelling: "same".to_owned(),
        })
    );
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols.resolve(file, "same").unwrap(), Some(first));
}

#[test]
fn expression_facts_enforce_c_value_category_invariants() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);

    let value = ExprFacts::value(int);
    assert_eq!(value.category(), ExprCategory::Value);
    assert!(!value.is_modifiable_lvalue());
    assert!(!value.is_addressable());

    let object = ExprFacts::lvalue(int, true, true).unwrap();
    assert!(object.is_modifiable_lvalue());
    assert!(object.is_addressable());

    let bit_field = ExprFacts::bit_field(int, true);
    assert!(bit_field.is_bit_field());
    assert!(!bit_field.is_addressable());

    assert_eq!(
        ExprFacts::try_new(int, ExprCategory::Value, true, false, false),
        Err(ExprFactsError::ModifiableNonLvalue)
    );
    assert_eq!(
        ExprFacts::try_new(int, ExprCategory::LValue, true, true, true),
        Err(ExprFactsError::AddressableBitField)
    );
}

#[test]
fn assignment_operators_and_scalar_places_are_typed_not_strings() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let object = symbols
        .declare(
            file,
            "object",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1),
        )
        .unwrap();
    let alias = symbols
        .declare(file, "alias", SymbolData::Typedef, int, span(2))
        .unwrap();

    let place = ScalarPlace::bind(&symbols, &types, object).unwrap();
    assert_eq!(place.symbol(), object);
    assert_eq!(place.ty(), int);
    assert_eq!(
        ScalarPlace::bind(&symbols, &types, alias),
        Err(ScalarPlaceError::NotScalarObject {
            symbol: alias,
            kind: SymbolKind::Typedef,
        })
    );

    assert_eq!(
        AssignmentOp::try_from(evocc_chir::BinaryOp::Assign),
        Ok(AssignmentOp::Simple)
    );
    assert_eq!(
        AssignmentOp::try_from(evocc_chir::BinaryOp::ShlAssign),
        Ok(AssignmentOp::ShiftLeft)
    );
    assert!(AssignmentOp::ShiftLeft.is_compound());
    assert!(AssignmentOp::try_from(evocc_chir::BinaryOp::Add).is_err());
}

#[test]
fn tag_namespace_payloads_and_rollback_remain_source_exact() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let function_ty = types.intern(CanonicalType {
        kind: TypeKind::Function {
            return_type: int,
            parameters: FunctionParameters::Prototype {
                types: Vec::new(),
                variadic: false,
            },
        },
        qualifiers: Qualifiers::default(),
    });
    let outer_tag = types.declare_tag(TagKind::Struct, Some("same".to_owned()));
    let inner_tag = types.declare_tag(TagKind::Struct, Some("same".to_owned()));
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let ordinary = symbols
        .declare(
            file,
            "same",
            SymbolData::Object {
                storage: StorageClass::Static,
            },
            int,
            span(1),
        )
        .unwrap();
    symbols
        .declare_tag(file, "same", outer_tag, span(2))
        .unwrap();
    let function = symbols
        .declare(
            file,
            "run",
            SymbolData::Function {
                storage: StorageClass::Extern,
            },
            function_ty,
            span(3),
        )
        .unwrap();
    let checkpoint = symbols.checkpoint();
    let for_scope = symbols
        .create_scope(file, ScopeKind::For, Some(span(4)))
        .unwrap();
    symbols
        .declare_tag(for_scope, "same", inner_tag, span(5))
        .unwrap();
    let enumerator = symbols
        .declare(
            for_scope,
            "answer",
            SymbolData::Enumerator { value: 42 },
            int,
            span(6),
        )
        .unwrap();

    assert_eq!(symbols.resolve(for_scope, "same").unwrap(), Some(ordinary));
    assert_eq!(
        symbols.resolve_tag(for_scope, "same").unwrap(),
        Some(inner_tag)
    );
    assert_eq!(
        symbols.symbol(ordinary).unwrap().storage_class(),
        Some(StorageClass::Static)
    );
    assert_eq!(
        symbols.symbol(function).unwrap().storage_class(),
        Some(StorageClass::Extern)
    );
    assert_eq!(
        symbols.symbol(enumerator).unwrap().enumerator_value(),
        Some(42)
    );
    assert_eq!(
        symbols
            .tags_in_source_order()
            .iter()
            .map(|binding| binding.tag())
            .collect::<Vec<_>>(),
        [outer_tag, inner_tag]
    );

    symbols.rollback(checkpoint).unwrap();
    assert_eq!(symbols.scopes_in_source_order().len(), 1);
    assert_eq!(symbols.symbols_in_source_order().len(), 2);
    assert_eq!(symbols.tags_in_source_order().len(), 1);
    assert_eq!(symbols.resolve_tag(file, "same").unwrap(), Some(outer_tag));
}

#[test]
fn scalar_place_rejects_aggregate_objects() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let array = types.intern(CanonicalType {
        kind: TypeKind::Array {
            element: int,
            bound: ArrayBound::Fixed(2),
        },
        qualifiers: Qualifiers::default(),
    });
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let aggregate = symbols
        .declare(
            file,
            "items",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            array,
            span(1),
        )
        .unwrap();

    assert_eq!(
        ScalarPlace::bind(&symbols, &types, aggregate),
        Err(ScalarPlaceError::NonScalarType {
            symbol: aggregate,
            ty: array,
        })
    );
}

#[test]
fn checkpoints_restore_stale_branches_and_reject_foreign_tables() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let object = || SymbolData::Object {
        storage: StorageClass::None,
    };
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    symbols.declare(file, "a", object(), int, span(1)).unwrap();
    let after_a = symbols.checkpoint();
    symbols.declare(file, "b", object(), int, span(2)).unwrap();
    let after_b = symbols.checkpoint();

    symbols.rollback(after_a).unwrap();
    symbols.declare(file, "c", object(), int, span(3)).unwrap();
    assert_eq!(
        symbols
            .symbols_in_source_order()
            .iter()
            .map(|symbol| symbol.spelling())
            .collect::<Vec<_>>(),
        ["a", "c"]
    );

    symbols.rollback(after_b).unwrap();
    assert_eq!(
        symbols
            .symbols_in_source_order()
            .iter()
            .map(|symbol| symbol.spelling())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );

    let foreign = SymbolTable::new().checkpoint();
    assert_eq!(
        symbols.rollback(foreign),
        Err(SymbolTableError::InvalidCheckpoint)
    );
}

#[test]
fn parser_transactions_are_lifo_and_undo_every_symbol_table_mutation() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let long = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Long),
        qualifiers: Qualifiers::default(),
    });
    let tag = types.declare_tag(TagKind::Struct, Some("temporary".to_owned()));
    let object = || SymbolData::Object {
        storage: StorageClass::None,
    };
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let base = symbols
        .declare(file, "base", object(), int, span(1))
        .unwrap();
    let block = symbols
        .create_scope(file, ScopeKind::Block, Some(span(2)))
        .unwrap();

    let outer = symbols.begin_transaction();
    symbols.bind_existing(block, base).unwrap();
    symbols.update_symbol_type(base, long).unwrap();
    let added = symbols
        .declare(file, "added", object(), int, span(3))
        .unwrap();
    symbols
        .declare_tag(file, "temporary", tag, span(4))
        .unwrap();
    let nested_scope = symbols
        .create_scope(block, ScopeKind::For, Some(span(5)))
        .unwrap();

    let inner = symbols.begin_transaction();
    symbols
        .declare(nested_scope, "nested", object(), int, span(6))
        .unwrap();
    assert_eq!(
        symbols.commit_transaction(&outer),
        Err(SymbolTableError::InvalidCheckpoint)
    );
    symbols.commit_transaction(&inner).unwrap();
    symbols.rollback_transaction(&outer).unwrap();

    assert_eq!(symbols.symbol(base).unwrap().ty(), int);
    assert_eq!(
        symbols.symbol(added),
        Err(SymbolTableError::UnknownSymbol(added))
    );
    assert_eq!(symbols.scope_symbols(block).unwrap(), &[]);
    assert_eq!(symbols.scopes_in_source_order().len(), 2);
    assert_eq!(symbols.tags_in_source_order(), &[]);
    assert_eq!(symbols.resolve_tag(file, "temporary").unwrap(), None);

    let foreign = SymbolTable::new().begin_transaction();
    assert_eq!(
        symbols.rollback_transaction(&foreign),
        Err(SymbolTableError::InvalidCheckpoint)
    );
}

#[test]
fn committed_parser_transactions_persist_and_tokens_are_one_shot() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let transaction = symbols.begin_transaction();
    let kept = symbols
        .declare(
            file,
            "kept",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            span(1),
        )
        .unwrap();
    symbols.commit_transaction(&transaction).unwrap();

    assert_eq!(symbols.resolve(file, "kept").unwrap(), Some(kept));
    assert_eq!(
        symbols.rollback_transaction(&transaction),
        Err(SymbolTableError::InvalidCheckpoint)
    );
}

#[test]
fn linked_aliases_and_composite_type_updates_preserve_identity_and_roll_back() {
    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    let long = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Long),
        qualifiers: Qualifiers::default(),
    });
    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let entity = symbols
        .declare(
            file,
            "linked",
            SymbolData::Object {
                storage: StorageClass::Extern,
            },
            int,
            span(1),
        )
        .unwrap();
    let block = symbols
        .create_scope(file, ScopeKind::Block, Some(span(2)))
        .unwrap();
    let checkpoint = symbols.checkpoint();

    assert_eq!(symbols.bind_existing(block, entity), Ok(entity));
    assert_eq!(symbols.bind_existing(block, entity), Ok(entity));
    assert_eq!(symbols.resolve(block, "linked").unwrap(), Some(entity));
    assert_eq!(symbols.scope_symbols(block).unwrap(), &[entity]);
    assert_eq!(symbols.symbols_in_source_order().len(), 1);

    symbols.update_symbol_type(entity, long).unwrap();
    assert_eq!(symbols.symbol(entity).unwrap().ty(), long);

    symbols.rollback(checkpoint).unwrap();
    assert_eq!(symbols.scope_symbols(block).unwrap(), &[]);
    assert_eq!(symbols.resolve(block, "linked").unwrap(), Some(entity));
    assert_eq!(symbols.symbol(entity).unwrap().ty(), int);
    assert_eq!(symbols.symbols_in_source_order().len(), 1);
}

#[test]
fn every_assignment_operator_round_trips_and_invalid_facts_fail_closed() {
    let assignments = [
        AssignmentOp::Simple,
        AssignmentOp::Add,
        AssignmentOp::Subtract,
        AssignmentOp::Multiply,
        AssignmentOp::Divide,
        AssignmentOp::Remainder,
        AssignmentOp::ShiftLeft,
        AssignmentOp::ShiftRight,
        AssignmentOp::BitAnd,
        AssignmentOp::BitOr,
        AssignmentOp::BitXor,
    ];
    for assignment in assignments {
        let binary = evocc_chir::BinaryOp::from(assignment);
        assert_eq!(AssignmentOp::try_from(binary), Ok(assignment));
    }

    let mut types = TypeInterner::new();
    let int = int_type(&mut types);
    assert_eq!(
        ExprFacts::try_new(int, ExprCategory::Value, false, true, false),
        Err(ExprFactsError::BitFieldNonLvalue)
    );
    assert_eq!(
        ExprFacts::try_new(int, ExprCategory::Value, false, false, true),
        Err(ExprFactsError::AddressableValue)
    );
}
