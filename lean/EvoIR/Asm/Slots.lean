import EvoIR.Asm.Body

/-!
Spill slots, incoming stack arguments and outgoing stack arguments. Each is
eight bytes of the stack region at a fixed offset: spill slot `k` at
`sp + off(k)`, outgoing slot `k` at `sp + 8k`, incoming slot `k` at
`x29 + 16 + 8k`, which is the outgoing area of the caller.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem address_nat (a off : Nat) (h : a + off < 2^64) :
    (BitVec.ofNat 64 a + BitVec.ofInt 64 (off : Int)).toNat = a + off := by
  simp [BitVec.toNat_add]
  omega

theorem Local.sp_lt {cur : Frame} {st : Storage} {t : Asm.State} (hl : Local cur st t) :
    cur.entry < 2^64 := Nat.lt_of_le_of_lt hl.top hl.high

theorem Local.baseValue_sp {cur : Frame} {st : Storage} {t : Asm.State} (hl : Local cur st t) :
    t.baseValue .sp = .ok t.sp := by
  have := hl.sp_lt
  have hs : t.sp.toNat % 16 = 0 := by
    rw [hl.sp, BitVec.toNat_ofNat,
      Nat.mod_eq_of_lt (a := cur.sp) (b := 2^64) (by unfold Frame.entry at this; omega)]
    exact hl.sp16
  simp [State.baseValue, hs]

theorem store_in (t : Asm.State) (a : BitVec 64) (n : Nat) (v : BitVec 64)
    (lo : t.low ≤ a.toNat) (hi : a.toNat + n ≤ t.high) :
    t.store a n v = .ok {t with memory := storeBytes t.memory a.toNat n v.toNat} := by
  simp [State.store, State.inStack, lo, hi]

theorem load_in (t : Asm.State) (a : BitVec 64) (n : Nat)
    (lo : t.low ≤ a.toNat) (hi : a.toNat + n ≤ t.high) :
    t.load a n = .ok (BitVec.ofNat 64 (loadBytes t.memory a.toNat n)) := by
  simp [State.load, State.inStack, lo, hi]

theorem loadBytes_add (m : Nat → BitVec 8) :
    ∀ (a n k : Nat), loadBytes m a (n + k) = loadBytes m a n + 256^n * loadBytes m (a + n) k
  | a, 0, k => by simp [loadBytes]
  | a, n+1, k => by
    rw [Nat.add_right_comm, loadBytes, loadBytes, loadBytes_add m (a+1) n k, Nat.pow_succ]
    simp only [Nat.add_right_comm a 1 n, Nat.add_assoc a n 1, Nat.add_comm n 1]
    rw [Nat.mul_add, ← Nat.mul_assoc, Nat.mul_comm 256 (256^n), Nat.add_assoc]

theorem load32_load64 (m : Nat → BitVec 8) (a : Nat) : load32 m a = view .w (load64 m a) := by
  unfold load32 load64 view
  apply BitVec.eq_of_toNat_eq
  have split := loadBytes_add m a 4 4
  have low := loadBytes_lt m a 4
  have high := loadBytes_lt m (a + 4) 4
  simp only [Nat.reduceAdd, Nat.reducePow] at split low high
  simp only [BitVec.toNat_ofNat, BitVec.toNat_setWidth, Nat.reducePow]
  omega

@[simp] theorem widthBytes_x : widthBytes .x = 8 := rfl
@[simp] theorem widthBytes_w : widthBytes .w = 4 := rfl

/-- The value that a slot read through view `dv` gives. -/
theorem slot_value (mem : Nat → BitVec 8) (A : Nat) (b : BitVec 64) (dd dv : Machine.Width)
    (rel : SlotRel mem A ⟨.value (.int b), dd⟩) (fits : dv = .x → dd = .x) :
    view dv (BitVec.ofNat 64 (loadBytes mem A (widthBytes dv))) = Machine.normalizeAt dv b := by
  cases dv <;> cases dd <;> simp only [SlotRel] at rel
  · simp only [widthBytes_w]
    change view .w (load32 mem A) = _
    rw [rel, view_eq, view_eq, normalizeAt_idem]
  · simp only [widthBytes_w]
    change view .w (load32 mem A) = _
    rw [load32_load64, rel, view_eq, view_eq, normalizeAt_idem]
  · cases fits rfl
  · simp only [widthBytes_x]
    change view .x (load64 mem A) = _
    rw [rel, view_eq]

