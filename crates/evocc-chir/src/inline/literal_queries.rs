//! Query-only literal memory facts, not source integer constant expressions.

use super::*;
use crate::integer::CInteger;
use evocc_types::{ArrayBound, IntRank};

#[derive(Clone, Copy)]
pub(super) struct Bytes<'a> {
    literal: &'a StringLit,
    length: u64,
}

impl<'a> Substitution<'a, '_> {
    pub(super) fn initialized_array_bytes(&self, object: &ObjectDecl<'a>) -> Option<Bytes<'a>> {
        let ty = self.unit.types.get(object.ty)?;
        let TypeKind::Array {
            element,
            bound: ArrayBound::Fixed(length),
        } = ty.kind
        else {
            return None;
        };
        let element_ty = self.unit.types.get(element)?;
        if ty.qualifiers.is_volatile
            || ty.qualifiers.is_atomic
            || element_ty.qualifiers.is_volatile
            || element_ty.qualifiers.is_atomic
            || object.asm_label.is_some()
            || !self.query_byte_type(element)
        {
            return None;
        }
        let [InitAction::StoreString { path, literal, .. }] =
            object.initializer.as_ref()?.plan.actions()
        else {
            return None;
        };
        if !path.is_root() || !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
            return None;
        }
        literal.object_bytes(self.target)?;
        Some(Bytes {
            literal: self.alloc(literal.clone()),
            length,
        })
    }

    pub(super) fn immutable_array(&self, ty: TypeId) -> bool {
        self.unit.types.get(ty).is_some_and(|ty| match ty.kind {
            TypeKind::Array { element, .. } => {
                self.unit.types.get(element).is_some_and(|element| {
                    (ty.qualifiers.is_const || element.qualifiers.is_const)
                        && !element.qualifiers.is_volatile
                        && !element.qualifiers.is_atomic
                })
            }
            _ => false,
        })
    }

    pub(super) fn literal_byte_query(
        &self,
        expression: &CHirExpr<'a>,
        c: &Context<'a>,
    ) -> Option<CInteger> {
        let (pointer, projections, value_ty, qualifiers) = match expression {
            CHirExpr::MemoryRead { place, .. } => {
                place
                    .validate_against(&self.unit.symbols, &self.unit.types)
                    .ok()?;
                let pointer = match place.root() {
                    PlaceRoot::PointerExpression { pointer, .. } => *pointer,
                    PlaceRoot::Object { symbol } => self.ident(*symbol, expression.span()),
                    _ => return None,
                };
                (
                    pointer,
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
        let (data, mut offset) = self.literal_query_pointer(pointer, c, 32)?;
        for projection in projections {
            let PlaceProjection::Index(index) = projection else {
                return None;
            };
            if !self.query_byte_type(index.element_ty()) {
                return None;
            }
            offset = offset.checked_add(self.literal_query_index(index.index())?)?;
        }
        let offset = u64::try_from(offset).ok()?;
        if offset >= data.length {
            return None;
        }
        let (bytes, _) = data.literal.object_bytes(self.target)?;
        // A string initializer zero-fills its array tail; no padding or
        // uninitialized byte is inferred from an arbitrary object initializer.
        let byte = bytes
            .get(usize::try_from(offset).ok()?)
            .copied()
            .unwrap_or(0);
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
        expression: &'e CHirExpr<'a>,
        c: &Context<'a>,
        depth: u8,
    ) -> Option<(Bytes<'e>, i128)>
    where
        'a: 'e,
    {
        let depth = depth.checked_sub(1)?;
        match expression {
            CHirExpr::StringLit(literal, _)
                if matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) =>
            {
                Some((
                    Bytes {
                        literal,
                        length: literal.explicit_unit_count()?.checked_add(1)?,
                    },
                    0,
                ))
            }
            CHirExpr::Ident { symbol, .. } => c
                .byte_objects
                .get(symbol)
                .or_else(|| self.static_byte_objects.get(symbol))
                .copied()
                .map(|data| (data, 0)),
            CHirExpr::Unary(UnaryOp::AddrOf, literal @ CHirExpr::StringLit(..), _) => {
                self.literal_query_pointer(literal, c, depth)
            }
            CHirExpr::Cast(ty, pointer, _) => {
                let TypeKind::Pointer(pointee) = self.unit.types.get(*ty)?.kind else {
                    return None;
                };
                self.query_byte_type(pointee).then_some(())?;
                self.literal_query_pointer(pointer, c, depth)
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
                let (data, offset) = self.literal_query_pointer(pointer, c, depth)?;
                let index = self.literal_query_index(index)?;
                let offset = if *op == BinaryOp::Add {
                    offset.checked_add(index)?
                } else {
                    offset.checked_sub(index)?
                };
                // A one-past pointer can be formed but must not be read.
                let length = i128::from(data.length);
                (0..=length).contains(&offset).then_some((data, offset))
            }
            // No object loads, calls, assignments or speculative evaluation.
            _ => None,
        }
    }
}
