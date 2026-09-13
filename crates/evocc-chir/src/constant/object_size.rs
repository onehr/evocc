//! Unevaluated, declaration-backed object extents. A pointer type alone never
//! proves an allocation; a declared fixed member can prove its own extent.

use evocc_types::{ArrayBound, TargetDataLayout, TypeId, TypeInterner, TypeKind};

use super::{attribute_query_expression_type, eval_integer_constant};
use crate::{
    record_member_path, BinaryOp, CHirExpr, FieldProjection, InitAction, MemoryPlace,
    PlaceProjection, PlaceRoot, PointerTypeDesc, SymbolTable, UnaryOp,
};

#[derive(Clone, Copy, Default)]
struct Bounds {
    offset: i128,
    whole: Option<(i128, u64)>,
    subobject: Option<(i128, u64)>,
}

impl Bounds {
    fn object(size: u64) -> Self {
        Self {
            offset: 0,
            whole: Some((0, size)),
            subobject: Some((0, size)),
        }
    }

    fn remaining(self, mode: u8) -> Option<u64> {
        let (start, size) = if mode & 1 == 0 {
            self.whole
        } else {
            self.subobject
        }?;
        let offset = self.offset.checked_sub(start)?;
        if offset < 0 || offset > i128::from(size) {
            Some(0)
        } else {
            let remaining = size - u64::try_from(offset).ok()?;
            // A cast may give a member a larger declared type, but it cannot
            // enlarge the actual object backing that member's address.
            Some(if mode & 1 != 0 && self.whole.is_some() {
                remaining.min(self.remaining(0)?)
            } else {
                remaining
            })
        }
    }
}

/// Shared by the parser and validating consumers. No C expression is executed
/// and no object is read; unsupported or effectful paths simply stay unknown.
#[must_use]
pub fn builtin_object_size(
    operand: &CHirExpr<'_>,
    mode: u8,
    types: &TypeInterner,
    symbols: &SymbolTable,
    target: &TargetDataLayout,
) -> u64 {
    Query {
        types,
        symbols,
        target,
    }
    .pointer(operand, 0)
    .and_then(|bounds| bounds.remaining(mode))
    .unwrap_or(if mode <= 1 { u64::MAX } else { 0 })
}

struct Query<'a> {
    types: &'a TypeInterner,
    symbols: &'a SymbolTable,
    target: &'a TargetDataLayout,
}

