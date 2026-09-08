import EvoIR.Proofs.Bits

/-!
# The lattice: merge, join, and the side-condition predicates

Segment B of the receipt, apart from the transfer table.  Every theorem here
is width parametric, proved one bit at a time from the definitions rather than
by a decision procedure at a fixed width.
-/

set_option linter.unusedSimpArgs false

namespace EvoIR

/-! ## Predicates -/

/-- 11.6 `fully_known`: a fully known fact pins its value. -/
theorem fullyKnownValue {n : Nat} (K : Fact n) (x : BitVec n) (hk : fullyKnown K)
    (hg : gamma K x) : x = K.value := by
  simp only [gamma, fullyKnown] at hg hk
  rw [hk, BitVec.and_allOnes] at hg
  exact hg

/-- 11.6 `known_zero(?z)`: a known-zero fact pins the value to zero. -/
theorem knownZeroValue {n : Nat} (K : Fact n) (z : BitVec n) (hk : knownZero K)
    (hg : gamma K z) : z = 0#n := by
  simp only [gamma, knownZero] at hg hk
  rw [hk.1, BitVec.and_allOnes] at hg
  rw [hg, hk.2]

/-- 11.6 `covers(?m,?x)`: masking `x` with `m` keeps `x`.

A bit set in `x` cannot be known zero in `X`, so `covers` makes it known one
in `M`, so it is set in `m`. -/
theorem coversAnd {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : x &&& m = x := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [covers] at hc
  have ec := congrArg (fun z => BitVec.getLsbD z i) hc
  have em := gamma_bit hm i
  have ex := gamma_bit hx i
  have en := norm_bit M hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_not, BitVec.getLsbD_zero,
    hi, decide_true, Bool.true_and] at ec ⊢
  revert ec em ex en
  cases M.mask.getLsbD i <;> cases M.value.getLsbD i <;> cases X.mask.getLsbD i <;>
    cases X.value.getLsbD i <;> cases m.getLsbD i <;> cases x.getLsbD i <;> simp

/-- The same with the operands in the other order, for `and(?m,?x)`. -/
theorem coversAndLeft {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : m &&& x = x := by
  rw [BitVec.and_comm]
  exact coversAnd M X m x hc hm hx

/-- The dual: `or`ing `x` into `m` keeps `m`. -/
theorem coversOr {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : x ||| m = m := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [covers] at hc
  have ec := congrArg (fun z => BitVec.getLsbD z i) hc
  have em := gamma_bit hm i
  have ex := gamma_bit hx i
  have en := norm_bit M hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_not, BitVec.getLsbD_zero,
    hi, decide_true, Bool.true_and] at ec ⊢
  revert ec em ex en
  cases M.mask.getLsbD i <;> cases M.value.getLsbD i <;> cases X.mask.getLsbD i <;>
    cases X.value.getLsbD i <;> cases m.getLsbD i <;> cases x.getLsbD i <;> simp

/-- The same with the operands in the other order, for `or(?m,?x)`. -/
theorem coversOrLeft {n : Nat} (M X : Fact n) (m x : BitVec n) (hc : covers M X)
    (hm : gamma M m) (hx : gamma X x) : m ||| x = m := by
  rw [BitVec.or_comm]
  exact coversOr M X m x hc hm hx

/-! ## Merge and join -/

/-- 8.10 merge: two facts that both hold of one value merge, and the merged
fact holds of it.  The existential is the point: the merge is not a conflict,
and what it produces is sound. -/
theorem MeetSound {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K x) (h' : gamma K' x) :
    ∃ K'', K.meet K' = some K'' ∧ gamma K'' x := by
  have hcond : (K.value ^^^ K'.value) &&& K.mask &&& K'.mask = 0#n := by
    apply BitVec.eq_of_getLsbD_eq
    intro i hi
    have e := gamma_bit h i
    have e' := gamma_bit h' i
    simp only [BitVec.getLsbD_and, BitVec.getLsbD_xor, BitVec.getLsbD_zero]
    revert e e'
    cases K.mask.getLsbD i <;> cases K.value.getLsbD i <;> cases K'.mask.getLsbD i <;>
      cases K'.value.getLsbD i <;> cases x.getLsbD i <;> simp
  refine ⟨Fact.mk' (K.mask ||| K'.mask) (K.value ||| K'.value), by simp [Fact.meet, hcond], ?_⟩
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, Fact.mk']
  have e := gamma_bit h i
  have e' := gamma_bit h' i
  have n1 := norm_bit K hi
  have n2 := norm_bit K' hi
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or]
  revert e e' n1 n2
  cases K.mask.getLsbD i <;> cases K.value.getLsbD i <;> cases K'.mask.getLsbD i <;>
    cases K'.value.getLsbD i <;> cases x.getLsbD i <;> simp

/-- A conflict means no value satisfies both, so two facts about one value
never conflict.  This is what makes the union check sound. -/
theorem MeetConflict {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K x)
    (h' : gamma K' x) : K.meet K' ≠ none := by
  obtain ⟨K'', hK, _⟩ := MeetSound K K' x h h'
  rw [hK]
  simp

/-- 8.10 join: what two facts arriving on different edges agree on holds of
the value that arrived on either edge. -/
theorem JoinSoundLeft {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K x) :
    gamma (K.join K') x := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, Fact.join, Fact.mk']
  have e := gamma_bit h i
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_xor, BitVec.getLsbD_not,
    hi, decide_true, Bool.true_and]
  revert e
  cases K.mask.getLsbD i <;> cases K.value.getLsbD i <;> cases K'.mask.getLsbD i <;>
    cases K'.value.getLsbD i <;> cases x.getLsbD i <;> simp

theorem JoinSoundRight {n : Nat} (K K' : Fact n) (x : BitVec n) (h : gamma K' x) :
    gamma (K.join K') x := by
  apply BitVec.eq_of_getLsbD_eq
  intro i hi
  simp only [gamma, Fact.join, Fact.mk']
  have e := gamma_bit h i
  simp only [BitVec.getLsbD_and, BitVec.getLsbD_or, BitVec.getLsbD_xor, BitVec.getLsbD_not,
    hi, decide_true, Bool.true_and]
  revert e
  cases K.mask.getLsbD i <;> cases K.value.getLsbD i <;> cases K'.mask.getLsbD i <;>
    cases K'.value.getLsbD i <;> cases x.getLsbD i <;> simp

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
