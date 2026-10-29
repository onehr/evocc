//! Lowering constant values.

use super::*;

/// The mask of a unit access that selects `bits`, or none when they are
/// every bit of the unit (EvoIR 3.8).
pub(super) fn unit_mask(unit: CIntegerType, bits: u128) -> Option<u128> {
    let all = u128::MAX >> (128 - u32::from(unit.bits()));
    (bits & all != all).then_some(bits & all)
}

pub(super) fn bit_field_unit_integer(bytes: u32) -> CIntegerType {
    match bytes {
        1 => CIntegerType::UCHAR,
        2 => CIntegerType::USHORT,
        4 => CIntegerType::UINT,
        8 => CIntegerType::ULONG,
        16 => CIntegerType::UINT128,
        _ => unreachable!("bit-field storage units have scalar widths"),
    }
}

pub(super) fn bits(integer: CIntegerType) -> EvoIRType {
    EvoIRType::Bits(u16::from(integer.bits()))
}

pub(super) fn conditional_branch(
    condition: Value,
    known_truth: Option<bool>,
    true_target: BlockId,
    true_arguments: Vec<Value>,
    false_target: BlockId,
    false_arguments: Vec<Value>,
) -> Inst {
    match known_truth {
        Some(true) => Inst::Br(true_target, true_arguments),
        Some(false) => Inst::Br(false_target, false_arguments),
        None => Inst::CondBr(
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        ),
    }
}

pub(super) fn constant_truth(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> Option<bool> {
    if unit.runtime_floating_environment() {
        return None;
    }
    if let CHirExpr::Comma(left, right, _) = expression {
        constant_truth(unit, target, left)?;
        return constant_truth(unit, target, right);
    }
    evocc_chir::constant::eval_executable_integer_constant(
        expression,
        unit.types(),
        unit.symbols(),
        target,
    )
    .map(CInteger::truth)
}

pub(super) fn static_null_pointer(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> bool {
    if null_pointer_constant(unit, target, expression) {
        return true;
    }
    match expression {
        CHirExpr::Cast(ty, operand, _)
            if unit
                .types()
                .get(*ty)
                .is_some_and(|ty| matches!(ty.kind, TypeKind::Pointer(_))) =>
        {
            static_null_pointer(unit, target, operand)
        }
        CHirExpr::Ternary(condition, left, right, _) => constant_truth(unit, target, condition)
            .is_some_and(|condition| {
                static_null_pointer(unit, target, if condition { left } else { right })
            }),
        _ => false,
    }
}

pub(super) fn null_pointer_constant(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> bool {
    if constant_integer(unit, target, expression).is_some_and(|value| !value.truth()) {
        return true;
    }
    let CHirExpr::Cast(pointer, operand, _) = expression else {
        return false;
    };
    let Some(TypeKind::Pointer(pointee)) = unit.types().get(*pointer).map(|ty| &ty.kind) else {
        return false;
    };
    if !matches!(
        unit.types().get(*pointee).map(|ty| &ty.kind),
        Some(TypeKind::Void)
    ) {
        return false;
    }
    constant_integer(unit, target, operand).is_some_and(|value| !value.truth())
}

pub(super) fn literal_address<'s, 'a>(
    types: &TypeInterner,
    expression: &'s CHirExpr<'a>,
) -> Option<(TypeId, &'s ObjectInitializer<'a>, u64)> {
    match expression {
        CHirExpr::Unary(UnaryOp::AddrOf, CHirExpr::MemoryRead { place, .. }, _)
            if place.projections().is_empty() =>
        {
            match place.root() {
                PlaceRoot::CompoundLiteral {
                    expression: CHirExpr::CompoundLit(ty, initializer, _),
                    ..
                } => Some((*ty, initializer, 0)),
                _ => None,
            }
        }
        CHirExpr::Cast(ty, operand, _) if matches!(types.get(*ty)?.kind, TypeKind::Pointer(_)) => {
            literal_address(types, operand)
        }
        CHirExpr::Unary(UnaryOp::AddrOf, CHirExpr::CompoundLit(ty, initializer, _), _) => {
            Some((*ty, initializer, 0))
        }
        CHirExpr::CompoundLit(ty, initializer, _)
            if matches!(types.get(*ty)?.kind, TypeKind::Array { .. }) =>
        {
            Some((*ty, initializer, 0))
        }
        _ => None,
    }
}

pub(super) fn constant_integer(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
) -> Option<CInteger> {
    // A comma is allowed by the existing static-address initializer rules,
    // but deliberately is not an ICE or a builtin_constant_p operand.
    if let CHirExpr::Comma(left, right, _) = expression {
        constant_integer(unit, target, left)?;
        return constant_integer(unit, target, right);
    }
    evocc_chir::constant::eval_integer_constant(expression, unit.types(), unit.symbols(), target)
}

pub(super) fn integer_promotion(integer: CIntegerType) -> CIntegerType {
    if integer.rank() < CIntegerType::INT.rank() {
        CIntegerType::INT
    } else {
        integer
    }
}

pub(super) fn arithmetic_common_type(left: CIntegerType, right: CIntegerType) -> CIntegerType {
    usual_arithmetic_type(integer_promotion(left), integer_promotion(right))
}

pub(super) fn contains_targeted_label(
    statement: &CHirStmt<'_>,
    names: &BTreeSet<String>,
    cases: &BTreeMap<usize, String>,
) -> bool {
    if cases.contains_key(&(statement as *const CHirStmt<'_> as usize)) {
        return true;
    }
    match statement {
        CHirStmt::Label(label, nested, _) => {
            names.contains(&label.spelling) || contains_targeted_label(nested, names, cases)
        }
        CHirStmt::If(_, then_branch, else_branch, _) => {
            contains_targeted_label(then_branch, names, cases)
                || else_branch
                    .is_some_and(|branch| contains_targeted_label(branch, names, cases))
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::Default(body, _) => contains_targeted_label(body, names, cases),
        CHirStmt::For(_, _, _, body, _) => contains_targeted_label(body, names, cases),
        CHirStmt::Compound(items, _, _) => items.iter().any(|item| {
            matches!(item, CHirBlockItem::Statement(statement) if contains_targeted_label(statement, names, cases))
        }),
        CHirStmt::Expr(_, _)
        | CHirStmt::Asm(_)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}
