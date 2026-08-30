use evocc_chir::{
    CHirBlockItem, CHirExpr, CHirStmt, ExprFacts, IntBase, IntLit, IntSuffix, MemoryPlace,
    MemoryPlaceError, PlaceRoot, PointerTypeDesc, SymbolTable,
};
use evocc_common::{FileId, SourceSpan};
use evocc_types::{CanonicalType, IntRank, Qualifiers, TypeInterner, TypeKind};

fn span() -> SourceSpan {
    SourceSpan::new(FileId::new(10_119), 0, 1, 1, 1)
}

#[test]
fn statement_expression_pointer_facts_must_match_its_final_value() {
    let mut types = TypeInterner::new();
    let int = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Int),
        qualifiers: Qualifiers::default(),
    });
    let long = types.intern(CanonicalType {
        kind: TypeKind::Int(IntRank::Long),
        qualifiers: Qualifiers::default(),
    });
    let int_pointer = types.intern(CanonicalType {
        kind: TypeKind::Pointer(int),
        qualifiers: Qualifiers::default(),
    });
    let long_pointer = types.intern(CanonicalType {
        kind: TypeKind::Pointer(long),
        qualifiers: Qualifiers::default(),
    });
    let zero = CHirExpr::Lit(
        IntLit {
            value: 0,
            suffix: IntSuffix::None,
            base: IntBase::Dec,
        },
        span(),
    );
    let final_value = CHirExpr::Cast(int_pointer, &zero, span());
    let final_statement = CHirStmt::Expr(&final_value, span());
    let body = CHirStmt::Compound(vec![CHirBlockItem::Statement(&final_statement)], span());
    let forged = CHirExpr::StatementExpr {
        body: &body,
        value: Some(&final_value),
        facts: ExprFacts::value(long_pointer),
        span: span(),
    };
    let claimed = PointerTypeDesc::from_canonical_pointer(&types, long_pointer).unwrap();

    let result = MemoryPlace::derive(
        &SymbolTable::new(),
        &types,
        PlaceRoot::PointerExpression {
            pointer: &forged,
            pointer_type: claimed,
        },
        Vec::new(),
        long,
    );

    assert!(
        matches!(result, Err(MemoryPlaceError::PointerExpressionTypeMismatch)),
        "statement-expression facts must be checked against the final value, got {result:?}"
    );
}
