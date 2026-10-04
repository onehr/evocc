import EvoIR.Core.StateExpression
import EvoIR.Core.SymbolicEntry

/-! Exact source parameter storage for stateful whole-function checking.
Types, const qualifiers, ordered arguments and duplicate IDs are checked. -/
namespace EvoIR.Core.Symbolic

def parameterStore {Γ : Var → Nat} :
    List Source.Parameter → List (CWord Γ) → Option (LocalStore Γ)
  | [], [] => some []
  | p :: ps, v :: vs => do
    if p.type.type != v.type then none else
    let rest ← parameterStore ps vs
    if rest.any (·.symbol == p.symbol) then none
    else some (⟨p.symbol, p.type, some (v.cast p.type.type).term⟩ :: rest)
  | _, _ => none

theorem parameter_store_sound {Γ : Var → Nat} (σ : Subst Γ)
    (ps : List Source.Parameter) (args : List (CWord Γ)) (locals : LocalStore Γ)
    (accepted : parameterStore ps args = some locals) :
    Source.parameterLocals ps (args.map (CWord.denote σ)) = .ok (locals.denote σ) := by
  induction ps generalizing args locals with
  | nil =>
    cases args with
    | nil => cases Option.some.inj accepted; rfl
    | cons _ _ => simp [parameterStore] at accepted
  | cons p ps ih =>
    cases args with
    | nil => simp [parameterStore] at accepted
    | cons v vs =>
      simp only [parameterStore, bind, Option.bind] at accepted
      split at accepted
      · contradiction
      · rename_i same
        have typed : p.type.type = v.type := by simpa using same
        split at accepted
        · contradiction
        · rename_i rest tail
          dsimp only at accepted
          split at accepted
          · contradiction
          · rename_i fresh
            cases Option.some.inj accepted
            have recursive := ih vs rest tail
            have noDuplicate : (rest.denote σ).any (·.symbol == p.symbol) = false := by
              simpa only [LocalStore.denote, List.any_map, Function.comp_def,
                LocalSlot.denote] using Bool.eq_false_iff.mpr fresh
            have value : (v.cast p.type.type).denote σ = v.denote σ := by
              rw [cast_sound, typed]
              exact Source.value_cast_self (v.denote σ)
            simp only [List.map_cons, Source.parameterLocals, CWord.denote,
              typed, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte,
              recursive, bind, Except.bind, noDuplicate, pure, Except.pure]
            change Except.ok (Source.Slot.mk p.symbol p.type (some (v.denote σ)) :: _) =
              Except.ok (Source.Slot.mk p.symbol p.type (some ((v.cast p.type.type).denote σ)) :: _)
            rw [value]
            rfl

def StateTree.erase {Γ : Var → Nat} : StateTree Γ → ValueTree Γ
  | .value _ word => .value word
  | .checked _ guard next => .checked guard next.erase
  | .branch condition yes no => .branch condition yes.erase no.erase

theorem StateTree.erase_sound {Γ : Var → Nat} (σ : Subst Γ) (tree : StateTree Γ) :
    tree.erase.denote σ = (tree.denote σ).2 := by
  induction tree with
  | value locals word => rfl
  | checked locals guard next ih =>
    simp only [erase, ValueTree.denote, StateTree.denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => simpa only [bind, Except.bind] using ih
  | branch condition yes no ih ih' =>
    simp only [erase, ValueTree.denote, StateTree.denote]
    split <;> assumption

end EvoIR.Core.Symbolic
