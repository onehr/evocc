import EvoIR.Core.PrivateTargetTypes

/-! A checkpoint rebinding drops only symbolic history. The concrete current
memory and every represented live cell remain the same physical objects. -/
namespace EvoIR.Core.Symbolic

structure PrivateRebinding {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (values newValues : List IR.Value) (before : PrivateConfig Γ) (after : PrivateConfig Δ) where
  shape : after.WellShaped = true
  typed : newValues.map IR.Value.type = after.tokens
  world : privateToken newValues after.world = .world
  memory : after.memoryValue newValues = before.memoryValue values
  scope : after.scope.denote τ (privateToken newValues) = before.scope.denote σ (privateToken values)
  route : PrivateCell Δ → PrivateCell Γ
  member : ∀ cell ∈ after.cells, route cell ∈ before.cells
  injective : ∀ a ∈ after.cells, ∀ b ∈ after.cells,
    a.pointer ≠ b.pointer → (route a).pointer ≠ (route b).pointer
  pointer : ∀ cell ∈ after.cells,
    privateToken newValues cell.pointer = privateToken values (route cell).pointer
  width : ∀ cell ∈ after.cells, cell.width = (route cell).width
  live : ∀ cell ∈ after.cells, cell.live = (route cell).live
  bytes : ∀ cell ∈ after.cells, cell.bytes τ = (route cell).bytes σ

theorem private_rebind_realizes {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (values newValues : List IR.Value) (incoming : IR.Memory) (owner frame : Nat)
    (before : PrivateConfig Γ) (after : PrivateConfig Δ)
    (related : before.Realizes σ values incoming owner frame)
    (rebind : PrivateRebinding σ τ values newValues before after) :
    after.Realizes τ newValues incoming owner frame := by
  obtain ⟨_, _, _, incomingSafe, memory, current, safe, frameBound,
    ownerBound, owned, distinct, cells⟩ := related
  have sameId : ∀ cell ∈ after.cells, cell.id newValues = (rebind.route cell).id values := by
    intro cell member
    simp only [PrivateCell.id, rebind.pointer cell member]
  refine ⟨rebind.shape, rebind.typed, rebind.world, incomingSafe,
    memory, rebind.memory.trans current, safe, frameBound, ownerBound, owned, ?_, ?_⟩
  · intro a am b bm different
    rw [sameId a am, sameId b bm]
    exact distinct (rebind.route a) (rebind.member a am) (rebind.route b)
      (rebind.member b bm) (rebind.injective a am b bm different)
  · intro cell member
    obtain ⟨pointer, cutoff, object, found, identity, owner, live, bytes, aligned⟩ :=
      cells (rebind.route cell) (rebind.member cell member)
    refine ⟨?_, ?_, object, ?_, ?_, owner, ?_, ?_, ?_⟩
    · rw [rebind.pointer cell member, sameId cell member]
      exact pointer
    · simpa only [sameId cell member] using cutoff
    · simpa only [sameId cell member] using found
    · exact identity.trans (sameId cell member).symm
    · exact live.trans (rebind.live cell member).symm
    · exact bytes.trans (rebind.bytes cell member).symm
    · simpa only [rebind.width cell member] using aligned

end EvoIR.Core.Symbolic
