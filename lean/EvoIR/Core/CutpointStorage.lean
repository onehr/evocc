import EvoIR.Core.SourceSpan

/-! Fresh symbolic valuations at loop heads cover the exact incoming store.
Rebinding preserves rank, qualifiers, slot order and initializedness. A dead
source local may remain in this store even when no target parameter carries it.
No branch fact, range assumption or sampled input is introduced. -/
namespace EvoIR.Core.Symbolic

structure LocalShape where
  symbol : Nat
  type : Source.QualifiedType
  initialized : Bool
  deriving DecidableEq

def LocalSlot.shape {Γ : Var → Nat} (slot : LocalSlot Γ) : LocalShape :=
  ⟨slot.symbol, slot.type, slot.value.isSome⟩

def canonicalStore (shape : List LocalShape) : LocalStore inputWidths :=
  shape.mapIdx fun i slot => ⟨slot.symbol, slot.type,
    if slot.initialized then some (inputWord slot.type.type i).term else none⟩

/-- Zero for an uninitialized slot is used only in the unused valuation
coordinate; canonicalStore retains `none`, never a fabricated initialized 0. -/
def storedValue {Γ : Var → Nat} (σ : Subst Γ) (slot : LocalSlot Γ) : Source.Value :=
  ⟨slot.type.type, (slot.value.map (denoteTerm σ)).getD 0⟩

def storeSubst {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ) : Subst inputWidths :=
  inputSubst (locals.map (storedValue σ))

theorem canonical_store_covers {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ) :
    (canonicalStore (locals.map LocalSlot.shape)).denote (storeSubst σ locals) = locals.denote σ := by
  apply List.ext_getElem
  · simp [LocalStore.denote, canonicalStore]
  · intro i leftBound rightBound
    have bound : i < locals.length := by simpa [LocalStore.denote] using rightBound
    have canonicalBound : i < (canonicalStore (locals.map LocalSlot.shape)).length := by
      simpa [LocalStore.denote] using leftBound
    have coordinate := input_word_covers (locals.map (storedValue σ)) i (storedValue σ locals[i])
      (by simp [bound])
    have value : (inputWord locals[i].type.type i).denote (storeSubst σ locals) =
        storedValue σ locals[i] := coordinate
    have slot : (canonicalStore (locals.map LocalSlot.shape))[i]'canonicalBound =
        ⟨locals[i].symbol, locals[i].type,
          if locals[i].value.isSome then some (inputWord locals[i].type.type i).term else none⟩ := by
      unfold canonicalStore
      rw [List.getElem_mapIdx, List.getElem_map]
      rfl
    simp only [LocalStore.denote, List.getElem_map]
    rw [slot]
    simp only [LocalSlot.denote]
    cases initialized : locals[i].value with
    | none => simp
    | some term =>
      simp only [Option.isSome_some, ↓reduceIte, Option.map_some]
      change Source.Slot.mk locals[i].symbol locals[i].type
        (some ((inputWord locals[i].type.type i).denote (storeSubst σ locals))) =
        Source.Slot.mk locals[i].symbol locals[i].type
          (some (Source.Value.mk locals[i].type.type (denoteTerm σ term)))
      rw [value]
      simp [storedValue, initialized]

theorem canonical_store_of_shape {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (shape : List LocalShape) (same : locals.map LocalSlot.shape = shape) :
    ∃ τ, (canonicalStore shape).denote τ = locals.denote σ := by
  exact ⟨storeSubst σ locals, same ▸ canonical_store_covers σ locals⟩

/-- Proposed target binding, source symbol and exact C integer type. The
whole cutpoint checker must compare the complete projected target scope. -/
abbrev LocalLink := Nat × Nat × Source.IntType

def projectLocals {Γ : Var → Nat} (locals : LocalStore Γ) :
    List LocalLink → Option (Scope Γ)
  | [] => some []
  | (id, symbol, type) :: links => do
    let value ← locals.read symbol type
    let rest ← projectLocals locals links
    return (id, .word value.word) :: rest

theorem project_locals_congr {Γ Δ : Var → Nat} (σ : Subst Γ) (τ : Subst Δ)
    (ρ π : Nat → IR.Value) (a : LocalStore Γ) (b : LocalStore Δ)
    (links : List LocalLink) (first : Scope Γ) (second : Scope Δ)
    (left : projectLocals a links = some first) (right : projectLocals b links = some second)
    (same : a.denote σ = b.denote τ) : first.denote σ ρ = second.denote τ π := by
  induction links generalizing first second with
  | nil =>
    simp only [projectLocals, Option.some.injEq] at left right
    subst first; subst second; rfl
  | cons link links ih =>
    rcases link with ⟨id, symbol, type⟩
    cases ar : a.read symbol type with
    | none => simp [projectLocals, ar] at left
    | some av =>
      cases br : b.read symbol type with
      | none => simp [projectLocals, br] at right
      | some bv =>
        cases atail : projectLocals a links with
        | none => simp [projectLocals, ar, atail] at left
        | some ascope =>
          cases btail : projectLocals b links with
          | none => simp [projectLocals, br, btail] at right
          | some bscope =>
            simp only [projectLocals, ar, br, atail, btail, bind, Option.bind, pure,
              Option.some.injEq] at left right
            subst first; subst second
            have ax := (local_read_sound σ a symbol type av ar).1
            have bx := (local_read_sound τ b symbol type bv br).1
            rw [same] at ax
            have value : av.denote σ = bv.denote τ := Except.ok.inj (ax.symm.trans bx)
            have word : av.word.denote σ = bv.word.denote τ := congrArg sourceValueIR value
            have rest := ih ascope bscope atail btail
            simp only [Scope.denote, List.map_cons, Slot.denote]
            rw [word]
            exact congrArg (List.cons _) rest

end EvoIR.Core.Symbolic
