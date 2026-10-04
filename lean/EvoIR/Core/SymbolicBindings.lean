import EvoIR.Core.SymbolicScalar
import EvoIR.Core.IRExecution

/-! Block-local symbolic bindings. Opaque entries carry unexamined parameters
(for example memory/world tokens) through a scalar diagram. They cannot be
consumed as integer operands. The denotation quantifies over their values;
the checker does not replace an opaque value by zero or an assumed integer. -/
namespace EvoIR.Core.Symbolic

inductive Slot (Γ : Var → Nat) where
  | word (value : Word Γ)
  | opaque (parameter : Nat)

def Slot.denote {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value) : Slot Γ → IR.Value
  | .word w => w.denote σ
  | .opaque id => ρ id

abbrev Scope (Γ : Var → Nat) := List (Nat × Slot Γ)

def Scope.denote {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope : Scope Γ) : IR.Bindings :=
  scope.map fun (id, slot) => (id, slot.denote σ ρ)

def Scope.word {Γ : Var → Nat} (scope : Scope Γ) (id : Nat) : Option (Word Γ) :=
  match scope.lookup id with
  | some (.word w) => some w
  | _ => none

theorem lookup_denote {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope : Scope Γ) (id : Nat) :
    (scope.denote σ ρ).lookup id = (scope.lookup id).map (Slot.denote σ ρ) := by
  induction scope with
  | nil => rfl
  | cons entry rest ih =>
    rcases entry with ⟨key, slot⟩
    simp only [Scope.denote, List.map_cons, List.lookup]
    split <;> simp_all [Scope.denote]

theorem word_lookup_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope : Scope Γ) (id : Nat) (w : Word Γ) (found : scope.word id = some w) :
    IR.lookup (scope.denote σ ρ) id = .ok (w.denote σ) := by
  unfold Scope.word at found
  split at found <;> simp_all [IR.lookup, lookup_denote, Slot.denote]

/-- Results are checked against actual IR binding types and all prior IDs,
including opaque entries. Scalar diagrams never silently replace an SSA ID. -/
def Scope.appendWord {Γ : Var → Nat} (scope : Scope Γ) (results : List IR.Binding)
    (w : Word Γ) : Option (Scope Γ) := do
  let [binding] := results | none
  if binding.type != .bits w.1 || (scope.lookup binding.id).isSome then none
  else some ((binding.id, .word w) :: scope)

theorem append_word_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (scope next : Scope Γ) (results : List IR.Binding) (w : Word Γ)
    (accepted : scope.appendWord results w = some next) :
    IR.appendResults (scope.denote σ ρ) results [w.denote σ] =
      .ok (next.denote σ ρ) := by
  cases results with
  | nil => simp [Scope.appendWord] at accepted
  | cons binding rest =>
    cases rest with
    | cons _ _ => simp [Scope.appendWord] at accepted
    | nil =>
      simp only [Scope.appendWord] at accepted
      split at accepted
      · contradiction
      · rename_i fresh
        have type : binding.type = .bits w.1 := by simp_all
        have missing : scope.lookup binding.id = none := by
          cases found : scope.lookup binding.id with
          | none => rfl
          | some _ => simp [found] at fresh
        have missingValues : (scope.denote σ ρ).lookup binding.id = none := by
          rw [lookup_denote, missing]; rfl
        simp only [Option.some.injEq] at accepted
        subst next
        simp [IR.appendResults, IR.bind, Word.denote, IR.Value.type, type, missingValues]
        rfl

end EvoIR.Core.Symbolic
