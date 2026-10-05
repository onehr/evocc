import EvoIR.Asm.Semantics
import EvoIR.MachineExecution

/-!
Each AArch64 data-processing rule of `EvoIR.Asm.Semantics` gives the value
of the machine evaluator of `EvoIR.MachineExecution`. The flag lemmas relate
the NZCV bits of `subs`, `adds` and `tst` to the machine flag forms and their
conditions, for all fourteen conditions and both widths.

The proofs reduce the bit vectors to residues and close the cases with
`omega`. No lemma assumes a particular operand value.
-/
namespace EvoIR.Asm

theorem toInt_def {n : Nat} (x : BitVec n) :
    x.toInt = if 2 * x.toNat < 2^n then (x.toNat : Int) else (x.toNat : Int) - 2^n := rfl

theorem beq_toNat {n : Nat} (x y : BitVec n) : (x == y) = decide (x.toNat = y.toNat) := by
  by_cases h : x = y
  · subst h; simp
  · have : x.toNat ≠ y.toNat := fun e => h (BitVec.eq_of_toNat_eq e)
    simp [h, this]

theorem toNat_zero_bv (n : Nat) : (0 : BitVec n).toNat = 0 := rfl

/-- Close a goal of Booleans over linear facts of the residues. -/
macro "bool_arith" : tactic => `(tactic| (
  repeat' split
  all_goals (simp only [← decide_not, Bool.beq_eq_decide_eq, decide_eq_decide, ← Bool.decide_and,
    ← Bool.decide_or, bne, Bool.not_not, decide_eq_true_eq]; omega)))

set_option hygiene false in
/-- Replace the residues of `x`, `y` and `~~~y` modulo `M` by variables. -/
macro "residue_arith" M:term : tactic => `(tactic| (
  have hpb : x.toNat % $M < $M := Nat.mod_lt _ (by decide)
  have hqb : y.toNat % $M < $M := Nat.mod_lt _ (by decide)
  have hq : (18446744073709551616 - 1 - y.toNat) % $M + y.toNat % $M = $M - 1 := by omega
  generalize (18446744073709551616 - 1 - y.toNat) % $M = nq at *
  generalize x.toNat % $M = p at *
  generalize y.toNat % $M = q at *
  bool_arith))

theorem alu_eq (op : Machine.BinOp) (width : Machine.Width) (a b : BitVec 64) :
    alu op width a b = Machine.evalBinary op width a b := by
  cases op <;> simp [alu, Machine.evalBinary]

theorem view_eq (width : Machine.Width) (b : BitVec 64) : view width b = Machine.normalizeAt width b := by
  cases width <;> simp [view, Machine.normalizeAt, Machine.Width.bits]

theorem wideMul_eq (op : Machine.WideMul) (a b : BitVec 64) :
    wideMul op a b = Machine.evalWideMultiply op a b := by
  cases op <;> rfl

theorem extend_eq (op : Machine.ExtOp) (a : BitVec 64) : extend op a = Machine.evalExtend op a := rfl

theorem bitfield_eq (signed : Bool) (width : Machine.Width) (a : BitVec 64) (lsb bits : Nat) :
    view width (bitfield signed width a lsb bits) = Machine.evalBfx signed width a lsb bits := by
  simp only [bitfield, Machine.evalBfx, view_eq]

/-- Conditions of the arithmetic flag form are ConditionHolds of its bits. -/
theorem condition_arithmetic (n z c v : Bool) (cond : Machine.Cond) :
    Machine.condition (.arithmetic n z c v) cond = .ok ((Flags.mk n z c v).holds cond) := by
  cases cond <;> rfl

set_option linter.unusedSimpArgs false in
/-- `cmp` sets NZCV such that every condition agrees with the machine form
`.integer x y width`. -/
theorem cmp_condition (width : Machine.Width) (x y : BitVec 64) (c : Machine.Cond) :
    Machine.condition (.integer x y width) c = .ok ((flagAlu .subs width x y).2.holds c) := by
  have hx := x.isLt
  have hy := y.isLt
  cases width <;> cases c <;> simp only [Machine.condition, flagAlu, addWithCarry, Flags.holds,
      BitVec.msb_eq_toInt, toInt_def, beq_toNat, bne, Except.ok.injEq, toNat_zero_bv,
      BitVec.toNat_sub] <;>
    simp only [Machine.Width.bits, BitVec.toNat_setWidth, BitVec.toNat_not, BitVec.toNat_ofNat,
      BitVec.toNat_sub, Bool.toNat_true, BitVec.toNat_zero, Nat.reducePow]
  case w.eq | w.ne | w.lt | w.le | w.gt | w.ge | w.lo | w.ls | w.hi | w.hs | w.vs | w.vc | w.cs
      | w.cc => residue_arith 4294967296
  all_goals residue_arith 18446744073709551616

