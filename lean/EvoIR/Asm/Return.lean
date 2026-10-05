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

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem outermost_pop (a a' : Activation) (frame : ReturnFrame) (rest : List ReturnFrame)
    (h : a'.function = frame.caller.function) :
    outermost a' rest = outermost a (frame :: rest) := by
  cases rest with
  | nil => simp [outermost, h]
  | cons x more =>
    simp only [outermost, List.getLast?_cons_cons]
    rw [List.getLast?_eq_some_getLast (l := x :: more) (by simp)]
    rfl

/-- The chunk of a return. -/
theorem advance_ret {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    (rv : ResultView ctx.p entry type)
    {s : Alloc.State} {t : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (c : Cursor ctx entry s t) (hpc : c.a.pc = c.b.insts.length) (locs : List Loc)
    (hterm : c.b.term = .ret locs) : Advance ctx entry type s0 s t := by
  obtain ⟨a, cur, frs, pos, f, g, b, hs, ht, outer, ok, name, block, pc, at_, hl, need, chain⟩ := c
  simp only at hpc hterm
  have hf : ctx.p.functions.find? (·.name == a.function) = some f := by
    rw [← name]; exact names_find ctx checked cur.index f ok.at_.1
  have malformed : ∀ msg, Alloc.execute ctx.p s a = .error (.malformed msg) → False := by
    intro msg he
    have hstep : Alloc.step ctx.p s = some {s with control := .halted (.error (.malformed msg))} := by
      simp [Alloc.step, hs, he]
    exact no_malformed safe reach hstep msg rfl
  -- The template: the epilogue and `ret`.
  obtain ⟨pre, post, tmpl, tail, split, htmpl, htail, hd⟩ :=
    At_term cur.hint f g.body b a.pc pos at_ (by omega)
  rw [hterm] at htmpl
  simp only [termTemplate, Option.some.injEq] at htmpl
  subst htmpl
  have entryLe := hl.top
  have spLow := hl.bottom
  have high := hl.high
  unfold Frame.entry at entryLe
  obtain ⟨t1, hrun, hsp1, hfl1, hlow1, hhigh1, hmem1, hregs1⟩ :=
    epilogue_run cur.index pos cur.hint ok.layout t cur.sp ht hl.sp hl.sp16 spLow (by omega) high
  have hd1 : g.body.drop pos = (epilogue cur.hint).map .inst ++ (.inst .ret :: tail) := by
    rw [hd]; simp
  obtain ⟨path1, ctrl1⟩ := run_path ctx.asm cur.index g ok.at_.2.2 (epilogue cur.hint) pos _ t t1 ht
    hd1 (epilogue_straight cur.hint) hrun
  have hd2 : g.body.drop (pos + (epilogue cur.hint).length) = .inst .ret :: tail := by
    rw [← List.drop_drop, hd1]; simp
  have hret := step_at ctx.asm t1 cur.index _ g .ret tail ctrl1 ok.at_.2.2 hd2
  simp only [execute, pure, Except.pure] at hret
  have r30 : t1.regs 30 = load64 t.memory (cur.fp + 8) := by
    rw [hregs1, update_same]; rfl
  have r0 : t1.regs 0 = t.regs 0 := by
    rw [hregs1, update_other _ _ _ _ (by decide), update_other _ _ _ _ (by decide),
      restored_low _ ok.layout _ _ _ _ rfl]
  have rcs : ∀ r, isCalleeSaved r = true → t1.regs r = restoreView cur t.memory t.regs r := by
    intro r hr
    have r29 : r ≠ 29 := by intro e; rw [e] at hr; simp [isCalleeSaved] at hr
    have r30 : r ≠ 30 := by intro e; rw [e] at hr; simp [isCalleeSaved] at hr
    rw [hregs1, update_other _ _ _ _ r30, update_other _ _ _ _ r29]; rfl
  have rlow : ∀ n, n < 8 → t1.regs n = t.regs n := by
    intro n hn
    rw [hregs1, update_other _ _ _ _ (by omega), update_other _ _ _ _ (by omega),
      restored_low _ ok.layout _ _ _ _ (by simp [isCalleeSaved]; omega)]
  have hexec_bad := execute_ret_bad ctx.p s a f b locs hf block hpc hterm
  cases habi : abiRegisters locs
  · obtain ⟨msg, he⟩ := hexec_bad (.inl habi); exact absurd (malformed msg he) id
  cases hvals : locs.mapM (Alloc.readValue a.storage) with
  | error e =>
    obtain ⟨msg, he⟩ := hexec_bad (.inr (.inl ⟨e, hvals⟩)); exact absurd (malformed msg he) id
  | ok values =>
  cases hstack : s.stack with
  | nil =>
    -- The outermost return: both machines stop.
    cases frs with
    | cons _ _ => rw [hstack] at chain; cases chain
    | nil =>
    rw [hstack] at chain
    have hexec := execute_ret_top ctx.p s a f b locs hf block hpc hterm habi values hvals hstack
    let sf : Alloc.State := { s with
      control := .halted (.ok values)
      memory := s.memory.endFrame a.owner }
    have hstep : Alloc.step ctx.p s = some sf := by simp [Alloc.step, hs, hexec, sf]
    have reachF : Reaches (Alloc.step ctx.p) s0 sf := reach.trans (.next hstep (.refl _))
    obtain ⟨o, ho⟩ : ∃ o, Alloc.observe type sf = some o := by
      rcases safe sf reachF with ⟨_, h⟩ | h
      · simp [Alloc.step, sf] at h
      · exact h
    obtain ⟨bits, rfl, hw, rfl⟩ := observe_returned type sf _ rfl _ ho
    obtain ⟨l, rfl, hl0⟩ := mapM_single _ locs _ hvals
    obtain ⟨n, w, rfl, hn⟩ := abiRegisters_first [l] habi l (List.mem_singleton_self l)
    have n0 : n = 0 := by
      simp only [abiRegisters, abiRegistersFrom, Bool.and_eq_true, beq_iff_eq, Loc.place] at habi
      exact Place.gpr.inj habi.2.1
    subst n0
    have hbits := readValue_gpr hl.shape hl.regs hl0
    cases hbits
    have hW : type.width ≤ w.bits := by
      have outer' : a.function = entry := by
        simpa [outermost, hstack] using outer
      exact rv f (List.mem_of_getElem? ok.at_.1) (by rw [name]; exact outer') b
        (List.mem_of_find?_eq_some block) _ hterm (Loc.gpr 0 w) (List.mem_singleton_self _)
    let tf : Asm.State := {t1 with control := .halted (.ok (t1.regs 0))}
    have hstep2 : Asm.step ctx.asm t1 = some tf := by
      rw [hret, outside_returnControl ctx.asm t1 (by rw [r30]; exact chain.2)]
    refine .inl ⟨sf, tf, _, .next hstep (.refl _), (pathAppend path1 (.next hstep2 (.refl _))).reaches,
      by simp [Alloc.step, sf], halted_stuck ctx.asm tf _ rfl, ho, ?_⟩
    simp only [Asm.observe, tf, r0]
    rcases hw with hw | hw <;> simp [hw, setWidth_normalizeAt_le w _ _ hW] at hW ⊢
  | cons frame rest =>
    cases frs with
    | nil => rw [hstack] at chain; cases chain
    | cons fr frs' =>
    rw [hstack] at chain
    obtain ⟨hspFr, h16Fr, needFr, hfp, ⟨pos', placed, span, hra⟩, shapeFr, ownFr, savedFr, tail'⟩ :=
      chain
    cases hres : (!abiRegisters frame.results || frame.results.length != locs.length)
    case true =>
      obtain ⟨msg, he⟩ := hexec_bad (.inr (.inr ⟨frame, rest, hstack, hres⟩))
      exact absurd (malformed msg he) id
    case false =>
    have hexec := execute_ret_call ctx.p s a f b locs hf block hpc hterm habi values hvals frame
      rest hstack hres
    let a' : Activation := {frame.caller with
      storage := returnStorage frame.caller.storage a.storage locs.length}
    have hstep : Alloc.step ctx.p s = some { s with
        control := .running a'
        stack := rest
        memory := s.memory.endFrame a.owner } := by
      simp [Alloc.step, hs, hexec, a']
    obtain ⟨f', g', b', fa', name', block', pc', at'⟩ := placed
    obtain ⟨h'', g'', ok'⟩ := funOk_of_check ctx checked fr.index f' fa'.1
    have hh : h'' = fr.hint := by have := ok'.at_.2.1; rw [fa'.2.1] at this; cases this; rfl
    subst hh
    have hg : g'' = g' := by have := ok'.at_.2.2; rw [fa'.2.2] at this; cases this; rfl
    subst hg
    have hlen : fr.index < ctx.asm.functions.length := by
      rcases Nat.lt_or_ge fr.index ctx.asm.functions.length with h | h
      · exact h
      · have := fa'.2.2; rw [List.getElem?_eq_none h] at this; cases this
    let t2 : Asm.State := {t1 with control := .running fr.index pos'}
    have hstep2 : Asm.step ctx.asm t1 = some t2 := by
      rw [hret, decode_returnControl ctx.asm t1 fr.index pos' ok'.index span hlen
        (by rw [r30]; exact hra)]
    have path : Path (Asm.step ctx.asm) ((epilogue cur.hint).length + 1) t t2 :=
      pathAppend path1 (.next hstep2 (.refl _))
    refine .inr ⟨_, t2, .single hstep, path.positive (by omega),
      reach.trans (.next hstep (.refl _)), ?_⟩
    have count := abiRegisters_length locs habi
    obtain ⟨shape', regs', flags', out', own'⟩ := returnStorage_rel frame.caller.storage a.storage
      locs.length count shapeFr hl.shape t.regs t1.regs (restoreView cur t.memory t.regs) t1.flags
      t1.high t1.memory fr hl.regs savedFr
      (by rw [hhigh1, hmem1]; exact ownFr) rlow rcs
    have topFr := chain_top tail'
    have hl' : Local fr a'.storage t2 := by
      refine ⟨?_, h16Fr, ?_, ?_, ?_, ?_, shape', regs', flags', out', own'⟩
      · simp only [t2]; rw [hsp1, hspFr]; rfl
      · simp only [t2]; rw [hregs1, update_other _ _ _ _ (by decide), update_same, ← hfp]; rfl
      · simp only [t2]; rw [hhigh1]; exact high
      · simp only [t2]; rw [hhigh1]; exact topFr
      · simp only [t2]; rw [hlow1, hspFr]; unfold Frame.entry; omega
    have chain' : Chain ctx t2.low t2.high t2.memory fr t2.regs rest frs' := by
      simp only [t2]
      rw [hlow1, hhigh1, hmem1]
      exact chain_congr ctx t.low t.high rest frs' fr t.memory t.memory _ _ (fun _ _ => rfl)
        (fun _ _ _ _ _ => rfl) (fun r hr _ => rcs r hr) tail'
    exact running_intro a' fr frs' pos' f' g'' b' rfl rfl
      (by rw [outermost_pop a a' frame rest rfl, ← hstack]; exact outer) fa' name' block' pc' at'
      hl' (by simp only [t2]; rw [hlow1]; exact needFr) chain'

end EvoIR.Asm.Proof
