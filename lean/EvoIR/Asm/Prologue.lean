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
      t.sp = BitVec.ofNat 64 S → S % 16 = 0 → t.high < 2^64 →
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
      t.sp = BitVec.ofNat 64 S → S % 16 = 0 → t.high < 2^64 →
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

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem address_sub (E k : Nat) (hk : k ≤ E) (hE : E < 2^64) :
    (BitVec.ofNat 64 E + BitVec.ofInt 64 (-(k : Int))).toNat = E - k := by
  have : BitVec.ofInt 64 (-(k : Int)) = BitVec.ofNat 64 (2^64 - k) := by
    apply BitVec.eq_of_toNat_eq
    simp only [BitVec.toNat_ofInt, BitVec.toNat_ofNat]
    omega
  rw [this, BitVec.toNat_add, BitVec.toNat_ofNat, BitVec.toNat_ofNat]
  omega

theorem sub_nat (E k : Nat) (hk : k ≤ E) (hE : E < 2^64) :
    BitVec.ofNat 64 E - BitVec.ofNat 64 k = BitVec.ofNat 64 (E - k) := by
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_sub, BitVec.toNat_ofNat]
  omega

theorem add_nat (E k : Nat) (hE : E + k < 2^64) :
    BitVec.ofNat 64 E + BitVec.ofNat 64 k = BitVec.ofNat 64 (E + k) := by
  apply BitVec.eq_of_toNat_eq
  simp only [BitVec.toNat_add, BitVec.toNat_ofNat]
  omega

theorem ofInt_neg16 : BitVec.ofInt 64 (-16 : Int) = BitVec.ofInt 64 (-((16 : Nat) : Int)) := rfl

theorem stp_effect (t : Asm.State) (E : Nat) (hsp : t.sp = BitVec.ofNat 64 E) (h16 : E % 16 = 0)
    (low : t.low + 16 ≤ E) (top : E ≤ t.high) (high : t.high < 2^64) :
    effect t (.stpPre 29 30 (-16)) = .ok {t with
      memory := storeBytes (storeBytes t.memory (E - 16) 8 (t.regs 29).toNat) (E - 8) 8
        (t.regs 30).toNat,
      sp := BitVec.ofNat 64 (E - 16)} := by
  have hE : E < 2^64 := by omega
  have hbase : t.baseValue .sp = .ok t.sp := by
    have : t.sp.toNat % 16 = 0 := by rw [hsp, BitVec.toNat_ofNat, Nat.mod_eq_of_lt hE]; exact h16
    simp [State.baseValue, this]
  have a16 : (t.sp + BitVec.ofInt 64 (-16 : Int)).toNat = E - 16 := by
    rw [hsp, ofInt_neg16]; exact address_sub E 16 (by omega) hE
  have eight : (8 : BitVec 64).toNat = 8 := rfl
  have a8 : (t.sp + BitVec.ofInt 64 (-16 : Int) + 8).toNat = E - 8 := by
    rw [BitVec.toNat_add, a16, eight]; omega
  have aeq : t.sp + BitVec.ofInt 64 (-16 : Int) = BitVec.ofNat 64 (E - 16) := by
    apply BitVec.eq_of_toNat_eq; rw [a16, BitVec.toNat_ofNat]; omega
  simp only [effect, hbase, bind, Except.bind]
  rw [store_in t _ _ _ (by rw [a16]; omega) (by rw [a16]; omega), a16]
  simp only
  rw [store_in _ _ _ _ (by simp only; rw [a8]; omega) (by simp only; rw [a8]; omega), a8]
  simp only [pure, Except.pure, aeq]