theorem add_ofNat (n : Nat) (a b : BitVec n) : a + b = BitVec.ofNat n (a.toNat + b.toNat + 0) := by
  apply BitVec.eq_of_toNat_eq; simp [BitVec.toNat_add]

theorem sub_ofNat (n : Nat) (a b : BitVec n) : a - b = BitVec.ofNat n (a.toNat + (~~~b).toNat + 1) := by
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_sub, BitVec.toNat_ofNat, BitVec.toNat_not]
  have := a.isLt; have := b.isLt
  have h : 2 ^ n - 1 - b.toNat + 1 = 2^n - b.toNat := by omega
  rw [Nat.add_assoc, h, Nat.add_comm]

theorem setWidth_not (width : Machine.Width) (y : BitVec 64) :
    (~~~y).setWidth width.bits = ~~~(y.setWidth width.bits) := by
  apply BitVec.eq_of_toNat_eq
  have := y.isLt
  simp only [BitVec.toNat_setWidth, BitVec.toNat_not]
  cases width <;> simp only [Machine.Width.bits, Nat.reducePow] <;> omega

theorem adds_beq_subs : (Machine.FlagOp.adds == Machine.FlagOp.subs) = false := rfl
theorem subs_beq_subs : (Machine.FlagOp.subs == Machine.FlagOp.subs) = true := rfl

set_option hygiene false in
macro "pair_arith" M:term : tactic => `(tactic| (
  have hpb : x.toNat % $M < $M := Nat.mod_lt _ (by decide)
  have hqb : y.toNat % $M < $M := Nat.mod_lt _ (by decide)
  generalize x.toNat % $M = p at *
  generalize y.toNat % $M = q at *
  bool_arith))

set_option linter.unusedSimpArgs false in
theorem flagAlu_eq (op : Machine.FlagOp) (width : Machine.Width) (x y : BitVec 64) :
    Machine.arithmeticFlags op width x y =
      ((flagAlu op width x y).1, .arithmetic (flagAlu op width x y).2.n
        (flagAlu op width x y).2.z (flagAlu op width x y).2.c (flagAlu op width x y).2.v) := by
  have hx := x.isLt
  have hy := y.isLt
  cases op <;> simp only [Machine.arithmeticFlags, flagAlu, addWithCarry, Prod.mk.injEq,
    Machine.Flags.arithmetic.injEq, adds_beq_subs, subs_beq_subs, Bool.false_eq_true, reduceIte,
    setWidth_not, ← add_ofNat, ← sub_ofNat, Bool.toNat_false, Bool.toNat_true, true_and] <;>
  (repeat' apply And.intro) <;>
  simp only [BitVec.msb_eq_toInt, toInt_def, beq_toNat, toNat_zero_bv, BitVec.toNat_add,
      BitVec.toNat_sub, BitVec.toNat_not] <;>
  cases width <;>
  simp only [Machine.Width.bits, BitVec.toNat_setWidth, BitVec.toNat_ofNat, Nat.reducePow,
      Nat.add_zero, Nat.reduceSub, Nat.reduceAdd] <;>
  first
    | pair_arith 4294967296
    | pair_arith 18446744073709551616
    | residue_arith 4294967296
    | residue_arith 18446744073709551616

theorem setWidth_ofNat (width : Machine.Width) (imm : Nat) :
    (BitVec.ofNat 64 imm).setWidth width.bits = BitVec.ofNat width.bits imm := by
  apply BitVec.eq_of_toNat_eq
  cases width <;> simp only [Machine.Width.bits, BitVec.toNat_setWidth, BitVec.toNat_ofNat,
    Nat.reducePow] <;> omega

/-- `tst` sets the flags of the machine `tstImm` form. -/
theorem logicalFlags_eq (width : Machine.Width) (x : BitVec 64) (imm : Nat) :
    logicalFlags width x (BitVec.ofNat 64 imm) =
      let bits := x.setWidth width.bits &&& BitVec.ofNat width.bits imm
      ⟨decide (bits.toInt < 0), bits == 0, false, false⟩ := by
  simp only [logicalFlags, setWidth_ofNat, BitVec.msb_eq_toInt]

end EvoIR.Asm
