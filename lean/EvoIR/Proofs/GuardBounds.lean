import EvoIR.Proofs.Bits

/-!
Bounds obtained only from the existing normalized known-bits fact. These are
endpoint calculations, not a new abstract domain or path-condition fact.
-/

namespace EvoIR

theorem bit_subset_nat {n : Nat} (a b : BitVec n)
    (h : ∀ i, a.getLsbD i = true → b.getLsbD i = true) : a.toNat ≤ b.toNat := by
  apply Nat.le_of_testBit
  simpa only [BitVec.testBit_toNat] using h

theorem fact_unsigned_bounds {n : Nat} {K : Fact n} {x : BitVec n}
    (h : gamma K x) : K.value.toNat ≤ x.toNat ∧ x.toNat ≤ K.hi.toNat := by
  constructor
  · have e := congrArg BitVec.toNat h
    simp only [BitVec.toNat_and] at e
    rw [← e]
    exact Nat.and_le_left
  · apply bit_subset_nat
    intro i hx
    by_cases hi : i < n
    · have e := gamma_bit h i
      simp only [Fact.hi, BitVec.getLsbD_or, BitVec.getLsbD_not, hi, decide_true,
        Bool.true_and]
      cases hm : K.mask.getLsbD i <;> simp_all
    · simp [BitVec.getLsbD_of_ge x i (by omega)] at hx

theorem gamma_msb {n : Nat} {K : Fact n} {x : BitVec n} (h : gamma K x) :
    (x.msb && K.mask.msb) = K.value.msb := by
  simpa only [BitVec.msb_and] using congrArg BitVec.msb h

theorem norm_msb {n : Nat} (hn : 0 < n) (K : Fact n) :
    (K.value.msb && !K.mask.msb) = false := by
  simpa only [BitVec.msb_and, BitVec.msb_not, BitVec.msb_zero, hn, decide_true,
    Bool.true_and]
    using congrArg BitVec.msb K.normalized

theorem same_sign_mono {n : Nat} {a b : BitVec n}
    (hs : a.msb = b.msb) (h : a.toNat ≤ b.toNat) : a.toInt ≤ b.toInt := by
  rw [BitVec.toInt_eq_msb_cond, BitVec.toInt_eq_msb_cond, hs]
  cases b.msb <;> simp only [Bool.false_eq_true, if_false, if_true] <;> omega

theorem fact_signed_bounds {n : Nat} (hn : 0 < n) {K : Fact n} {x : BitVec n}
    (h : gamma K x) : K.signedLo.toInt ≤ x.toInt ∧ x.toInt ≤ K.signedHi.toInt := by
  obtain ⟨lo, hi⟩ := fact_unsigned_bounds h
  have em := gamma_msb h
  by_cases hm : K.mask.msb = true
  · have hx : x.msb = K.value.msb := by simpa [hm] using em
    have hh : K.hi.msb = K.value.msb := by simp [Fact.hi, hm]
    simp only [Fact.signedLo, Fact.signedHi, hm, if_true]
    exact ⟨same_sign_mono hx.symm lo, same_sign_mono (hx.trans hh.symm) hi⟩
  · have hm : K.mask.msb = false := Bool.eq_false_iff.mpr hm
    have hl : (K.value ||| BitVec.intMin n).msb = true := by
      simp [BitVec.msb_or, BitVec.msb_intMin, hn]
    have hh : (K.hi &&& ~~~(BitVec.intMin n)).msb = false := by
      simp [BitVec.msb_and, BitVec.msb_not, BitVec.msb_intMin, hn]
    simp only [Fact.signedLo, Fact.signedHi, hm, Bool.false_eq_true, if_false]
    constructor
    · by_cases hx : x.msb = true
      · apply same_sign_mono (hl.trans hx.symm)
        apply bit_subset_nat
        intro i hb
        simp only [BitVec.getLsbD_or, BitVec.getLsbD_intMin, Bool.or_eq_true] at hb
        rcases hb with hb | hb
        · have e := gamma_bit h i
          rw [hb] at e
          simp at e
          exact e.1
        · have he : i = n - 1 := by simp only [decide_eq_true_eq] at hb; omega
          simpa [BitVec.msb_eq_getLsbD_last, he] using hx
      · have hx : x.msb = false := Bool.eq_false_iff.mpr hx
        have := BitVec.toInt_neg_of_msb_true hl
        have := BitVec.toInt_nonneg_of_msb_false hx
        omega
    · by_cases hx : x.msb = false
      · apply same_sign_mono (hx.trans hh.symm)
        apply bit_subset_nat
        intro i hb
        by_cases hin : i < n
        · have e := gamma_bit h i
          have hs : i + 1 ≠ n := by
            intro he
            have he' : i = n - 1 := by omega
            have : x.getLsbD i = false := by
              simpa [BitVec.msb_eq_getLsbD_last, he'] using hx
            simp_all
          simp only [BitVec.getLsbD_and, BitVec.getLsbD_not, hin, decide_true,
            Bool.true_and, BitVec.getLsbD_intMin, hs, decide_false, Bool.not_false,
            Bool.and_true, Fact.hi, BitVec.getLsbD_or]
          cases hm' : K.mask.getLsbD i
          · simp
          · have hv : K.value.getLsbD i = true := by simpa [hb, hm'] using e.symm
            simp [hv]
        · simp [BitVec.getLsbD_of_ge x i (by omega)] at hb
      · have hx : x.msb = true := by cases he : x.msb <;> simp_all
        have := BitVec.toInt_neg_of_msb_true hx
        have := BitVec.toInt_nonneg_of_msb_false hh
        omega

end EvoIR