/-- The cell after a store through view `sv` of the bits `v`. -/
theorem slot_store (mem : Nat → BitVec 8) (A : Nat) (v : BitVec 64) (sv : Machine.Width) :
    SlotRel (storeBytes mem A (widthBytes sv) (view sv v).toNat) A
      ⟨.value (.int (Machine.normalizeAt sv v)), sv⟩ := by
  cases sv
  · simp only [SlotRel, widthBytes_w]
    change load32 _ A = _
    unfold load32
    rw [load32_store64, ← view_eq]
    simp [view]
  · simp only [SlotRel, widthBytes_x]
    change load64 _ A = _
    unfold load64
    rw [load64_store64]
    simp [view, normalizeAt_x]

/-- A read of a stack slot location that succeeds gives a defined integer
cell. A 64-bit read of a 32-bit cell is malformed. -/
theorem readLoc_slot {st : Storage} (shape : Shape st) {loc : Loc} (notFlags : loc ≠ .flags)
    {val : Machine.Value} (h : readLoc st loc = .ok (.value val)) :
    ∃ b dd, st loc.place = some ⟨.value (.int b), dd⟩ ∧ (loc.width = .x → dd = .x) ∧
      val = .int (Machine.normalizeAt loc.width b) := by
  simp only [readLoc] at h
  cases hc : st loc.place with
  | none => simp [hc] at h
  | some cell =>
    obtain ⟨slot, d⟩ := cell
    cases slot with
    | flags _ => simp [hc] at h
    | value v =>
      obtain ⟨b, rfl⟩ := shape.1 _ _ _ hc
      simp only [hc] at h
      refine ⟨b, d, rfl, ?_⟩
      cases loc with
      | flags => exact absurd rfl notFlags
      | gpr _ w | spill _ w | incoming _ w | outgoing _ w =>
        simp only [Loc.width] at h ⊢
        split at h
        · cases h
        · rename_i hwd
          cases h
          refine ⟨fun hx => ?_, by rw [normalize_class]⟩
          cases d
          · exact absurd ⟨hx, rfl⟩ hwd
          · rfl

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem mem_at_unchanged (m : Nat → BitVec 8) (A wb v B : Nat) (hwb : wb ≤ 8)
    (h : A + 8 ≤ B ∨ B + 8 ≤ A) : ∀ j, j < 8 → storeBytes m A wb v (B + j) = m (B + j) :=
  fun j hj => storeBytes_outside m A wb v (B + j) (by omega)

