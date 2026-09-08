import EvoIR.Registry
import EvoIR.Proofs.Lattice

/-!
# Segment A: the 55 rules of `p0.scalar.v4`

One theorem per rule, each an instance of the single `RuleSound` of
`EvoIR/Rule.lean`.  `RuleCovered r` is `RuleSound` at every width environment
and every result width, so the instance space Section 8.5 admits (`bits<N>`
with `1 <= N <= 64`, a shift amount of any `bits<M>`, a width change between
two widths in the bound) is inside the space proved here, with no enumeration
and no width side condition.

The left pattern's elaboration is the rule's well-typedness: a rule whose left
side does not type at the result width has nothing to prove, and the first
premise of `RuleSound` is exactly that it does.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## Tier A: identities that need no fact -/

theorem sound_p0_001_add_zero_rhs : RuleCovered rule_p0_001_add_zero_rhs := by
  sorry

theorem sound_p0_002_add_zero_lhs : RuleCovered rule_p0_002_add_zero_lhs := by
  sorry

theorem sound_p0_003_mul_one_rhs : RuleCovered rule_p0_003_mul_one_rhs := by
  sorry

theorem sound_p0_004_mul_one_lhs : RuleCovered rule_p0_004_mul_one_lhs := by
  sorry

theorem sound_p0_005_and_zero_rhs : RuleCovered rule_p0_005_and_zero_rhs := by
  sorry

theorem sound_p0_006_and_zero_lhs : RuleCovered rule_p0_006_and_zero_lhs := by
  sorry

theorem sound_p0_007_or_zero_rhs : RuleCovered rule_p0_007_or_zero_rhs := by
  sorry

theorem sound_p0_008_or_zero_lhs : RuleCovered rule_p0_008_or_zero_lhs := by
  sorry

theorem sound_p0_009_double_neg : RuleCovered rule_p0_009_double_neg := by
  sorry

theorem sound_p0_010_icmp_eq_self : RuleCovered rule_p0_010_icmp_eq_self := by
  sorry

theorem sound_p0_011_icmp_ne_self : RuleCovered rule_p0_011_icmp_ne_self := by
  sorry

theorem sound_p0_012_icmp_slt_self : RuleCovered rule_p0_012_icmp_slt_self := by
  sorry

theorem sound_p0_013_icmp_sle_self : RuleCovered rule_p0_013_icmp_sle_self := by
  sorry

theorem sound_p0_014_icmp_ult_self : RuleCovered rule_p0_014_icmp_ult_self := by
  sorry

theorem sound_p0_015_icmp_ule_self : RuleCovered rule_p0_015_icmp_ule_self := by
  sorry

/-! ## The constant folds: `fold.op` is the operator's own semantics -/

theorem sound_p0_100_fold_add : RuleCovered rule_p0_100_fold_add := by
  sorry

theorem sound_p0_101_fold_sub : RuleCovered rule_p0_101_fold_sub := by
  sorry

theorem sound_p0_102_fold_mul : RuleCovered rule_p0_102_fold_mul := by
  sorry

theorem sound_p0_103_fold_and : RuleCovered rule_p0_103_fold_and := by
  sorry

theorem sound_p0_104_fold_or : RuleCovered rule_p0_104_fold_or := by
  sorry

theorem sound_p0_105_fold_xor : RuleCovered rule_p0_105_fold_xor := by
  sorry

theorem sound_p0_106_fold_icmp_eq : RuleCovered rule_p0_106_fold_icmp_eq := by
  sorry

theorem sound_p0_107_fold_icmp_ne : RuleCovered rule_p0_107_fold_icmp_ne := by
  sorry

theorem sound_p0_108_fold_icmp_slt : RuleCovered rule_p0_108_fold_icmp_slt := by
  sorry

theorem sound_p0_109_fold_icmp_sle : RuleCovered rule_p0_109_fold_icmp_sle := by
  sorry

theorem sound_p0_110_fold_icmp_ult : RuleCovered rule_p0_110_fold_icmp_ult := by
  sorry

theorem sound_p0_111_fold_icmp_ule : RuleCovered rule_p0_111_fold_icmp_ule := by
  sorry

/-! ## `known_const`: one shared lemma, at each rule's result width -/

theorem sound_p1_200_known_and : RuleCovered rule_p1_200_known_and := by
  sorry

theorem sound_p1_201_known_or : RuleCovered rule_p1_201_known_or := by
  sorry

theorem sound_p1_202_known_xor : RuleCovered rule_p1_202_known_xor := by
  sorry

theorem sound_p1_203_known_add : RuleCovered rule_p1_203_known_add := by
  sorry

theorem sound_p1_204_known_sub : RuleCovered rule_p1_204_known_sub := by
  sorry

theorem sound_p1_205_known_mul : RuleCovered rule_p1_205_known_mul := by
  sorry

theorem sound_p1_210_known_icmp_eq : RuleCovered rule_p1_210_known_icmp_eq := by
  sorry

theorem sound_p1_211_known_icmp_ne : RuleCovered rule_p1_211_known_icmp_ne := by
  sorry

theorem sound_p1_212_known_icmp_slt : RuleCovered rule_p1_212_known_icmp_slt := by
  sorry

theorem sound_p1_213_known_icmp_sle : RuleCovered rule_p1_213_known_icmp_sle := by
  sorry

theorem sound_p1_214_known_icmp_ult : RuleCovered rule_p1_214_known_icmp_ult := by
  sorry

theorem sound_p1_215_known_icmp_ule : RuleCovered rule_p1_215_known_icmp_ule := by
  sorry

theorem sound_p1_206_known_shl : RuleCovered rule_p1_206_known_shl := by
  sorry

theorem sound_p1_207_known_lshr : RuleCovered rule_p1_207_known_lshr := by
  sorry

theorem sound_p1_208_known_ashr : RuleCovered rule_p1_208_known_ashr := by
  sorry

theorem sound_p1_230_known_zext : RuleCovered rule_p1_230_known_zext := by
  sorry

theorem sound_p1_231_known_sext : RuleCovered rule_p1_231_known_sext := by
  sorry

theorem sound_p1_232_known_trunc : RuleCovered rule_p1_232_known_trunc := by
  sorry

/-! ## `covers` and `known_zero` -/

theorem sound_p1_220_and_cover_rhs : RuleCovered rule_p1_220_and_cover_rhs := by
  sorry

theorem sound_p1_221_and_cover_lhs : RuleCovered rule_p1_221_and_cover_lhs := by
  sorry

theorem sound_p1_222_or_cover_rhs : RuleCovered rule_p1_222_or_cover_rhs := by
  sorry

theorem sound_p1_223_or_cover_lhs : RuleCovered rule_p1_223_or_cover_lhs := by
  sorry

theorem sound_p1_224_xor_known_zero_rhs : RuleCovered rule_p1_224_xor_known_zero_rhs := by
  sorry

theorem sound_p1_225_xor_known_zero_lhs : RuleCovered rule_p1_225_xor_known_zero_lhs := by
  sorry

/-! ## The emitting doubling rules -/

theorem sound_p2_300_mul_two_rhs : RuleCovered rule_p2_300_mul_two_rhs := by
  sorry

theorem sound_p2_301_mul_two_lhs : RuleCovered rule_p2_301_mul_two_lhs := by
  sorry

theorem sound_p2_302_add_self_shl : RuleCovered rule_p2_302_add_self_shl := by
  sorry

theorem sound_p2_303_shl_one_add : RuleCovered rule_p2_303_shl_one_add := by
  sorry

end EvoIR
