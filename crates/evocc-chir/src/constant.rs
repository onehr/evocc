//! Conservative front-end constant knowledge shared by every C-HIR consumer.

use evocc_common::SourceSpan;
use evocc_types::{
    enum_integer_rank, TagDefinition, TagState, TargetDataLayout, TypeId, TypeInterner, TypeKind,
};

use crate::integer::{
    eval_binary_integer, eval_bit_builtin, eval_unary_integer, floating_literal_to_integer,
    integer_literal_value, integer_type_for_rank, usual_arithmetic_type, CInteger, CIntegerType,
};
use crate::{BinaryOp, CHirExpr, CHirUnit, FrontEndBuiltinRecord, SymbolKind, SymbolTable};

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
        match record {
            FrontEndBuiltinRecord::Expect { .. } => {}
            FrontEndBuiltinRecord::ConstantP {
                span,
                operand,
                result,
            } => {
                if builtin_constant_p(operand, unit.types(), unit.symbols(), target) != *result {
                    return Err(*span);
                }
            }
            FrontEndBuiltinRecord::ObjectSize {
                span, mode, result, ..
            } => {
                let expected = if *mode <= 1 { u64::MAX } else { 0 };
                if *mode > 3 || *result != expected {
                    return Err(*span);
                }
            }
            FrontEndBuiltinRecord::HasAttribute { .. } => {}
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
        CHirExpr::EvaluateOnce(operand, _) => {
            eval_integer_constant(operand, types, symbols, target)
        }
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
        CHirExpr::Elvis(operand, fallback, _) => {
            let operand_value = eval_integer_constant(operand, types, symbols, target)?;
            let selected = if operand_value.truth() {
                operand_value
            } else {
                eval_integer_constant(fallback, types, symbols, target)?
            };
            let operand_type = CInteger::new(0, integer_expression_type(operand, types, symbols)?)
                .promoted()
                .ty();
            let fallback_type =
                CInteger::new(0, integer_expression_type(fallback, types, symbols)?)
                    .promoted()
                    .ty();
            Some(selected.cast(usual_arithmetic_type(operand_type, fallback_type)))
        }
        CHirExpr::Cast(ty, operand, _) => {
            let target_ty = canonical_integer_type(types, *ty)?;
            if let CHirExpr::FloatLit(literal, _) = operand {
                return floating_literal_to_integer(*literal, target_ty).ok();
            }
            Some(eval_integer_constant(operand, types, symbols, target)?.cast(target_ty))
        }
        CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => Some(CInteger::new(
            target.sizeof_type(types, *ty).ok()?,
            CIntegerType::ULONG,
        )),
        CHirExpr::AlignOf(ty, _) => Some(CInteger::new(
            u64::from(target.layout_of(types, *ty).ok()?.align_bytes),
            CIntegerType::ULONG,
        )),
        CHirExpr::BitBuiltin { op, operand, .. } => {
            eval_bit_builtin(*op, eval_integer_constant(operand, types, symbols, target)?)
        }
        CHirExpr::FloatLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::Call(_, _, _)
        | CHirExpr::CheckedArithmetic { .. }
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinVarArgs { .. }
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
            Some(integer_type_for_rank(enum_integer_rank(
                minimum,
                maximum,
                definition.layout.packed,
            )))
        }
        _ => None,
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
        | CHirExpr::BitBuiltin { facts, .. }
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
        CHirExpr::EvaluateOnce(operand, _) => integer_expression_type(operand, types, symbols),
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
        CHirExpr::Elvis(operand, fallback, _) => {
            let operand = CInteger::new(0, integer_expression_type(operand, types, symbols)?)
                .promoted()
                .ty();
            let fallback = CInteger::new(0, integer_expression_type(fallback, types, symbols)?)
                .promoted()
                .ty();
            Some(usual_arithmetic_type(operand, fallback))
        }
        CHirExpr::Cast(ty, _, _) => canonical_integer_type(types, *ty),
        CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _, _) | CHirExpr::AlignOf(_, _) => {
            Some(CIntegerType::ULONG)
        }
        CHirExpr::Comma(_, right, _) => integer_expression_type(right, types, symbols),
        CHirExpr::FloatLit(_, _)
        | CHirExpr::StringLit(_, _)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::Call(_, _, _)
        | CHirExpr::Member(_, _, _)
        | CHirExpr::PtrMember(_, _, _)
        | CHirExpr::Index(_, _, _)
        | CHirExpr::CompoundLit(_, _, _) => None,
        CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. }
        | CHirExpr::BuiltinVarArgs { .. } => None,
    }
}
