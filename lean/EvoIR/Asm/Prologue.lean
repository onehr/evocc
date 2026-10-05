import EvoIR.Asm.Branch

/-!
Runs of the prologue and the epilogue. The prologue stores the frame record
below the entry stack pointer, sets the frame pointer, lowers the stack
pointer by the frame size and stores each saved register into its slot. The
epilogue loads each saved register, raises the stack pointer and loads the
frame record.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem runFrom_append (fn : Nat) :
    ∀ (A B : List Inst) (pos : Nat) (t : Asm.State),
      runFrom fn pos (A ++ B) t = (runFrom fn pos A t) >>= fun t' => runFrom fn (pos + A.length) B t'
  | [], B, pos, t => by simp [runFrom, bind, Except.bind]
  | i :: A, B, pos, t => by
    simp only [List.cons_append, runFrom, bind, Except.bind]
    cases effect t i with
    | error e => rfl
    | ok t1 =>
      simp only
      rw [runFrom_append fn A B (pos+1)]
      simp only [bind, Except.bind, List.length_cons]
      rw [show pos + 1 + A.length = pos + (A.length + 1) by omega]

theorem runFrom_control (fn : Nat) :
    ∀ (code : List Inst) (pos : Nat) (t t' : Asm.State), t.control = .running fn pos →
      runFrom fn pos code t = .ok t' → t'.control = .running fn (pos + code.length)
  | [], pos, t, t', hc, hr => by simp only [runFrom, Except.ok.injEq] at hr; subst hr; simpa using hc
  | i :: code, pos, t, t', hc, hr => by
    simp only [runFrom, bind, Except.bind] at hr
    split at hr
    · cases hr
    · have := runFrom_control fn code (pos+1) _ t' rfl hr
      simpa [Nat.add_assoc, Nat.add_comm 1] using this

/-- The save stores of the prologue. -/
theorem saves_run (fn S : Nat) :
    ∀ (L : List (Nat × Nat)) (pos : Nat) (t : Asm.State),
      t.sp = BitVec.ofNat 64 S → S % 16 = 0 → t.high ≤ 2^64 →
      (∀ x ∈ L, t.low ≤ S + x.2 ∧ S + x.2 + 8 ≤ t.high) →
      L.Pairwise (fun x y => Disjoint (x.2, 8) (y.2, 8)) →
      ∃ t', runFrom fn pos (L.map (fun x => Inst.str .x (.r x.1) .sp (x.2 : Int))) t = .ok t' ∧
        t'.regs = t.regs ∧ t'.sp = t.sp ∧ t'.flags = t.flags ∧ t'.low = t.low ∧ t'.high = t.high ∧
        (∀ x ∈ L, load64 t'.memory (S + x.2) = t.regs x.1) ∧
        (∀ a, (∀ x ∈ L, a < S + x.2 ∨ S + x.2 + 8 ≤ a) → t'.memory a = t.memory a)
  | [], pos, t, _, _, _, _, _ => ⟨t, rfl, rfl, rfl, rfl, rfl, rfl, by simp, fun _ _ => rfl⟩
  | (r, off) :: L, pos, t, hsp, h16, high, inside, pw => by
    rcases List.pairwise_cons.mp pw with ⟨first, pw'⟩
    have hin := inside (r, off) (by simp)
    simp only at hin
    have hS : S < 2^64 := by omega
    have hbase : t.baseValue .sp = .ok t.sp := by
      have : t.sp.toNat % 16 = 0 := by rw [hsp, BitVec.toNat_ofNat, Nat.mod_eq_of_lt hS]; exact h16
      simp [State.baseValue, this]
    have haddr : (t.sp + BitVec.ofInt 64 (off : Int)).toNat = S + off := by
      rw [hsp]; exact address_nat _ _ (by omega)
    let t1 : Asm.State := {t with memory := storeBytes t.memory (S + off) 8 (t.regs r).toNat}
    have heff : effect t (.str .x (.r r) .sp (off : Int)) = .ok t1 := by
      simp only [effect, hbase, bind, Except.bind]
      rw [store_in t _ _ _ (by rw [haddr]; omega) (by rw [haddr]; simp; omega), haddr]
      rfl
    obtain ⟨t', hrun, hregs, hsp', hfl, hlow, hhigh, hload, hkeep⟩ :=
      saves_run fn S L (pos+1) {t1 with control := .running fn (pos+1)} hsp h16 high
        (fun x m => inside x (by simp [m])) pw'
    refine ⟨t', ?_, hregs, hsp', hfl, hlow, hhigh, ?_, ?_⟩
    · simp only [List.map_cons, runFrom, heff, bind, Except.bind]; exact hrun
    · intro x m
      rcases List.mem_cons.mp m with rfl | m
      · simp only
        rw [load64_congr _ _ _ (fun j hj => hkeep _ (fun y my => by
          have := first y my; unfold Disjoint at this; simp at this; omega))]
        simp only [t1]
        unfold load64
        rw [load64_store64]
      · rw [hload x m]
    · intro a ha
      rw [hkeep a (fun y my => ha y (by simp [my]))]
      exact storeBytes_outside _ _ _ _ _ (by have := ha (r, off) (by simp); simp at this; omega)

theorem lookup_cons_self (k v : Nat) (L : List (Nat × Nat)) :
    lookupOffset ((k, v) :: L) k = some v := by
  simp [lookupOffset, List.lookup]

theorem lookup_cons_other (k v r : Nat) (L : List (Nat × Nat)) (ne : r ≠ k) :
    lookupOffset ((k, v) :: L) r = lookupOffset L r := by
  have : (r == k) = false := by simpa using ne
  simp [lookupOffset, List.lookup, this]

theorem lookup_none (L : List (Nat × Nat)) (k : Nat) (h : k ∉ L.map Prod.fst) :
    lookupOffset L k = none := by
  induction L with
  | nil => rfl
  | cons x L ih =>
    obtain ⟨a, b⟩ := x
    have ne : k ≠ a := by intro e; apply h; simp [e]
    rw [lookup_cons_other a b k L ne]
    exact ih (by intro m; apply h; simp [m])

/-- The restore loads of the epilogue. -/
theorem restores_run (fn S : Nat) :
    ∀ (L : List (Nat × Nat)) (pos : Nat) (t : Asm.State),
      t.sp = BitVec.ofNat 64 S → S % 16 = 0 → t.high ≤ 2^64 →
      (∀ x ∈ L, t.low ≤ S + x.2 ∧ S + x.2 + 8 ≤ t.high) →
      keysUnique (L.map Prod.fst) = true →
      ∃ t', runFrom fn pos (L.map (fun x => Inst.ldr .x (.r x.1) .sp (x.2 : Int))) t = .ok t' ∧
        t'.memory = t.memory ∧ t'.sp = t.sp ∧ t'.flags = t.flags ∧ t'.low = t.low ∧
        t'.high = t.high ∧
        ∀ r, t'.regs r = match lookupOffset L r with
          | some off => load64 t.memory (S + off)
          | none => t.regs r
  | [], pos, t, _, _, _, _, _ => ⟨t, rfl, rfl, rfl, rfl, rfl, rfl, fun r => by simp [lookupOffset]⟩
  | (k, off) :: L, pos, t, hsp, h16, high, inside, keys => by
    simp only [List.map_cons, keysUnique, Bool.and_eq_true, Bool.not_eq_true',
      decide_eq_false_iff_not] at keys
    have hin := inside (k, off) (by simp)
    simp only at hin
    have hS : S < 2^64 := by omega
    have hbase : t.baseValue .sp = .ok t.sp := by
      have : t.sp.toNat % 16 = 0 := by rw [hsp, BitVec.toNat_ofNat, Nat.mod_eq_of_lt hS]; exact h16
      simp [State.baseValue, this]
    have haddr : (t.sp + BitVec.ofInt 64 (off : Int)).toNat = S + off := by
      rw [hsp]; exact address_nat _ _ (by omega)
    let t1 : Asm.State := t.set .x (.r k) (load64 t.memory (S + off))
    have heff : effect t (.ldr .x (.r k) .sp (off : Int)) = .ok t1 := by
      simp only [effect, hbase, bind, Except.bind]
      rw [load_in t _ _ (by rw [haddr]; omega) (by rw [haddr]; simp; omega), haddr]
      rfl
    obtain ⟨t', hrun, hmem, hsp', hfl, hlow, hhigh, hregs⟩ :=
      restores_run fn S L (pos+1) {t1 with control := .running fn (pos+1)} hsp h16 high
        (fun x m => inside x (by simp [m])) keys.2
    refine ⟨t', ?_, hmem, hsp', hfl, hlow, hhigh, fun r => ?_⟩
    · simp only [List.map_cons, runFrom, heff, bind, Except.bind]; exact hrun
    · rw [hregs r]
      by_cases e : r = k
      · subst e
        rw [lookup_none L r keys.1, lookup_cons_self]
        simp [t1, view]
      · rw [lookup_cons_other k off r L e]
        cases lookupOffset L r with
        | some o => rfl
        | none => simp [t1, update_other _ _ _ _ e]

end EvoIR.Asm.Proof
