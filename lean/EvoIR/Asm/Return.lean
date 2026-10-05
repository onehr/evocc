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

theorem execute_ret_bad (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (locs : List Loc) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hpc : a.pc = b.insts.length)
    (hterm : b.term = .ret locs)
    (bad : abiRegisters locs = false ∨ (∃ e, locs.mapM (Alloc.readValue a.storage) = .error e) ∨
      (∃ frame rest, s.stack = frame :: rest ∧
        (!abiRegisters frame.results || frame.results.length != locs.length) = true)) :
    ∃ e, Alloc.execute p s a = .error e ∧
      (abiRegisters locs = false ∨ (∃ msg, e = .malformed msg) ∨
        ∃ e', locs.mapM (Alloc.readValue a.storage) = .error e' ∧ e = e') := by
  rw [execute_term_eq p s a f b hf hb hpc, hterm]
  rcases bad with habi | ⟨e, he⟩ | ⟨frame, rest, hstack, hres⟩
  · exact ⟨_, by simp [habi, bind, Except.bind]; rfl, .inl habi⟩
  · cases habi : abiRegisters locs
    · exact ⟨_, by simp [habi, bind, Except.bind]; rfl, .inl rfl⟩
    · exact ⟨e, by simp [habi, he, bind, Except.bind], .inr (.inr ⟨e, he, rfl⟩)⟩
  · cases habi : abiRegisters locs
    · exact ⟨_, by simp [habi, bind, Except.bind]; rfl, .inl rfl⟩
    · cases hvals : locs.mapM (Alloc.readValue a.storage) with
      | error e => exact ⟨e, by simp [habi, hvals, bind, Except.bind], .inr (.inr ⟨e, rfl, rfl⟩)⟩
      | ok values =>
        exact ⟨_, by simp only [habi, hvals, hstack, hres, Bool.not_true, Bool.false_eq_true,
          if_false, if_true, bind, Except.bind]; rfl, .inr (.inl ⟨_, rfl⟩)⟩

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

end EvoIR.Asm.Proof
