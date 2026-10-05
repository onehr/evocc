import EvoIR.Asm.Local

/-!
Reads and writes of the current frame, and the simulation of body
instructions that change only registers and flags.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem bind_ok {α β : Type} {x : Except Fault α} {f : α → Except Fault β} {b : β}
    (h : (x >>= f) = .ok b) : ∃ a, x = .ok a ∧ f a = .ok b := by
  cases hx : x with
  | error e => rw [hx] at h; cases h
  | ok a => rw [hx] at h; exact ⟨a, rfl, h⟩

theorem normalize_class (w : Machine.Width) (x : BitVec 64) :
    Machine.normalize w.class (.int x) = .int (Machine.normalizeAt w x) := by
  cases w <;> simp [Machine.normalize, Machine.Width.class, Machine.normalizeAt, Machine.Width.bits]

@[simp] theorem normalizeAt_idem (w : Machine.Width) (x : BitVec 64) :
    Machine.normalizeAt w (Machine.normalizeAt w x) = Machine.normalizeAt w x := by
  cases w <;> simp [Machine.normalizeAt, Machine.Width.bits]

theorem setWidth_normalizeAt (w : Machine.Width) (x : BitVec 64) :
    (Machine.normalizeAt w x).setWidth w.bits = x.setWidth w.bits := by
  cases w <;> simp [Machine.normalizeAt, Machine.Width.bits]

theorem normalizeAt_w_x (x : BitVec 64) :
    Machine.normalizeAt .w (Machine.normalizeAt .x x) = Machine.normalizeAt .w x := by
  simp [Machine.normalizeAt, Machine.Width.bits]

theorem normalizeAt_x (x : BitVec 64) : Machine.normalizeAt .x x = x := by
  simp [Machine.normalizeAt, Machine.Width.bits]

theorem normalizeAt_x_w (x : BitVec 64) :
    Machine.normalizeAt .x (Machine.normalizeAt .w x) = Machine.normalizeAt .w x := by
  simp [Machine.normalizeAt, Machine.Width.bits]

theorem evalBinary_normalize (op : Machine.BinOp) (w : Machine.Width) (x y : BitVec 64) :
    Machine.evalBinary op w (Machine.normalizeAt w x) (Machine.normalizeAt w y) =
      Machine.evalBinary op w x y := by
  simp only [Machine.evalBinary, setWidth_normalizeAt]

theorem evalBinary_normalize_left (op : Machine.BinOp) (w : Machine.Width) (x y : BitVec 64) :
    Machine.evalBinary op w (Machine.normalizeAt w x) y = Machine.evalBinary op w x y := by
  simp only [Machine.evalBinary, setWidth_normalizeAt]

theorem evalBinary_normalized (op : Machine.BinOp) (w : Machine.Width) (x y : BitVec 64) :
    Machine.normalizeAt w (Machine.evalBinary op w x y) = Machine.evalBinary op w x y := by
  cases w <;> simp [Machine.evalBinary, Machine.normalizeAt, Machine.Width.bits]

theorem alu_normalize (op : Machine.BinOp) (w : Machine.Width) (x y : BitVec 64) :
    alu op w (Machine.normalizeAt w x) (Machine.normalizeAt w y) = alu op w x y := by
  rw [alu_eq, alu_eq, evalBinary_normalize]

theorem flagAlu_normalize (op : Machine.FlagOp) (w : Machine.Width) (x y : BitVec 64) :
    flagAlu op w (Machine.normalizeAt w x) (Machine.normalizeAt w y) = flagAlu op w x y := by
  have hn : (~~~Machine.normalizeAt w y).setWidth w.bits = (~~~y).setWidth w.bits := by
    rw [setWidth_not, setWidth_not, setWidth_normalizeAt]
  cases op <;> simp only [flagAlu, addWithCarry, setWidth_normalizeAt, hn]

/-! Reads of the current storage. -/

