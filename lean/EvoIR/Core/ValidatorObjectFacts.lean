import EvoIR.Core.ValidatorScopeFacts

/-! Declaration extends the actual local store with one uninitialized slot.
Its independent checker freshness condition supplies both uniqueness and the
pending const-initializer permission; neither is a runtime success premise. -/
namespace EvoIR.Core.Source

def ValidatorEnvironment.declareObject (environment : ValidatorEnvironment)
    (id : Nat) (destination : QualifiedType) : ValidatorEnvironment :=
  {state := ⟨(id, destination) :: environment.state.bindings, environment.state.initialized⟩,
    layout := (id, destination) :: environment.layout,
    freshConst := if destination.isConst then id :: environment.freshConst else environment.freshConst}

theorem ValidatorEnvironment.Models.no_active_duplicate
    (environment : ValidatorEnvironment) (locals : Locals)
    (models : environment.Models locals) (id : Nat)
    (absent : lookup environment.state id = none) :
    locals.any (·.symbol == id) = false := by
  have noSlot : locals.find? (·.symbol == id) = none :=
    Option.map_eq_none_iff.mp ((models.realizes.bindings id).trans absent)
  simpa only [List.find?_eq_none, List.any_eq_false] using noSlot

theorem ValidatorEnvironment.Models.declareObject
    (environment : ValidatorEnvironment) (locals : Locals)
    (models : environment.Models locals) (id : Nat) (destination : QualifiedType)
    (absent : lookup environment.state id = none) :
    (environment.declareObject id destination).Models (⟨id, destination, none⟩ :: locals) := by
  have noSlot : locals.find? (·.symbol == id) = none :=
    Option.map_eq_none_iff.mp ((models.realizes.bindings id).trans absent)
  have freshSymbol : id ∉ locals.map Slot.symbol := by
    intro member
    obtain ⟨slot, member, same⟩ := List.mem_map.mp member
    have rejected := List.find?_eq_none.mp noSlot slot member
    simp [same] at rejected
  refine ⟨models.realizes.declare environment.state locals id destination absent,
    ?_, ?_, ?_⟩
  · simpa [ValidatorEnvironment.declareObject, localLayout] using
      congrArg (List.cons (id, destination)) models.layout
  · exact List.nodup_cons.mpr ⟨freshSymbol, models.distinct⟩
  · intro other member
    have retained (member : other ∈ environment.freshConst) : ∃ slot,
        ((⟨id, destination, none⟩ : Slot) :: locals).find? (·.symbol == other) = some slot ∧
          slot.type.isConst = true ∧ slot.value = none := by
      obtain ⟨slot, found, isConst, empty⟩ := models.fresh other member
      have different : id ≠ other := by
        intro same
        subst other
        rw [noSlot] at found
        cases found
      exact ⟨slot, by simpa [List.find?, beq_eq_false_iff_ne.mpr different] using found,
        isConst, empty⟩
    change other ∈ (if destination.isConst then id :: environment.freshConst
      else environment.freshConst) at member
    cases const : destination.isConst with
    | false => exact retained (by simpa [const] using member)
    | true =>
      simp only [const, ↓reduceIte, List.mem_cons] at member
      rcases member with rfl | member
      · exact ⟨⟨other, destination, none⟩, by simp [List.find?], const, rfl⟩
      · exact retained member

end EvoIR.Core.Source
