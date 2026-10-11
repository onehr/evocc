import EvoIR.Proofs.Rules

open EvoIR

-- Prove the registered rule, including its width and typing conditions.
theorem add_zero_example : RuleCovered rule_p0_001_add_zero_rhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hw : Γ "x" = out
  · subst hw
    simp_all [rule_p0_001_add_zero_rhs, denotePat, denoteRhs, elabPat,
      denoteOp, CondHolds]
  · simp [rule_p0_001_add_zero_rhs, denotePat, elabPat, hw] at hl

#print axioms add_zero_example
