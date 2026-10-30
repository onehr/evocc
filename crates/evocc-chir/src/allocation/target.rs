use super::*;
use crate::MemoryPlace;

impl<'arena> AllocationSiteManifest<'arena> {
    /// Derive the complete manifest without recursively walking host stack.
    ///
    /// The walk covers both written initializers and normalized scalar-plan
    /// expressions. Expression and initializer identities act as
    /// cycle/shared-node guards; declarations and statements instead undergo
    /// occurrence validation, so mounting either node twice is a structural
    /// error. Merged file objects select the initializer-bearing definition,
    /// or the first tentative definition when no initializer exists, then use
    /// that selected anchor's canonical traversal encounter to assign
    /// module-wide order. `FileId` allocation order is never treated as
    /// translation-unit source order.
    pub fn derive(unit: &crate::CHirUnit<'arena>) -> Result<Self, AllocationSiteManifestError> {
        Self::for_target(
            unit,
            &evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
    }

    pub fn for_target(
        unit: &crate::CHirUnit<'arena>,
        target: &evocc_types::TargetDataLayout,
    ) -> Result<Self, AllocationSiteManifestError> {
        Deriver::new(unit, target).derive()
    }
}

impl Deriver<'_, '_> {
    pub(super) fn validate_memory_place(
        &self,
        place: &MemoryPlace<'_>,
    ) -> Result<(), crate::place::MemoryPlaceError> {
        place.validate_against_with_target(self.unit.symbols(), self.unit.types(), self.target)
    }
}
