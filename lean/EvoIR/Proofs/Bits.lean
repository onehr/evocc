import EvoIR.Fact

/-!
# Shared bit-level lemmas

Small facts the rule and transfer proofs reuse.  Everything here is width
parametric; nothing here calls a SAT solver.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

open Lean in
/-- State one theorem per width in `1 .. 64`, the bound Section 8.5 puts on a
`bits<N>` root or witness.  `each_width nm over (f) using tac` produces
`nm_1 : TransferRowSound (f 1)` through `nm_64 : TransferRowSound (f 64)`. -/
macro "each_width " nm:ident " over " f:term " using " tac:tacticSeq : command => do
  let mut cmds : Array (TSyntax `command) := #[]
  for i in [1:65] do
    let lit := Syntax.mkNumLit (toString i)
    let thm := mkIdent (nm.getId.appendAfter s!"_{i}")
    cmds := cmds.push (← `(command| theorem $thm : TransferRowSound ($f $lit) := by $tac))
  return ⟨mkNullNode cmds⟩

/-! ## The low-bit mask a left shift brings in -/

theorem lowMask_eq (N s : Nat) (hs : s < N) :
    (1#N <<< s) - 1#N = BitVec.ofNat N (2 ^ s - 1) := by
  apply BitVec.eq_of_toNat_eq
  have hpos : 0 < (2:Nat) ^ s := Nat.two_pow_pos s
  have h2 : (2:Nat) ^ s < 2 ^ N := Nat.pow_lt_pow_right (by omega) hs
  have h1 : (1:Nat) < 2 ^ N := by
    calc (1:Nat) ≤ 2 ^ s := hpos
    _ < 2 ^ N := h2
  have h3 : (2:Nat) ^ s - 1 < 2 ^ N := by omega
  rw [BitVec.toNat_sub, BitVec.toNat_shiftLeft, BitVec.toNat_ofNat, BitVec.toNat_ofNat,
      Nat.shiftLeft_eq, Nat.mod_eq_of_lt h1, Nat.one_mul, Nat.mod_eq_of_lt h2,
      Nat.mod_eq_of_lt h3]
  have h4 : 2 ^ N - 1 + 2 ^ s = 2 ^ N + (2 ^ s - 1) := by omega
  rw [h4, Nat.add_mod_left, Nat.mod_eq_of_lt h3]

theorem lowMask_getLsbD (N s i : Nat) (hs : s < N) :
    ((1#N <<< s) - 1#N).getLsbD i = (decide (i < N) && decide (i < s)) := by
  rw [lowMask_eq N s hs]
  simp [BitVec.getLsbD_ofNat, Nat.testBit_two_pow_sub_one]

/-! ## Widths of at least two -/

theorem two_lt_two_pow (n : Nat) (h : 2 ≤ n) : (2:Nat) < 2 ^ n := by
  have : (2:Nat) ^ 2 ≤ 2 ^ n := Nat.pow_le_pow_right (by omega) h
  simp at this
  omega

theorem ofNat_one_toNat (n : Nat) (h : 2 ≤ n) : (BitVec.ofNat n 1).toNat = 1 := by
  rw [BitVec.toNat_ofNat, Nat.mod_eq_of_lt (by have := two_lt_two_pow n h; omega)]

/-- `mul(x,2) = add(x,x)` at any width of at least two. -/
theorem mul_two_eq {n : Nat} (h : 2 ≤ n) (x : BitVec n) : x * BitVec.ofNat n 2 = x + x := by
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_mul, BitVec.toNat_add, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt (two_lt_two_pow n h), Nat.mul_two]

/-- `shl(x,1) = add(x,x)` at any width of at least two.  The amount is decoded
at its own width, which for `const_like(?x,1)` is the width of `?x`. -/
theorem shl_one_eq {n : Nat} (h : 2 ≤ n) (x : BitVec n) :
    x <<< (BitVec.ofNat n 1).toNat = x + x := by
  rw [ofNat_one_toNat n h]
  apply BitVec.eq_of_toNat_eq
  rw [BitVec.toNat_shiftLeft, BitVec.toNat_add, Nat.shiftLeft_eq, Nat.pow_one, Nat.mul_two]

/-! ## A value compared with itself -/

theorem slt_self {n : Nat} (x : BitVec n) : x.slt x = false := by simp [BitVec.slt]
theorem sle_self {n : Nat} (x : BitVec n) : x.sle x = true := by simp [BitVec.sle]
theorem ult_self {n : Nat} (x : BitVec n) : x.ult x = false := by simp [BitVec.ult]
theorem ule_self {n : Nat} (x : BitVec n) : x.ule x = true := by simp [BitVec.ule]

/-! ## Reading a fact at one bit -/

/-- The bit-`i` reading of a fact holding of a value. -/
theorem gamma_bit {n : Nat} {K : Fact n} {x : BitVec n} (h : gamma K x) (i : Nat) :
    (x.getLsbD i && K.mask.getLsbD i) = K.value.getLsbD i := by
  simpa only [gamma, BitVec.getLsbD_and] using congrArg (fun z => BitVec.getLsbD z i) h

/-- The bit-`i` reading of the normalization invariant. -/
theorem norm_bit {n : Nat} (K : Fact n) {i : Nat} (hi : i < n) :
    (K.value.getLsbD i && !K.mask.getLsbD i) = false := by
  have := congrArg (fun z => BitVec.getLsbD z i) K.normalized
  simpa only [BitVec.getLsbD_and, BitVec.getLsbD_not, BitVec.getLsbD_zero, hi, decide_true,
    Bool.true_and] using this

end EvoIR
