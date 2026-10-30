use super::*;

pub(super) fn pointer_expression_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    target: &TargetDataLayout,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    if let CHirExpr::StringLit(literal, _) = expression {
        return string_literal_pointer(types, literal);
    }
    if let CHirExpr::StatementExpr {
        value: Some(value),
        facts,
        ..
    } = expression
    {
        // The producer records the post-lvalue-conversion result type, but a
        // checked place must not trust that record by itself. Reconstruct the
        // final expression's pointer descriptor, recursively for nested GNU
        // statement expressions, and require the two authorities to agree.
        if facts.category() != crate::ExprCategory::Value {
            return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
        }
        let derived = pointer_expression_type(symbols, types, value, target)?;
        let claimed = PointerTypeDesc::from_canonical_pointer(types, facts.ty())?;
        return (derived == claimed)
            .then_some(claimed)
            .ok_or(MemoryPlaceError::PointerExpressionTypeMismatch);
    }
    if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = expression {
        let pointee = addressed_expression_type(symbols, types, addressed, target)?;
        return PointerTypeDesc::to_object(types, pointee)
            .ok_or(MemoryPlaceError::UnknownType(pointee));
    }

    match expression {
        CHirExpr::Atomic(access) => {
            PointerTypeDesc::from_canonical_pointer(types, access.facts.ty())
        }
        CHirExpr::Floating(access) => {
            PointerTypeDesc::from_canonical_pointer(types, access.facts.ty())
        }
        CHirExpr::Vector(access) => {
            PointerTypeDesc::from_canonical_pointer(types, access.facts.ty())
        }
        CHirExpr::Complex(_) => Err(MemoryPlaceError::PointerExpressionTypeMismatch),
        CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            ..
        } => {
            let declaration = symbols
                .symbol(*symbol)
                .map_err(|_| MemoryPlaceError::UnknownSymbol(*symbol))?;
            let ty = declaration.ty();
            let category_matches = match declaration.kind() {
                SymbolKind::Function => facts.category() == crate::ExprCategory::FunctionDesignator,
                SymbolKind::Object | SymbolKind::Parameter => {
                    facts.category() == crate::ExprCategory::LValue
                }
                SymbolKind::Enumerator | SymbolKind::Typedef => false,
            };
            if declaration.spelling() != spelling
                || !crate::type_at_use_matches_binding(types, facts.ty(), ty)
                || !category_matches
            {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, ty)
        }
        CHirExpr::MemoryRead { place, facts, .. } => {
            if facts.ty() != place.value_ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, place.value_ty())
        }
        CHirExpr::Assign { place, facts, .. } | CHirExpr::Update { place, facts, .. } => {
            if facts.ty() != place.ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, place.ty())
        }
        CHirExpr::MemoryAssign { place, facts, .. }
        | CHirExpr::MemoryUpdate { place, facts, .. } => {
            if facts.ty() != place.value_ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            converted_pointer_type(types, place.value_ty())
        }
        CHirExpr::Cast(ty, _, _) | CHirExpr::CompoundLit(ty, _, _) => {
            converted_pointer_type(types, *ty)
        }
        CHirExpr::Call(callee, _, _) => converted_pointer_type(
            types,
            call_expression_return_type(symbols, types, callee, target)?,
        ),
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => converted_pointer_type(
            types,
            pointer_expression_type(symbols, types, pointer, target)?.pointee(),
        ),
        CHirExpr::EvaluateOnce(operand, _) => {
            pointer_expression_type(symbols, types, operand, target)
        }
        CHirExpr::Binary(BinaryOp::Add, left, right, _) => {
            pointer_arithmetic_result(symbols, types, left, right, false, target)
        }
        CHirExpr::Binary(BinaryOp::Sub, left, right, _) => {
            pointer_arithmetic_result(symbols, types, left, right, true, target)
        }
        CHirExpr::Binary(BinaryOp::Comma, _, right, _) | CHirExpr::Comma(_, right, _) => {
            pointer_expression_type(symbols, types, right, target)
        }
        CHirExpr::Ternary(_, when_true, when_false, _) => {
            conditional_pointer_type(symbols, types, when_true, when_false, target)
        }
        CHirExpr::Elvis(operand, fallback, _) => {
            conditional_pointer_type(symbols, types, operand, fallback, target)
        }
        CHirExpr::BuiltinVarArgs { facts, .. }
        | CHirExpr::BuiltinVarArgPack { facts, .. }
        | CHirExpr::BuiltinConstantP { facts, .. }
        | CHirExpr::BuiltinObjectSize { facts, .. }
        | CHirExpr::LabelAddress { facts, .. }
        | CHirExpr::BuiltinReturnAddress { facts, .. }
        | CHirExpr::BuiltinFrameAddress { facts, .. } => converted_pointer_type(types, facts.ty()),
        CHirExpr::Member(base, name, _) => {
            // Array members of record values decay too (C11 6.3.2.1p3).
            // Reconstruct the base's record type, not an invented address-of
            // the call/assignment/conditional that produced the value.
            let base = record_expression_type(symbols, types, base, target)?;
            let ty = record_member_path(types, base, name)
                .and_then(|path| path.last().copied())
                .map(FieldProjection::field_ty)
                .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?;
            converted_pointer_type(types, ty)
        }
        CHirExpr::PtrMember(_, _, _) | CHirExpr::Index(_, _, _) => converted_pointer_type(
            types,
            addressed_expression_type(symbols, types, expression, target)?,
        ),
        CHirExpr::Lit(_, _)
        | CHirExpr::FloatLit(_, _)
        | CHirExpr::CharLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Unary(_, _, _)
        | CHirExpr::Binary(_, _, _, _)
        | CHirExpr::BuiltinExpect(_, _, _)
        | CHirExpr::CheckedArithmetic { .. }
        | CHirExpr::BitBuiltin { .. }
        | CHirExpr::BuiltinTrap { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::SizeOf(_, _)
        | CHirExpr::SizeOfExpr(_, _, _)
        | CHirExpr::AlignOf(_, _)
        | CHirExpr::StatementExpr { value: None, .. } => {
            Err(MemoryPlaceError::PointerExpressionTypeUnavailable)
        }
        CHirExpr::StatementExpr { value: Some(_), .. } => {
            unreachable!("statement expressions are validated before the general match")
        }
    }
}

