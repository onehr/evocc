import EvoIR.MachineExecution

/-!
The checked multiplication tests emitted by the real integer lowerer.
SMULL compares the exact 32-bit product with its sign-extended low half;
SMULH compares the high 64 bits with the sign fill of the low 64 bits.
Both are proved against the mathematical signed-representability predicate.
No bounded input enumeration or native decision axiom is used.
-/
namespace EvoIR.Machine

private theorem signed_bmod_self (n : Nat) (positive : 0 < n) (value : Int) :
    value.bmod (2^n) = value ↔ signedRepresentable n value := by
  have pow : 0 < 2^n := Nat.two_pow_pos n
  rw [Int.bmod_eq_iff pow]
  have nform : n = (n-1)+1 := by omega
  simp only [Int.sub_self, Int.dvd_zero, and_true, signedRepresentable, signedMin, signedMax]
  rw [nform]
  simp [Nat.add_one_sub_one, Nat.pow_succ, Int.natCast_mul,
    Int.natCast_pow]
  omega

private theorem wide_product_exact (x y : BitVec 32) :
    (BitVec.ofInt 64 (x.toInt * y.toInt)).toInt = x.toInt * y.toInt := by
  have lo := BitVec.le_toInt_mul_toInt (x := x) (y := y)
  have hi := BitVec.toInt_mul_toInt_le (x := x) (y := y)
  apply BitVec.toInt_ofInt_eq_self (by decide)
  · omega
  · omega

private theorem low_signed_view (value : BitVec 64) :
    (value.setWidth 32).toInt = value.toInt.bmod (2^32) := by
  rw [← BitVec.signExtend_eq_setWidth_of_le value (by decide : 32 ≤ 64)]
  exact BitVec.toInt_signExtend_eq_toInt_bmod_of_le value (by decide)

theorem smull_guard_correct (x y : BitVec 32) :
    condition (.integer
      (evalWideMultiply .smull (x.setWidth 64) (y.setWidth 64))
      (evalBfx true .x (evalWideMultiply .smull (x.setWidth 64) (y.setWidth 64)) 0 32)
      .x) .eq = .ok (decide (signedRepresentable 32 (x.toInt * y.toInt))) := by
  simp only [evalWideMultiply, BitVec.setWidth_setWidth_of_le _ (by decide : 32 ≤ 64),
    BitVec.setWidth_eq, evalBfx, normalizeAt, Width.bits, ite_true,
    BitVec.ushiftRight_zero, condition]
  congr 1
  change ((BitVec.ofInt 64 (x.toInt * y.toInt)) ==
    ((BitVec.ofInt 64 (x.toInt * y.toInt)).setWidth 32).signExtend 64) = _
  apply Bool.eq_iff_iff.mpr
  simp only [beq_iff_eq, decide_eq_true_eq]
  rw [← BitVec.toInt_inj, BitVec.toInt_signExtend_of_le (by decide : 32 ≤ 64),
    low_signed_view, wide_product_exact]
  exact (eq_comm.trans (signed_bmod_self 32 (by decide) _))

private theorem high_product_exact (x y : BitVec 64) :
    (BitVec.ofInt 64 ((x.toInt * y.toInt) / 2^64)).toInt =
      (x.toInt * y.toInt) / 2^64 := by
  have lo := BitVec.le_toInt_mul_toInt (x := x) (y := y)
  have hi := BitVec.toInt_mul_toInt_le (x := x) (y := y)
  apply BitVec.toInt_ofInt_eq_self (by decide)
  · omega
  · omega

private theorem high_matches_sign_iff (value : Int) :
    value / 2^64 = (value.bmod (2^64)) / 2^63 ↔ signedRepresentable 64 value := by
  simp only [Int.bmod_def, signedRepresentable, signedMin, signedMax]
  split <;> omega

theorem smulh_guard_correct (x y : BitVec 64) :
    condition (.integer (evalWideMultiply .smulh x y)
      (evalBinary .asr .x (evalBinary .mul .x x y) (BitVec.ofNat 64 63)) .x) .eq =
        .ok (decide (signedRepresentable 64 (x.toInt * y.toInt))) := by
  simp only [evalWideMultiply, Int.shiftRight_eq_div_pow, evalBinary, Width.bits,
    BitVec.setWidth_eq, BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceMod, condition]
  congr 1
  change ((BitVec.ofInt 64 ((x.toInt * y.toInt) / 2^64)) ==
    (x * y).sshiftRight 63) = _
  apply Bool.eq_iff_iff.mpr
  simp only [beq_iff_eq, decide_eq_true_eq]
  rw [← BitVec.toInt_inj, high_product_exact, BitVec.toInt_sshiftRight,
    Int.shiftRight_eq_div_pow, BitVec.toInt_mul]
  exact high_matches_sign_iff _

end EvoIR.Machine
