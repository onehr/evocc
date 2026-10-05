import EvoIR.Core.BranchExpression

/-! Literal branch decisions in source/target diagrams have two encodings:
C int truth tests and one-bit IR constants. Normalize only that observed
representation difference; do not discharge runtime guards or add EqSat rules. -/
namespace EvoIR.Core.Symbolic

/-- A callee can retest a C int boolean which its caller already widened. -/
def branchCondition {Γ : Var → Nat} (term : Term Γ 1) : Term Γ 1 :=
  match term with
  | .cmp (n := n) .ne (.cast (w := 1) .zext _ condition) (.const _ zero) =>
    if 0 < n ∧ zero = 0 then condition else term
  | _ => term

theorem branch_condition_sound {Γ : Var → Nat} (σ : Subst Γ) (term : Term Γ 1) :
    ((denoteTerm σ (branchCondition term)).toNat != 0) =
      ((denoteTerm σ term).toNat != 0) := by
  unfold branchCondition
  split
  · rename_i n condition zero
    split
    · rename_i admitted
      obtain ⟨positive, rfl⟩ := admitted
      have bound := (denoteTerm σ condition).isLt
      have alternatives : denoteTerm σ condition = 0#1 ∨ denoteTerm σ condition = 1#1 := by
        have : (denoteTerm σ condition).toNat = 0 ∨ (denoteTerm σ condition).toNat = 1 := by omega
        rcases this with h | h
        · exact .inl (BitVec.eq_of_toNat_eq h)
        · exact .inr (BitVec.eq_of_toNat_eq h)
      rcases alternatives with h | h <;>
        simp [denoteTerm, denoteOp, h, BitVec.zeroExtend,
          BitVec.toNat_setWidth_of_le positive, BitVec.toNat_eq]
    · rfl
  · rfl

def constantBranch {Γ : Var → Nat} (term : Term Γ 1) : Option Bool :=
  term.constantValue.map (fun value => value.toNat != 0)

theorem constant_branch_sound {Γ : Var → Nat} (σ : Subst Γ) (condition : Term Γ 1)
    (value : Bool) (accepted : constantBranch condition = some value) :
    ((denoteTerm σ condition).toNat != 0) = value := by
  obtain ⟨bits, known, rfl⟩ := Option.map_eq_some_iff.mp accepted
  rw [condition.constant_value_sound σ bits known]

def ValueTree.normalizeBranches {Γ : Var → Nat} : ValueTree Γ → ValueTree Γ
  | .value word => .value word
  | .checked guard tree => .checked guard tree.normalizeBranches
  | .branch condition yes no => match constantBranch (branchCondition condition) with
    | some true => yes.normalizeBranches
    | some false => no.normalizeBranches
    | none => .branch (branchCondition condition) yes.normalizeBranches no.normalizeBranches

theorem ValueTree.normalize_branches_sound {Γ : Var → Nat} (σ : Subst Γ) (tree : ValueTree Γ) :
    tree.normalizeBranches.denote σ = tree.denote σ := by
  induction tree with
  | value word => rfl
  | checked guard tree ih => simp only [normalizeBranches, denote, ih]
  | branch condition yes no first second =>
    simp only [normalizeBranches]
    cases h : constantBranch (branchCondition condition) with
    | none => simp only [denote, first, second, branch_condition_sound]
    | some value =>
      have same := constant_branch_sound σ (branchCondition condition) value h
      rw [branch_condition_sound] at same
      cases value <;> simp [denote, same, first, second]

theorem normalized_trees_equal {Γ : Var → Nat} (a b : ValueTree Γ)
    (equal : a.normalizeBranches = b.normalizeBranches) (σ : Subst Γ) :
    a.denote σ = b.denote σ := by
  rw [← ValueTree.normalize_branches_sound σ a, equal, ValueTree.normalize_branches_sound]

end EvoIR.Core.Symbolic
