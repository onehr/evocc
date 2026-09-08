import Std.Tactic.BVDecide

-- gamma(K) = { x | x &&& mask = value }, value has no ones outside mask (checker rejects otherwise)

-- ult interval row at 64: lo(a) = va, hi(a) = va ||| ~~~ma
theorem ult_row_64 (x y ma va mb vb : BitVec 64)
    (hx : x &&& ma = va) (hy : y &&& mb = vb) (hva : va &&& ~~~ma = 0) (hvb : vb &&& ~~~mb = 0)
    (h : (va ||| ~~~ma) < vb) : x.ult y = true := by bv_decide
#print axioms ult_row_64

-- slt interval row at 64, both sign bits known
theorem slt_row_64 (x y ma va mb vb : BitVec 64)
    (hx : x &&& ma = va) (hy : y &&& mb = vb) (hva : va &&& ~~~ma = 0) (hvb : vb &&& ~~~mb = 0)
    (sa : ma.msb = true) (sb : mb.msb = true)
    (h : (va ||| ~~~ma).slt vb = true) : x.slt y = true := by bv_decide
#print axioms slt_row_64

-- zext row 32 -> 64
theorem zext_row (x ma va : BitVec 32) (hx : x &&& ma = va) :
    (x.zeroExtend 64) &&& (ma.zeroExtend 64 ||| ~~~((BitVec.allOnes 32).zeroExtend 64)) = va.zeroExtend 64 := by
  bv_decide
#print axioms zext_row

-- sext row 32 -> 64: sign-extending the mask is the transfer
theorem sext_row (x ma va : BitVec 32) (hx : x &&& ma = va) (hva : va &&& ~~~ma = 0) :
    (x.signExtend 64) &&& (ma.signExtend 64) = va.signExtend 64 := by bv_decide
#print axioms sext_row

-- trunc row 64 -> 32
theorem trunc_row (x ma va : BitVec 64) (hx : x &&& ma = va) :
    (x.truncate 32) &&& (ma.truncate 32) = va.truncate 32 := by bv_decide
#print axioms trunc_row

-- shl row at 64 with a fully known amount below the width (amount width 64)
theorem shl_row_64 (x k ma va vk : BitVec 64) (hx : x &&& ma = va) (hk : k = vk) (hlt : vk < 64#64) :
    (x <<< k) &&& ((ma <<< vk) ||| ((1#64 <<< vk) - 1)) = va <<< vk := by bv_decide
#print axioms shl_row_64

-- mul row at 64: trailing known-zero runs add
abbrev lowbit0 (kz : BitVec n) : BitVec n := (~~~kz) &&& (kz + 1)
abbrev mulMask (ma va mb vb : BitVec n) : BitVec n := lowbit0 (ma &&& ~~~va) * lowbit0 (mb &&& ~~~vb) - 1
theorem mul_row_64 (x y ma va mb vb : BitVec 64)
    (hx : x &&& ma = va) (hy : y &&& mb = vb) :
    (x * y) &&& mulMask ma va mb vb = 0#64 := by
  simp only [mulMask, lowbit0]; bv_decide (config := { timeout := 120 })
#print axioms mul_row_64
