import EvoIR.MachineExecution
import EvoIR.MachineSelectionTerms

/-!
Kernel-only arithmetic bridges between the independent machine denotations
and EvoIR. These quantify over arbitrary operands. Machine shifts require
their actual source count bound: the AArch64 count mask is not IR semantics.
-/
namespace EvoIR.Machine.Selection

open Core.Symbolic (divisionBits)

def binaryOpcode : EvoIR.BinOp → BinOp
  | .add => .add | .sub => .sub | .mul => .mul
  | .and => .and | .or => .orr | .xor => .eor

def comparisonCondition : EvoIR.CmpOp → Cond
  | .eq => .eq | .ne => .ne | .slt => .lt
  | .sle => .le | .ult => .lo | .ule => .ls

def shiftOpcode : EvoIR.ShOp → BinOp
  | .shl => .lsl | .lshr => .lsr | .ashr => .asr

def flagOpcode : Bool → FlagOp
  | false => .adds | true => .subs

theorem width_le_64 (width : Width) : width.bits ≤ 64 := by cases width <;> decide

@[simp] theorem encode_view (width : Width) (value : BitVec width.bits) :
    (value.setWidth 64).setWidth width.bits = value := by
  rw [BitVec.setWidth_setWidth_of_le value (width_le_64 width), BitVec.setWidth_eq]

theorem binary_commutes (width : Width) (op : EvoIR.BinOp)
    (lhs rhs : BitVec width.bits) :
    evalBinary (binaryOpcode op) width (lhs.setWidth 64) (rhs.setWidth 64) =
      (denoteOp (.bin op width.bits) (lhs, rhs)).setWidth 64 := by
  cases op <;> simp only [evalBinary, binaryOpcode, denoteOp, Op.res, encode_view]

theorem bool_encode (b : Bool) :
    BitVec.ofNat 64 (if b then 1 else 0) = (BitVec.ofBool b).setWidth 64 := by
  cases b <;> rfl

theorem comparison_commutes (width : Width) (op : EvoIR.CmpOp)
    (lhs rhs : BitVec width.bits) :
    condition (.integer (lhs.setWidth 64) (rhs.setWidth 64) width)
      (comparisonCondition op) =
        .ok ((denoteOp (.cmp op width.bits) (lhs, rhs)).getLsbD 0) := by
  cases op <;> simp [condition, comparisonCondition, denoteOp, Op.res,
    BitVec.getLsbD_ofBool, BitVec.slt_eq_decide, BitVec.sle_eq_decide,
    BitVec.ult_eq_decide, BitVec.ule_eq_decide]

theorem flag_value_commutes (width : Width) (subtract : Bool)
    (lhs rhs : BitVec width.bits) :
    (arithmeticFlags (flagOpcode subtract) width (lhs.setWidth 64) (rhs.setWidth 64)).1 =
      (denoteOp (.bin (if subtract then .sub else .add) width.bits) (lhs, rhs)).setWidth 64 := by
  cases subtract <;> simp [arithmeticFlags, flagOpcode, denoteOp, Op.res]

theorem overflow_condition_commutes (width : Width) (subtract : Bool)
    (lhs rhs : BitVec width.bits) :
    condition (arithmeticFlags (flagOpcode subtract) width
      (lhs.setWidth 64) (rhs.setWidth 64)).2 .vc =
        .ok ((denoteOp (.guard (if subtract then .sub else .add) width.bits)
          (lhs, rhs)).getLsbD 0) := by
  cases subtract <;>
    simp [arithmeticFlags, flagOpcode, condition, denoteOp, Op.res,
      signedRepresentable, signedMin, signedMax, BitVec.getLsbD_ofBool]

theorem unsigned_division_commutes (width : Width) (lhs rhs : BitVec width.bits) :
    evalBinary .udiv width (lhs.setWidth 64) (rhs.setWidth 64) =
      (divisionBits false false lhs rhs).setWidth 64 := by
  simp only [evalBinary, encode_view, divisionBits, Bool.false_eq_true, ↓reduceIte]
  by_cases zero : rhs.toNat = 0
  · simp [zero]
  · simp [zero]

theorem signed_division_commutes (width : Width) (lhs rhs : BitVec width.bits) :
    evalBinary .sdiv width (lhs.setWidth 64) (rhs.setWidth 64) =
      (divisionBits true false lhs rhs).setWidth 64 := by
  simp only [evalBinary, encode_view, divisionBits, ↓reduceIte]
  by_cases zero : rhs.toNat = 0
  · have value : rhs = 0 := BitVec.eq_of_toNat_eq zero
    simp [zero, value]
  · simp [zero]

theorem signed_remainder_identity {n : Nat} (lhs rhs : BitVec n) :
    divisionBits true true lhs rhs = lhs - divisionBits true false lhs rhs * rhs := by
  simp only [divisionBits, Bool.false_eq_true, ↓reduceIte]
  rw [Int.tmod_def, Int.sub_eq_add_neg, BitVec.ofInt_add, BitVec.ofInt_neg,
    BitVec.ofInt_mul, BitVec.ofInt_toInt, BitVec.ofInt_toInt,
    ← BitVec.sub_eq_add_neg, BitVec.mul_comm]

theorem unsigned_remainder_identity {n : Nat} (lhs rhs : BitVec n) :
    divisionBits false true lhs rhs = lhs - divisionBits false false lhs rhs * rhs := by
  simp only [divisionBits, Bool.false_eq_true, ↓reduceIte]
  have law := congrArg (BitVec.ofNat n) (Nat.mod_add_div lhs.toNat rhs.toNat)
  rw [BitVec.ofNat_add, BitVec.ofNat_mul, BitVec.ofNat_toNat] at law
  have rearranged := congrArg (fun value => value - rhs * BitVec.ofNat n (lhs.toNat / rhs.toNat)) law
  simpa [BitVec.add_sub_cancel, BitVec.ofNat_toNat, BitVec.mul_comm] using rearranged

