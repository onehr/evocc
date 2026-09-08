import EvoIR.Registry
import EvoIR.Proofs.Lattice

/-!
# Segment A: the 55 rules of `p0.scalar.v4`

One theorem per rule, each an instance of the single `RuleSound` of
`EvoIR/Rule.lean`.  `RuleCovered r` is `RuleSound` at every width environment
and every result width, so the instance space Section 8.5 admits (`bits<N>`
with `1 <= N <= 64`, a shift amount of `bits<M>` inside the same bound, a
width change between two widths in the bound) is inside the space proved here,
with no enumeration and no width side condition.

The left pattern's elaboration is the rule's well-typedness: a rule whose left
side does not type at the result width has nothing to prove, and the first
premise of `RuleSound` is exactly that it does.  Every proof therefore begins
by splitting on whether the pattern variables have the result width; the
branch where they do not is closed by the premise being false.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## The proof shapes

Six tactics cover the registry.  A rule's shape, not its operator, decides
which one applies: `known_const` is one lemma for eighteen rules because the
lemma reads only the result class's fact, and the folds are definitional
because Section 11.6 gives `fold.op` the operator's own semantics. -/

/-- A rule over one variable, whose left side types exactly when that variable
has the result width. -/
macro "rule_one " r:ident " var " v:str : tactic =>
  `(tactic|
     (intro Γ out σ F R lhs hl hF hR hc
      by_cases hw : Γ $v = out
      case pos =>
        subst hw
        simp_all [$r:ident, denotePat, denoteRhs, elabPat, Pat.width, denoteOp, CondHolds]
      case neg => simp [$r:ident, denotePat, elabPat, Pat.width, hw] at hl))

/-- A comparison against itself: the left side types only at result width one,
and the answer does not depend on the operand width. -/
macro "rule_cmp " r:ident : tactic =>
  `(tactic|
     (intro Γ out σ F R lhs hl hF hR hc
      match out with
      | 0 => simp [$r:ident, denotePat, elabPat, Pat.width] at hl
      | 1 =>
          simp_all [$r:ident, denotePat, denoteRhs, elabPat, Pat.width, denoteOp,
            slt_self, sle_self, ult_self, ule_self]
      | _ + 2 => simp [$r:ident, denotePat, elabPat, Pat.width] at hl))

/-- A constant fold: the two sides elaborate to the same term. -/
macro "rule_fold_bin " r:ident : tactic =>
  `(tactic|
     (intro Γ out σ F R lhs hl hF hR hc
      simp only [$r:ident, denoteRhs, denotePat] at hl ⊢
      rw [elabPat_bin_fold]
      exact hl))

macro "rule_fold_cmp " r:ident : tactic =>
  `(tactic|
     (intro Γ out σ F R lhs hl hF hR hc
      simp only [$r:ident, denoteRhs, denotePat] at hl ⊢
      rw [elabPat_cmp_fold]
      exact hl))

