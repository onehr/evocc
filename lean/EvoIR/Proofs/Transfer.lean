import EvoIR.Proofs.Bits

/-!
# The transfer table of Section 8.10

One theorem per row.  The bitwise rows, the shifts and the width changes are
width parametric, proved one bit at a time; the shift rows are parametric in
the amount's own width `M` as well, so the split of the plan's section 4
(amount fully known or not, and `k.toNat` against `N`) is carried out once
rather than per width.  These proofs use only the kernel.

The carry rows and the comparison rows use `bv_decide`. They are in
`Proofs/TransferCarry.lean`, `Proofs/TransferMul.lean` and
`Proofs/TransferCompare.lean`. The rewrite checker uses only the rows of this
module (`Rewrite/Fact.lean`), so the `evoir-sem` build elaborates no
`bv_decide` proof and compiles none of their certificates.

Section 8.5 admits `bits<N>` with `1 <= N <= 64` for a root or witness and a
shift amount inside the same bound, so every row proved here contains the
instance space the profile admits.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## The constant row -/

/-- 8.10: a `const` node's fact is fully known, and holds of its value. -/
theorem transfer_const : ∀ (n : Nat) (v : BitVec n), gamma (Fact.const v) v := by
  intro n v
  rfl

/-! ## The bitwise rows, width parametric

`and` is known where either input is known zero or both are known one; `or`
where either is known one or both are known zero; `xor` where both are known.
Each is decided one bit at a time, so no width is left out. -/

theorem transfer_and : ∀ (n : Nat), TransferRowSound (Op.bin BinOp.and n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  show gamma (T_and A B) (x &&& y)
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, T_and, Fact.mk']
  have ea := gamma_bit hx i
  have eb := gamma_bit hy i
  have na := norm_bit A hi
  have nb := norm_bit B hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_not, hi, decide_true,
    Bool.true_and]
  revert ea eb na nb
  cases A.mask.getLsbD i <;> cases A.value.getLsbD i <;> cases B.mask.getLsbD i <;>
    cases B.value.getLsbD i <;> cases x.getLsbD i <;> cases y.getLsbD i <;> simp

theorem transfer_or : ∀ (n : Nat), TransferRowSound (Op.bin BinOp.or n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  show gamma (T_or A B) (x ||| y)
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, T_or, Fact.mk']
  have ea := gamma_bit hx i
  have eb := gamma_bit hy i
  have na := norm_bit A hi
  have nb := norm_bit B hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_not, hi, decide_true,
    Bool.true_and]
  revert ea eb na nb
  cases A.mask.getLsbD i <;> cases A.value.getLsbD i <;> cases B.mask.getLsbD i <;>
    cases B.value.getLsbD i <;> cases x.getLsbD i <;> cases y.getLsbD i <;> simp

theorem transfer_xor : ∀ (n : Nat), TransferRowSound (Op.bin BinOp.xor n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  show gamma (T_xor A B) (x ^^^ y)
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, T_xor, Fact.mk']
  have ea := gamma_bit hx i
  have eb := gamma_bit hy i
  have na := norm_bit A hi
  have nb := norm_bit B hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_xor]
  revert ea eb na nb
  cases A.mask.getLsbD i <;> cases A.value.getLsbD i <;> cases B.mask.getLsbD i <;>
    cases B.value.getLsbD i <;> cases x.getLsbD i <;> cases y.getLsbD i <;> simp

/-! ## The shift rows, parametric in the value width and in the amount width

The plan's split: an amount that is not fully known transfers to top, and
soundness is immediate; a fully known amount is its fact's value, so the
comparison against the width is `k.toNat < n` on `Nat` and never a truncation
of `n` into the amount's own width; and only the in-range case does bit
work. -/

