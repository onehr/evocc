use super::*;

impl<'a> Substitution<'a, '_> {
    pub(super) fn pointer_descriptor(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Result<PointerTypeDesc, crate::place::MemoryPlaceError> {
        PointerTypeDesc::from_expression_with_target(
            &self.unit.symbols,
            &self.unit.types,
            expression,
            self.target,
        )
    }
    pub(super) fn validate_memory_place(
        &self,
        place: &MemoryPlace<'_>,
    ) -> Result<(), crate::place::MemoryPlaceError> {
        place.validate_against_with_target(&self.unit.symbols, &self.unit.types, self.target)
    }
    pub(super) fn derive_memory_place(
        &self,
        root: PlaceRoot<'a>,
        projections: Vec<PlaceProjection<'a>>,
        ty: TypeId,
    ) -> Result<MemoryPlace<'a>, crate::place::MemoryPlaceError> {
        MemoryPlace::derive_with_target(
            &self.unit.symbols,
            &self.unit.types,
            root,
            projections,
            ty,
            self.target,
        )
    }
    pub(super) fn integer_expression_type(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<crate::integer::CIntegerType> {
        crate::constant::integer_expression_type_with_target(
            expression,
            &self.unit.types,
            &self.unit.symbols,
            self.target,
        )
    }
}
