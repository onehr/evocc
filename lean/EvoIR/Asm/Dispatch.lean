import EvoIR.Asm.Facts

/-!
Each admitted non-call instruction and its template keep the local relation.
The dispatch inverts the template check and applies the step lemma of the
instruction form.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

theorem gprOf_eq {l : Loc} {n : Nat} {v : Machine.Width} (h : gprOf l = some (n, v)) :
    l = .gpr n v := by
  cases l <;> simp [gprOf] at h; obtain ⟨rfl, rfl⟩ := h; rfl

theorem oneStep_run {cur : Frame} {st' : Storage} {t : Asm.State} {i : Inst} (fn pos : Nat)
    (h : OneStep cur st' t i) :
    ∃ t', runFrom fn pos [i] t = .ok t' ∧ Local cur st' t' ∧ Footprint cur t t' := by
  obtain ⟨t', he, hl, hf⟩ := h
  refine ⟨{t' with control := .running fn (pos+1)}, ?_, hl.control _, hf.control _⟩
  simp [runFrom, he, bind, Except.bind]

macro "template_inv" ht:ident : tactic => `(tactic| (
  simp only [instTemplate, Option.bind_eq_bind, Option.bind_eq_some_iff,
    Option.ite_none_right_eq_some, Option.some.injEq, Prod.exists, Option.pure_def] at $ht:ident))

theorem body_dispatch {cur : Frame} {st st' : Storage} {t : Asm.State} {owner : Nat}
    {mem mem' : Machine.Memory} (hl : Local cur st t) (layout : LayoutOk cur.hint)
    (inst : Instruction) (tmpl : List Inst) (ht : instTemplate cur.hint inst = some tmpl)
    (notCall : ∀ s a r, inst ≠ .bl s a r)
    (writable : ∀ n w, Loc.gpr n w ∈ instWritten inst → Writable cur n)
    (outOk : ∀ k src, inst = .outgoingArg k src → 8 * k + 8 ≤ cur.hint.outgoing)
    (heval : Alloc.evalInstruction owner inst st mem = .ok (st', mem')) (fn pos : Nat) :
    mem' = mem ∧ ∃ t', runFrom fn pos tmpl t = .ok t' ∧ Local cur st' t' ∧ Footprint cur t t' := by
  have w1 : ∀ (d : Nat) (v : Machine.Width), Loc.gpr d v ∈ instWritten inst → Writable cur d :=
    fun d v m => writable d v m
  cases inst with
  | mov dst src =>
    template_inv ht
    obtain ⟨d, dv, hd, s, sv, hs, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hs
    obtain ⟨rfl, h⟩ := step_mov hl d s _ _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | movImm dst imm =>
    template_inv ht
    obtain ⟨d, dv, hd, rfl⟩ := ht
    cases gprOf_eq hd
    exact step_movImm hl fn pos d imm _ (w1 d _ List.mem_cons_self) heval
  | binary op dst lhs rhs w =>
    template_inv ht
    obtain ⟨d, dv, hd, n, nv, hn, m, mv, hm, ⟨rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn; cases gprOf_eq hm
    obtain ⟨rfl, h⟩ := step_binary hl op d n m _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | binaryImm op dst lhs imm w =>
    template_inv ht
    obtain ⟨d, dv, hd, n, nv, hn, ⟨rfl, view⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn
    rcases view with rfl | ⟨rfl, rfl, rfl, small⟩
    · obtain ⟨rfl, h⟩ := step_binaryImm hl op d n imm _ (w1 d _ List.mem_cons_self) heval
      exact ⟨rfl, oneStep_run fn pos h⟩
    · obtain ⟨rfl, h⟩ := step_zeroMask hl d n imm small (w1 d _ List.mem_cons_self) heval
      exact ⟨rfl, oneStep_run fn pos h⟩
  | flagBinary op dst flags lhs rhs w =>
    template_inv ht
    obtain ⟨d, dv, hd, n, nv, hn, m, mv, hm, ⟨rfl, rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn; cases gprOf_eq hm
    obtain ⟨rfl, h⟩ := step_flagBinary hl op d n m _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | wideMultiply op dst lhs rhs =>
    template_inv ht
    obtain ⟨d, dv, hd, n, nv, hn, m, mv, hm, ⟨rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn; cases gprOf_eq hm
    obtain ⟨rfl, h⟩ := step_wideMultiply hl op d n m (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | msub dst n m a w =>
    template_inv ht
    obtain ⟨d, dv, hd, n', nv, hn, m', mv, hm, a', av, ha, ⟨rfl, rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn; cases gprOf_eq hm; cases gprOf_eq ha
    obtain ⟨rfl, h⟩ := step_msub hl d n' m' a' _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | madd dst n m a w =>
    template_inv ht
    obtain ⟨d, dv, hd, n', nv, hn, m', mv, hm, a', av, ha, ⟨rfl, rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn; cases gprOf_eq hm; cases gprOf_eq ha
    obtain ⟨rfl, h⟩ := step_madd hl d n' m' a' _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | extend op dst src =>
    template_inv ht
    obtain ⟨d, dv, hd, s, sv, hs, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hs
    obtain ⟨rfl, h⟩ := step_extend hl op d s _ _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | bfx signed dst src lsb bits w =>
    template_inv ht
    obtain ⟨d, dv, hd, s, sv, hs, ⟨rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hs
    obtain ⟨rfl, h⟩ := step_bfx hl signed d s lsb bits _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | cmp flags lhs rhs w =>
    template_inv ht
    obtain ⟨n, nv, hn, m, mv, hm, ⟨rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hn; cases gprOf_eq hm
    obtain ⟨rfl, h⟩ := step_cmp hl n m _ heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | cmpImm flags lhs imm w =>
    template_inv ht
    obtain ⟨n, nv, hn, ⟨rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hn
    obtain ⟨rfl, h⟩ := step_cmpImm hl n imm _ heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | tstImm flags lhs imm w =>
    template_inv ht
    obtain ⟨n, nv, hn, ⟨rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hn
    obtain ⟨rfl, h⟩ := step_tstImm hl n imm _ heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | cset dst flags cond =>
    template_inv ht
    obtain ⟨d, dv, hd, rfl, rfl⟩ := ht
    cases gprOf_eq hd
    obtain ⟨rfl, h⟩ := step_cset hl d _ cond (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | csel dst flags lhs rhs cond w =>
    template_inv ht
    obtain ⟨d, dv, hd, n, nv, hn, m, mv, hm, ⟨rfl, rfl, rfl, rfl⟩, rfl⟩ := ht
    cases gprOf_eq hd; cases gprOf_eq hn; cases gprOf_eq hm
    obtain ⟨rfl, h⟩ := step_csel hl d n m _ cond (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | load => simp [instTemplate] at ht
  | store => simp [instTemplate] at ht
  | frameSlot => simp [instTemplate] at ht
  | stackRelease => simp [instTemplate] at ht
  | bl s a r => exact absurd rfl (notCall s a r)
  | spillStore slot src =>
    template_inv ht
    obtain ⟨s, sv, hs, off, hoff, rfl⟩ := ht
    cases gprOf_eq hs
    obtain ⟨rfl, h⟩ := step_spillStore hl layout slot s off _ hoff heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | spillReload dst slot =>
    template_inv ht
    obtain ⟨d, dv, hd, off, hoff, rfl⟩ := ht
    cases gprOf_eq hd
    obtain ⟨rfl, h⟩ := step_spillReload hl layout d slot off _ (w1 d _ List.mem_cons_self)
      hoff heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | incomingArg dst slot =>
    template_inv ht
    obtain ⟨d, dv, hd, rfl⟩ := ht
    cases gprOf_eq hd
    obtain ⟨rfl, h⟩ := step_incomingArg hl d slot _ (w1 d _ List.mem_cons_self) heval
    exact ⟨rfl, oneStep_run fn pos h⟩
  | outgoingArg slot src =>
    template_inv ht
    obtain ⟨s, sv, hs, rfl⟩ := ht
    cases gprOf_eq hs
    obtain ⟨rfl, h⟩ := step_outgoingArg hl layout slot s _ (outOk slot _ rfl) heval
    exact ⟨rfl, oneStep_run fn pos h⟩

end EvoIR.Asm.Proof
