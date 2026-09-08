import Std.Tactic.BVDecide

theorem add_zero_32 (x : BitVec 32) : x + 0#32 = x := by bv_decide
#print axioms add_zero_32
theorem mul_two_64 (x : BitVec 64) : x * 2#64 = x + x := by bv_decide
#print axioms mul_two_64
theorem mul_one_64 (x : BitVec 64) : x * 1#64 = x := by bv_decide
#print axioms mul_one_64
theorem slt_self_64 (x : BitVec 64) : x.slt x = false := by bv_decide
#print axioms slt_self_64

-- shift amount at its own width, hypothesis as a BitVec comparison
theorem shl_wide_amount (x : BitVec 8) (k : BitVec 16) (h : 8#16 ≤ k) : x <<< k = 0#8 := by bv_decide
#print axioms shl_wide_amount
example : (1#8) <<< (256#16) = 0#8 := by decide

abbrev andMask (ma va mb vb : BitVec n) : BitVec n := (ma &&& ~~~va) ||| (mb &&& ~~~vb) ||| (ma &&& mb)
abbrev andVal  (ma va mb vb : BitVec n) : BitVec n := (va &&& vb) &&& andMask ma va mb vb
theorem and_row_64 (x y ma va mb vb : BitVec 64)
    (hx : x &&& ma = va) (hy : y &&& mb = vb) :
    (x &&& y) &&& andMask ma va mb vb = andVal ma va mb vb := by
  simp only [andMask, andVal]; bv_decide
#print axioms and_row_64

abbrev addMask (ma mb : BitVec n) : BitVec n := ((~~~(ma &&& mb)) &&& ((ma &&& mb) + 1)) - 1
theorem add_row_64 (x y ma va mb vb : BitVec 64)
    (hx : x &&& ma = va) (hy : y &&& mb = vb) :
    (x + y) &&& addMask ma mb = (va + vb) &&& addMask ma mb := by
  simp only [addMask]; bv_decide
#print axioms add_row_64

-- mul row: trailing known-zero runs add; lowbit0 k = one-hot of first zero of the known-zero mask
abbrev lowbit0 (kz : BitVec n) : BitVec n := (~~~kz) &&& (kz + 1)
abbrev mulMask (ma va mb vb : BitVec n) : BitVec n := lowbit0 (ma &&& ~~~va) * lowbit0 (mb &&& ~~~vb) - 1
theorem mul_row_16 (x y ma va mb vb : BitVec 16)
    (hx : x &&& ma = va) (hy : y &&& mb = vb) :
    (x * y) &&& mulMask ma va mb vb = 0#16 := by
  simp only [mulMask, lowbit0]; bv_decide
#print axioms mul_row_16
