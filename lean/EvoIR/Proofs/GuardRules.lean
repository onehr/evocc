import EvoIR.GuardRegistry
import EvoIR.Proofs.Rules

namespace EvoIR

theorem sound_g0_001_nonzero : RuleCovered Guards.rule_g0_001_nonzero := by
  rule_known_const Guards.rule_g0_001_nonzero

theorem sound_g0_002_div_rem : RuleCovered Guards.rule_g0_002_div_rem := by
  rule_known_const Guards.rule_g0_002_div_rem

theorem sound_g0_003_add : RuleCovered Guards.rule_g0_003_add := by
  rule_known_const Guards.rule_g0_003_add

theorem sound_g0_004_sub : RuleCovered Guards.rule_g0_004_sub := by
  rule_known_const Guards.rule_g0_004_sub

theorem sound_g0_005_mul : RuleCovered Guards.rule_g0_005_mul := by
  rule_known_const Guards.rule_g0_005_mul

theorem sound_g0_006_unsigned_count : RuleCovered Guards.rule_g0_006_unsigned_count := by
  rule_known_const Guards.rule_g0_006_unsigned_count

theorem sound_g0_007_signed_count : RuleCovered Guards.rule_g0_007_signed_count := by
  rule_known_const Guards.rule_g0_007_signed_count

end EvoIR
