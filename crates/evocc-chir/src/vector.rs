//! Typed GNU vector operations. Vector lanes never undergo scalar promotion.
use crate::{BinaryOp, CHirExpr, ExprFacts, UnaryOp};
use evocc_common::SourceSpan;
use evocc_types::{TargetDataLayout, TypeId, TypeInterner, TypeKind};

/// Check the scalar-to-vector part of a written declaration independently of
/// the parser's applied marker. The surrounding declarator remains unchanged.
#[must_use]
pub fn attribute_matches_base(
    types: &TypeInterner,
    target: &TargetDataLayout,
    attributes: &[crate::Attribute],
    current: TypeId,
    terminal: TypeId,
) -> bool {
    let (Some(vector), Some(base)) = (types.get(current), types.get(terminal)) else {
        return false;
    };
    let TypeKind::Vector {
        element,
        family: evocc_types::VectorFamily::Gnu,
        ..
    } = vector.kind
    else {
        return false;
    };
    vector.qualifiers.compatibility() == base.qualifiers.compatibility()
        && types.compatible_ignoring_top_qualifiers(element, terminal)
        && attributes.iter().any(|attribute| {
            attribute.semantic_name() == Some("vector_size")
                && attribute.applied == Some(crate::AppliedAttribute::VectorSize)
                && attribute.arguments.len() == 1
                && attribute.arguments[0].integer.is_some_and(|bytes| {
                    target
                        .layout_of(types, current)
                        .is_ok_and(|layout| layout.size_bytes == bytes)
                })
        })
}

pub fn assignment_binary(op: crate::AssignmentOp) -> Option<BinaryOp> {
    use crate::AssignmentOp::*;
    Some(match op {
        Simple => return None,
        Add => BinaryOp::Add,
        Subtract => BinaryOp::Sub,
        Multiply => BinaryOp::Mul,
        Divide => BinaryOp::Div,
        Remainder => BinaryOp::Mod,
        BitAnd => BinaryOp::BitAnd,
        BitOr => BinaryOp::BitOr,
        BitXor => BinaryOp::BitXor,
        ShiftLeft => BinaryOp::Shl,
        ShiftRight => BinaryOp::Shr,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VectorOp {
    Unary(UnaryOp),
    Binary(BinaryOp),
    Shuffle,
    ShuffleStatic(Vec<i32>),
    Convert,
    Reinterpret,
    Extract,
}

#[derive(Debug, Clone)]
pub struct VectorExpr<'arena> {
    pub op: VectorOp,
    pub operands: Vec<&'arena CHirExpr<'arena>>,
    pub facts: ExprFacts,
    pub span: SourceSpan,
}

#[must_use]
pub fn shape(types: &TypeInterner, ty: TypeId) -> Option<(TypeId, u16)> {
    match types.get(ty)?.kind {
        TypeKind::Vector { element, lanes, .. } => Some((element, lanes)),
        _ => None,
    }
}

/// The lane format and lane count of a GNU vector of `float` or `double`
/// lanes: the floating vectors that execute (L05). `long double` and half
/// lanes and the nominal `arm_neon.h` vectors have no executable lanes.
#[must_use]
pub fn floating(types: &TypeInterner, ty: TypeId) -> Option<(evocc_types::FloatKind, u16)> {
    let TypeKind::Vector {
        element,
        lanes,
        family: evocc_types::VectorFamily::Gnu,
    } = types.get(ty)?.kind
    else {
        return None;
    };
    match types.get(element)?.kind {
        TypeKind::Float(kind @ (evocc_types::FloatKind::F32 | evocc_types::FloatKind::F64)) => {
            Some((kind, lanes))
        }
        _ => None,
    }
}

/// Implicit integer-vector assignment preserves a lane representation even
/// when its signedness differs. This is not declaration type compatibility.
pub fn assignment_compatible(types: &TypeInterner, left: TypeId, right: TypeId) -> bool {
    let (Some((a, an)), Some((b, bn))) = (shape(types, left), shape(types, right)) else {
        return false;
    };
    an == bn
        && crate::constant::canonical_integer_type(types, a)
            .zip(crate::constant::canonical_integer_type(types, b))
            .is_some_and(|(a, b)| a.bits() == b.bits())
}

/// Recover a recorded value type without scalar promotions or array decay.
#[must_use]
pub fn expression_type(types: &TypeInterner, expression: &CHirExpr<'_>) -> Option<TypeId> {
    match expression {
        CHirExpr::Complex(operation) => Some(operation.facts.ty()),
        CHirExpr::Floating(operation) => Some(operation.facts.ty()),
        CHirExpr::Vector(operation) => Some(operation.facts.ty()),
        CHirExpr::Ident { facts, .. }
        | CHirExpr::MemoryRead { facts, .. }
        | CHirExpr::Assign { facts, .. }
        | CHirExpr::MemoryAssign { facts, .. }
        | CHirExpr::Update { facts, .. }
        | CHirExpr::MemoryUpdate { facts, .. }
        | CHirExpr::BuiltinVarArgs { facts, .. }
        | CHirExpr::StatementExpr { facts, .. } => Some(facts.ty()),
        CHirExpr::Cast(ty, _, _) | CHirExpr::CompoundLit(ty, _, _) => Some(*ty),
        CHirExpr::EvaluateOnce(value, _) => expression_type(types, value),
        CHirExpr::Comma(_, value, _) => expression_type(types, value),
        CHirExpr::Ternary(_, value, _, _) | CHirExpr::Elvis(value, _, _) => {
            expression_type(types, value)
        }
        CHirExpr::Member(base, name, _) => {
            let base = expression_type(types, base)?;
            Some(
                crate::record_member_path(types, base, name)?
                    .last()?
                    .field_ty(),
            )
        }
        CHirExpr::PtrMember(base, name, _) => {
            let TypeKind::Pointer(base) = types.get(expression_type(types, base)?)?.kind else {
                return None;
            };
            Some(
                crate::record_member_path(types, base, name)?
                    .last()?
                    .field_ty(),
            )
        }
        CHirExpr::Index(base, _, _) => match types.get(expression_type(types, base)?)?.kind {
            TypeKind::Pointer(element)
            | TypeKind::Array { element, .. }
            | TypeKind::Vector { element, .. } => Some(element),
            _ => None,
        },
        CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
            match types.get(expression_type(types, pointer)?)?.kind {
                TypeKind::Pointer(pointee) => Some(pointee),
                _ => None,
            }
        }
        CHirExpr::Call(callee, _, _) => {
            let mut ty = expression_type(types, callee)?;
            if let TypeKind::Pointer(pointee) = types.get(ty)?.kind {
                ty = pointee;
            }
            match types.get(ty)?.kind {
                TypeKind::Function { return_type, .. } => Some(return_type),
                _ => None,
            }
        }
        _ => None,
    }
}
