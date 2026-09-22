//! Query-only literal memory facts, not source integer constant expressions.

use super::*;
use crate::integer::CInteger;
use evocc_types::IntRank;

impl Substitution<'_, '_> {
    pub(super) fn literal_byte_query(&self, expression: &CHirExpr<'_>) -> Option<CInteger> {
        let (pointer, projections, value_ty, qualifiers) = match expression {
            CHirExpr::MemoryRead { place, .. } => {
                place
                    .validate_against(&self.unit.symbols, &self.unit.types)
                    .ok()?;
                let PlaceRoot::PointerExpression { pointer, .. } = place.root() else {
                    return None;
                };
                (
                    *pointer,
                    place.projections(),
                    place.value_ty(),
                    place.access_qualifiers(),
                )
            }
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                let ty =
                    PointerTypeDesc::from_expression(&self.unit.symbols, &self.unit.types, pointer)
                        .ok()?;
                (*pointer, &[][..], ty.pointee(), ty.pointee_qualifiers())
            }
            _ => return None,
        };
        if qualifiers.is_volatile || qualifiers.is_atomic || !self.query_byte_type(value_ty) {
            return None;
        }
        let (literal, mut offset) = self.literal_query_pointer(pointer, 32)?;
        for projection in projections {
            let PlaceProjection::Index(index) = projection else {
                return None;
            };
            if !self.query_byte_type(index.element_ty()) {
                return None;
            }
            offset = offset.checked_add(self.literal_query_index(index.index())?)?;
        }
        let (bytes, _) = literal.object_bytes(self.target)?;
        let byte = *bytes.get(usize::try_from(offset).ok()?)?;
        let ty = crate::constant::canonical_integer_type(&self.unit.types, value_ty)?;
        Some(CInteger::new(u64::from(byte), ty))
    }

    fn query_byte_type(&self, ty: TypeId) -> bool {
        matches!(
            self.unit.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Int(
                IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
            ))
        )
    }

    fn literal_query_index(&self, expression: &CHirExpr<'_>) -> Option<i128> {
        crate::constant::eval_executable_integer_constant(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        )?
        .try_mathematical_value()
    }

    fn literal_query_pointer<'e>(
        &self,
        expression: &'e CHirExpr<'_>,
        depth: u8,
    ) -> Option<(&'e StringLit, i128)> {
        let depth = depth.checked_sub(1)?;
        match expression {
            CHirExpr::StringLit(literal, _)
                if matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) =>
            {
                Some((literal, 0))
            }
            CHirExpr::Unary(UnaryOp::AddrOf, literal @ CHirExpr::StringLit(..), _) => {
                self.literal_query_pointer(literal, depth)
            }
            CHirExpr::Cast(ty, pointer, _) => {
                let TypeKind::Pointer(pointee) = self.unit.types.get(*ty)?.kind else {
                    return None;
                };
                self.query_byte_type(pointee).then_some(())?;
                self.literal_query_pointer(pointer, depth)
            }
            CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), left, right, _) => {
                let (pointer, index) =
                    if PointerTypeDesc::from_expression(&self.unit.symbols, &self.unit.types, left)
                        .is_ok()
                    {
                        (*left, *right)
                    } else if *op == BinaryOp::Add {
                        (*right, *left)
                    } else {
                        return None;
                    };
                let pointer_type =
                    PointerTypeDesc::from_expression(&self.unit.symbols, &self.unit.types, pointer)
                        .ok()?;
                self.query_byte_type(pointer_type.pointee()).then_some(())?;
                let (literal, offset) = self.literal_query_pointer(pointer, depth)?;
                let index = self.literal_query_index(index)?;
                let offset = if *op == BinaryOp::Add {
                    offset.checked_add(index)?
                } else {
                    offset.checked_sub(index)?
                };
                // A one-past pointer can be formed but must not be read.
                let length = i128::from(literal.explicit_unit_count()?.checked_add(1)?);
                (0..=length).contains(&offset).then_some((literal, offset))
            }
            // No object loads, calls, assignments or speculative evaluation.
            _ => None,
        }
    }
}