/-- A store of at most eight bytes into one spill or outgoing slot of the
current frame keeps the local relation. -/
theorem local_store {cur : Frame} {st : Storage} {t : Asm.State} (hl : Local cur st t)
    (off wb v : Nat) (hwb : wb ≤ 8) (place : Place) (cell : Cell)
    (inFrame : off + 8 ≤ cur.hint.size)
    (savesDisjoint : ∀ r a, (r, a) ∈ cur.hint.saved → Disjoint (off, 8) (a, 8))
    (spillsDisjoint : ∀ k a, (k, a) ∈ cur.hint.spills → place ≠ .spill k → Disjoint (off, 8) (a, 8))
    (outDisjoint : ∀ k, 8 * k + 8 ≤ cur.hint.outgoing → place ≠ .outgoing k →
      Disjoint (off, 8) (8 * k, 8))
    (newSpill : ∀ k, place = .spill k → lookupOffset cur.hint.spills k = some off)
    (newOut : ∀ k, place = .outgoing k → off = 8 * k ∧ 8 * k + 8 ≤ cur.hint.outgoing)
    (slotPlace : (∃ k, place = .spill k) ∨ (∃ k, place = .outgoing k))
    (rel : SlotRel (storeBytes t.memory (cur.sp + off) wb v) (cur.sp + off) cell)
    (shapeCell : ∀ val d, cell = ⟨.value val, d⟩ → ∃ b, val = .int b) :
    Local cur (fun p => if p = place then some cell else st p)
        {t with memory := storeBytes t.memory (cur.sp + off) wb v} ∧
      Footprint cur t {t with memory := storeBytes t.memory (cur.sp + off) wb v} := by
  have notGpr : ∀ n, Place.gpr n ≠ place := by
    intro n e; rcases slotPlace with ⟨k, rfl⟩ | ⟨k, rfl⟩ <;> cases e
  have notFlags : Place.flags ≠ place := by
    intro e; rcases slotPlace with ⟨k, rfl⟩ | ⟨k, rfl⟩ <;> cases e
  have notIncoming : ∀ k, Place.incoming k ≠ place := by
    intro n e; rcases slotPlace with ⟨k, rfl⟩ | ⟨k, rfl⟩ <;> cases e
  have entry := hl.top
  unfold Frame.entry at entry
  refine ⟨⟨hl.sp, hl.sp16, hl.fp, hl.high, hl.top, hl.bottom, ⟨?_, ?_⟩, ?_, ?_, ?_, ⟨?_, ?_⟩⟩, ?_⟩
  · intro p val d hc
    simp only at hc
    split at hc
    · exact shapeCell val d (Option.some.inj hc)
    · exact hl.shape.1 p val d hc
  · intro n hn; simp only [if_neg (notGpr n)]; exact hl.shape.2 n hn
  · intro n b d hc; simp only [if_neg (notGpr n)] at hc; exact hl.regs n b d hc
  · intro fl d c b hc; simp only [if_neg notFlags] at hc; exact hl.flags fl d c b hc
  · intro k c hc
    simp only at hc
    by_cases e : Place.outgoing k = place
    · rw [if_pos e] at hc
      cases hc
      obtain ⟨rfl, hk⟩ := newOut k e.symm
      exact ⟨hk, rel⟩
    · rw [if_neg e] at hc
      obtain ⟨hk, r⟩ := hl.outgoing k c hc
      refine ⟨hk, SlotRel_congr _ _ _ c (mem_at_unchanged _ _ _ _ _ hwb ?_) r⟩
      have := outDisjoint k hk (Ne.symm e)
      unfold Disjoint at this; omega
  · intro k c hc
    simp only at hc
    by_cases e : Place.spill k = place
    · rw [if_pos e] at hc
      cases hc
      exact ⟨off, newSpill k e.symm, rel⟩
    · rw [if_neg e] at hc
      obtain ⟨a, ha, r⟩ := hl.slots.1 k c hc
      refine ⟨a, ha, SlotRel_congr _ _ _ c (mem_at_unchanged _ _ _ _ _ hwb ?_) r⟩
      have := spillsDisjoint k a (lookupOffset_mem _ _ _ ha) (Ne.symm e)
      unfold Disjoint at this; omega
  · intro k c hc
    simp only [if_neg (notIncoming k)] at hc
    obtain ⟨b, r⟩ := hl.slots.2 k c hc
    refine ⟨b, SlotRel_congr _ _ _ c (mem_at_unchanged _ _ _ _ _ hwb ?_) r⟩
    unfold Frame.entry; omega
  · refine ⟨rfl, rfl, rfl, fun _ _ => rfl, fun a h => ?_, fun r a h j hj => ?_⟩
    · unfold Frame.fp at h
      exact storeBytes_outside _ _ _ _ _ (by omega)
    · have := savesDisjoint r a (lookupOffset_mem _ _ _ h)
      unfold Disjoint at this
      exact storeBytes_outside _ _ _ _ _ (by omega)

@[simp] theorem reg_class (l : Loc) : (Loc.reg l).class = l.class := rfl

theorem writeLoc_spill (st : Storage) (k : Nat) (w : Machine.Width) (x : BitVec 64) :
    writeLoc st (.spill k w) (.value (Machine.normalize (Loc.spill k w).class (.int x))) =
      fun p => if p = .spill k then some ⟨.value (.int (Machine.normalizeAt w x)), w⟩ else st p := by
  funext p
  simp [writeLoc, Loc.place, Loc.cell, Loc.written, Loc.width, Loc.class, normalize_class]

theorem writeLoc_outgoing (st : Storage) (k : Nat) (w : Machine.Width) (x : BitVec 64) :
    writeLoc st (.outgoing k w) (.value (Machine.normalize (Loc.outgoing k w).class (.int x))) =
      fun p => if p = .outgoing k then some ⟨.value (.int (Machine.normalizeAt w x)), w⟩ else st p := by
  funext p
  simp [writeLoc, Loc.place, Loc.cell, Loc.written, Loc.width, Loc.class, normalize_class]