/-- `known_const`: one lemma, at whatever result width the rule has.  It reads
only `R`, which is why the eighteen rules share it and why `R` has to stay an
independent premise rather than the transfer of the operands. -/
macro "rule_known_const " r:ident : tactic =>
  `(tactic|
     (intro Γ out σ F R lhs hl hF hR hc
      simp only [$r:ident, denoteRhs, CondHolds] at hc ⊢
      exact congrArg some (fullyKnownValue R lhs hc hR).symm))

/-! ## Tier A: identities that need no fact -/

theorem sound_p0_001_add_zero_rhs : RuleCovered rule_p0_001_add_zero_rhs := by
  rule_one rule_p0_001_add_zero_rhs var "x"

theorem sound_p0_002_add_zero_lhs : RuleCovered rule_p0_002_add_zero_lhs := by
  rule_one rule_p0_002_add_zero_lhs var "x"

theorem sound_p0_003_mul_one_rhs : RuleCovered rule_p0_003_mul_one_rhs := by
  rule_one rule_p0_003_mul_one_rhs var "x"

theorem sound_p0_004_mul_one_lhs : RuleCovered rule_p0_004_mul_one_lhs := by
  rule_one rule_p0_004_mul_one_lhs var "x"

theorem sound_p0_005_and_zero_rhs : RuleCovered rule_p0_005_and_zero_rhs := by
  rule_one rule_p0_005_and_zero_rhs var "x"

theorem sound_p0_006_and_zero_lhs : RuleCovered rule_p0_006_and_zero_lhs := by
  rule_one rule_p0_006_and_zero_lhs var "x"

theorem sound_p0_007_or_zero_rhs : RuleCovered rule_p0_007_or_zero_rhs := by
  rule_one rule_p0_007_or_zero_rhs var "x"

theorem sound_p0_008_or_zero_lhs : RuleCovered rule_p0_008_or_zero_lhs := by
  rule_one rule_p0_008_or_zero_lhs var "x"

theorem sound_p0_009_double_neg : RuleCovered rule_p0_009_double_neg := by
  rule_one rule_p0_009_double_neg var "x"

theorem sound_p0_010_icmp_eq_self : RuleCovered rule_p0_010_icmp_eq_self := by
  rule_cmp rule_p0_010_icmp_eq_self

theorem sound_p0_011_icmp_ne_self : RuleCovered rule_p0_011_icmp_ne_self := by
  rule_cmp rule_p0_011_icmp_ne_self

theorem sound_p0_012_icmp_slt_self : RuleCovered rule_p0_012_icmp_slt_self := by
  rule_cmp rule_p0_012_icmp_slt_self

theorem sound_p0_013_icmp_sle_self : RuleCovered rule_p0_013_icmp_sle_self := by
  rule_cmp rule_p0_013_icmp_sle_self

theorem sound_p0_014_icmp_ult_self : RuleCovered rule_p0_014_icmp_ult_self := by
  rule_cmp rule_p0_014_icmp_ult_self

theorem sound_p0_015_icmp_ule_self : RuleCovered rule_p0_015_icmp_ule_self := by
  rule_cmp rule_p0_015_icmp_ule_self

/-! ## The constant folds: `fold.op` is the operator's own semantics -/

theorem sound_p0_100_fold_add : RuleCovered rule_p0_100_fold_add := by
  rule_fold_bin rule_p0_100_fold_add

theorem sound_p0_101_fold_sub : RuleCovered rule_p0_101_fold_sub := by
  rule_fold_bin rule_p0_101_fold_sub

theorem sound_p0_102_fold_mul : RuleCovered rule_p0_102_fold_mul := by
  rule_fold_bin rule_p0_102_fold_mul

theorem sound_p0_103_fold_and : RuleCovered rule_p0_103_fold_and := by
  rule_fold_bin rule_p0_103_fold_and

theorem sound_p0_104_fold_or : RuleCovered rule_p0_104_fold_or := by
  rule_fold_bin rule_p0_104_fold_or

theorem sound_p0_105_fold_xor : RuleCovered rule_p0_105_fold_xor := by
  rule_fold_bin rule_p0_105_fold_xor

theorem sound_p0_106_fold_icmp_eq : RuleCovered rule_p0_106_fold_icmp_eq := by
  rule_fold_cmp rule_p0_106_fold_icmp_eq

theorem sound_p0_107_fold_icmp_ne : RuleCovered rule_p0_107_fold_icmp_ne := by
  rule_fold_cmp rule_p0_107_fold_icmp_ne

theorem sound_p0_108_fold_icmp_slt : RuleCovered rule_p0_108_fold_icmp_slt := by
  rule_fold_cmp rule_p0_108_fold_icmp_slt

theorem sound_p0_109_fold_icmp_sle : RuleCovered rule_p0_109_fold_icmp_sle := by
  rule_fold_cmp rule_p0_109_fold_icmp_sle

theorem sound_p0_110_fold_icmp_ult : RuleCovered rule_p0_110_fold_icmp_ult := by
  rule_fold_cmp rule_p0_110_fold_icmp_ult

theorem sound_p0_111_fold_icmp_ule : RuleCovered rule_p0_111_fold_icmp_ule := by
  rule_fold_cmp rule_p0_111_fold_icmp_ule

/-! ## `known_const`: one shared lemma, at each rule's result width -/

theorem sound_p1_200_known_and : RuleCovered rule_p1_200_known_and := by
  rule_known_const rule_p1_200_known_and

theorem sound_p1_201_known_or : RuleCovered rule_p1_201_known_or := by
  rule_known_const rule_p1_201_known_or

theorem sound_p1_202_known_xor : RuleCovered rule_p1_202_known_xor := by
  rule_known_const rule_p1_202_known_xor

theorem sound_p1_203_known_add : RuleCovered rule_p1_203_known_add := by
  rule_known_const rule_p1_203_known_add

theorem sound_p1_204_known_sub : RuleCovered rule_p1_204_known_sub := by
  rule_known_const rule_p1_204_known_sub

theorem sound_p1_205_known_mul : RuleCovered rule_p1_205_known_mul := by
  rule_known_const rule_p1_205_known_mul

theorem sound_p1_210_known_icmp_eq : RuleCovered rule_p1_210_known_icmp_eq := by
  rule_known_const rule_p1_210_known_icmp_eq

theorem sound_p1_211_known_icmp_ne : RuleCovered rule_p1_211_known_icmp_ne := by
  rule_known_const rule_p1_211_known_icmp_ne

theorem sound_p1_212_known_icmp_slt : RuleCovered rule_p1_212_known_icmp_slt := by
  rule_known_const rule_p1_212_known_icmp_slt

theorem sound_p1_213_known_icmp_sle : RuleCovered rule_p1_213_known_icmp_sle := by
  rule_known_const rule_p1_213_known_icmp_sle

theorem sound_p1_214_known_icmp_ult : RuleCovered rule_p1_214_known_icmp_ult := by
  rule_known_const rule_p1_214_known_icmp_ult

theorem sound_p1_215_known_icmp_ule : RuleCovered rule_p1_215_known_icmp_ule := by
  rule_known_const rule_p1_215_known_icmp_ule

theorem sound_p1_206_known_shl : RuleCovered rule_p1_206_known_shl := by
  rule_known_const rule_p1_206_known_shl

theorem sound_p1_207_known_lshr : RuleCovered rule_p1_207_known_lshr := by
  rule_known_const rule_p1_207_known_lshr

theorem sound_p1_208_known_ashr : RuleCovered rule_p1_208_known_ashr := by
  rule_known_const rule_p1_208_known_ashr

theorem sound_p1_230_known_zext : RuleCovered rule_p1_230_known_zext := by
  rule_known_const rule_p1_230_known_zext

theorem sound_p1_231_known_sext : RuleCovered rule_p1_231_known_sext := by
  rule_known_const rule_p1_231_known_sext

theorem sound_p1_232_known_trunc : RuleCovered rule_p1_232_known_trunc := by
  rule_known_const rule_p1_232_known_trunc

/-! ## `covers` and `known_zero`

Two variables, so two width splits: the result width fixes `?x`, and `?m` or
`?z` is only propositionally at that width, which is where the transported
forms of the lattice lemmas come in. -/

theorem sound_p1_220_and_cover_rhs : RuleCovered rule_p1_220_and_cover_rhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p1_220_and_cover_rhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    by_cases hm : Γ "m" = Γ "x"
    case neg => simp [rule_p1_220_and_cover_rhs, denotePat, elabPat, Pat.width, hm] at hl
    case pos =>
      simp only [rule_p1_220_and_cover_rhs, CondHolds, coversAt] at hc
      simp only [rule_p1_220_and_cover_rhs, denotePat, denoteRhs, elabPat, Pat.width,
        denoteTerm, denoteOp, hm, dif_pos, Option.map_some] at hl ⊢
      rw [← hl]
      exact congrArg some
        (coversAndH hm (F "m") (F "x") (σ "m") (σ "x") (hc hm) (hF "m") (hF "x")).symm

theorem sound_p1_221_and_cover_lhs : RuleCovered rule_p1_221_and_cover_lhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p1_221_and_cover_lhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    by_cases hm : Γ "m" = Γ "x"
    case neg => simp [rule_p1_221_and_cover_lhs, denotePat, elabPat, Pat.width, hm] at hl
    case pos =>
      simp only [rule_p1_221_and_cover_lhs, CondHolds, coversAt] at hc
      simp only [rule_p1_221_and_cover_lhs, denotePat, denoteRhs, elabPat, Pat.width,
        denoteTerm, denoteOp, hm, dif_pos, Option.map_some] at hl ⊢
      rw [← hl]
      exact congrArg some
        (coversAndLeftH hm (F "m") (F "x") (σ "m") (σ "x") (hc hm) (hF "m") (hF "x")).symm

theorem sound_p1_222_or_cover_rhs : RuleCovered rule_p1_222_or_cover_rhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p1_222_or_cover_rhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    by_cases hm : Γ "m" = Γ "x"
    case neg => simp [rule_p1_222_or_cover_rhs, denotePat, elabPat, Pat.width, hm] at hl
    case pos =>
      simp only [rule_p1_222_or_cover_rhs, CondHolds, coversAt] at hc
      simp only [rule_p1_222_or_cover_rhs, denotePat, denoteRhs, elabPat, Pat.width,
        denoteTerm, denoteOp, hm, dif_pos, Option.map_some] at hl ⊢
      rw [← hl]
      exact congrArg some
        (coversOrH hm (F "m") (F "x") (σ "m") (σ "x") (hc hm) (hF "m") (hF "x")).symm

theorem sound_p1_223_or_cover_lhs : RuleCovered rule_p1_223_or_cover_lhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p1_223_or_cover_lhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    by_cases hm : Γ "m" = Γ "x"
    case neg => simp [rule_p1_223_or_cover_lhs, denotePat, elabPat, Pat.width, hm] at hl
    case pos =>
      simp only [rule_p1_223_or_cover_lhs, CondHolds, coversAt] at hc
      simp only [rule_p1_223_or_cover_lhs, denotePat, denoteRhs, elabPat, Pat.width,
        denoteTerm, denoteOp, hm, dif_pos, Option.map_some] at hl ⊢
      rw [← hl]
      exact congrArg some
        (coversOrLeftH hm (F "m") (F "x") (σ "m") (σ "x") (hc hm) (hF "m") (hF "x")).symm

theorem sound_p1_224_xor_known_zero_rhs : RuleCovered rule_p1_224_xor_known_zero_rhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p1_224_xor_known_zero_rhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    by_cases hz : Γ "z" = Γ "x"
    case neg => simp [rule_p1_224_xor_known_zero_rhs, denotePat, elabPat, Pat.width, hz] at hl
    case pos =>
      simp only [rule_p1_224_xor_known_zero_rhs, CondHolds] at hc
      simp only [rule_p1_224_xor_known_zero_rhs, denotePat, denoteRhs, elabPat, Pat.width,
        denoteTerm, denoteOp, hz, dif_pos, Option.map_some] at hl ⊢
      rw [← hl, knownZeroValueH hz (F "z") (σ "z") hc (hF "z")]
      simp

theorem sound_p1_225_xor_known_zero_lhs : RuleCovered rule_p1_225_xor_known_zero_lhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p1_225_xor_known_zero_lhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    by_cases hz : Γ "z" = Γ "x"
    case neg => simp [rule_p1_225_xor_known_zero_lhs, denotePat, elabPat, Pat.width, hz] at hl
    case pos =>
      simp only [rule_p1_225_xor_known_zero_lhs, CondHolds] at hc
      simp only [rule_p1_225_xor_known_zero_lhs, denotePat, denoteRhs, elabPat, Pat.width,
        denoteTerm, denoteOp, hz, dif_pos, Option.map_some] at hl ⊢
      rw [← hl, knownZeroValueH hz (F "z") (σ "z") hc (hF "z")]
      simp

/-! ## The emitting doubling rules

`width_min(?x,2)` is the sort of the matched node, which for all four is the
sort of `?x`; the emitted `const_like(?x,1)` shift amount therefore has `?x`'s
own width, and `(1 : bits<N>).toNat` is 1 whenever `N >= 2`. -/

theorem sound_p2_300_mul_two_rhs : RuleCovered rule_p2_300_mul_two_rhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p2_300_mul_two_rhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    simp only [rule_p2_300_mul_two_rhs, CondHolds] at hc
    simp [rule_p2_300_mul_two_rhs, denotePat, denoteRhs, elabPat, Pat.width,
      denoteTerm, denoteOp] at hl ⊢
    rw [← hl, mul_two_eq hc]

theorem sound_p2_301_mul_two_lhs : RuleCovered rule_p2_301_mul_two_lhs := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p2_301_mul_two_lhs, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    simp only [rule_p2_301_mul_two_lhs, CondHolds] at hc
    simp [rule_p2_301_mul_two_lhs, denotePat, denoteRhs, elabPat, Pat.width,
      denoteTerm, denoteOp] at hl ⊢
    rw [← hl, two_mul_eq hc]

theorem sound_p2_302_add_self_shl : RuleCovered rule_p2_302_add_self_shl := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p2_302_add_self_shl, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    simp only [rule_p2_302_add_self_shl, CondHolds] at hc
    simp [rule_p2_302_add_self_shl, denotePat, denoteRhs, elabPat, Pat.width,
      denoteTerm, denoteOp] at hl ⊢
    rw [← hl, shl_one_eq hc]

theorem sound_p2_303_shl_one_add : RuleCovered rule_p2_303_shl_one_add := by
  intro Γ out σ F R lhs hl hF hR hc
  by_cases hx : Γ "x" = out
  case neg => simp [rule_p2_303_shl_one_add, denotePat, elabPat, Pat.width, hx] at hl
  case pos =>
    subst hx
    simp only [rule_p2_303_shl_one_add, CondHolds] at hc
    simp [rule_p2_303_shl_one_add, denotePat, denoteRhs, elabPat, Pat.width,
      denoteTerm, denoteOp] at hl ⊢
    rw [← hl, shl_one_eq hc]

end EvoIR
