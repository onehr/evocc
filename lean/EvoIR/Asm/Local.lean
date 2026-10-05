import EvoIR.Asm.Frame

/-!
The part of the relation that belongs to the current frame (`Local`), the
change that a body instruction may make to the assembly state
(`Footprint`), and the congruence of the caller chain under such a change.
A body instruction writes allocatable registers, the flags, spill slots and
outgoing slots of the current frame. It does not write the save slots, the
frame record or anything above the frame.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

structure Local (cur : Frame) (st : Storage) (t : Asm.State) : Prop where
  sp : t.sp = BitVec.ofNat 64 cur.sp
  sp16 : cur.sp % 16 = 0
  fp : t.regs 29 = BitVec.ofNat 64 cur.fp
  high : t.high ≤ 2^64
  top : cur.entry ≤ t.high
  bottom : t.low ≤ cur.sp
  shape : Shape st
  regs : RegsRel t.regs st
  flags : FlagsRel t.flags st
  outgoing : OutgoingRel t.memory cur st
  slots : OwnSlots t.high t.memory cur st

/-- The registers that a body instruction of the current frame does not
write: the non-allocatable registers and the callee-saved registers without a
save slot. -/
def Kept (cur : Frame) (r : Nat) : Prop :=
  allocatable r = false ∨ (isCalleeSaved r = true ∧ lookupOffset cur.hint.saved r = none)

