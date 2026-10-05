import EvoIR.Asm.Call

/-!
The return chunk and the `brk` chunk. A return runs the epilogue and `ret`.
In a callee, `ret` goes to the code address in the frame record, which is
the code after the call in the caller. In the outermost activation, `ret`
leaves the program and both machines stop with the value of `x0`.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

/-- The views of the returned locations of the entry function are at least as
wide as the result type. -/
def ResultView (p : Alloc.Program) (entry : String) (type : Core.Source.IntType) : Prop :=
  ∀ f ∈ p.functions, f.name = entry → ∀ b ∈ f.blocks, ∀ locs, b.term = .ret locs →
    ∀ l ∈ locs, type.width ≤ l.width.bits

theorem restored_eq_restoreView (fr : Frame) (mem : Nat → BitVec 8) (regs : Nat → BitVec 64) :
    restored fr.hint fr.sp mem regs = restoreView fr mem regs := rfl

theorem restored_low (h : FrameHint) (layout : LayoutOk h) (S : Nat) (mem : Nat → BitVec 8)
    (regs : Nat → BitVec 64) (r : Nat) (hr : isCalleeSaved r = false) :
    restored h S mem regs r = regs r := by
  unfold restored
  cases ho : lookupOffset h.saved r with
  | none => rfl
  | some off =>
    have := (layout.save r off (lookupOffset_mem _ _ _ ho)).2.2
    rw [hr] at this; cases this

theorem epilogue_straight (h : FrameHint) : (epilogue h).all straight = true := by
  simp only [epilogue, List.all_append, List.all_cons, List.all_nil, Bool.and_true,
    Bool.and_eq_true, List.all_map, List.all_eq_true]
  refine ⟨⟨fun _ _ => rfl, ?_⟩, rfl⟩
  split <;> simp [straight]

theorem execute_ret_top (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (locs : List Loc) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hpc : a.pc = b.insts.length)
    (hterm : b.term = .ret locs) (habi : abiRegisters locs = true) (values : List Machine.Value)
    (hvals : locs.mapM (Alloc.readValue a.storage) = .ok values) (hstack : s.stack = []) :
    Alloc.execute p s a = .ok { s with
      control := .halted (.ok values)
      memory := s.memory.endFrame a.owner } := by
  rw [execute_term_eq p s a f b hf hb hpc, hterm]
  simp only [habi, hvals, hstack, Bool.not_true, Bool.false_eq_true, if_false, bind, Except.bind,
    pure, Except.pure]

theorem execute_ret_call (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (locs : List Loc) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hpc : a.pc = b.insts.length)
    (hterm : b.term = .ret locs) (habi : abiRegisters locs = true) (values : List Machine.Value)
    (hvals : locs.mapM (Alloc.readValue a.storage) = .ok values) (frame : ReturnFrame)
    (rest : List ReturnFrame) (hstack : s.stack = frame :: rest)
    (hres : (!abiRegisters frame.results || frame.results.length != locs.length) = false) :
    Alloc.execute p s a = .ok { s with
      control := .running {frame.caller with
        storage := returnStorage frame.caller.storage a.storage locs.length}
      stack := rest
      memory := s.memory.endFrame a.owner } := by
  rw [execute_term_eq p s a f b hf hb hpc, hterm]
  simp only [habi, hvals, hstack, hres, Bool.not_true, Bool.false_eq_true, if_false, bind,
    Except.bind, pure, Except.pure]

theorem readValue_gpr {st : Storage} {regs : Nat → BitVec 64} (shape : Shape st) (rel : RegsRel regs st)
    {n : Nat} {w : Machine.Width} {v : Machine.Value} (h : Alloc.readValue st (.gpr n w) = .ok v) :
    v = .int (Machine.normalizeAt w (regs n)) := by
  simp only [Alloc.readValue] at h
  obtain ⟨slot, hr, hm⟩ := bind_ok h
  cases slot with
  | flags _ => cases hm
  | value val => cases hm; exact readLoc_gpr shape rel hr

theorem readValue_malformed (st : Storage) (l : Loc) : MalformedOnly (Alloc.readValue st l) := by
  unfold Alloc.readValue
  apply bind_malformed _ _ (readLoc_malformed st l)
  intro slot _
  cases slot with
  | value _ => exact pure_malformed _
  | flags _ => intro e h; cases h; exact ⟨_, rfl⟩

theorem mapM_malformed (st : Storage) :
    ∀ (locs : List Loc), MalformedOnly (locs.mapM (Alloc.readValue st))
  | [] => pure_malformed _
  | l :: rest => by
    simp only [List.mapM_cons]
    apply bind_malformed _ _ (readValue_malformed st l)
    intro _ _
    apply bind_malformed _ _ (mapM_malformed st rest)
    intro _ _
    exact pure_malformed _

