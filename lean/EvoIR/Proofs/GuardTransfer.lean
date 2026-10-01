import EvoIR.Proofs.GuardBounds

/-!
Seven g0.ub.v1 transfer rows, proved for arbitrary widths with integer order
reasoning. No finite-width enumeration, native decision oracle or Mathlib.
-/

namespace EvoIR

theorem guard_fact_sound (p : Prop) [Decidable p] (b : Bool)
    (h : p → b = true) : gamma (guardFact p) (BitVec.ofBool b) := by
  unfold guardFact
  split
  · rename_i hp
    rw [h hp]
    rfl
  · simp [gamma, Fact.top]

theorem mul_between (a l u x low high : Int) (hl : l ≤ x) (hu : x ≤ u)
    (el : low ≤ l * a ∧ l * a ≤ high) (eu : low ≤ u * a ∧ u * a ≤ high) :
    low ≤ x * a ∧ x * a ≤ high := by
  by_cases ha : 0 ≤ a
  · have h1 := Int.mul_le_mul_of_nonneg_right hl ha
    have h2 := Int.mul_le_mul_of_nonneg_right hu ha
    omega
  · have ha : a ≤ 0 := by omega
    have h1 := Int.mul_le_mul_of_nonpos_right hl ha
    have h2 := Int.mul_le_mul_of_nonpos_right hu ha
    omega

theorem mul_rectangle (al au bl bu x y low high : Int)
    (hx : al ≤ x ∧ x ≤ au) (hy : bl ≤ y ∧ y ≤ bu)
    (hll : low ≤ al * bl ∧ al * bl ≤ high)
    (hlu : low ≤ al * bu ∧ al * bu ≤ high)
    (hul : low ≤ au * bl ∧ au * bl ≤ high)
    (huu : low ≤ au * bu ∧ au * bu ≤ high) :
    low ≤ x * y ∧ x * y ≤ high := by
  have hl : low ≤ al * y ∧ al * y ≤ high := by
    simpa only [Int.mul_comm] using
      mul_between al bl bu y low high hy.1 hy.2
        (by simpa only [Int.mul_comm] using hll)
        (by simpa only [Int.mul_comm] using hlu)
  have hu : low ≤ au * y ∧ au * y ≤ high := by
    simpa only [Int.mul_comm] using
      mul_between au bl bu y low high hy.1 hy.2
        (by simpa only [Int.mul_comm] using hul)
        (by simpa only [Int.mul_comm] using huu)
  exact mul_between y al au x low high hx.1 hx.2 hl hu

theorem transfer_guard_add : ∀ n : Nat, TransferRowSound (Op.guard .add n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  change gamma (T_guard .add A B)
    (BitVec.ofBool (decide (signedRepresentable n (x.toInt + y.toInt))))
  unfold T_guard
  apply guard_fact_sound
  rintro ⟨hn, hlo, hhi⟩
  have h1 := fact_signed_bounds hn hx
  have h2 := fact_signed_bounds hn hy
  simp only [decide_eq_true_eq, signedRepresentable]
  omega

theorem transfer_guard_sub : ∀ n : Nat, TransferRowSound (Op.guard .sub n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  change gamma (T_guard .sub A B)
    (BitVec.ofBool (decide (signedRepresentable n (x.toInt - y.toInt))))
  unfold T_guard
  apply guard_fact_sound
  rintro ⟨hn, hlo, hhi⟩
  have h1 := fact_signed_bounds hn hx
  have h2 := fact_signed_bounds hn hy
  simp only [decide_eq_true_eq, signedRepresentable]
  omega

theorem transfer_guard_mul : ∀ n : Nat, TransferRowSound (Op.guard .mul n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  change gamma (T_guard .mul A B)
    (BitVec.ofBool (decide (signedRepresentable n (x.toInt * y.toInt))))
  unfold T_guard
  apply guard_fact_sound
  rintro ⟨hn, hll, hlu, hul, huu⟩
  simp only [decide_eq_true_eq, signedRepresentable] at *
  exact mul_rectangle _ _ _ _ _ _ _ _ (fact_signed_bounds hn hx)
    (fact_signed_bounds hn hy) hll hlu hul huu

theorem transfer_guard_div_rem : ∀ n : Nat, TransferRowSound (Op.guard .divRem n) := by
  intro n
  rintro ⟨A, B⟩ ⟨x, y⟩ ⟨hx, hy⟩
  change gamma (T_guard .divRem A B)
    (BitVec.ofBool (decide (¬ (x.toInt = signedMin n ∧ y.toInt = -1))))
  unfold T_guard
  apply guard_fact_sound
  rintro ⟨hn, he⟩
  simp only [decide_eq_true_eq]
  rintro ⟨ex, ey⟩
  have ex' : x = BitVec.intMin n := by
    apply BitVec.eq_of_toInt_eq
    simpa only [BitVec.toInt_intMin_of_pos hn, signedMin] using ex
  have ey' : y = BitVec.allOnes n := by
    apply BitVec.eq_of_toInt_eq
    simpa [BitVec.toInt_allOnes, hn] using ey
  rcases he with he | he
  · exact he (ex' ▸ hx)
  · exact he (ey' ▸ hy)

theorem transfer_guard_nonzero : ∀ n : Nat, TransferRowSound (Op.nonzero n) := by
  intro n
  unfold TransferRowSound TransferSound
  simp only [Op.factDom, Op.dom, Op.res, gammaDom, transfer, denoteOp]
  intro A x hx
  unfold T_nonzero denoteNonzero
  apply guard_fact_sound
  intro ha
  have h : x ≠ 0#n := by
    intro ex
    subst x
    have hv : A.value = 0#n := by simpa [gamma] using hx.symm
    exact ha hv
  simpa using h

theorem transfer_guard_unsigned_count : ∀ n m : Nat, TransferRowSound (Op.count false n m) := by
  intro n m
  rintro ⟨A, K⟩ ⟨x, k⟩ ⟨_, hk⟩
  change gamma (T_unsigned_count A K) (BitVec.ofBool (decide (k.toNat < n)))
  unfold T_unsigned_count
  apply guard_fact_sound
  intro hb
  have hi := (fact_unsigned_bounds hk).2
  simp only [decide_eq_true_eq]
  omega

theorem transfer_guard_signed_count : ∀ n m : Nat, TransferRowSound (Op.count true n m) := by
  intro n m
  rintro ⟨A, K⟩ ⟨x, k⟩ ⟨_, hk⟩
  change gamma (T_signed_count A K)
    (BitVec.ofBool (decide (0 ≤ k.toInt ∧ k.toInt < (n : Int))))
  unfold T_signed_count
  apply guard_fact_sound
  rintro ⟨hm, hv, hb⟩
  have hk' : k.msb = false := by simpa [hm, hv] using gamma_msb hk
  have hi := (fact_unsigned_bounds hk).2
  simp only [decide_eq_true_eq, BitVec.toInt_eq_toNat_of_msb hk']
  omega

end EvoIR