theorem widthBytes_le (w : Machine.Width) : widthBytes w ≤ 8 := by cases w <;> decide

theorem step_spillStore {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (layout : LayoutOk cur.hint)
    (slot s off : Nat) (sv : Machine.Width) (hoff : lookupOffset cur.hint.spills slot = some off)
    (heval : Alloc.evalInstruction owner (.spillStore slot (.gpr s sv)) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.str sv (.r s) .sp off) := by
  eval_prelude heval
  obtain ⟨v, hv, he⟩ := bind_ok he
  simp only [Loc.width] at hv he
  rw [value_gpr hl.shape hl.regs hv] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  have mem := lookupOffset_mem _ _ _ hoff
  obtain ⟨inFrame, aboveOut⟩ := layout.spill slot off mem
  have entry := hl.top
  have low := hl.bottom
  have top := hl.high
  unfold Frame.entry at entry
  have haddr : (t.sp + BitVec.ofInt 64 (off : Int)).toNat = cur.sp + off := by
    rw [hl.sp]; exact address_nat _ _ (by omega)
  refine ⟨rfl, ({t with memory := (storeBytes t.memory (cur.sp + off) (widthBytes sv) (t.get sv (.r s)).toNat)} : Asm.State), ?_, ?_⟩
  · simp only [effect, hl.baseValue_sp, bind, Except.bind]
    rw [store_in t _ _ _ (by rw [haddr]; omega) (by rw [haddr]; have := widthBytes_le sv; omega)]
    rw [haddr]
  · simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil, reg_class, writeLoc_spill,
      normalizeAt_idem]
    apply local_store hl off (widthBytes sv) _ (widthBytes_le sv) (.spill slot) _ inFrame
    · intro r a ha; exact layout.spillSave slot off r a mem ha
    · intro k a ha ne
      have : k ≠ slot := by intro e; exact ne (by rw [e])
      exact layout.spillSpill slot off k a mem ha (Ne.symm this)
    · intro k hk _; unfold Disjoint; omega
    · intro k e; cases e; exact hoff
    · intro k e; cases e
    · exact .inl ⟨slot, rfl⟩
    · simp only [State.get, State.reg]; exact slot_store t.memory _ (t.regs s) sv
    · intro val d e; cases e; exact ⟨_, rfl⟩

theorem step_outgoingArg {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (layout : LayoutOk cur.hint)
    (slot s : Nat) (sv : Machine.Width) (hk : 8 * slot + 8 ≤ cur.hint.outgoing)
    (heval : Alloc.evalInstruction owner (.outgoingArg slot (.gpr s sv)) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.str sv (.r s) .sp ((8 * slot : Nat) : Int)) := by
  eval_prelude heval
  obtain ⟨v, hv, he⟩ := bind_ok he
  simp only [Loc.width] at hv he
  rw [value_gpr hl.shape hl.regs hv] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  have entry := hl.top
  have low := hl.bottom
  have top := hl.high
  have out := layout.outgoing
  unfold Frame.entry at entry
  have haddr : (t.sp + BitVec.ofInt 64 ((8 * slot : Nat) : Int)).toNat = cur.sp + 8 * slot := by
    rw [hl.sp]; exact address_nat _ _ (by omega)
  refine ⟨rfl, ({t with memory := (storeBytes t.memory (cur.sp + 8 * slot) (widthBytes sv) (t.get sv (.r s)).toNat)} : Asm.State), ?_, ?_⟩
  · simp only [effect, hl.baseValue_sp, bind, Except.bind]
    rw [store_in t _ _ _ (by rw [haddr]; omega) (by rw [haddr]; have := widthBytes_le sv; omega)]
    rw [haddr]
  · simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil, reg_class, writeLoc_outgoing,
      normalizeAt_idem]
    apply local_store hl (8 * slot) (widthBytes sv) _ (widthBytes_le sv) (.outgoing slot) _
      (by omega)
    · intro r a ha; have := (layout.save r a ha).2.1; unfold Disjoint; omega
    · intro k a ha _; have := (layout.spill k a ha).2; unfold Disjoint; omega
    · intro k _ ne
      have : k ≠ slot := by intro e; exact ne (by rw [e])
      unfold Disjoint; omega
    · intro k e; cases e
    · intro k e; cases e; exact ⟨rfl, hk⟩
    · exact .inr ⟨slot, rfl⟩
    · simp only [State.get, State.reg]; exact slot_store t.memory _ (t.regs s) sv
    · intro val d e; cases e; exact ⟨_, rfl⟩

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem value_slot {st : Storage} (shape : Shape st) {loc : Loc} (notFlags : loc ≠ .flags)
    {v : Machine.Value} (h : Machine.operandValue (operandReader st) (Loc.reg loc) = .ok v) :
    ∃ b dd, st loc.place = some ⟨.value (.int b), dd⟩ ∧ (loc.width = .x → dd = .x) ∧
      v = .int (Machine.normalizeAt loc.width b) := by
  simp only [Machine.operandValue, operandReader_reg] at h
  obtain ⟨slot, hr, hm⟩ := bind_ok h
  cases slot with
  | flags _ => cases hm
  | value val => cases hm; exact readLoc_slot shape notFlags hr