theorem abiRegisters_first (locs : List Loc) (h : abiRegisters locs = true) :
    ∀ l ∈ locs, ∃ n w, l = .gpr n w ∧ n < 8 := by
  simp only [abiRegisters, Bool.and_eq_true, decide_eq_true_eq, abiRegisterCount] at h
  obtain ⟨len, from0⟩ := h
  suffices ∀ k (locs : List Loc), abiRegistersFrom k locs = true →
      ∀ l ∈ locs, ∃ n w, l = .gpr n w ∧ n < k + locs.length by
    intro l m
    obtain ⟨n, w, rfl, hn⟩ := this 0 locs from0 l m
    exact ⟨n, w, rfl, by omega⟩
  intro k locs hk
  induction locs generalizing k with
  | nil => intro l m; cases m
  | cons x rest ih =>
    simp only [abiRegistersFrom, Bool.and_eq_true, beq_iff_eq] at hk
    intro l m
    rcases List.mem_cons.mp m with rfl | m
    · cases l with
      | gpr n w => simp [Loc.place] at hk; exact ⟨n, w, rfl, by simp; omega⟩
      | _ => simp [Loc.place] at hk
    · obtain ⟨n, w, e, hn⟩ := ih (k+1) hk.2 l m
      exact ⟨n, w, e, by simp; omega⟩

theorem execute_ret_bad (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (locs : List Loc) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hpc : a.pc = b.insts.length)
    (hterm : b.term = .ret locs)
    (bad : abiRegisters locs = false ∨ (∃ e, locs.mapM (Alloc.readValue a.storage) = .error e) ∨
      (∃ frame rest, s.stack = frame :: rest ∧
        (!abiRegisters frame.results || frame.results.length != locs.length) = true)) :
    ∃ msg, Alloc.execute p s a = .error (.malformed msg) := by
  rw [execute_term_eq p s a f b hf hb hpc, hterm]
  rcases bad with habi | ⟨e, he⟩ | ⟨frame, rest, hstack, hres⟩
  · exact ⟨_, by simp [habi, bind, Except.bind]; rfl⟩
  · obtain ⟨msg, rfl⟩ := mapM_malformed a.storage locs e he
    cases habi : abiRegisters locs
    · exact ⟨_, by simp [habi, bind, Except.bind]; rfl⟩
    · exact ⟨msg, by simp [habi, he, bind, Except.bind]⟩
  · cases habi : abiRegisters locs
    · exact ⟨_, by simp [habi, bind, Except.bind]; rfl⟩
    · cases hvals : locs.mapM (Alloc.readValue a.storage) with
      | error e =>
        obtain ⟨msg, rfl⟩ := mapM_malformed a.storage locs e hvals
        exact ⟨msg, by simp [habi, hvals, bind, Except.bind]⟩
      | ok values =>
        exact ⟨_, by simp only [habi, hvals, hstack, hres, Bool.not_true, Bool.false_eq_true,
          if_false, if_true, bind, Except.bind]; rfl⟩

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem observe_returned (type : Core.Source.IntType) (s : Alloc.State) (values : List Machine.Value)
    (hc : s.control = .halted (.ok values)) (o : Core.Observation) (h : Alloc.observe type s = some o) :
    ∃ bits, values = [.int bits] ∧ (type.width = 32 ∨ type.width = 64) ∧
      o = .returned type (bits.setWidth type.width).toNat := by
  cases values with
  | nil => simp [Alloc.observe, hc] at h
  | cons v rest =>
    cases rest with
    | cons _ _ =>
      cases v <;> simp [Alloc.observe, hc] at h
    | nil =>
      cases v with
      | pointer _ _ => simp [Alloc.observe, hc] at h
      | int bits =>
        simp only [Alloc.observe, hc] at h
        by_cases hw : (type.width == 32 || type.width == 64) = true
        · rw [if_pos hw] at h
          cases h
          refine ⟨bits, rfl, ?_, rfl⟩
          simp only [Bool.or_eq_true, beq_iff_eq] at hw
          exact hw
        · rw [if_neg hw] at h; cases h

theorem mapM_single {α β : Type} (f : α → Except Core.Source.Fault β) (xs : List α) (y : β)
    (h : xs.mapM f = .ok [y]) : ∃ x, xs = [x] ∧ f x = .ok y := by
  cases xs with
  | nil => simp at h; cases h
  | cons x rest =>
    simp only [List.mapM_cons] at h
    obtain ⟨y', hy, h⟩ := bind_ok h
    obtain ⟨ys, hys, h⟩ := bind_ok h
    simp only [pure, Except.pure, Except.ok.injEq, List.cons.injEq] at h
    obtain ⟨rfl, rfl⟩ := h
    cases rest with
    | nil => exact ⟨x, rfl, hy⟩
    | cons z more =>
      simp only [List.mapM_cons] at hys
      obtain ⟨_, _, hys⟩ := bind_ok hys
      obtain ⟨_, _, hys⟩ := bind_ok hys
      simp [pure, Except.pure] at hys