pub(super) fn pointer_expression_matches(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    expected: PointerTypeDesc,
    target: &TargetDataLayout,
) -> bool {
    if pointer_expression_type(symbols, types, expression, target)
        .is_ok_and(|actual| actual == expected)
    {
        return true;
    }
    match expression {
        CHirExpr::StringLit(literal, _) => string_literal_matches(types, literal.prefix, expected),
        CHirExpr::Ternary(_, when_true, when_false, _) => {
            conditional_arm_matches(symbols, types, when_true, expected, target)
                && conditional_arm_matches(symbols, types, when_false, expected, target)
        }
        CHirExpr::Elvis(operand, fallback, _) => {
            conditional_arm_matches(symbols, types, operand, expected, target)
                && conditional_arm_matches(symbols, types, fallback, expected, target)
        }
        _ => false,
    }
}

pub(super) fn conditional_arm_matches(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    expected: PointerTypeDesc,
    target: &TargetDataLayout,
) -> bool {
    is_null_pointer_constant(symbols, types, expression, target)
        || pointer_expression_matches(symbols, types, expression, expected, target)
}

pub(super) fn string_literal_matches(
    types: &TypeInterner,
    prefix: crate::StringPrefix,
    expected: PointerTypeDesc,
) -> bool {
    let Some(pointee) = types.get(expected.pointee()) else {
        return false;
    };
    let matches_literal = matches!(
        (prefix, &pointee.kind),
        (
            crate::StringPrefix::None | crate::StringPrefix::U8,
            TypeKind::Int(
                evocc_types::IntRank::Char
                    | evocc_types::IntRank::SignedPlainChar
                    | evocc_types::IntRank::SChar
                    | evocc_types::IntRank::UChar
            )
        )
    ) || pointee.kind == TypeKind::Int(prefix.element_rank());
    matches_literal && expected.pointee_qualifiers() == pointee.qualifiers
}

pub(super) fn converted_pointer_type(
    types: &TypeInterner,
    ty: TypeId,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    match types.get(ty).map(|canonical| &canonical.kind) {
        Some(TypeKind::Pointer(_)) => PointerTypeDesc::from_canonical_pointer(types, ty),
        Some(TypeKind::Array { element, .. }) => PointerTypeDesc::to_object(types, *element)
            .ok_or(MemoryPlaceError::UnknownType(*element)),
        Some(TypeKind::Function { .. }) => {
            PointerTypeDesc::to_object(types, ty).ok_or(MemoryPlaceError::UnknownType(ty))
        }
        Some(_) => Err(MemoryPlaceError::NotPointerType(ty)),
        None => Err(MemoryPlaceError::UnknownType(ty)),
    }
}