structure Footprint (cur : Frame) (t t' : Asm.State) : Prop where
  sp : t'.sp = t.sp
  low : t'.low = t.low
  high : t'.high = t.high
  regs : ∀ r, Kept cur r → t'.regs r = t.regs r
  above : ∀ a, cur.fp ≤ a → t'.memory a = t.memory a
  saves : ∀ r off, lookupOffset cur.hint.saved r = some off → ∀ j, j < 8 →
    t'.memory (cur.sp + off + j) = t.memory (cur.sp + off + j)

theorem Footprint.refl (cur : Frame) (t : Asm.State) : Footprint cur t t :=
  ⟨rfl, rfl, rfl, fun _ _ => rfl, fun _ _ => rfl, fun _ _ _ _ _ => rfl⟩

theorem Footprint.trans {cur : Frame} {t u v : Asm.State} (a : Footprint cur t u)
    (b : Footprint cur u v) : Footprint cur t v :=
  ⟨b.sp.trans a.sp, b.low.trans a.low, b.high.trans a.high,
    fun r k => (b.regs r k).trans (a.regs r k), fun x h => (b.above x h).trans (a.above x h),
    fun r off h j hj => (b.saves r off h j hj).trans (a.saves r off h j hj)⟩

theorem load64_congr (mem mem' : Nat → BitVec 8) (a : Nat) (h : ∀ j, j < 8 → mem' (a + j) = mem (a + j)) :
    load64 mem' a = load64 mem a := by
  unfold load64
  rw [loadBytes_congr mem' mem a 8 (fun x l u => by
    have := h (x - a) (by omega); rwa [Nat.add_sub_cancel' l] at this)]

theorem load32_congr (mem mem' : Nat → BitVec 8) (a : Nat) (h : ∀ j, j < 8 → mem' (a + j) = mem (a + j)) :
    load32 mem' a = load32 mem a := by
  unfold load32
  rw [loadBytes_congr mem' mem a 4 (fun x l u => by
    have := h (x - a) (by omega); rwa [Nat.add_sub_cancel' l] at this)]

theorem SlotRel_congr (mem mem' : Nat → BitVec 8) (a : Nat) (cell : Cell)
    (h : ∀ j, j < 8 → mem' (a + j) = mem (a + j)) (rel : SlotRel mem a cell) :
    SlotRel mem' a cell := by
  rcases cell with ⟨slot, d⟩
  cases slot with
  | flags _ => trivial
  | value v =>
    cases v with
    | pointer _ _ => cases d <;> exact rel
    | int b =>
      cases d
      · simp only [SlotRel] at rel ⊢; rw [load32_congr mem mem' a h]; exact rel
      · simp only [SlotRel] at rel ⊢; rw [load64_congr mem mem' a h]; exact rel

theorem OwnSlots_congr (high : Nat) (mem mem' : Nat → BitVec 8) (fr : Frame) (st : Storage)
    (h : ∀ a, fr.sp ≤ a → mem' a = mem a) (rel : OwnSlots high mem fr st) :
    OwnSlots high mem' fr st := by
  refine ⟨fun k cell hk => ?_, fun k cell hk => ?_⟩
  · obtain ⟨off, ho, r⟩ := rel.1 k cell hk
    exact ⟨off, ho, SlotRel_congr mem mem' _ cell (fun j _ => h _ (by omega)) r⟩
  · obtain ⟨b, r⟩ := rel.2 k cell hk
    exact ⟨b, SlotRel_congr mem mem' _ cell (fun j _ => h _ (by unfold Frame.entry; omega)) r⟩

/-- The caller chain depends only on the memory from the frame pointer of
the inner frame upward, on the save slots of the inner frame, and on the
callee-saved registers that the inner frame does not save. -/
theorem chain_congr (ctx : Ctx) (high : Nat) :
    ∀ (callers : List ReturnFrame) (frs : List Frame) (inner : Frame)
      (mem mem' : Nat → BitVec 8) (vis vis' : Nat → BitVec 64),
      (∀ a, inner.fp ≤ a → mem' a = mem a) →
      (∀ r off, lookupOffset inner.hint.saved r = some off → ∀ j, j < 8 →
        mem' (inner.sp + off + j) = mem (inner.sp + off + j)) →
      (∀ r, isCalleeSaved r = true → lookupOffset inner.hint.saved r = none → vis' r = vis r) →
      Chain ctx high mem inner vis callers frs → Chain ctx high mem' inner vis' callers frs
  | [], [], inner, mem, mem', vis, vis', above, _, _, chain => by
    obtain ⟨top, ret⟩ := chain
    refine ⟨top, ?_⟩
    rw [load64_congr mem mem' _ (fun j _ => above _ (by omega))]
    exact ret
  | rf :: rest, fr :: frs, inner, mem, mem', vis, vis', above, saves, regs, chain => by
    obtain ⟨hsp, h16, hfp, ⟨pos, placed, span, hra⟩, shape, own, saved, tail⟩ := chain
    have view : ∀ r, isCalleeSaved r = true →
        restoreView inner mem' vis' r = restoreView inner mem vis r := by
      intro r hr
      unfold restoreView
      cases ho : lookupOffset inner.hint.saved r with
      | none => exact regs r hr ho
      | some off => exact load64_congr mem mem' _ (fun j hj => saves r off ho j hj)
    have above' : ∀ a, fr.sp ≤ a → mem' a = mem a := fun a h =>
      above a (by unfold Frame.entry at hsp; unfold Frame.fp; omega)
    refine ⟨hsp, h16, ?_, ⟨pos, placed, span, ?_⟩, shape, OwnSlots_congr high mem mem' fr _ above' own,
      ?_, ?_⟩
    · rw [load64_congr mem mem' _ (fun j _ => above _ (by omega))]; exact hfp
    · rw [load64_congr mem mem' _ (fun j _ => above _ (by omega))]; exact hra
    · intro r b d hr hc
      rw [view r hr]; exact saved r b d hr hc
    · apply chain_congr ctx high rest frs fr mem mem' _ _
        (fun a h => above' a (by unfold Frame.fp at h; omega))
        (fun r off _ j _ => above' _ (by omega))
        (fun r hr _ => view r hr) tail
  | [], _ :: _, _, _, _, _, _, _, _, _, chain => by cases chain
  | _ :: _, [], _, _, _, _, _, _, _, _, chain => by cases chain

end EvoIR.Asm.Proof
