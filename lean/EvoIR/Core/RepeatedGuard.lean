import EvoIR.Core.ConstantBranch

/-! Division has both explicit source obligations and intrinsic EvoIR traps.
Compare their diagnostic diagrams without executing the same immutable guard
twice. This only normalizes proof diagrams: it never edits either input program,
introduces branch facts, or removes a first check. -/
namespace EvoIR.Core.Symbolic

def ValueTree.normalizeChecks {Γ : Var → Nat} (seen : List (Guard Γ)) :
    ValueTree Γ → ValueTree Γ
  | .value word => .value word
  | .checked guard next =>
    if guard ∈ seen then next.normalizeChecks seen
    else .checked guard (next.normalizeChecks (guard :: seen))
  | .branch condition yes no =>
    .branch condition (yes.normalizeChecks seen) (no.normalizeChecks seen)

theorem ValueTree.normalize_checks_sound {Γ : Var → Nat} (σ : Subst Γ)
    (tree : ValueTree Γ) (seen : List (Guard Γ))
    (passed : ∀ g ∈ seen, (denoteTerm σ g.condition).toNat ≠ 0) :
    (tree.normalizeChecks seen).denote σ = tree.denote σ := by
  induction tree generalizing seen with
  | value word => rfl
  | checked guard next ih =>
    simp only [normalizeChecks]
    split
    · rename_i member
      rw [ih seen passed]
      simp [denote, guards, passed guard member, bind, Except.bind]
    · simp only [denote, guards]
      by_cases zero : (denoteTerm σ guard.condition).toNat = 0
      · simp [zero, bind, Except.bind]
      · simp only [beq_iff_eq, zero, ↓reduceIte, bind, Except.bind]
        exact ih (guard :: seen) (by
          intro g member
          rcases List.mem_cons.mp member with same | old
          · subst g; exact zero
          · exact passed g old)
  | branch condition yes no first second =>
    simp only [normalizeChecks, denote]
    split
    · exact first seen passed
    · exact second seen passed

def ValueTree.normalizeTrace {Γ : Var → Nat} (tree : ValueTree Γ) : ValueTree Γ :=
  (tree.normalizeChecks []).normalizeBranches

theorem normalized_traces_equal {Γ : Var → Nat} (a b : ValueTree Γ)
    (equal : a.normalizeTrace = b.normalizeTrace) (σ : Subst Γ) :
    a.denote σ = b.denote σ := by
  have noChecks : ∀ g ∈ ([] : List (Guard Γ)), (denoteTerm σ g.condition).toNat ≠ 0 := by simp
  have same := normalized_trees_equal (a.normalizeChecks []) (b.normalizeChecks []) equal σ
  simpa only [ValueTree.normalize_checks_sound σ a [] noChecks,
    ValueTree.normalize_checks_sound σ b [] noChecks] using same

end EvoIR.Core.Symbolic