pub(super) fn pointer_arithmetic_result(
    symbols: &SymbolTable,
    types: &TypeInterner,
    left: &CHirExpr<'_>,
    right: &CHirExpr<'_>,
    subtraction: bool,
    target: &TargetDataLayout,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    let left_pointer = pointer_expression_type(symbols, types, left, target);
    let right_pointer = pointer_expression_type(symbols, types, right, target);
    match (left_pointer, right_pointer, subtraction) {
        (Ok(pointer), Err(_), _) | (Err(_), Ok(pointer), false) => Ok(pointer),
        _ => Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    }
}

fn string_literal_pointer(
    types: &TypeInterner,
    literal: &crate::StringLit,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    let rank = literal.prefix.element_rank();
    let ty = types
        .find(&evocc_types::CanonicalType {
            kind: TypeKind::Int(rank),
            qualifiers: Qualifiers::default(),
        })
        .ok_or(MemoryPlaceError::PointerExpressionTypeMismatch)?;
    PointerTypeDesc::to_object(types, ty).ok_or(MemoryPlaceError::PointerExpressionTypeMismatch)
}

/// Whether an expression produces effects but no C value. Calls are resolved
/// through their function type, not through an assumption about the spelling.
#[must_use]
pub fn expression_is_void(
    expression: &CHirExpr<'_>,
    symbols: &SymbolTable,
    types: &TypeInterner,
    target: &evocc_types::TargetDataLayout,
) -> bool {
    let is_void = |ty| {
        types
            .get(ty)
            .is_some_and(|ty| matches!(ty.kind, evocc_types::TypeKind::Void))
    };
    match expression {
        CHirExpr::Cast(ty, _, _) => is_void(*ty),
        CHirExpr::Call(callee, _, _) => {
            PointerTypeDesc::from_expression_with_target(symbols, types, callee, target)
                .ok()
                .and_then(|pointer| types.get(pointer.pointee()))
                .is_some_and(|ty| match ty.kind {
                    evocc_types::TypeKind::Function { return_type, .. } => is_void(return_type),
                    _ => false,
                })
        }
        CHirExpr::Comma(_, right, _) | CHirExpr::EvaluateOnce(right, _) => {
            expression_is_void(right, symbols, types, target)
        }
        CHirExpr::Ternary(_, left, right, _) => {
            expression_is_void(left, symbols, types, target)
                && expression_is_void(right, symbols, types, target)
        }
        CHirExpr::StatementExpr { facts, value, .. } => {
            is_void(facts.ty())
                && value.is_none_or(|value| expression_is_void(value, symbols, types, target))
        }
        CHirExpr::Atomic(access) => is_void(access.facts.ty()),
        CHirExpr::Floating(access) => is_void(access.facts.ty()),
        CHirExpr::Vector(access) => is_void(access.facts.ty()),
        CHirExpr::Complex(access) => is_void(access.facts.ty()),
        CHirExpr::BuiltinVarArgs { facts, .. }
        | CHirExpr::BuiltinTrap { facts, .. }
        | CHirExpr::BuiltinUnreachable { facts, .. } => is_void(facts.ty()),
        _ => false,
    }
}

pub(super) fn conditional_pointer_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    left: &CHirExpr<'_>,
    right: &CHirExpr<'_>,
    target: &TargetDataLayout,
) -> Result<PointerTypeDesc, MemoryPlaceError> {
    let left_pointer = pointer_expression_type(symbols, types, left, target);
    let right_pointer = pointer_expression_type(symbols, types, right, target);
    match (left_pointer, right_pointer) {
        (Ok(_), Ok(right)) if is_null_pointer_constant(symbols, types, left, target) => Ok(right),
        (Ok(left), Ok(_)) if is_null_pointer_constant(symbols, types, right, target) => Ok(left),
        (Ok(left), Ok(right)) => PointerTypeDesc::composite(types, left, right),
        (Ok(left), Err(_)) if is_null_pointer_constant(symbols, types, right, target) => Ok(left),
        (Err(_), Ok(right)) if is_null_pointer_constant(symbols, types, left, target) => Ok(right),
        _ => Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    }
}

pub(super) fn is_null_pointer_constant(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    target: &TargetDataLayout,
) -> bool {
    if eval_integer_constant(expression, types, symbols, target)
        .is_some_and(|value| value.mathematical_value() == 0)
    {
        return true;
    }
    let CHirExpr::Cast(pointer, operand, _) = expression else {
        return false;
    };
    let Some(TypeKind::Pointer(_)) = types.get(*pointer).map(|ty| &ty.kind) else {
        return false;
    };
    eval_integer_constant(operand, types, symbols, target)
        .is_some_and(|value| value.mathematical_value() == 0)
}

