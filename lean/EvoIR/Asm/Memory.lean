import EvoIR.Asm.Semantics

/-!
Read-after-write and separation facts of the little-endian byte memory of
`EvoIR.Asm.Semantics`. They are about the actual `loadBytes` and
`storeBytes` functions; they add no memory model.
-/
namespace EvoIR.Asm

theorem storeBytes_outside (m : Nat → BitVec 8) (a n v x : Nat) (h : x < a ∨ a + n ≤ x) :
    storeBytes m a n v x = m x := by
  induction n generalizing a v with
  | zero => rfl
  | succ n ih =>
    simp only [storeBytes]
    rw [update_other _ _ _ _ (by omega)]
    exact ih (a+1) (v / 256) (by omega)

theorem loadBytes_congr (m m' : Nat → BitVec 8) (a n : Nat)
    (h : ∀ x, a ≤ x → x < a + n → m x = m' x) : loadBytes m a n = loadBytes m' a n := by
  induction n generalizing a with
  | zero => rfl
  | succ n ih =>
    simp only [loadBytes]
    rw [h a (Nat.le_refl a) (by omega), ih (a+1) (fun x l u => h x (by omega) (by omega))]

theorem loadBytes_lt (m : Nat → BitVec 8) (a n : Nat) : loadBytes m a n < 256^n := by
  induction n generalizing a with
  | zero => simp [loadBytes]
  | succ n ih =>
    simp only [loadBytes, Nat.pow_succ]
    have := (m a).isLt
    have := ih (a+1)
    have : (m a).toNat < 256 := by simpa using (m a).isLt
    have : 256 * loadBytes m (a + 1) n ≤ 256 * (256^n - 1) := Nat.mul_le_mul_left _ (by omega)
    rw [Nat.mul_sub, Nat.mul_one] at this
    have : 256 ≤ 256 * 256^n := by
      have := Nat.one_le_two_pow (n := 8*n)
      rw [show (256:Nat) = 2^8 from rfl, ← Nat.pow_mul]; omega
    rw [Nat.mul_comm (256^n)]; omega

theorem loadBytes_storeBytes (m : Nat → BitVec 8) (a n v : Nat) :
    loadBytes (storeBytes m a n v) a n = v % 256^n := by
  induction n generalizing a v with
  | zero => simp [loadBytes, Nat.mod_one]
  | succ n ih =>
    simp only [loadBytes, storeBytes, update_same, BitVec.toNat_ofNat]
    rw [loadBytes_congr (update (storeBytes m (a+1) n (v/256)) a (BitVec.ofNat 8 (v)))
      (storeBytes m (a+1) n (v/256)) (a+1) n (fun x l _ => update_other _ _ _ _ (by omega)),
      ih (a+1) (v/256)]
    have key : v % 256^(n+1) = v % 256 + 256 * (v / 256 % 256^n) := by
      rw [Nat.pow_succ', Nat.mod_mul]
    rw [key]

theorem loadBytes_storeBytes_disjoint (m : Nat → BitVec 8) (a n b k v : Nat)
    (h : a + n ≤ b ∨ b + k ≤ a) : loadBytes (storeBytes m b k v) a n = loadBytes m a n :=
  loadBytes_congr _ _ a n (fun x l u => storeBytes_outside m b k v x (by omega))

/-- An eight-byte store followed by an eight-byte load at the same address
gives the stored bits. -/
theorem load64_store64 (m : Nat → BitVec 8) (a : Nat) (v : BitVec 64) :
    BitVec.ofNat 64 (loadBytes (storeBytes m a 8 v.toNat) a 8) = v := by
  rw [loadBytes_storeBytes]
  apply BitVec.eq_of_toNat_eq
  have := v.isLt
  simp only [BitVec.toNat_ofNat]
  simp only [Nat.reducePow] at *
  omega

/-- A four-byte store followed by a four-byte load gives the low half,
zero-extended. -/
theorem load32_store64 (m : Nat → BitVec 8) (a : Nat) (v : BitVec 64) :
    BitVec.ofNat 64 (loadBytes (storeBytes m a 4 v.toNat) a 4) = (v.setWidth 32).setWidth 64 := by
  rw [loadBytes_storeBytes]
  apply BitVec.eq_of_toNat_eq
  have := v.isLt
  simp only [BitVec.toNat_ofNat, BitVec.toNat_setWidth]

end EvoIR.Asm

namespace EvoIR.Asm

/-- Code addresses of the encoding decode to their function and line. -/
theorem decode_codeAddress (function pc : Nat) (hpc : pc < functionSpan)
    (hf : function < 2^30) : decodeAddress (codeAddress function pc) = some (function, pc) := by
  simp only [decodeAddress, codeAddress, codeBase, functionSpan, BitVec.toNat_ofNat] at *
  simp only [Nat.reducePow] at *
  have h1 : (4611686018427387904 + 4 * (function * 16777216 + pc)) % 18446744073709551616 =
      4611686018427387904 + 4 * (function * 16777216 + pc) := by omega
  rw [h1]
  have h2 : ¬(4611686018427387904 + 4 * (function * 16777216 + pc) < 4611686018427387904 ∨
      (4611686018427387904 + 4 * (function * 16777216 + pc)) % 4 ≠ 0) := by omega
  simp only [h2, reduceIte, Option.some.injEq, Prod.mk.injEq]
  omega

end EvoIR.Asm
