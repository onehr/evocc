import EvoIR.Core.BranchExpression

/-! Literal branch decisions in source/target diagrams have two encodings:
C int truth tests and one-bit IR constants. Normalize only that observed
representation difference; do not discharge runtime guards or add EqSat rules. -/
namespace EvoIR.Core.Symbolic

def constantBranch {Γ : Var → Nat} : Term Γ 1 → Option Bool
  | .const _ value => some (value.toNat != 0)
  | .cmp (n := n) op (.const _ a) (.const _ b) =>
      some ((denoteOp (.cmp op n) (a, b)).toNat != 0)
  | _ => none

theorem constant_branch_sound {Γ : Var → Nat} (σ : Subst Γ) (condition : Term Γ 1)
    (value : Bool) (accepted : constantBranch condition = some value) :
    ((denoteTerm σ condition).toNat != 0) = value := by
  unfold constantBranch at accepted
  split at accepted
  · exact Option.some.inj accepted
  · exact Option.some.inj accepted
  · contradiction

def ValueTree.normalizeBranches {Γ : Var → Nat} : ValueTree Γ → ValueTree Γ
  | .value word => .value word
  | .checked guard tree => .checked guard tree.normalizeBranches
  | .branch condition yes no => match constantBranch condition with
    | some true => yes.normalizeBranches
    | some false => no.normalizeBranches
    | none => .branch condition yes.normalizeBranches no.normalizeBranches

theorem ValueTree.normalize_branches_sound {Γ : Var → Nat} (σ : Subst Γ) (tree : ValueTree Γ) :
    tree.normalizeBranches.denote σ = tree.denote σ := by
  induction tree with
  | value word => rfl
  | checked guard tree ih => simp only [normalizeBranches, denote, ih]
  | branch condition yes no first second =>
    simp only [normalizeBranches]
    cases h : constantBranch condition with
    | none => simp only [denote, first, second]
    | some value =>
      have same := constant_branch_sound σ condition value h
      cases value <;> simp [denote, same, first, second]

theorem normalized_trees_equal {Γ : Var → Nat} (a b : ValueTree Γ)
    (equal : a.normalizeBranches = b.normalizeBranches) (σ : Subst Γ) :
    a.denote σ = b.denote σ := by
  rw [← ValueTree.normalize_branches_sound σ a, equal, ValueTree.normalize_branches_sound]

end EvoIR.Core.Symbolic