theorem bounded_machine_count (width : Width) {m : Nat} (count : BitVec m)
    (bounded : count.toNat < width.bits) :
    ((count.setWidth 64).setWidth width.bits).toNat % width.bits = count.toNat := by
  rw [BitVec.setWidth_setWidth_of_le count (width_le_64 width), BitVec.toNat_setWidth]
  have fits : count.toNat < 2^width.bits := by
    apply Nat.lt_trans bounded
    cases width <;> decide
  rw [Nat.mod_eq_of_lt fits, Nat.mod_eq_of_lt bounded]

theorem shift_commutes (width : Width) (op : EvoIR.ShOp) {m : Nat}
    (lhs : BitVec width.bits) (count : BitVec m) (bounded : count.toNat < width.bits) :
    evalBinary (shiftOpcode op) width (lhs.setWidth 64) (count.setWidth 64) =
      (denoteOp (.shift op width.bits m) (lhs, count)).setWidth 64 := by
  cases op <;> simp only [evalBinary, shiftOpcode, denoteOp, Op.res, encode_view,
    bounded_machine_count width count bounded]

theorem sign_extend_32_commutes (value : BitVec 32) :
    evalExtend .sxtw (value.setWidth 64) = denoteOp (.cast .sext 32 64) value := by
  simp [evalExtend, ExtOp.bits, ExtOp.signed, denoteOp, BitVec.setWidth_setWidth_of_le]

theorem count_unsigned_as_comparison {n m : Nat} (value : BitVec n) (count : BitVec m)
    (boundFits : n < 2^m) :
    denoteOp (.count false n m) (value, count) =
      denoteOp (.cmp .ult m) (count, BitVec.ofNat m n) := by
  simp [denoteOp, BitVec.ult_eq_decide, BitVec.toNat_ofNat, Nat.mod_eq_of_lt boundFits]

theorem count_signed_as_comparison {n m : Nat} (value : BitVec n) (count : BitVec m)
    (positive : 0 < m) (boundSmall : n ≤ 2^(m-1)) :
    denoteOp (.count true n m) (value, count) =
      denoteOp (.cmp .ult m) (count, BitVec.ofNat m n) := by
  have modulus : 2^m = 2 * 2^(m-1) := by
    calc
      2^m = 2^((m-1)+1) := by congr 1; omega
      _ = 2 * 2^(m-1) := by rw [Nat.pow_succ, Nat.mul_comm]
  have boundFits : n < 2^m := by
    have positivePow := Nat.two_pow_pos (m-1)
    omega
  simp only [denoteOp, BitVec.ult_eq_decide, BitVec.toNat_ofNat,
    Nat.mod_eq_of_lt boundFits]
  apply congrArg BitVec.ofBool
  apply Bool.eq_iff_iff.mpr
  simp only [decide_eq_true_eq]
  rw [BitVec.toInt_eq_toNat_cond]
  have inRange := count.isLt
  split <;> omega

theorem nonzero_as_comparison {n : Nat} (value : BitVec n) :
    denoteOp (.nonzero n) value = denoteOp (.cmp .ne n) (value, 0) := rfl

theorem division_guard_as_disjunction {n : Nat} (lhs rhs : BitVec n) (positive : 0 < n) :
    denoteOp (.guard .divRem n) (lhs, rhs) =
      denoteOp (.bin .or 1)
        (denoteOp (.cmp .ne n) (lhs, BitVec.intMin n),
         denoteOp (.cmp .ne n) (rhs, BitVec.allOnes n)) := by
  have minimum : lhs.toInt = signedMin n ↔ lhs = BitVec.intMin n := by
    rw [signedMin, ← BitVec.toInt_intMin_of_pos positive, BitVec.toInt_inj]
  have minusOne : rhs.toInt = -1 ↔ rhs = BitVec.allOnes n := by
    rw [← show (BitVec.allOnes n).toInt = -1 by simp [positive], BitVec.toInt_inj]
  simp only [denoteOp, BitVec.ofBool_or_ofBool]
  apply congrArg BitVec.ofBool
  cases a : lhs == BitVec.intMin n <;> cases b : rhs == BitVec.allOnes n <;>
    simp_all [beq_iff_eq, bne, minimum, minusOne]

theorem selection_mask_identity {n : Nat} (condition : BitVec 1) (yes no : BitVec n) :
    no ^^^ ((yes ^^^ no) &&& (0 - condition.setWidth n)) =
      if condition.toNat == 0 then no else yes := by
  have size := condition.isLt
  by_cases zero : condition.toNat = 0
  · have value : condition = 0 := BitVec.eq_of_toNat_eq zero
    simp [zero, value]
  · have one : condition.toNat = 1 := by omega
    have value : condition = 1 := BitVec.eq_of_toNat_eq one
    have extended : (1#1).setWidth n = 1#n := by
      apply BitVec.eq_of_toNat_eq
      simp [BitVec.toNat_setWidth]
    rw [value]
    change no ^^^ ((yes ^^^ no) &&& (0#n - (1#1).setWidth n)) = yes
    rw [extended, BitVec.zero_sub, BitVec.neg_one_eq_allOnes, BitVec.and_allOnes]
    rw [BitVec.xor_comm yes no, ← BitVec.xor_assoc, BitVec.xor_self, BitVec.zero_xor]

end EvoIR.Machine.Selection
