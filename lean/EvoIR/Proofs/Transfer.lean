import EvoIR.Proofs.Bits

/-!
# The transfer table of Section 8.10

One theorem per row.  The bitwise rows, the shifts and the width changes are
width parametric, proved one bit at a time; the shift rows are parametric in
the amount's own width `M` as well, so the split of the plan's section 4
(amount fully known or not, and `k.toNat` against `N`) is carried out once
rather than per width.  The carry rows and the comparison rows are enumerated
over `N` in `1 .. 64` by `bv_decide`; each is 64 theorems named `<row>_1`
through `<row>_64`. Each `bv_decide` sets a SAT timeout of 60 s, not the
default 10 s: a parallel build on a busy host can stretch a 3 s query past 10 s.

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

/-! ## The carry rows, enumerated over the widths Section 8.5 admits

The mask is the trailing run of bits known in both inputs; the value is the
sum or difference of the known values inside that run.  The argument is carry
propagation, which `bv_decide` settles at each width. -/

each_width transfer_add over (Op.bin BinOp.add) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_add, carryMask, Fact.mk', denoteOp, Op.res] at *
  bv_decide (timeout := 60)

each_width transfer_sub over (Op.bin BinOp.sub) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_sub, carryMask, Fact.mk', denoteOp, Op.res] at *
  bv_decide (timeout := 60)

each_width transfer_mul over (Op.bin BinOp.mul) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_mul, lowbit0, Fact.mk', denoteOp, Op.res] at *
  bv_decide (timeout := 60)

/-! ## The comparison rows, enumerated over the widths Section 8.5 admits

Equality is decided by a bit known in both with different values, or by both
operands being fully known; the ordered comparisons by the interval rule, the
signed ones only when both sign bits are known.  Each `split` is one branch of
that procedure, and the `top` branch is sound because `top` says nothing. -/

each_width transfer_icmp_eq over (Op.cmp CmpOp.eq) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · obtain ⟨e1, e2⟩ := h2
      subst e1; subst e2
      bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_ne over (Op.cmp CmpOp.ne) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · obtain ⟨e1, e2⟩ := h2
      subst e1; subst e2
      bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_ult over (Op.cmp CmpOp.ult) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_ule over (Op.cmp CmpOp.ule) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i h1
  · bv_decide (timeout := 60)
  · split <;> rename_i h2
    · bv_decide (timeout := 60)
    · bv_decide (timeout := 60)

each_width transfer_icmp_slt over (Op.cmp CmpOp.slt) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i hs
  · obtain ⟨s1, s2⟩ := hs
    split <;> rename_i h1
    · bv_decide (timeout := 60)
    · split <;> rename_i h2
      · bv_decide (timeout := 60)
      · bv_decide (timeout := 60)
  · bv_decide (timeout := 60)

each_width transfer_icmp_sle over (Op.cmp CmpOp.sle) using
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  obtain ⟨ma, va, na⟩ := A
  obtain ⟨mb, vb, nb⟩ := B
  simp only [gamma, transfer, T_icmp, Fact.lo, Fact.hi, fullyKnown, Fact.bit, Fact.const,
    Fact.top, Fact.mk', denoteOp, Op.res] at *
  split <;> rename_i hs
  · obtain ⟨s1, s2⟩ := hs
    split <;> rename_i h1
    · bv_decide (timeout := 60)
    · split <;> rename_i h2
      · bv_decide (timeout := 60)
      · bv_decide (timeout := 60)
  · bv_decide (timeout := 60)

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
