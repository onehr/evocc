import EvoIR.Proofs.GuardRules

/-!
The rule tables of the rewrite checker. Each entry carries its `RuleCovered`
theorem from `EvoIR/Proofs/Rules.lean` or `EvoIR/Proofs/GuardRules.lean`.
The two `rfl` theorems below state that the tables are exactly the frozen
registries, in their order. A certificate names a registry by name and
digest. The narrower `p0` revisions are prefixes of `p0.scalar.v4`.
-/

namespace EvoIR.Rewrite

abbrev CoveredRule := {r : Rule // RuleCovered r}

def p0Covered : List CoveredRule :=
  [ ⟨rule_p0_001_add_zero_rhs, sound_p0_001_add_zero_rhs⟩
  , ⟨rule_p0_002_add_zero_lhs, sound_p0_002_add_zero_lhs⟩
  , ⟨rule_p0_003_mul_one_rhs, sound_p0_003_mul_one_rhs⟩
  , ⟨rule_p0_004_mul_one_lhs, sound_p0_004_mul_one_lhs⟩
  , ⟨rule_p0_005_and_zero_rhs, sound_p0_005_and_zero_rhs⟩
  , ⟨rule_p0_006_and_zero_lhs, sound_p0_006_and_zero_lhs⟩
  , ⟨rule_p0_007_or_zero_rhs, sound_p0_007_or_zero_rhs⟩
  , ⟨rule_p0_008_or_zero_lhs, sound_p0_008_or_zero_lhs⟩
  , ⟨rule_p0_009_double_neg, sound_p0_009_double_neg⟩
  , ⟨rule_p0_010_icmp_eq_self, sound_p0_010_icmp_eq_self⟩
  , ⟨rule_p0_011_icmp_ne_self, sound_p0_011_icmp_ne_self⟩
  , ⟨rule_p0_012_icmp_slt_self, sound_p0_012_icmp_slt_self⟩
  , ⟨rule_p0_013_icmp_sle_self, sound_p0_013_icmp_sle_self⟩
  , ⟨rule_p0_014_icmp_ult_self, sound_p0_014_icmp_ult_self⟩
  , ⟨rule_p0_015_icmp_ule_self, sound_p0_015_icmp_ule_self⟩
  , ⟨rule_p0_100_fold_add, sound_p0_100_fold_add⟩
  , ⟨rule_p0_101_fold_sub, sound_p0_101_fold_sub⟩
  , ⟨rule_p0_102_fold_mul, sound_p0_102_fold_mul⟩
  , ⟨rule_p0_103_fold_and, sound_p0_103_fold_and⟩
  , ⟨rule_p0_104_fold_or, sound_p0_104_fold_or⟩
  , ⟨rule_p0_105_fold_xor, sound_p0_105_fold_xor⟩
  , ⟨rule_p0_106_fold_icmp_eq, sound_p0_106_fold_icmp_eq⟩
  , ⟨rule_p0_107_fold_icmp_ne, sound_p0_107_fold_icmp_ne⟩
  , ⟨rule_p0_108_fold_icmp_slt, sound_p0_108_fold_icmp_slt⟩
  , ⟨rule_p0_109_fold_icmp_sle, sound_p0_109_fold_icmp_sle⟩
  , ⟨rule_p0_110_fold_icmp_ult, sound_p0_110_fold_icmp_ult⟩
  , ⟨rule_p0_111_fold_icmp_ule, sound_p0_111_fold_icmp_ule⟩
  , ⟨rule_p1_200_known_and, sound_p1_200_known_and⟩
  , ⟨rule_p1_201_known_or, sound_p1_201_known_or⟩
  , ⟨rule_p1_202_known_xor, sound_p1_202_known_xor⟩
  , ⟨rule_p1_203_known_add, sound_p1_203_known_add⟩
  , ⟨rule_p1_204_known_sub, sound_p1_204_known_sub⟩
  , ⟨rule_p1_205_known_mul, sound_p1_205_known_mul⟩
  , ⟨rule_p1_210_known_icmp_eq, sound_p1_210_known_icmp_eq⟩
  , ⟨rule_p1_211_known_icmp_ne, sound_p1_211_known_icmp_ne⟩
  , ⟨rule_p1_212_known_icmp_slt, sound_p1_212_known_icmp_slt⟩
  , ⟨rule_p1_213_known_icmp_sle, sound_p1_213_known_icmp_sle⟩
  , ⟨rule_p1_214_known_icmp_ult, sound_p1_214_known_icmp_ult⟩
  , ⟨rule_p1_215_known_icmp_ule, sound_p1_215_known_icmp_ule⟩
  , ⟨rule_p1_220_and_cover_rhs, sound_p1_220_and_cover_rhs⟩
  , ⟨rule_p1_221_and_cover_lhs, sound_p1_221_and_cover_lhs⟩
  , ⟨rule_p1_222_or_cover_rhs, sound_p1_222_or_cover_rhs⟩
  , ⟨rule_p1_223_or_cover_lhs, sound_p1_223_or_cover_lhs⟩
  , ⟨rule_p1_224_xor_known_zero_rhs, sound_p1_224_xor_known_zero_rhs⟩
  , ⟨rule_p1_225_xor_known_zero_lhs, sound_p1_225_xor_known_zero_lhs⟩
  , ⟨rule_p1_206_known_shl, sound_p1_206_known_shl⟩
  , ⟨rule_p1_207_known_lshr, sound_p1_207_known_lshr⟩
  , ⟨rule_p1_208_known_ashr, sound_p1_208_known_ashr⟩
  , ⟨rule_p1_230_known_zext, sound_p1_230_known_zext⟩
  , ⟨rule_p1_231_known_sext, sound_p1_231_known_sext⟩
  , ⟨rule_p1_232_known_trunc, sound_p1_232_known_trunc⟩
  , ⟨rule_p2_300_mul_two_rhs, sound_p2_300_mul_two_rhs⟩
  , ⟨rule_p2_301_mul_two_lhs, sound_p2_301_mul_two_lhs⟩
  , ⟨rule_p2_302_add_self_shl, sound_p2_302_add_self_shl⟩
  , ⟨rule_p2_303_shl_one_add, sound_p2_303_shl_one_add⟩ ]

def g0Covered : List CoveredRule :=
  [ ⟨Guards.rule_g0_001_nonzero, sound_g0_001_nonzero⟩
  , ⟨Guards.rule_g0_002_div_rem, sound_g0_002_div_rem⟩
  , ⟨Guards.rule_g0_003_add, sound_g0_003_add⟩
  , ⟨Guards.rule_g0_004_sub, sound_g0_004_sub⟩
  , ⟨Guards.rule_g0_005_mul, sound_g0_005_mul⟩
  , ⟨Guards.rule_g0_006_unsigned_count, sound_g0_006_unsigned_count⟩
  , ⟨Guards.rule_g0_007_signed_count, sound_g0_007_signed_count⟩ ]

theorem p0Covered_registry : p0Covered.map Subtype.val = registry := rfl

theorem g0Covered_registry : g0Covered.map Subtype.val = Guards.registry := rfl

/-- The registries a certificate may name, with the rules each one holds.
`p0.scalar.v1` holds the first 27 rules and `p0.scalar.v2` the first 45. -/
def registryRules (name digest : String) : Option (List CoveredRule) :=
  if name == "p0.scalar.v1" &&
      digest == "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9" then
    some (p0Covered.take 27)
  else if name == "p0.scalar.v2" &&
      digest == "c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce" then
    some (p0Covered.take 45)
  else if name == registryPrefixName && digest == registryPrefixDigest then
    some (p0Covered.take registryPrefixCount)
  else if name == registryName && digest == registryDigest then some p0Covered
  else if name == Guards.registryName && digest == Guards.registryDigest then some g0Covered
  else none

def isGuardRegistry (name : String) : Bool := name == Guards.registryName

end EvoIR.Rewrite
