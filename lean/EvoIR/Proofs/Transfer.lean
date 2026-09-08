import EvoIR.Proofs.Bits

/-!
# The transfer table of Section 8.10

One theorem per row.  The bitwise rows, the shifts and the width changes are
width parametric, and the shift rows are parametric in the amount's own width
`M` as well, which is what Section 8.5 leaves unbounded.  The carry rows and
the comparison rows are enumerated over `N` in `1 .. 64`, the bound Section
8.5 puts on a root or witness; each enumerated row is 64 theorems named
`<row>_1` through `<row>_64`.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## The constant row -/

/-- 8.10: a `const` node's fact is fully known. -/
theorem transfer_const : ∀ (n : Nat) (v : BitVec n), gamma (Fact.const v) v := by
  sorry

/-! ## The bitwise rows, width parametric -/

theorem transfer_and : ∀ (n : Nat), TransferRowSound (Op.bin BinOp.and n) := by
  sorry

theorem transfer_or : ∀ (n : Nat), TransferRowSound (Op.bin BinOp.or n) := by
  sorry

theorem transfer_xor : ∀ (n : Nat), TransferRowSound (Op.bin BinOp.xor n) := by
  sorry

/-! ## The carry rows, enumerated over the widths of Section 8.5 -/

each_width transfer_add over (Op.bin BinOp.add) using
  sorry

each_width transfer_sub over (Op.bin BinOp.sub) using
  sorry

each_width transfer_mul over (Op.bin BinOp.mul) using
  sorry

/-! ## The comparison rows, enumerated over the widths of Section 8.5 -/

each_width transfer_icmp_eq over (Op.cmp CmpOp.eq) using
  sorry

each_width transfer_icmp_ne over (Op.cmp CmpOp.ne) using
  sorry

each_width transfer_icmp_slt over (Op.cmp CmpOp.slt) using
  sorry

each_width transfer_icmp_sle over (Op.cmp CmpOp.sle) using
  sorry

each_width transfer_icmp_ult over (Op.cmp CmpOp.ult) using
  sorry

each_width transfer_icmp_ule over (Op.cmp CmpOp.ule) using
  sorry

/-! ## The shift rows, parametric in the value width and in the amount width -/

theorem transfer_shl : ∀ (n m : Nat), TransferRowSound (Op.shift ShOp.shl n m) := by
  sorry

theorem transfer_lshr : ∀ (n m : Nat), TransferRowSound (Op.shift ShOp.lshr n m) := by
  sorry

theorem transfer_ashr : ∀ (n m : Nat), TransferRowSound (Op.shift ShOp.ashr n m) := by
  sorry

/-! ## The width-change rows, parametric in both widths -/

theorem transfer_zext : ∀ (w n : Nat), TransferRowSound (Op.cast CastOp.zext w n) := by
  sorry

theorem transfer_sext : ∀ (w n : Nat), TransferRowSound (Op.cast CastOp.sext w n) := by
  sorry

theorem transfer_trunc : ∀ (w n : Nat), TransferRowSound (Op.cast CastOp.trunc w n) := by
  sorry

end EvoIR
