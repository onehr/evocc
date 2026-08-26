//! Conservative front-end constant knowledge shared by every C-HIR consumer.

use evocc_common::SourceSpan;
use evocc_types::{TagDefinition, TagState, TargetDataLayout, TypeId, TypeInterner, TypeKind};

use crate::integer::{
    enum_integer_type, eval_binary_integer, eval_unary_integer, floating_literal_to_integer,
    integer_literal_value, integer_type_for_rank, usual_arithmetic_type, CInteger, CIntegerType,
};
use crate::{
    BinaryOp, CHirExpr, CHirUnit, ExprFacts, FloatSuffix, FrontEndBuiltinRecord, StringPrefix,
    SymbolKind, SymbolTable,
};

/// The exact conservative predicate frozen for `__builtin_constant_p`.
///
/// This function deliberately knows only source-language integer constant
/// expressions and string literals. Object values, address constants, calls,
/// side effects, and optimizer facts all fail closed.
#[must_use]
pub fn builtin_constant_p(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> bool {
    matches!(expression, CHirExpr::StringLit(_, _))
        || eval_integer_constant(expression, types, symbols, target).is_some()
}

/// Recompute every retained constant-knowledge decision. Consumers call this
/// before execution or lowering instead of trusting a producer-supplied bit.
pub fn validate_front_end_builtin_records(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
) -> Result<(), SourceSpan> {
    for record in unit.front_end_builtins() {
        if let FrontEndBuiltinRecord::ConstantP {
            span,
            operand,
            result,
        } = record
        {
            if builtin_constant_p(operand, unit.types(), unit.symbols(), target) != *result {
                return Err(*span);
            }
        }
    }
    Ok(())
}

fn eval_integer_constant(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<CInteger> {
    match expression {
        CHirExpr::Lit(literal, _) => integer_literal_value(*literal).ok(),
        CHirExpr::CharLit(literal, _) => {
            let value = i32::try_from(literal.value).ok()?;
            Some(CInteger::int(value))
        }
        CHirExpr::Ident { symbol, .. } => {
            let symbol = symbols.symbol(*symbol).ok()?;
            if symbol.kind() != SymbolKind::Enumerator {
                return None;
            }
            Some(CInteger::new(
                symbol.enumerator_value()? as u64,
                canonical_integer_type(types, symbol.ty())?,
            ))
        }
        CHirExpr::Unary(operator, operand, _) => eval_unary_integer(
            *operator,
            eval_integer_constant(operand, types, symbols, target)?,
        )
        .ok(),
        CHirExpr::Binary(operator, left, right, _) => {
            let left = eval_integer_constant(left, types, symbols, target)?;
            if *operator == BinaryOp::And && !left.truth() {
                return Some(CInteger::int(0));
            }
            if *operator == BinaryOp::Or && left.truth() {
                return Some(CInteger::int(1));
            }
            let right = eval_integer_constant(right, types, symbols, target)?;
            eval_binary_integer(*operator, left, right).ok()
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            let selected = if eval_integer_constant(condition, types, symbols, target)?.truth() {
                when_true
            } else {
                when_false
            };
            eval_integer_constant(selected, types, symbols, target)
        }
        CHirExpr::Cast(ty, operand, _) => {
            let target_ty = canonical_integer_type(types, *ty)?;
            if let CHirExpr::FloatLit(literal, _) = operand {
                return floating_literal_to_integer(*literal, target_ty).ok();
            }
            Some(eval_integer_constant(operand, types, symbols, target)?.cast(target_ty))
        }
        CHirExpr::SizeOf(ty, _) => Some(CInteger::new(
            target.layout_of(types, *ty).ok()?.size_bytes,
            CIntegerType::ULONG,
        )),
        CHirExpr::SizeOfExpr(operand, _) => Some(CInteger::new(
            expression_size(operand, types, symbols, target)?,
            CIntegerType::ULONG,
        )),
        CHirExpr::AlignOf(ty, _) => Some(CInteger::new(
            u64::from(target.layout_of(types, *ty).ok()?.align_bytes),
            CIntegerType::ULONG,
        )),
        CHirExpr::FloatLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Call(_, _, _)
        | CHirExpr::CheckedArithmetic { .. }
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::Assign { .. }
        | CHirExpr::MemoryAssign { .. }
        | CHirExpr::MemoryRead { .. }
        | CHirExpr::Update { .. }
        | CHirExpr::MemoryUpdate { .. }
        | CHirExpr::StatementExpr { .. }
        | CHirExpr::CompoundLit(_, _, _)
        | CHirExpr::Comma(_, _, _) => None,
    }
}

fn canonical_integer_type(types: &TypeInterner, ty: TypeId) -> Option<CIntegerType> {
    match types.get(ty)?.kind {
        TypeKind::Int(rank) => Some(integer_type_for_rank(rank)),
        TypeKind::Enum(tag) => {
            let definition = types.tag(tag)?;
            let TagState::Complete(TagDefinition::Enum(constants)) = &definition.state else {
                return None;
            };
            let minimum = constants
                .iter()
                .map(|constant| constant.value)
                .min()
                .unwrap_or(0);
            let maximum = constants
                .iter()
                .map(|constant| constant.value)
                .max()
                .unwrap_or(0);
            Some(enum_integer_type(minimum, maximum))
        }
        _ => None,
    }
}

fn facts_size(facts: ExprFacts, types: &TypeInterner, target: &TargetDataLayout) -> Option<u64> {
    target
        .layout_of(types, facts.ty())
        .ok()
        .map(|layout| layout.size_bytes)
}

fn expression_size(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> Option<u64> {
    match expression {
        CHirExpr::Lit(literal, _) => {
            Some(u64::from(integer_literal_value(*literal).ok()?.ty().bits()).div_ceil(8))
        }
        CHirExpr::CharLit(_, _) => Some(4),
        CHirExpr::FloatLit(literal, _) => Some(match literal.suffix {
            FloatSuffix::F => 4,
            FloatSuffix::None => 8,
            FloatSuffix::L => 16,
        }),
        CHirExpr::StringLit(literal, _) => {
            let unit_bytes = match literal.prefix {
                StringPrefix::None | StringPrefix::U8 => 1,
                StringPrefix::U16 => 2,
                StringPrefix::L | StringPrefix::U => 4,
            };
            literal
                .explicit_unit_count()?
                .checked_add(1)?
                .checked_mul(unit_bytes)
        }
        CHirExpr::Ident { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::MemoryRead { facts, .. }
        | CHirExpr::CheckedArithmetic { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. }
        | CHirExpr::StatementExpr { facts, .. } => facts_size(*facts, types, target),
        CHirExpr::Cast(ty, _, _) | CHirExpr::CompoundLit(ty, _, _) => target
            .layout_of(types, *ty)
            .ok()
            .map(|layout| layout.size_bytes),
        CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _) | CHirExpr::AlignOf(_, _) => {
            Some(u64::from(CIntegerType::ULONG.bits()).div_ceil(8))
        }
        CHirExpr::Unary(crate::UnaryOp::AddrOf, _, _) => Some(target.pointer_layout().size_bytes),
        CHirExpr::Unary(crate::UnaryOp::Deref, operand, _) => {
            let pointer = expression_type_id(operand)?;
            let TypeKind::Pointer(pointee) = types.get(pointer)?.kind else {
                return None;
            };
            target
                .layout_of(types, pointee)
                .ok()
                .map(|layout| layout.size_bytes)
        }
        CHirExpr::Unary(_, _, _)
        | CHirExpr::Binary(_, _, _, _)
        | CHirExpr::Ternary(_, _, _, _)
        | CHirExpr::Comma(_, _, _) => {
            Some(u64::from(integer_expression_type(expression, types, symbols)?.bits()).div_ceil(8))
        }
        CHirExpr::Call(_, _, _)
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _) => None,
    }
}

fn integer_expression_type(
    expression: &CHirExpr<'_>,
    types: &TypeInterner,
    symbols: &SymbolTable,
) -> Option<CIntegerType> {
    match expression {
        CHirExpr::Lit(literal, _) => Some(integer_literal_value(*literal).ok()?.ty()),
        CHirExpr::CharLit(_, _) => Some(CIntegerType::INT),
        CHirExpr::Ident { symbol, facts, .. } => {
            let symbol = symbols.symbol(*symbol).ok()?;
            canonical_integer_type(types, facts.ty()).or_else(|| {
                (symbol.kind() == SymbolKind::Enumerator)
                    .then(|| canonical_integer_type(types, symbol.ty()))
                    .flatten()
            })
        }
        CHirExpr::MemoryRead { facts, .. }
        | CHirExpr::CheckedArithmetic { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. }
        | CHirExpr::StatementExpr { facts, .. } => canonical_integer_type(types, facts.ty()),
        CHirExpr::Unary(operator, operand, _) => match operator {
            crate::UnaryOp::Not => Some(CIntegerType::INT),
            crate::UnaryOp::Plus | crate::UnaryOp::Neg | crate::UnaryOp::BitNot => Some(
                CInteger::new(0, integer_expression_type(operand, types, symbols)?)
                    .promoted()
                    .ty(),
            ),
            crate::UnaryOp::AddrOf | crate::UnaryOp::Deref => None,
        },
        CHirExpr::Binary(operator, left, right, _) => {
            let left = CInteger::new(0, integer_expression_type(left, types, symbols)?)
                .promoted()
                .ty();
            let right = CInteger::new(0, integer_expression_type(right, types, symbols)?)
                .promoted()
                .ty();
            Some(match operator {
                BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::And
                | BinaryOp::Or => CIntegerType::INT,
                BinaryOp::Shl | BinaryOp::Shr => left,
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Mod
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor => usual_arithmetic_type(left, right),
                BinaryOp::Comma => right,
                BinaryOp::Assign
                | BinaryOp::AddAssign
                | BinaryOp::SubAssign
                | BinaryOp::MulAssign
                | BinaryOp::DivAssign
                | BinaryOp::ModAssign
                | BinaryOp::ShlAssign
                | BinaryOp::ShrAssign
                | BinaryOp::BitAndAssign
                | BinaryOp::BitOrAssign
                | BinaryOp::BitXorAssign => return None,
            })
        }
        CHirExpr::Ternary(_, when_true, when_false, _) => {
            let when_true = CInteger::new(0, integer_expression_type(when_true, types, symbols)?)
                .promoted()
                .ty();
            let when_false = CInteger::new(0, integer_expression_type(when_false, types, symbols)?)
                .promoted()
                .ty();
            Some(usual_arithmetic_type(when_true, when_false))
        }
        CHirExpr::Cast(ty, _, _) => canonical_integer_type(types, *ty),
        CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _) | CHirExpr::AlignOf(_, _) => {
            Some(CIntegerType::ULONG)
        }
        CHirExpr::Comma(_, right, _) => integer_expression_type(right, types, symbols),
        CHirExpr::FloatLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::Call(_, _, _)
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::CompoundLit(_, _, _) => None,
    }
}

fn expression_type_id(expression: &CHirExpr<'_>) -> Option<TypeId> {
    match expression {
        CHirExpr::Ident { facts, .. }
        | CHirExpr::CheckedArithmetic { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::MemoryRead { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. }
        | CHirExpr::StatementExpr { facts, .. } => Some(facts.ty()),
        CHirExpr::Cast(ty, _, _)
        | CHirExpr::SizeOf(ty, _)
        | CHirExpr::AlignOf(ty, _)
        | CHirExpr::CompoundLit(ty, _, _) => Some(*ty),
        _ => None,
    }
}