theorem transfer_shl : ∀ (n m : Nat), TransferRowSound (Op.shift ShOp.shl n m) := by
  intro n m
  rintro ⟨A, K⟩ ⟨x, k⟩ ⟨hx, hk⟩
  show gamma (T_shl A K) (x <<< k.toNat)
  unfold T_shl
  cases hsa : shiftAmount (n := n) K with
  | none => simp [gamma, Fact.top]
  | some s =>
      simp only [shiftAmount, fullyKnown] at hsa
      split at hsa
      · rename_i hcond
        have hks : k = K.value := by
          simp only [gamma, hcond.1, BitVec.and_allOnes] at hk; exact hk
        have hsv : s = K.value.toNat := by injection hsa with h; omega
        have hslt : s < n := by rw [hsv]; exact hcond.2
        subst hks
        rw [← hsv]
        simp only [gamma, Fact.mk']
        apply BitVec.eq_of_getLsbD_eq
        intro i hi
        simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_shiftLeft,
          lowMask_getLsbD n s i hslt, hi, decide_true, Bool.true_and]
        by_cases hs : i < s
        · simp [hs]
        · simp only [hs, decide_false, Bool.not_false, Bool.true_and, Bool.false_and,
            Bool.or_false]
          have ea := gamma_bit hx (i - s)
          have na := norm_bit A (show i - s < n by omega)
          revert ea na
          cases A.mask.getLsbD (i - s) <;> cases A.value.getLsbD (i - s) <;>
            cases x.getLsbD (i - s) <;> simp
      · exact absurd hsa (by simp)

theorem transfer_lshr : ∀ (n m : Nat), TransferRowSound (Op.shift ShOp.lshr n m) := by
  intro n m
  rintro ⟨A, K⟩ ⟨x, k⟩ ⟨hx, hk⟩
  show gamma (T_lshr A K) (x >>> k.toNat)
  unfold T_lshr
  cases hsa : shiftAmount (n := n) K with
  | none => simp [gamma, Fact.top]
  | some s =>
      simp only [shiftAmount, fullyKnown] at hsa
      split at hsa
      · rename_i hcond
        have hks : k = K.value := by
          simp only [gamma, hcond.1, BitVec.and_allOnes] at hk; exact hk
        have hsv : s = K.value.toNat := by injection hsa with h; omega
        have hslt : s < n := by rw [hsv]; exact hcond.2
        subst hks
        rw [← hsv]
        simp only [gamma, Fact.mk']
        apply BitVec.eq_of_getLsbD_eq
        intro i hi
        simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_ushiftRight,
          BitVec.getLsbD_not, BitVec.getLsbD_allOnes, hi, decide_true, Bool.true_and]
        by_cases hsi : s + i < n
        · simp only [hsi, decide_true, Bool.not_true, Bool.or_false]
          have ea := gamma_bit hx (s + i)
          have na := norm_bit A hsi
          revert ea na
          cases A.mask.getLsbD (s + i) <;> cases A.value.getLsbD (s + i) <;>
            cases x.getLsbD (s + i) <;> simp
        · rw [BitVec.getLsbD_of_ge x (s + i) (by omega),
              BitVec.getLsbD_of_ge A.mask (s + i) (by omega),
              BitVec.getLsbD_of_ge A.value (s + i) (by omega)]
      · exact absurd hsa (by simp)