theorem setWidth_normalizeAt_le (v : Machine.Width) (x : BitVec 64) (W : Nat) (hW : W ≤ v.bits) :
    (Machine.normalizeAt v x).setWidth W = x.setWidth W := by
  apply BitVec.eq_of_toNat_eq
  cases v <;> simp only [Machine.normalizeAt, Machine.Width.bits, BitVec.toNat_setWidth] at hW ⊢
  · rw [Nat.mod_eq_of_lt (a := x.toNat % 2^32) (b := 2^64) (by omega),
      Nat.mod_mod_of_dvd _ (Nat.pow_dvd_pow 2 hW)]
  · rw [Nat.mod_mod_of_dvd _ (Nat.pow_dvd_pow 2 hW), Nat.mod_mod_of_dvd _ (Nat.pow_dvd_pow 2 hW)]

theorem decode_returnControl (file : File) (t : Asm.State) (i pos : Nat) (hi : i < 2^30)
    (hpos : pos < functionSpan) (hlen : i < file.functions.length)
    (h30 : t.regs 30 = codeAddress i pos) : returnControl file t = .running i pos := by
  simp [returnControl, h30, decode_codeAddress i pos hpos hi, hlen]

theorem outside_returnControl (file : File) (t : Asm.State)
    (h30 : decodeAddress (t.regs 30) = none) : returnControl file t = .halted (.ok (t.regs 0)) := by
  simp [returnControl, h30]

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

/-- The caller storage after a return relates to the registers after the
epilogue: the results come from the callee, the callee-saved registers are
the restored values, all other call-clobbered places are unspecified. -/
theorem returnStorage_rel (caller callee : Storage) (count : Nat) (small : count ≤ 8)
    (shapeCaller : Shape caller) (shapeCallee : Shape callee)
    (regsOld regsNew : Nat → BitVec 64) (vis : Nat → BitVec 64) (flags : Flags)
    (high : Nat) (mem : Nat → BitVec 8) (fr : Frame)
    (calleeRegs : RegsRel regsOld callee) (savedCaller : SavedRel vis caller)
    (own : OwnSlots high mem fr caller)
    (low8 : ∀ n, n < 8 → regsNew n = regsOld n)
    (saved : ∀ r, isCalleeSaved r = true → regsNew r = vis r) :
    Shape (returnStorage caller callee count) ∧ RegsRel regsNew (returnStorage caller callee count) ∧
      FlagsRel flags (returnStorage caller callee count) ∧
      OutgoingRel mem fr (returnStorage caller callee count) ∧
      OwnSlots high mem fr (returnStorage caller callee count) := by
  refine ⟨⟨fun place v d h => ?_, fun n hn => ?_⟩, fun n b d h => ?_, fun fl d c b h => ?_,
    fun k cell h => ?_, ⟨fun k cell h => ?_, fun k cell h => ?_⟩⟩
  · cases place with
    | gpr n =>
      simp only [returnStorage] at h
      split at h
      · exact shapeCallee.1 _ v d h
      · split at h
        · cases h
        · exact shapeCaller.1 _ v d h
    | _ =>
      simp only [returnStorage] at h
      split at h
      · cases h
      · exact shapeCaller.1 _ v d h
  · simp only [returnStorage]
    have : ¬ n < count := by intro lt; simp [allocatable, isCalleeSaved] at hn; omega
    simp only [this, if_false]
    split
    · rfl
    · exact shapeCaller.2 n hn
  · simp only [returnStorage] at h
    split at h
    · rename_i lt
      rw [low8 n (by omega)]; exact calleeRegs n b d h
    · split at h
      · cases h
      · rename_i _ notClobbered
        simp only [Place.callClobbered, decide_eq_true_eq] at notClobbered
        by_cases cs : isCalleeSaved n = true
        · rw [saved n cs]; exact savedCaller n b d cs h
        · have : allocatable n = false := by
            simp only [allocatable, cs, Bool.or_false]
            simp only [decide_eq_false_iff_not]
            intro le
            exact notClobbered (by omega)
          rw [shapeCaller.2 n this] at h; cases h
  · simp [returnStorage, Place.callClobbered] at h
  · simp [returnStorage, Place.callClobbered] at h
  · simp only [returnStorage, Place.callClobbered] at h
    exact own.1 k cell (by simpa using h)
  · simp only [returnStorage, Place.callClobbered] at h
    exact own.2 k cell (by simpa using h)

end EvoIR.Asm.Proof