pub(crate) fn record_expression_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    target: &TargetDataLayout,
) -> Result<TypeId, MemoryPlaceError> {
    let ty = match expression {
        CHirExpr::Call(callee, ..) => call_expression_return_type(symbols, types, callee, target)?,
        CHirExpr::MemoryAssign { place, facts, .. } => {
            if facts.ty() != place.value_ty() || facts.category() != crate::ExprCategory::Value {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            place.value_ty()
        }
        CHirExpr::Comma(_, right, _) | CHirExpr::EvaluateOnce(right, _) => {
            record_expression_type(symbols, types, right, target)?
        }
        CHirExpr::Ternary(_, left, right, _) => {
            let ty = record_expression_type(symbols, types, left, target)?;
            let other = record_expression_type(symbols, types, right, target)?;
            if !types.compatible_ignoring_top_qualifiers(ty, other) {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            ty
        }
        CHirExpr::Member(base, name, _) => {
            let base = record_expression_type(symbols, types, base, target)?;
            record_member_path(types, base, name)
                .and_then(|path| path.last().copied())
                .map(FieldProjection::field_ty)
                .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?
        }
        CHirExpr::StatementExpr {
            value: Some(value),
            facts,
            ..
        } => {
            let ty = record_expression_type(symbols, types, value, target)?;
            if !types.compatible_ignoring_top_qualifiers(ty, facts.ty()) {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            ty
        }
        _ => addressed_expression_type(symbols, types, expression, target)?,
    };
    if matches!(
        types.get(ty).map(|ty| &ty.kind),
        Some(TypeKind::Struct(_) | TypeKind::Union(_))
    ) {
        Ok(ty)
    } else {
        Err(MemoryPlaceError::PointerExpressionTypeMismatch)
    }
}

/// The bounded addressable forms needed to validate a pointer yielded by a
/// statement expression. More complex projected places arrive as a checked
/// `MemoryRead`; raw projection syntax remains fail-closed here.
pub(super) fn addressed_expression_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    expression: &CHirExpr<'_>,
    target: &TargetDataLayout,
) -> Result<TypeId, MemoryPlaceError> {
    let ty = match expression {
        CHirExpr::StringLit(literal, _) => literal
            .array_type(types)
            .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?,
        CHirExpr::Ident { symbol, facts, .. } => {
            let ty = symbols
                .symbol(*symbol)
                .map_err(|_| MemoryPlaceError::UnknownSymbol(*symbol))?
                .ty();
            if !crate::type_at_use_matches_binding(types, facts.ty(), ty) {
                return Err(MemoryPlaceError::PointerExpressionTypeMismatch);
            }
            ty
        }
        CHirExpr::MemoryRead { place, .. } => place.value_ty(),
        CHirExpr::CompoundLit(ty, _, _) => *ty,
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
            pointer_expression_type(symbols, types, pointer, target)?.pointee()
        }
        CHirExpr::Member(base, name, _) => {
            let base = addressed_expression_type(symbols, types, base, target)?;
            record_member_path(types, base, name)
                .and_then(|path| path.last().copied())
                .map(FieldProjection::field_ty)
                .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?
        }
        CHirExpr::PtrMember(pointer, name, _) => {
            let base = pointer_expression_type(symbols, types, pointer, target)?.pointee();
            record_member_path(types, base, name)
                .and_then(|path| path.last().copied())
                .map(FieldProjection::field_ty)
                .ok_or(MemoryPlaceError::PointerExpressionTypeUnavailable)?
        }
        CHirExpr::Index(left, right, _) => {
            let base = pointer_expression_type(symbols, types, left, target)
                .or_else(|_| pointer_expression_type(symbols, types, right, target))?;
            base.pointee()
        }
        _ => return Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    };
    types
        .get(ty)
        .map(|_| ty)
        .ok_or(MemoryPlaceError::UnknownType(ty))
}

pub(super) fn call_expression_return_type(
    symbols: &SymbolTable,
    types: &TypeInterner,
    callee: &CHirExpr<'_>,
    target: &TargetDataLayout,
) -> Result<TypeId, MemoryPlaceError> {
    let function_ty = pointer_expression_type(symbols, types, callee, target)?.pointee();
    match types.get(function_ty).map(|ty| &ty.kind) {
        Some(TypeKind::Function { return_type, .. }) => Ok(*return_type),
        _ => Err(MemoryPlaceError::PointerExpressionTypeUnavailable),
    }
}
