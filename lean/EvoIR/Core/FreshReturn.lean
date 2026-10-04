import EvoIR.Core.SymbolicEntry
import EvoIR.Core.SymbolicStorage

/-! Reserve one independent symbolic return value without capturing any
caller input. Renaming preserves exact typed terms, initializedness, source
locals and IR bindings, including opaque memory/world entries. -/
namespace EvoIR.Core.Symbolic

def Term.renameInputs (name : Var → Var) : {n : Nat} → Term inputWidths n → Term inputWidths n
  | _, .var v n h => .var (name v) n h
  | _, .const n c => .const n c
  | _, .bin op a b => .bin op (a.renameInputs name) (b.renameInputs name)
  | _, .cmp op a b => .cmp op (a.renameInputs name) (b.renameInputs name)
  | _, .shift op a b => .shift op (a.renameInputs name) (b.renameInputs name)
  | _, .cast op n a => .cast op n (a.renameInputs name)
  | _, .guard op a b => .guard op (a.renameInputs name) (b.renameInputs name)
  | _, .nonzero a => .nonzero (a.renameInputs name)
  | _, .count s a b => .count s (a.renameInputs name) (b.renameInputs name)
  | _, .division s r a b => .division s r (a.renameInputs name) (b.renameInputs name)

theorem Term.rename_inputs_sound (name : Var → Var) (σ : Subst inputWidths)
    {n : Nat} (term : Term inputWidths n) :
    denoteTerm σ (term.renameInputs name) = denoteTerm (fun v => σ (name v)) term := by
  induction term <;> simp_all only [renameInputs, denoteTerm]

def returnName (name : Var) : Var := String.ofList ('r' :: name.toList)

def returnSubst (σ : Subst inputWidths) (value : Source.Value) : Subst inputWidths := fun name =>
  match name.toList with
  | [] => value.bits.setWidth 64
  | _ :: rest => σ (String.ofList rest)

theorem return_subst_old (σ : Subst inputWidths) (value : Source.Value) (name : Var) :
    returnSubst σ value (returnName name) = σ name := by
  simp only [returnSubst, returnName, String.toList_ofList]
  dsimp [Subst, inputWidths] at σ
  exact congrArg σ String.ofList_toList

theorem return_subst_fresh (σ : Subst inputWidths) (value : Source.Value) :
    (inputWord value.type 0).denote (returnSubst σ value) = value := by
  rw [inputWord, cast_sound]
  have width : value.type.width ≤ 64 := by cases value.type <;> decide
  have extended := BitVec.toNat_setWidth_of_le (b := value.bits) width
  change Source.Value.ofInt value.type ((value.bits.setWidth 64).toNat : Int) = value
  rw [extended]
  apply congrArg (Source.Value.mk value.type)
  simpa only [BitVec.ofInt_natCast, BitVec.setWidth_eq] using
    BitVec.ofNat_toNat value.type.width value.bits

def LocalStore.forReturn (locals : LocalStore inputWidths) : LocalStore inputWidths :=
  locals.map fun slot => ⟨slot.symbol, slot.type, slot.value.map (Term.renameInputs returnName)⟩

def Scope.forReturn (scope : Scope inputWidths) : Scope inputWidths :=
  scope.map fun (id, slot) => (id, match slot with
    | .word ⟨n, term⟩ => .word ⟨n, term.renameInputs returnName⟩
    | .opaque id => .opaque id)

theorem local_store_for_return (σ : Subst inputWidths) (value : Source.Value)
    (locals : LocalStore inputWidths) :
    locals.forReturn.denote (returnSubst σ value) = locals.denote σ := by
  simp only [LocalStore.forReturn, LocalStore.denote, List.map_map]
  apply List.map_congr_left
  intro slot _
  cases slot with
  | mk id type contents =>
    cases contents <;>
      simp [LocalSlot.denote, Term.rename_inputs_sound, return_subst_old] <;> rfl

theorem scope_for_return (σ : Subst inputWidths) (value : Source.Value)
    (ρ : Nat → IR.Value) (scope : Scope inputWidths) :
    scope.forReturn.denote (returnSubst σ value) ρ = scope.denote σ ρ := by
  simp only [Scope.forReturn, Scope.denote, List.map_map]
  apply List.map_congr_left
  rintro ⟨id, slot⟩ _
  cases slot with
  | «opaque» id => rfl
  | word word => simp [Slot.denote, Word.denote, Term.rename_inputs_sound, return_subst_old]; rfl

end EvoIR.Core.Symbolic
