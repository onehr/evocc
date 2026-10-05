import EvoIR.Core.CutTree
import EvoIR.Core.FreshReturn

/-! Path premises are scoped to the actually selected branches of a checked
source/target diagram. They are not producer facts, an EqSat fact domain or
unconditional properties of an SSA value. A fresh call result renames every
saved premise along with the caller's values. -/
namespace EvoIR.Core.Symbolic

abbrev PathConditions (Γ : Var → Nat) := List (Term Γ 1 × Bool)

def PathConditions.Holds {Γ : Var → Nat} (σ : Subst Γ) : PathConditions Γ → Prop
  | [] => True
  | (condition, value) :: rest =>
    ((denoteTerm σ condition).toNat != 0) = value ∧ Holds σ rest

def PathConditions.lookup {Γ : Var → Nat} (condition : Term Γ 1) :
    PathConditions Γ → Option Bool
  | [] => none
  | (test, value) :: rest =>
    if condition = test then some value else lookup condition rest

theorem PathConditions.lookup_sound {Γ : Var → Nat} (σ : Subst Γ)
    (path : PathConditions Γ) (condition : Term Γ 1) (value : Bool)
    (known : path.lookup condition = some value) (holds : path.Holds σ) :
    ((denoteTerm σ condition).toNat != 0) = value := by
  induction path with
  | nil => simp [lookup] at known
  | cons head rest ih =>
    rcases head with ⟨test, selected⟩
    simp only [lookup] at known
    split at known
    · rename_i same
      cases Option.some.inj known
      simpa only [same] using holds.1
    · exact ih known holds.2

def PathConditions.forReturn (path : PathConditions inputWidths) : PathConditions inputWidths :=
  path.map fun (condition, value) => (condition.renameInputs returnName, value)

theorem PathConditions.for_return_sound (σ : Subst inputWidths) (value : Source.Value)
    (path : PathConditions inputWidths) :
    path.forReturn.Holds (returnSubst σ value) ↔ path.Holds σ := by
  induction path with
  | nil => rfl
  | cons head rest ih =>
    rcases head with ⟨condition, selected⟩
    simp only [forReturn, List.map_cons, Holds, Term.rename_inputs_sound]
    have same : (fun v => returnSubst σ value (returnName v)) = σ :=
      funext (return_subst_old σ value)
    rw [same]
    exact and_congr Iff.rfl ih

def CutMatchUnder {Γ : Var → Nat} {α β : Type}
    (related : Subst Γ → α → β → Prop) (path : PathConditions Γ)
    (a : CutTree Γ α) (b : CutTree Γ β) : Prop :=
  ∀ σ, path.Holds σ →
    (∃ fault, a.eval σ = .error fault ∧ b.eval σ = .error fault) ∨
    (∃ x y, a.eval σ = .ok x ∧ b.eval σ = .ok y ∧ related σ x y)

/-- Construction fuel is not an execution bound. A branch may be omitted
only under a premise established by the enclosing matched branch. -/
def cutMatchUnder {Γ : Var → Nat} {α β : Type}
    {related : Subst Γ → α → β → Prop}
    (check : (path : PathConditions Γ) → (a : α) → (b : β) →
      Option (PLift (∀ σ, path.Holds σ → related σ a b))) :
    (budget : Nat) → (path : PathConditions Γ) → (a : CutTree Γ α) → (b : CutTree Γ β) →
    Option (PLift (CutMatchUnder related path a b))
  | 0, _, _, _ => none
  | budget+1, path, a, b =>
    match a with
    | .branch condition yes no =>
      match known : path.lookup condition with
      | some selected => do
        let tail ← cutMatchUnder check budget path (if selected then yes else no) b
        return ⟨by
          intro σ holds
          have truth := path.lookup_sound σ condition selected known holds
          have result := tail.down σ holds
          cases selected <;> simpa only [CutTree.eval, truth, Bool.false_eq_true, ↓reduceIte] using result⟩
      | none => match b with
        | .branch test first second =>
          if same : condition = test then do
            let yesProof ← cutMatchUnder check budget ((condition, true) :: path) yes first
            let noProof ← cutMatchUnder check budget ((condition, false) :: path) no second
            return ⟨by
              intro σ holds
              cases truth : ((denoteTerm σ condition).toNat != 0) with
              | false =>
                have result := noProof.down σ ⟨truth, holds⟩
                simpa only [CutTree.eval, ← same, truth, Bool.false_eq_true, ↓reduceIte] using result
              | true =>
                have result := yesProof.down σ ⟨truth, holds⟩
                simpa only [CutTree.eval, ← same, truth, ↓reduceIte] using result⟩
          else none
        | _ => none
    | sourceRest => match b with
      | .branch condition yes no =>
        match known : path.lookup condition with
        | none => none
        | some selected => do
          let tail ← cutMatchUnder check budget path sourceRest (if selected then yes else no)
          return ⟨by
            intro σ holds
            have truth := path.lookup_sound σ condition selected known holds
            have result := tail.down σ holds
            cases selected <;> simpa only [CutTree.eval, truth, Bool.false_eq_true, ↓reduceIte] using result⟩
      | targetRest => match sourceRest, targetRest with
        | .leaf x, .leaf y => do
          let proof ← check path x y
          return ⟨by intro σ holds; exact .inr ⟨x, y, rfl, rfl, proof.down σ holds⟩⟩
        | .checked ga rest, .checked gb rest' =>
          if same : ga = gb then do
            let tail ← cutMatchUnder check budget path rest rest'
            return ⟨by
              intro σ holds
              cases checked : guards σ [ga] with
              | error fault => exact .inl ⟨fault, by simp [CutTree.eval, checked, bind, Except.bind],
                  by simp [CutTree.eval, ← same, checked, bind, Except.bind]⟩
              | ok u => simpa only [CutTree.eval, ← same, checked, bind, Except.bind]
                  using tail.down σ holds⟩
          else none
        | _, _ => none

end EvoIR.Core.Symbolic