theorem readLoc_gpr {st : Storage} {regs : Nat → BitVec 64} (shape : Shape st) (rel : RegsRel regs st)
    {n : Nat} {w : Machine.Width} {val : Machine.Value}
    (h : readLoc st (.gpr n w) = .ok (.value val)) :
    val = .int (Machine.normalizeAt w (regs n)) := by
  simp only [readLoc, Loc.place] at h
  cases hc : st (.gpr n) with
  | none => rw [hc] at h; cases h
  | some cell =>
    rw [hc] at h
    obtain ⟨slot, d⟩ := cell
    cases slot with
    | flags _ => cases h
    | value val' =>
      obtain ⟨b, rfl⟩ := shape.1 _ _ _ hc
      have hb := rel n b d hc
      simp only [Loc.width] at h
      split at h
      · cases h
      · cases h; rw [normalize_class, hb]

theorem value_gpr {st : Storage} {regs : Nat → BitVec 64} (shape : Shape st) (rel : RegsRel regs st)
    {n : Nat} {w : Machine.Width} {v : Machine.Value}
    (h : Machine.operandValue (operandReader st) (Loc.reg (.gpr n w)) = .ok v) :
    v = .int (Machine.normalizeAt w (regs n)) := by
  simp only [Machine.operandValue, operandReader_reg] at h
  obtain ⟨slot, hr, hm⟩ := bind_ok h
  cases slot with
  | flags _ => cases hm
  | value val => cases hm; exact readLoc_gpr shape rel hr

theorem bits_gpr {st : Storage} {regs : Nat → BitVec 64} (shape : Shape st) (rel : RegsRel regs st)
    {n : Nat} {w : Machine.Width} {b : BitVec 64}
    (h : Machine.operandBits (operandReader st) (Loc.reg (.gpr n w)) = .ok b) :
    b = Machine.normalizeAt w (regs n) := by
  simp only [Machine.operandBits, bind, Except.bind] at h
  split at h
  · cases h
  · rename_i v hv
    rw [value_gpr shape rel hv] at h
    cases h; rfl

theorem flags_loc {st : Storage} {fl : Machine.Flags}
    (h : Machine.operandFlags (operandReader st) (Loc.reg .flags) = .ok fl) :
    ∃ d, st .flags = some ⟨.flags fl, d⟩ := by
  simp only [Machine.operandFlags, operandReader_reg] at h
  obtain ⟨slot, hr, hm⟩ := bind_ok h
  cases slot with
  | value _ => cases hm
  | flags f =>
    cases hm
    simp only [readLoc, Loc.place] at hr
    cases hc : st .flags with
    | none => rw [hc] at hr; cases hr
    | some cell =>
      rw [hc] at hr
      obtain ⟨slot, d⟩ := cell
      cases slot with
      | flags f' => cases hr; exact ⟨d, rfl⟩
      | value _ => simp at hr

/-! Writes of the current storage. -/

theorem writeLoc_gpr (st : Storage) (d : Nat) (w : Machine.Width) (x : BitVec 64) (place : Place) :
    writeLoc st (.gpr d w) (.value (Machine.normalize w.class (.int x))) place =
      if place = .gpr d then some ⟨.value (.int (Machine.normalizeAt w x)), .x⟩ else st place := by
  simp only [writeLoc, Loc.place, Loc.cell, Loc.written, Loc.width, normalize_class,
    normalizeAt_idem]

theorem writeLoc_flags (st : Storage) (fl : Machine.Flags) (place : Place) :
    writeLoc st .flags (.flags fl) place =
      if place = .flags then some ⟨.flags fl, .x⟩ else st place := by
  simp only [writeLoc, Loc.place, Loc.cell]

/-- A register that a checked function writes is not kept. -/
def Writable (cur : Frame) (d : Nat) : Prop :=
  allocatable d = true ∧ (isCalleeSaved d = true → (lookupOffset cur.hint.saved d).isSome = true)