theorem transfer_ashr : ∀ (n m : Nat), TransferRowSound (Op.shift ShOp.ashr n m) := by
  intro n m
  rintro ⟨A, K⟩ ⟨x, k⟩ ⟨hx, hk⟩
  show gamma (T_ashr A K) (x.sshiftRight k.toNat)
  unfold T_ashr
  cases hsa : shiftAmount (n := n) K with
  | none => simp [gamma, Fact.top]
  | some s =>
      simp only [shiftAmount, fullyKnown] at hsa
      split at hsa
      · rename_i hcond
        have hks : k = K.value := by
          simp only [gamma, hcond.1, BitVec.and_allOnes] at hk; exact hk
        have hsv : s = K.value.toNat := by injection hsa with h; omega
        have hslt : s < n := by rw [hsv]; exact hcond.2
        subst hks
        rw [← hsv]
        simp only [gamma, Fact.mk']
        apply BitVec.eq_of_getLsbD_eq
        intro i hi
        simp only [BitVec.getLsbD_and, BitVec.getLsbD_sshiftRight,
          show ¬ (n ≤ i) by omega, decide_false, Bool.not_false, Bool.true_and]
        by_cases hsi : s + i < n
        · simp only [hsi, if_pos]
          have ea := gamma_bit hx (s + i)
          have na := norm_bit A hsi
          revert ea na
          cases A.mask.getLsbD (s + i) <;> cases A.value.getLsbD (s + i) <;>
            cases x.getLsbD (s + i) <;> simp
        · simp only [hsi, if_neg, if_false, BitVec.msb_eq_getLsbD_last]
          have ea := gamma_bit hx (n - 1)
          have na := norm_bit A (show n - 1 < n by omega)
          revert ea na
          cases A.mask.getLsbD (n - 1) <;> cases A.value.getLsbD (n - 1) <;>
            cases x.getLsbD (n - 1) <;> simp
      · exact absurd hsa (by simp)

/-! ## The width-change rows, parametric in both widths

The soundness argument needs no relation between the two widths: when the
target is narrower the extension truncates, and the row still holds.  Section
3.6 admits only `N >= W` for `zext` and `sext` and `N <= W` for `trunc`, so
these theorems contain that space too. -/

theorem transfer_zext : ∀ (w n : Nat), TransferRowSound (Op.cast CastOp.zext w n) := by
  intro w n
  rintro A x hx
  show gamma (T_zext n A) (BitVec.zeroExtend n x)
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, T_zext, Fact.mk']
  have ea := gamma_bit hx i
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_not, BitVec.getLsbD_setWidth,
    BitVec.getLsbD_allOnes, BitVec.zeroExtend_eq_setWidth, hi, decide_true, Bool.true_and]
  by_cases hiw : i < w
  · simp only [hiw, decide_true, Bool.true_and, Bool.not_true, Bool.or_false]
    have na := norm_bit A hiw
    revert ea na
    cases A.mask.getLsbD i <;> cases A.value.getLsbD i <;> cases x.getLsbD i <;> simp
  · rw [BitVec.getLsbD_of_ge x i (by omega), BitVec.getLsbD_of_ge A.mask i (by omega),
        BitVec.getLsbD_of_ge A.value i (by omega)]

theorem transfer_sext : ∀ (w n : Nat), TransferRowSound (Op.cast CastOp.sext w n) := by
  intro w n
  rintro A x hx
  show gamma (T_sext n A) (BitVec.signExtend n x)
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, T_sext, Fact.mk']
  have ea := gamma_bit hx i
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_signExtend, hi, decide_true, Bool.true_and]
  by_cases hiw : i < w
  · simp only [hiw, if_pos]
    have na := norm_bit A hiw
    revert ea na
    cases A.mask.getLsbD i <;> cases A.value.getLsbD i <;> cases x.getLsbD i <;> simp
  · simp only [hiw, if_neg, if_false]
    by_cases hw : 0 < w
    · have eb := gamma_bit hx (w - 1)
      have nb := norm_bit A (show w - 1 < w by omega)
      simp only [BitVec.msb_eq_getLsbD_last]
      revert eb nb
      cases A.mask.getLsbD (w - 1) <;> cases A.value.getLsbD (w - 1) <;>
        cases x.getLsbD (w - 1) <;> simp
    · simp only [BitVec.msb_eq_getLsbD_last]
      rw [BitVec.getLsbD_of_ge x (w - 1) (by omega),
          BitVec.getLsbD_of_ge A.mask (w - 1) (by omega),
          BitVec.getLsbD_of_ge A.value (w - 1) (by omega)]

theorem transfer_trunc : ∀ (w n : Nat), TransferRowSound (Op.cast CastOp.trunc w n) := by
  intro w n
  rintro A x hx
  show gamma (T_trunc n A) (BitVec.truncate n x)
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, T_trunc, Fact.mk']
  have ea := gamma_bit hx i
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_setWidth, BitVec.truncate_eq_setWidth,
    hi, decide_true, Bool.true_and]
  by_cases hiw : i < w
  · have na := norm_bit A hiw
    revert ea na
    cases A.mask.getLsbD i <;> cases A.value.getLsbD i <;> cases x.getLsbD i <;> simp
  · rw [BitVec.getLsbD_of_ge x i (by omega), BitVec.getLsbD_of_ge A.mask i (by omega),
        BitVec.getLsbD_of_ge A.value i (by omega)]

end EvoIR