/-- The prologue run from the entry stack pointer `E`. -/
theorem prologue_run (fn : Nat) (h : FrameHint) (layout : LayoutOk h) (t : Asm.State) (E : Nat)
    (hsp : t.sp = BitVec.ofNat 64 E) (h16 : E % 16 = 0)
    (low : t.low + h.size + 16 ≤ E) (top : E ≤ t.high) (high : t.high < 2^64) :
    ∃ t', runFrom fn 0 (prologue h) t = .ok t' ∧
      t'.sp = BitVec.ofNat 64 (E - 16 - h.size) ∧ t'.flags = t.flags ∧ t'.low = t.low ∧
      t'.high = t.high ∧ t'.regs = update t.regs 29 (BitVec.ofNat 64 (E - 16)) ∧
      load64 t'.memory (E - 16) = t.regs 29 ∧ load64 t'.memory (E - 8) = t.regs 30 ∧
      (∀ x ∈ h.saved, load64 t'.memory (E - 16 - h.size + x.2) = t.regs x.1) ∧
      (∀ a, E ≤ a → t'.memory a = t.memory a) := by
  have hE : E < 2^64 := by omega
  let m2 := storeBytes (storeBytes t.memory (E - 16) 8 (t.regs 29).toNat) (E - 8) 8 (t.regs 30).toNat
  have hrec29 : load64 m2 (E - 16) = t.regs 29 := by
    rw [load64_congr (storeBytes t.memory (E - 16) 8 (t.regs 29).toNat) m2 _
      (fun j _ => storeBytes_outside _ _ _ _ _ (by omega))]
    unfold load64; exact load64_store64 _ _ _
  have hrec30 : load64 m2 (E - 8) = t.regs 30 := by
    unfold load64; exact load64_store64 _ _ _
  have hkeep2 : ∀ a, E ≤ a → m2 a = t.memory a := by
    intro a ha
    simp only [m2]
    rw [storeBytes_outside _ _ _ _ _ (by omega), storeBytes_outside _ _ _ _ _ (by omega)]
  -- The state after the record, the frame pointer and the frame size.
  let sizeCode : List Inst := if h.size = 0 then [] else [.subSp h.size]
  let t4 : Asm.State := {t with
    memory := m2, sp := BitVec.ofNat 64 (E - 16 - h.size),
    regs := update t.regs 29 (BitVec.ofNat 64 (E - 16)),
    control := .running fn (2 + sizeCode.length)}
  have head : runFrom fn 0 ([.stpPre 29 30 (-16), .movFromSp 29] ++ sizeCode) t = .ok t4 := by
    rw [runFrom_append]
    simp only [runFrom, stp_effect t E hsp h16 (by omega) top high, bind, Except.bind]
    by_cases z : h.size = 0
    · simp only [sizeCode, z, if_true, runFrom, List.length_nil]
      simp only [t4, sizeCode, z, if_true, List.length_nil, Nat.sub_zero]
      rfl
    · have hsub : BitVec.ofNat 64 (E - 16) - BitVec.ofNat 64 h.size =
          BitVec.ofNat 64 (E - 16 - h.size) := sub_nat _ _ (by omega) (by omega)
      simp only [sizeCode, z, if_false, runFrom, effect, bind, Except.bind, pure, Except.pure]
      simp only [t4, sizeCode, z, if_false, List.length_singleton, List.length_cons, List.length_nil]
      simp only [State.set, view, hsub]
      rfl
  obtain ⟨t', hrun, hregs, hsp', hfl, hlow, hhigh, hload, hkeep⟩ :=
    saves_run fn (E - 16 - h.size) h.saved (2 + sizeCode.length) t4 rfl
      (by have := layout.size16; omega) high
      (fun x m => by have := layout.save x.1 x.2 m; simp only [t4]; omega) layout.savedPw
  refine ⟨t', ?_, hsp', hfl, hlow, hhigh, hregs, ?_, ?_, ?_, ?_⟩
  · have : prologue h = ([.stpPre 29 30 (-16), .movFromSp 29] ++ sizeCode) ++
        h.saved.map (fun x => Inst.str .x (.r x.1) .sp (x.2 : Int)) := by
      simp only [prologue, sizeCode, List.append_assoc]
    rw [this, runFrom_append, head]
    simp only [bind, Except.bind, List.length_append, List.length_cons, List.length_nil]
    rw [show 0 + (0 + 1 + 1 + sizeCode.length) = 2 + sizeCode.length by omega]
    exact hrun
  · rw [load64_congr _ _ _ (fun j _ => hkeep _ (fun x m => by
      have := (layout.save x.1 x.2 m).1; omega))]
    exact hrec29
  · rw [load64_congr _ _ _ (fun j _ => hkeep _ (fun x m => by
      have := (layout.save x.1 x.2 m).1; omega))]
    exact hrec30
  · intro x m
    rw [hload x m]
    have := (layout.save x.1 x.2 m).2.2
    have ne : x.1 ≠ 29 := by intro e; rw [e] at this; simp [isCalleeSaved] at this
    simp only [t4, update_other _ _ _ _ ne]
  · intro a ha
    rw [hkeep a (fun x m => by have := (layout.save x.1 x.2 m).1; omega)]
    exact hkeep2 a ha

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

/-- The registers after the restores of the epilogue. -/
def restored (h : FrameHint) (S : Nat) (mem : Nat → BitVec 8) (regs : Nat → BitVec 64) :
    Nat → BitVec 64 := fun r =>
  match lookupOffset h.saved r with
  | some off => load64 mem (S + off)
  | none => regs r

/-- The epilogue run from the body stack pointer `S`. -/
theorem epilogue_run (fn pos : Nat) (h : FrameHint) (layout : LayoutOk h) (t : Asm.State) (S : Nat)
    (hc : t.control = .running fn pos) (hsp : t.sp = BitVec.ofNat 64 S) (h16 : S % 16 = 0)
    (low : t.low ≤ S) (top : S + h.size + 16 ≤ t.high) (high : t.high < 2^64) :
    ∃ t', runFrom fn pos (epilogue h) t = .ok t' ∧
      t'.sp = BitVec.ofNat 64 (S + h.size + 16) ∧ t'.flags = t.flags ∧ t'.low = t.low ∧
      t'.high = t.high ∧ t'.memory = t.memory ∧
      t'.regs = update (update (restored h S t.memory t.regs) 29 (load64 t.memory (S + h.size))) 30
        (load64 t.memory (S + h.size + 8)) := by
  obtain ⟨t1, hrun1, hmem1, hsp1, hfl1, hlow1, hhigh1, hregs1⟩ :=
    restores_run fn S h.saved pos t hsp h16 high
      (fun x m => by have := layout.save x.1 x.2 m; omega) layout.savedKeys
  let F := S + h.size
  have hF : F % 16 = 0 := by have := layout.size16; omega
  let sizeCode : List Inst := if h.size = 0 then [] else [.addSp h.size]
  let t2 : Asm.State := { t1 with
    sp := BitVec.ofNat 64 F
    control := Control.running fn (pos + h.saved.length + sizeCode.length) }
  have hsize : runFrom fn (pos + h.saved.length) sizeCode t1 = .ok t2 := by
    have c1 := runFrom_control fn _ pos t t1 hc hrun1
    by_cases z : h.size = 0
    · simp only [sizeCode, z, if_true, runFrom, Except.ok.injEq]
      simp only [t2, sizeCode, z, if_true, List.length_nil, Nat.add_zero, F]
      simp only [List.length_map] at c1
      rw [← c1, ← hsp, ← hsp1]
    · have hadd : BitVec.ofNat 64 S + BitVec.ofNat 64 h.size = BitVec.ofNat 64 F :=
        add_nat _ _ (by omega)
      simp only [sizeCode, z, if_false, runFrom, effect, bind, Except.bind, pure, Except.pure]
      simp only [t2, sizeCode, z, if_false, List.length_cons, List.length_nil, hsp1, hsp, hadd]
  have hbase : t2.baseValue .sp = .ok t2.sp := by
    have : t2.sp.toNat % 16 = 0 := by
      simp only [t2, BitVec.toNat_ofNat]
      rw [Nat.mod_eq_of_lt (a := F) (b := 2^64) (by omega)]; exact hF
    simp [State.baseValue, this]
  have eight : (t2.sp + 8).toNat = F + 8 := by
    simp only [t2, BitVec.toNat_add, BitVec.toNat_ofNat]
    have : (8 : BitVec 64).toNat = 8 := rfl
    omega
  have spnat : t2.sp.toNat = F := by simp only [t2, BitVec.toNat_ofNat]; omega
  have hmem2 : t2.memory = t.memory := hmem1
  let t3 : Asm.State := {((t2.set .x (.r 29) (load64 t.memory F)).set .x (.r 30)
      (load64 t.memory (F + 8))) with sp := (BitVec.ofNat 64 (F + 16))}
  have hldp : effect t2 (.ldpPost 29 30 16) = .ok t3 := by
    simp only [effect, hbase, bind, Except.bind]
    rw [load_in t2 _ _ (by rw [spnat]; simp [t2]; omega) (by rw [spnat]; simp [t2]; omega), spnat,
      load_in t2 _ _ (by rw [eight]; simp [t2]; omega) (by rw [eight]; simp [t2]; omega), eight,
      hmem2]
    simp only [pure, Except.pure, t3]
    congr 2
    simp only [t2]
    have : BitVec.ofInt 64 (16 : Int) = BitVec.ofNat 64 16 := rfl
    rw [this, add_nat _ _ (by omega)]
  refine ⟨{t3 with control := (Control.running fn (pos + h.saved.length + sizeCode.length + 1))}, ?_, ?_,
    ?_, ?_, ?_, ?_, ?_⟩
  · have : epilogue h = (h.saved.map (fun x => Inst.ldr .x (.r x.1) .sp (x.2 : Int)) ++ sizeCode) ++
        [.ldpPost 29 30 16] := by
      simp only [epilogue, sizeCode, List.append_assoc]
    rw [this, runFrom_append, runFrom_append, hrun1]
    simp only [bind, Except.bind, List.length_map]
    rw [hsize]
    simp only [runFrom, hldp, bind, Except.bind, List.length_append, List.length_map, Nat.add_assoc]
  · rfl
  · simp only [t3, t2, State.set]; exact hfl1
  · simp only [t3, t2, State.set]; exact hlow1
  · simp only [t3, t2, State.set]; exact hhigh1
  · simp only [t3, t2, State.set]; exact hmem1
  · simp only [t3, t2, State.set, view]
    congr 2
    funext r
    rw [hregs1 r]
    rfl

end EvoIR.Asm.Proof
