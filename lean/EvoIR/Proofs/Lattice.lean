import EvoIR.Proofs.Bits

/-!
# The lattice: merge, join, and the side-condition predicates

Segment B of the receipt, apart from the transfer table.  Every theorem here
is width parametric.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## Predicates -/

/-- 11.6 `fully_known`: a fully known fact pins its value. -/
theorem fullyKnownValue {n : Nat} (K : Fact n) (x : BitVec n) (hk : fullyKnown K)
    (hg : gamma K x) : x = K.value := by
  sorry

/-- 11.6 `known_zero(?z)`: a known-zero fact pins the value to zero. -/
theorem knownZeroValue {n : Nat} (K : Fact n) (z : BitVec n) (hk : knownZero K)
    (hg : gamma K z) : z = 0#n := by
  sorry

/-- 11.6 `covers(?m,?x)`: masking `x` with `m` keeps `x`. -/
theorem coversAnd {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : x &&& m = x := by
  sorry

/-- The same with the operands in the other order, for `and(?m,?x)`. -/
theorem coversAndLeft {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : m &&& x = x := by
  sorry

/-- The dual: `or`ing `x` into `m` keeps `m`. -/
theorem coversOr {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : x ||| m = m := by
  sorry

/-- The same with the operands in the other order, for `or(?m,?x)`. -/
theorem coversOrLeft {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : m ||| x = m := by
  sorry

/-! ## Merge and join -/

/-- 8.10 merge: two facts that both hold of one value merge, and the merged
fact holds of it.  The existential is the point: the merge is not a conflict,
and what it produces is sound. -/
theorem MeetSound {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K x) (h' : gamma K' x) :
    ∃ K'', K.meet K' = some K'' ∧ gamma K'' x := by
  sorry

/-- A conflict means no value satisfies both, so two facts about one value
never conflict.  This is what makes the union check sound. -/
theorem MeetConflict {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K x)
    (h' : gamma K' x) : K.meet K' ≠ none := by
  sorry

/-- 8.10 join: what two facts arriving on different edges agree on holds of
the value that arrived on either edge. -/
theorem JoinSoundLeft {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K x) :
    gamma (K.join K') x := by
  sorry

theorem JoinSoundRight {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K' x) :
    gamma (K.join K') x := by
  sorry

/-! ## Transported forms

A rule's two operands are typed at the rule's result width, so one variable's
width is substituted away and the other's is only propositionally equal to it.
These are the same lemmas with that transport applied once. -/

theorem coversAndH {a b : Nat} (h : a = b) (M : Fact a) (X : Fact b) (m : BitVec a)
    (x : BitVec b) (hc : covers (h ▸ M) X) (hm : gamma M m) (hx : gamma X x) :
    x &&& (h ▸ m) = x := by
  subst h; exact coversAnd M X m x hc hm hx

theorem coversAndLeftH {a b : Nat} (h : a = b) (M : Fact a) (X : Fact b) (m : BitVec a)
    (x : BitVec b) (hc : covers (h ▸ M) X) (hm : gamma M m) (hx : gamma X x) :
    (h ▸ m) &&& x = x := by
  subst h; exact coversAndLeft M X m x hc hm hx

theorem coversOrH {a b : Nat} (h : a = b) (M : Fact a) (X : Fact b) (m : BitVec a)
    (x : BitVec b) (hc : covers (h ▸ M) X) (hm : gamma M m) (hx : gamma X x) :
    x ||| (h ▸ m) = (h ▸ m) := by
  subst h; exact coversOr M X m x hc hm hx

theorem coversOrLeftH {a b : Nat} (h : a = b) (M : Fact a) (X : Fact b) (m : BitVec a)
    (x : BitVec b) (hc : covers (h ▸ M) X) (hm : gamma M m) (hx : gamma X x) :
    (h ▸ m) ||| x = (h ▸ m) := by
  subst h; exact coversOrLeft M X m x hc hm hx

theorem knownZeroValueH {a b : Nat} (h : a = b) (K : Fact a) (z : BitVec a)
    (hk : knownZero K) (hg : gamma K z) : (h ▸ z) = 0#b := by
  subst h; exact knownZeroValue K z hk hg

end EvoIR