theorem not_kept {cur : Frame} {d : Nat} (h : Writable cur d) : ¬ Kept cur d := by
  intro k
  rcases k with k | ⟨cs, none⟩
  · rw [h.1] at k; cases k
  · have := h.2 cs; rw [none] at this; cases this

@[simp] theorem set_r_regs (t : Asm.State) (w : Machine.Width) (d : Nat) (y : BitVec 64) :
    (t.set w (.r d) y).regs = update t.regs d (view w y) := rfl

@[simp] theorem set_r_memory (t : Asm.State) (w : Machine.Width) (d : Nat) (y : BitVec 64) :
    (t.set w (.r d) y).memory = t.memory := rfl

@[simp] theorem set_r_sp (t : Asm.State) (w : Machine.Width) (d : Nat) (y : BitVec 64) :
    (t.set w (.r d) y).sp = t.sp := rfl

@[simp] theorem set_r_flags (t : Asm.State) (w : Machine.Width) (d : Nat) (y : BitVec 64) :
    (t.set w (.r d) y).flags = t.flags := rfl

@[simp] theorem set_r_low (t : Asm.State) (w : Machine.Width) (d : Nat) (y : BitVec 64) :
    (t.set w (.r d) y).low = t.low := rfl

@[simp] theorem set_r_high (t : Asm.State) (w : Machine.Width) (d : Nat) (y : BitVec 64) :
    (t.set w (.r d) y).high = t.high := rfl