impl Query<'_> {
    fn size(&self, ty: TypeId) -> Option<u64> {
        if matches!(
            self.types.get(ty)?.kind,
            TypeKind::Array {
                bound: ArrayBound::Unbounded | ArrayBound::Variable,
                ..
            }
        ) {
            return None;
        }
        Some(self.target.layout_of(self.types, ty).ok()?.size_bytes)
    }

    fn array(&self, ty: TypeId) -> bool {
        matches!(
            self.types.get(ty).map(|t| &t.kind),
            Some(TypeKind::Array { .. })
        )
    }

    fn pointer(&self, e: &CHirExpr<'_>, depth: u32) -> Option<Bounds> {
        if depth >= 64 {
            return None;
        }
        match e {
            CHirExpr::Ident { facts, .. } if self.array(facts.ty()) => self.lvalue(e, depth + 1),
            CHirExpr::Ident { facts, .. } => {
                let ty = self.types.get(facts.ty())?;
                (matches!(ty.kind, TypeKind::Pointer(_))
                    && !ty.qualifiers.is_volatile
                    && !ty.qualifiers.is_atomic)
                    .then_some(Bounds::default())
            }
            CHirExpr::StringLit(literal, _) => Some(Bounds::object(
                u64::try_from(literal.object_bytes(self.target)?.0.len()).ok()?,
            )),
            CHirExpr::Unary(UnaryOp::AddrOf, value, _) => self.lvalue(value, depth + 1),
            CHirExpr::Cast(ty, value, _)
                if matches!(self.types.get(*ty)?.kind, TypeKind::Pointer(_)) =>
            {
                self.pointer(value, depth + 1)
            }
            CHirExpr::EvaluateOnce(value, _) => self.pointer(value, depth + 1),
            CHirExpr::MemoryRead { place, facts, .. } => {
                let bounds = self.place(place, depth + 1)?;
                if self.array(facts.ty()) {
                    Some(bounds)
                } else if matches!(self.types.get(facts.ty())?.kind, TypeKind::Pointer(_))
                    && !place.access_qualifiers().is_volatile
                    && !place.access_qualifiers().is_atomic
                {
                    Some(Bounds::default())
                } else {
                    None
                }
            }
            CHirExpr::Binary(op @ (BinaryOp::Add | BinaryOp::Sub), a, b, _) => {
                let (pointer, index) =
                    if PointerTypeDesc::from_expression(self.symbols, self.types, a).is_ok() {
                        (*a, *b)
                    } else if *op == BinaryOp::Add {
                        (*b, *a)
                    } else {
                        return None;
                    };
                let ty = PointerTypeDesc::from_expression(self.symbols, self.types, pointer)
                    .ok()?
                    .pointee();
                let count = eval_integer_constant(index, self.types, self.symbols, self.target)?
                    .try_mathematical_value()?;
                let delta = count.checked_mul(i128::from(self.size(ty)?))?;
                let mut bounds = self.pointer(pointer, depth + 1)?;
                bounds.offset = if *op == BinaryOp::Sub {
                    bounds.offset.checked_sub(delta)?
                } else {
                    bounds.offset.checked_add(delta)?
                };
                Some(bounds)
            }
            CHirExpr::Ternary(c, a, b, _) => {
                let c = eval_integer_constant(c, self.types, self.symbols, self.target)?;
                self.pointer(if c.raw_bits() == 0 { b } else { a }, depth + 1)
            }
            CHirExpr::CompoundLit(ty, _, _) if self.array(*ty) => self.lvalue(e, depth + 1),
            CHirExpr::Member(..) | CHirExpr::PtrMember(..) | CHirExpr::Index(..)
                if self.array(attribute_query_expression_type(e, self.types)?) =>
            {
                self.lvalue(e, depth + 1)
            }
            _ => None,
        }
    }

    fn lvalue(&self, e: &CHirExpr<'_>, depth: u32) -> Option<Bounds> {
        if depth >= 64 {
            return None;
        }
        match e {
            CHirExpr::Ident { facts, .. } => Some(Bounds::object(self.size(facts.ty())?)),
            CHirExpr::MemoryRead { place, .. } => self.place(place, depth + 1),
            CHirExpr::StringLit(..) => self.pointer(e, depth + 1),
            CHirExpr::Unary(UnaryOp::Deref, p, _) => self.pointer(p, depth + 1),
            CHirExpr::CompoundLit(ty, initializer, _) => {
                for action in initializer.plan().actions() {
                    match action {
                        InitAction::StoreScalar { value, .. } => {
                            eval_integer_constant(value, self.types, self.symbols, self.target)?;
                        }
                        InitAction::CopyAggregate { .. } => return None,
                        _ => {}
                    }
                }
                Some(Bounds::object(self.size(*ty)?))
            }
            CHirExpr::Member(base, name, _) | CHirExpr::PtrMember(base, name, _) => {
                let indirect = matches!(e, CHirExpr::PtrMember(..));
                let ty = if indirect {
                    PointerTypeDesc::from_expression(self.symbols, self.types, base)
                        .ok()?
                        .pointee()
                } else {
                    attribute_query_expression_type(base, self.types)?
                };
                let mut bounds = if indirect {
                    self.pointer(base, depth + 1)?
                } else {
                    self.lvalue(base, depth + 1)?
                };
                for field in record_member_path(self.types, ty, name)? {
                    bounds = self.field(bounds, field)?;
                }
                Some(bounds)
            }
            _ => None,
        }
    }

    fn field(&self, mut bounds: Bounds, field: FieldProjection) -> Option<Bounds> {
        let record = self.target.record_layout(self.types, field.tag()).ok()?;
        let member = record
            .fields
            .iter()
            .find(|f| f.field_index == field.field_index())?;
        if member.bit_field.is_some() {
            return None;
        }
        bounds.offset = bounds.offset.checked_add(i128::from(member.byte_offset))?;
        bounds.subobject = self
            .size(field.field_ty())
            .map(|size| (bounds.offset, size));
        Some(bounds)
    }

    fn place(&self, place: &MemoryPlace<'_>, depth: u32) -> Option<Bounds> {
        if depth >= 64 || place.is_bit_field() {
            return None;
        }
        place.validate_against(self.symbols, self.types).ok()?;
        let mut bounds = match place.root() {
            PlaceRoot::Object { symbol } => {
                Bounds::object(self.size(self.symbols.symbol(*symbol).ok()?.ty())?)
            }
            PlaceRoot::PointerExpression { pointer, .. } => self.pointer(pointer, depth + 1)?,
            PlaceRoot::CompoundLiteral { expression, .. } => self.lvalue(expression, depth + 1)?,
        };
        for projection in place.projections() {
            match projection {
                PlaceProjection::Field(field) => bounds = self.field(bounds, *field)?,
                PlaceProjection::Index(index) => {
                    let count = eval_integer_constant(
                        index.index(),
                        self.types,
                        self.symbols,
                        self.target,
                    )?
                    .try_mathematical_value()?;
                    bounds.offset = bounds.offset.checked_add(
                        count.checked_mul(i128::from(self.size(index.element_ty())?))?,
                    )?;
                    // GCC and Clang disagree on the surrounding row extent.
                    if self.array(index.element_ty()) {
                        bounds.subobject = None;
                    }
                }
            }
        }
        Some(bounds)
    }
}