theorem step_spillReload {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (layout : LayoutOk cur.hint)
    (d slot off : Nat) (dv : Machine.Width) (dw : Writable cur d)
    (hoff : lookupOffset cur.hint.spills slot = some off)
    (heval : Alloc.evalInstruction owner (.spillReload (.gpr d dv) slot) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.ldr dv (.r d) .sp off) := by
  eval_prelude heval
  obtain ⟨v, hv, he⟩ := bind_ok he
  simp only [Loc.width] at hv he
  obtain ⟨b, dd, hc, fits, rfl⟩ := value_slot hl.shape (by intro e; cases e) hv
  simp only [Loc.place, Loc.width] at hc fits
  obtain ⟨off', hoff', rel⟩ := hl.slots.1 slot _ hc
  rw [hoff] at hoff'
  cases hoff'
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  obtain ⟨inFrame, _⟩ := layout.spill slot off (lookupOffset_mem _ _ _ hoff)
  have entry := hl.top
  have low := hl.bottom
  have top := hl.high
  unfold Frame.entry at entry
  have haddr : (t.sp + BitVec.ofInt 64 (off : Int)).toNat = cur.sp + off := by
    rw [hl.sp]; exact address_nat _ _ (by omega)
  refine ⟨rfl, t.set dv (.r d) (BitVec.ofNat 64 (loadBytes t.memory (cur.sp + off) (widthBytes dv))),
    ?_, ?_⟩
  · simp only [effect, hl.baseValue_sp, bind, Except.bind]
    rw [load_in t _ _ (by rw [haddr]; omega) (by rw [haddr]; have := widthBytes_le dv; omega), haddr]
    rfl
  · simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
    apply local_set_gpr hl d dw dv dv
    rw [slot_value t.memory _ b dd dv rel fits]; simp [Loc.width]

theorem step_incomingArg {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (d slot : Nat) (dv : Machine.Width)
    (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.incomingArg (.gpr d dv) slot) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.ldr dv (.r d) (.r 29) ((16 + 8 * slot : Nat) : Int)) := by
  eval_prelude heval
  obtain ⟨v, hv, he⟩ := bind_ok he
  simp only [Loc.width] at hv he
  obtain ⟨b, dd, hc, fits, rfl⟩ := value_slot hl.shape (by intro e; cases e) hv
  simp only [Loc.place, Loc.width] at hc fits
  obtain ⟨bound, rel⟩ := hl.slots.2 slot _ hc
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  have low := hl.bottom
  have top := hl.high
  have haddr : (t.regs 29 + BitVec.ofInt 64 ((16 + 8 * slot : Nat) : Int)).toNat =
      cur.entry + 8 * slot := by
    rw [hl.fp]; unfold Frame.entry at bound ⊢; unfold Frame.fp
    rw [address_nat _ _ (by omega)]; omega
  refine ⟨rfl, t.set dv (.r d) (BitVec.ofNat 64 (loadBytes t.memory (cur.entry + 8 * slot)
    (widthBytes dv))), ?_, ?_⟩
  · simp only [effect, State.baseValue, bind, Except.bind, pure, Except.pure]
    rw [load_in t _ _ (by rw [haddr]; unfold Frame.entry; omega)
      (by rw [haddr]; have := widthBytes_le dv; omega), haddr]
  · simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
    apply local_set_gpr hl d dw dv dv
    rw [slot_value t.memory _ b dd dv rel fits]; simp [Loc.width]

end EvoIR.Asm.Proof