/-- Writing a general register keeps the local relation when the assembly
register gets the bits of the new cell. -/
theorem local_set_gpr {cur : Frame} {st : Storage} {t : Asm.State} (hl : Local cur st t)
    (d : Nat) (dw : Writable cur d) (w w' : Machine.Width) (x y : BitVec 64)
    (hy : view w' y = Machine.normalizeAt w x) :
    Local cur (writeLoc st (.gpr d w) (.value (Machine.normalize w.class (.int x)))) (t.set w' (.r d) y) ∧
      Footprint cur t (t.set w' (.r d) y) := by
  have ne29 : d ≠ 29 := by
    intro e; subst e; have := dw.1; simp [allocatable, isCalleeSaved] at this
  refine ⟨⟨hl.sp, hl.sp16, ?_, hl.high, hl.top, hl.bottom, ⟨?_, ?_⟩, ?_, ?_, ?_, ?_⟩, ?_⟩
  · simp [update_other _ _ _ _ (Ne.symm ne29), hl.fp]
  · intro place v dd hc
    rw [writeLoc_gpr] at hc
    split at hc
    · cases hc; exact ⟨_, rfl⟩
    · exact hl.shape.1 _ _ _ hc
  · intro n hn
    rw [writeLoc_gpr]
    have : Place.gpr n ≠ Place.gpr d := by
      intro e; cases e; rw [dw.1] at hn; cases hn
    simp [this, hl.shape.2 n hn]
  · intro n b dd hc
    rw [writeLoc_gpr] at hc
    simp only [set_r_regs]
    by_cases e : n = d
    · subst e; simp at hc; obtain ⟨rfl, _⟩ := hc; simp [hy]
    · have : Place.gpr n ≠ Place.gpr d := by intro h; cases h; exact e rfl
      simp only [this, reduceIte] at hc
      rw [update_other _ _ _ _ e]; exact hl.regs n b dd hc
  · intro fl dd c b hc
    rw [writeLoc_gpr] at hc
    simp at hc
    exact hl.flags fl dd c b hc
  · intro k cell hc
    rw [writeLoc_gpr] at hc
    simp at hc
    exact hl.outgoing k cell hc
  · refine ⟨fun k cell hc => ?_, fun k cell hc => ?_⟩
    · rw [writeLoc_gpr] at hc; simp at hc; exact hl.slots.1 k cell hc
    · rw [writeLoc_gpr] at hc; simp at hc; exact hl.slots.2 k cell hc
  · refine ⟨rfl, rfl, rfl, fun r k => ?_, fun _ _ => rfl, fun _ _ _ _ _ => rfl⟩
    simp only [set_r_regs]
    apply update_other
    intro e; subst e; exact not_kept dw k

/-- Writing the flags keeps the local relation when every successful
condition of the new flags agrees with the assembly NZCV bits. -/
theorem local_set_flags {cur : Frame} {st : Storage} {t : Asm.State} (hl : Local cur st t)
    (fl : Machine.Flags) (nzcv : Flags)
    (hf : ∀ c b, Machine.condition fl c = .ok b → nzcv.holds c = b) :
    Local cur (writeLoc st .flags (.flags fl)) {t with flags := nzcv} ∧
      Footprint cur t {t with flags := nzcv} := by
  refine ⟨⟨hl.sp, hl.sp16, hl.fp, hl.high, hl.top, hl.bottom, ⟨?_, ?_⟩, ?_, ?_, ?_, ?_⟩,
    ⟨rfl, rfl, rfl, fun _ _ => rfl, fun _ _ => rfl, fun _ _ _ _ _ => rfl⟩⟩
  · intro place v dd hc
    rw [writeLoc_flags] at hc
    split at hc
    · cases hc
    · exact hl.shape.1 _ _ _ hc
  · intro n hn; rw [writeLoc_flags]; simp [hl.shape.2 n hn]
  · intro n b dd hc; rw [writeLoc_flags] at hc; simp at hc; exact hl.regs n b dd hc
  · intro fl' dd c b hc hcond
    rw [writeLoc_flags] at hc; simp at hc; obtain ⟨rfl, _⟩ := hc; exact hf c b hcond
  · intro k cell hc; rw [writeLoc_flags] at hc; simp at hc; exact hl.outgoing k cell hc
  · refine ⟨fun k cell hc => ?_, fun k cell hc => ?_⟩
    · rw [writeLoc_flags] at hc; simp at hc; exact hl.slots.1 k cell hc
    · rw [writeLoc_flags] at hc; simp at hc; exact hl.slots.2 k cell hc

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem eval_inv {owner : Nat} {inst : Instruction} {st st' : Storage} {mem mem' : Machine.Memory}
    (h : Alloc.evalInstruction owner inst st mem = .ok (st', mem')) :
    ∃ writes, Machine.evalInstructionOperands owner (inst.toMachine Loc.reg) (operandReader st) mem =
      .ok (writes, mem') ∧ st' = applyWrites st writes := by
  simp only [Alloc.evalInstruction] at h
  obtain ⟨⟨writes, memory⟩, he, hr⟩ := bind_ok h
  cases hr
  exact ⟨writes, he, rfl⟩

/-- The common conclusion of a body instruction with a one-line template. -/
def OneStep (cur : Frame) (st' : Storage) (t : Asm.State) (i : Inst) : Prop :=
  ∃ t', effect t i = .ok t' ∧ Local cur st' t' ∧ Footprint cur t t'

theorem step_binary {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (op : Machine.BinOp) (d n m : Nat)
    (w : Machine.Width) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.binary op (.gpr d w) (.gpr n w) (.gpr m w) w) st mem =
      .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.alu op w (.r d) (.r n) (.r m)) := by
  obtain ⟨writes, he, rfl⟩ := eval_inv heval
  simp only [Instruction.toMachine, Machine.evalInstructionOperands] at he
  obtain ⟨left, hleft, he⟩ := bind_ok he
  obtain ⟨right, hright, he⟩ := bind_ok he
  rw [value_gpr hl.shape hl.regs hleft, value_gpr hl.shape hl.regs hright] at he
  simp only [Machine.evalBinaryValue, bind, Except.bind, pure, Except.pure, Except.ok.injEq,
    Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw w w
  rw [view_eq, alu_eq, evalBinary_normalize]
  rfl

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

set_option hygiene false in
/-- Prepare an instruction step: the writes of the evaluation. -/
macro "eval_prelude" heval:ident : tactic => `(tactic| (
  obtain ⟨writes, he, rfl⟩ := eval_inv $heval
  simp only [Instruction.toMachine, Machine.evalInstructionOperands] at he))

theorem step_binaryImm {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (op : Machine.BinOp) (d n imm : Nat)
    (w : Machine.Width) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.binaryImm op (.gpr d w) (.gpr n w) imm w) st mem =
      .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.aluImm op w (.r d) (.r n) imm) := by
  eval_prelude heval
  obtain ⟨left, hleft, he⟩ := bind_ok he
  rw [value_gpr hl.shape hl.regs hleft] at he
  simp only [Machine.evalBinaryValue, bind, Except.bind, pure, Except.pure, Except.ok.injEq,
    Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw w w
  rw [view_eq, alu_eq, evalBinary_normalize_left]
  rfl

theorem step_mov {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (d s : Nat) (dv sv : Machine.Width)
    (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.mov (.gpr d dv) (.gpr s sv)) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.mov (if dv = sv then dv else .w) (.r d) (.r s)) := by
  eval_prelude heval
  obtain ⟨v, hv, he⟩ := bind_ok he
  rw [value_gpr hl.shape hl.regs hv] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw dv
  simp only [State.get, State.reg, view_eq]
  split
  · subst sv; simp
  · cases dv <;> cases sv <;> simp_all [normalizeAt_x]

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem flagAlu_normalize_left (op : Machine.FlagOp) (w : Machine.Width) (x y : BitVec 64) :
    flagAlu op w (Machine.normalizeAt w x) y = flagAlu op w x y := by
  cases op <;> simp only [flagAlu, addWithCarry, setWidth_normalizeAt]

theorem evalExtend_normalize (op : Machine.ExtOp) (w : Machine.Width) (x : BitVec 64) :
    Machine.evalExtend op (Machine.normalizeAt w x) = Machine.evalExtend op x := by
  have : (Machine.normalizeAt w x).setWidth op.bits = x.setWidth op.bits := by
    apply BitVec.eq_of_toNat_eq
    cases w <;> cases op <;>
      simp [Machine.normalizeAt, Machine.Width.bits, Machine.ExtOp.bits, BitVec.toNat_setWidth] <;>
      omega
  simp only [Machine.evalExtend, this]

theorem evalWideMultiply_normalize (op : Machine.WideMul) (x y : BitVec 64) :
    let source : Machine.Width := match op with | .smull => .w | .smulh | .umulh => .x
    Machine.evalWideMultiply op (Machine.normalizeAt source x) (Machine.normalizeAt source y) =
      Machine.evalWideMultiply op x y := by
  cases op
  · have hx := setWidth_normalizeAt .w x
    have hy := setWidth_normalizeAt .w y
    simp only [Machine.Width.bits] at hx hy
    simp only [Machine.evalWideMultiply, hx, hy]
  · simp [normalizeAt_x]
  · simp [normalizeAt_x]

theorem evalBfx_normalize (signed : Bool) (w : Machine.Width) (x : BitVec 64) (lsb bits : Nat) :
    Machine.evalBfx signed w (Machine.normalizeAt w x) lsb bits = Machine.evalBfx signed w x lsb bits := by
  simp only [Machine.evalBfx, normalizeAt_idem]

theorem evalBfx_normalized (signed : Bool) (w : Machine.Width) (x : BitVec 64) (lsb bits : Nat) :
    Machine.normalizeAt w (Machine.evalBfx signed w x lsb bits) = Machine.evalBfx signed w x lsb bits := by
  simp only [Machine.evalBfx, normalizeAt_idem]

theorem step_flagBinary {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (op : Machine.FlagOp) (d n m : Nat)
    (w : Machine.Width) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.flagBinary op (.gpr d w) .flags (.gpr n w) (.gpr m w) w)
      st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.flagAlu op w (.r d) (.r n) (.r m)) := by
  eval_prelude heval
  obtain ⟨left, hleft, he⟩ := bind_ok he
  obtain ⟨right, hright, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hleft, bits_gpr hl.shape hl.regs hright, flagAlu_eq,
    flagAlu_normalize] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, List.cons_append, List.nil_append, applyWrites_cons,
    applyWrites_nil]
  obtain ⟨l1, f1⟩ := local_set_gpr hl d dw w w (flagAlu op w (t.regs n) (t.regs m)).1
    (flagAlu op w (t.regs n) (t.regs m)).1 (by rw [view_eq])
  obtain ⟨l2, f2⟩ := local_set_flags l1 _ (flagAlu op w (t.regs n) (t.regs m)).2
    (fun c b h => by rw [condition_arithmetic] at h; cases h; rfl)
  exact ⟨l2, f1.trans f2⟩

theorem step_wideMultiply {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (op : Machine.WideMul) (d n m : Nat)
    (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.wideMultiply op (.gpr d .x)
      (.gpr n (match op with | .smull => .w | .smulh | .umulh => .x))
      (.gpr m (match op with | .smull => .w | .smulh | .umulh => .x))) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.wideMul op (.r d) (.r n) (.r m)) := by
  eval_prelude heval
  obtain ⟨left, hleft, he⟩ := bind_ok he
  obtain ⟨right, hright, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hleft, bits_gpr hl.shape hl.regs hright,
    evalWideMultiply_normalize] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw .x .x
  rw [view_eq, wideMul_eq]; rfl

theorem step_msub {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (d n m a : Nat) (w : Machine.Width)
    (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.msub (.gpr d w) (.gpr n w) (.gpr m w) (.gpr a w) w)
      st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.msub w (.r d) (.r n) (.r m) (.r a)) := by
  eval_prelude heval
  obtain ⟨x, hx, he⟩ := bind_ok he
  obtain ⟨y, hy, he⟩ := bind_ok he
  obtain ⟨z, hz, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hx, bits_gpr hl.shape hl.regs hy, bits_gpr hl.shape hl.regs hz,
    evalBinary_normalize, evalBinary_normalize_left] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw w w
  rw [view_eq, alu_eq, alu_eq]; rfl

theorem step_madd {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (d n m a : Nat) (w : Machine.Width)
    (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.madd (.gpr d w) (.gpr n w) (.gpr m w) (.gpr a w) w)
      st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.madd w (.r d) (.r n) (.r m) (.r a)) := by
  eval_prelude heval
  obtain ⟨x, hx, he⟩ := bind_ok he
  obtain ⟨y, hy, he⟩ := bind_ok he
  obtain ⟨z, hz, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hx, bits_gpr hl.shape hl.regs hy, bits_gpr hl.shape hl.regs hz,
    evalBinary_normalize, evalBinary_normalize_left] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw w w
  rw [view_eq, alu_eq, alu_eq]; rfl

theorem step_extend {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (op : Machine.ExtOp) (d s : Nat)
    (dv sv : Machine.Width) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.extend op (.gpr d dv) (.gpr s sv)) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.extend op dv (.r d) (.r s)) := by
  eval_prelude heval
  obtain ⟨x, hx, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hx, evalExtend_normalize] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw dv dv
  rw [view_eq, extend_eq]; rfl

theorem step_bfx {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (signed : Bool) (d s lsb bits : Nat)
    (w : Machine.Width) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.bfx signed (.gpr d w) (.gpr s w) lsb bits w) st mem =
      .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.bfx signed w (.r d) (.r s) lsb bits) := by
  eval_prelude heval
  obtain ⟨x, hx, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hx, evalBfx_normalize] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw w w
  rw [bitfield_eq, evalBfx_normalized]; rfl

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem step_cmp {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (n m : Nat) (w : Machine.Width)
    (heval : Alloc.evalInstruction owner (.cmp .flags (.gpr n w) (.gpr m w) w) st mem =
      .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.cmp w (.r n) (.r m)) := by
  eval_prelude heval
  obtain ⟨left, hleft, he⟩ := bind_ok he
  obtain ⟨right, hright, he⟩ := bind_ok he
  rw [value_gpr hl.shape hl.regs hleft, value_gpr hl.shape hl.regs hright] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [applyWrites_cons, applyWrites_nil, Machine.compare]
  apply local_set_flags hl
  intro c b h
  rw [cmp_condition, flagAlu_normalize] at h
  cases h; rfl

theorem step_cmpImm {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (n imm : Nat) (w : Machine.Width)
    (heval : Alloc.evalInstruction owner (.cmpImm .flags (.gpr n w) imm w) st mem =
      .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.cmpImm w (.r n) imm) := by
  eval_prelude heval
  obtain ⟨left, hleft, he⟩ := bind_ok he
  rw [value_gpr hl.shape hl.regs hleft] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [applyWrites_cons, applyWrites_nil, Machine.compare]
  apply local_set_flags hl
  intro c b h
  rw [cmp_condition, flagAlu_normalize_left] at h
  cases h; rfl

theorem step_tstImm {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (n imm : Nat) (w : Machine.Width)
    (heval : Alloc.evalInstruction owner (.tstImm .flags (.gpr n w) imm w) st mem =
      .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.tstImm w (.r n) imm) := by
  eval_prelude heval
  obtain ⟨x, hx, he⟩ := bind_ok he
  rw [bits_gpr hl.shape hl.regs hx, setWidth_normalizeAt] at he
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [applyWrites_cons, applyWrites_nil]
  apply local_set_flags hl
  intro c b h
  rw [condition_arithmetic] at h
  cases h
  simp only [State.reg, logicalFlags_eq]

theorem step_cset {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (d : Nat) (dv : Machine.Width)
    (cond : Machine.Cond) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.cset (.gpr d dv) .flags cond) st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.cset dv (.r d) cond) := by
  eval_prelude heval
  obtain ⟨fl, hfl, he⟩ := bind_ok he
  obtain ⟨yes, hyes, he⟩ := bind_ok he
  obtain ⟨d', hd'⟩ := flags_loc hfl
  have holds := hl.flags fl d' cond yes hd' hyes
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw dv dv
  rw [view_eq, holds]
  cases yes <;> rfl

theorem step_csel {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (d n m : Nat) (w : Machine.Width)
    (cond : Machine.Cond) (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.csel (.gpr d w) .flags (.gpr n w) (.gpr m w) cond w)
      st mem = .ok (st', mem')) :
    mem' = mem ∧ OneStep cur st' t (.csel w (.r d) (.r n) (.r m) cond) := by
  eval_prelude heval
  obtain ⟨fl, hfl, he⟩ := bind_ok he
  obtain ⟨yes, hyes, he⟩ := bind_ok he
  obtain ⟨d', hd'⟩ := flags_loc hfl
  have holds := hl.flags fl d' cond yes hd' hyes
  obtain ⟨v, hv, he⟩ := bind_ok he
  have hsel : v = .int (Machine.normalizeAt w (t.regs (if yes then n else m))) := by
    cases yes
    · exact value_gpr hl.shape hl.regs hv
    · exact value_gpr hl.shape hl.regs hv
  subst hsel
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  refine ⟨rfl, _, rfl, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  apply local_set_gpr hl d dw w w
  rw [view_eq, holds]
  cases yes <;> simp [State.reg]

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Source (Fault)

theorem update_update {α : Type} (f : Nat → α) (n : Nat) (a b : α) :
    update (update f n a) n b = update f n b := by
  funext m; by_cases e : m = n <;> simp [update, e]

macro "imm_close" : tactic => `(tactic| (
  simp only [State.set, State.reg, update_same, update_update, State.mk.injEq, List.length_cons,
    List.length_nil]
  refine ⟨trivial, ?_, trivial, trivial, trivial, trivial, trivial⟩
  congr 1
  apply BitVec.eq_of_toNat_eq
  simp only [view, insertHalf, State.reg, State.set, update_same, BitVec.toNat_setWidth,
    BitVec.toNat_ofNat, Nat.reducePow, Nat.reduceAdd, Nat.reduceMul] at *
  omega))

/-- The run of an immediate sequence sets the register to the immediate. -/
theorem immediate_regs (fn pos : Nat) (v : Machine.Width) (d imm : Nat) (t : State) :
    ∃ t', runFrom fn pos (immediateLines v d imm) t = .ok t' ∧
      t' = {t.set v (.r d) (BitVec.ofNat 64 imm) with
        control := .running fn (pos + (immediateLines v d imm).length)} := by
  cases v
  · simp only [immediateLines, Machine.Width.bits, Nat.reduceDiv, Nat.reduceSub, List.range_one,
      List.filterMap_cons, List.filterMap_nil]
    by_cases h1 : imm / 2^(16 * (0+1)) % 65536 = 0 <;>
      simp only [h1, reduceIte, runFrom, effect, bind, Except.bind, pure, Except.pure] <;>
      refine ⟨_, rfl, ?_⟩ <;> imm_close
  · simp only [immediateLines, Machine.Width.bits, Nat.reduceDiv, Nat.reduceSub]
    simp only [List.range_succ, List.range_zero, List.nil_append, List.cons_append, List.filterMap_cons,
      List.filterMap_nil]
    by_cases h1 : imm / 2^(16 * (0+1)) % 65536 = 0 <;>
    by_cases h2 : imm / 2^(16 * (1+1)) % 65536 = 0 <;>
    by_cases h3 : imm / 2^(16 * (2+1)) % 65536 = 0 <;>
      simp only [h1, h2, h3, reduceIte, runFrom, effect, bind, Except.bind, pure, Except.pure] <;>
      refine ⟨_, rfl, ?_⟩ <;> imm_close

theorem Local.control {cur : Frame} {st : Storage} {u : Asm.State} (c : Control)
    (hl : Local cur st u) : Local cur st {u with control := c} :=
  ⟨hl.sp, hl.sp16, hl.fp, hl.high, hl.top, hl.bottom, hl.shape, hl.regs, hl.flags, hl.outgoing,
    hl.slots⟩

theorem Footprint.control {cur : Frame} {t u : Asm.State} (c : Control)
    (hf : Footprint cur t u) : Footprint cur t {u with control := c} :=
  ⟨hf.sp, hf.low, hf.high, hf.regs, hf.above, hf.saves⟩

theorem step_movImm {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (fn pos d imm : Nat) (v : Machine.Width)
    (dw : Writable cur d)
    (heval : Alloc.evalInstruction owner (.movImm (.gpr d v) imm) st mem = .ok (st', mem')) :
    mem' = mem ∧ ∃ t', runFrom fn pos (immediateLines v d imm) t = .ok t' ∧
      Local cur st' t' ∧ Footprint cur t t' := by
  eval_prelude heval
  simp only [pure, Except.pure, Except.ok.injEq, Prod.mk.injEq] at he
  obtain ⟨rfl, rfl⟩ := he
  obtain ⟨t', hrun, rfl⟩ := immediate_regs fn pos v d imm t
  refine ⟨rfl, _, hrun, ?_⟩
  simp only [Machine.valueWrite, applyWrites_cons, applyWrites_nil]
  obtain ⟨l, f⟩ := local_set_gpr hl d dw v v (BitVec.ofNat 64 imm) (BitVec.ofNat 64 imm) (view_eq v _)
  exact ⟨l.control _, f.control _⟩

end EvoIR.Asm.Proof
