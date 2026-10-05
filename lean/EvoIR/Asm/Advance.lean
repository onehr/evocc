import EvoIR.Asm.Dispatch

/-!
Chunks of the simulation at the level of whole states. Each chunk is one
allocated step and a positive number of assembly steps. This file has the
common parts and the chunk of a non-call instruction.

The allocated program is assumed safe from its entry state: each reachable
state either steps or has an observation. The allocation stage proves this
for its target program (`Refines.progress`).
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

/-- Every reachable allocated state steps or has an observation. -/
def Safe (p : Alloc.Program) (type : Core.Source.IntType) (s0 : Alloc.State) : Prop :=
  ∀ s, Reaches (Alloc.step p) s0 s →
    (∃ s', Alloc.step p s = some s') ∨ ∃ o, Alloc.observe type s = some o

theorem no_malformed {p : Alloc.Program} {type : Core.Source.IntType} {s0 s s' : Alloc.State}
    (safe : Safe p type s0) (reach : Reaches (Alloc.step p) s0 s) (hstep : Alloc.step p s = some s')
    (msg : String) (halted : s'.control = .halted (.error (.malformed msg))) : False := by
  have reach' : Reaches (Alloc.step p) s0 s' := reach.trans (.next hstep (.refl _))
  rcases safe s' reach' with ⟨_, h⟩ | ⟨_, h⟩
  · simp [Alloc.step, halted] at h
  · simp [Alloc.observe, halted] at h

theorem chain_top {ctx : Ctx} {low high : Nat} {mem : Nat → BitVec 8} :
    ∀ {callers : List ReturnFrame} {frs : List Frame} {inner : Frame} {vis : Nat → BitVec 64},
      Chain ctx low high mem inner vis callers frs → inner.entry ≤ high
  | [], [], _, _, chain => chain.1
  | _ :: _, _ :: _, inner, _, chain => by
    obtain ⟨hsp, _, _, _, _, _, _, _, tail⟩ := chain
    have := chain_top tail
    unfold Frame.entry at *
    omega
  | [], _ :: _, _, _, chain => by cases chain
  | _ :: _, [], _, _, chain => by cases chain

/-- The parts of `Running` that a chunk proof uses. -/
structure Cursor (ctx : Ctx) (entry : String) (s : Alloc.State) (t : Asm.State) where
  a : Activation
  cur : Frame
  frs : List Frame
  pos : Nat
  f : Alloc.Function
  g : Asm.Function
  b : Alloc.Block
  hs : s.control = .running a
  ht : t.control = .running cur.index pos
  outer : outermost a s.stack = entry
  ok : FunOk ctx cur.index f cur.hint g
  name : f.name = a.function
  block : f.blocks.find? (·.id == a.block) = some b
  pc : a.pc ≤ b.insts.length
  at_ : At cur.hint f g.body b a.pc pos
  local_ : Local cur a.storage t
  need : t.low + needOf ctx.p ctx.hints a.function ≤ cur.entry
  chain : Chain ctx t.low t.high t.memory cur t.regs s.stack frs

theorem cursor_of_running {ctx : Ctx} {entry : String} {s : Alloc.State} {t : Asm.State}
    (checked : checkAsm ctx.p ctx.asm ctx.hints = true) (rel : Running ctx entry s t) :
    Nonempty (Cursor ctx entry s t) := by
  obtain ⟨a, cur, frs, pos, hs, ht, outer, ⟨f, g, b, fa, name, block, pc, at_⟩, sp, sp16, fp, need,
    high, shape, regs, flags, outgoing, slots, chain⟩ := rel
  obtain ⟨h', g', ok⟩ := funOk_of_check ctx checked cur.index f fa.1
  have : h' = cur.hint := by have := ok.at_.2.1; rw [fa.2.1] at this; cases this; rfl
  subst this
  have : g' = g := by have := ok.at_.2.2; rw [fa.2.2] at this; cases this; rfl
  subst this
  have top := chain_top chain
  have bottom : t.low ≤ cur.sp := by
    have := ok.need
    rw [name] at this
    unfold frameBytes at this; unfold Frame.entry at need; omega
  exact ⟨⟨a, cur, frs, pos, f, g', b, hs, ht, outer, ok, name, block, pc, at_,
    ⟨sp, sp16, fp, high, top, bottom, shape, regs, flags, outgoing, slots⟩, need, chain⟩⟩

theorem running_intro {ctx : Ctx} {entry : String} {s : Alloc.State} {t : Asm.State}
    (a : Activation) (cur : Frame) (frs : List Frame) (pos : Nat) (f : Alloc.Function)
    (g : Asm.Function) (b : Alloc.Block)
    (hs : s.control = .running a) (ht : t.control = .running cur.index pos)
    (outer : outermost a s.stack = entry) (fa : FunAt ctx cur.index f cur.hint g)
    (name : f.name = a.function) (block : f.blocks.find? (·.id == a.block) = some b)
    (pc : a.pc ≤ b.insts.length) (at_ : At cur.hint f g.body b a.pc pos)
    (hl : Local cur a.storage t) (need : t.low + needOf ctx.p ctx.hints a.function ≤ cur.entry)
    (chain : Chain ctx t.low t.high t.memory cur t.regs s.stack frs) : Running ctx entry s t :=
  ⟨a, cur, frs, pos, hs, ht, outer, ⟨f, g, b, fa, name, block, pc, at_⟩, hl.sp, hl.sp16, hl.fp,
    need, hl.high, hl.shape, hl.regs, hl.flags, hl.outgoing, hl.slots, chain⟩

/-! The allocated step of a non-call instruction. -/

theorem execute_inst (p : Alloc.Program) (s : Alloc.State) (a : Activation) (f : Alloc.Function)
    (b : Alloc.Block) (inst : Instruction) (hf : p.functions.find? (·.name == a.function) = some f)
    (hb : f.blocks.find? (·.id == a.block) = some b) (hi : b.insts[a.pc]? = some inst)
    (notCall : ∀ sym args res, inst ≠ .bl sym args res) :
    Alloc.execute p s a = (do
      let (storage, memory) ← Alloc.evalInstruction a.owner inst a.storage s.memory
      return {s with control := .running {a with pc := a.pc+1, storage}, memory}) := by
  cases inst
  case bl sym args res => exact absurd rfl (notCall sym args res)
  all_goals simp only [Alloc.execute, hf, hb, hi]

theorem immediateLines_straight (v : Machine.Width) (d imm : Nat) :
    (immediateLines v d imm).all straight = true ∧ 0 < (immediateLines v d imm).length := by
  refine ⟨?_, by simp [immediateLines]⟩
  simp only [immediateLines, List.all_cons, List.all_filterMap, Bool.and_eq_true, List.all_eq_true,
    List.mem_range]
  refine ⟨rfl, fun x _ => ?_⟩
  split
  · rename_i b hb
    split at hb
    · cases hb
    · cases hb; rfl
  · rfl

theorem template_straight (h : FrameHint) (inst : Instruction) (tmpl : List Inst)
    (ht : instTemplate h inst = some tmpl) (notCall : ∀ sym args res, inst ≠ .bl sym args res) :
    tmpl.all straight = true ∧ 0 < tmpl.length := by
  cases inst <;> simp only [instTemplate, Option.bind_eq_bind, Option.bind_eq_some_iff,
    Option.ite_none_right_eq_some, Option.some.injEq, Prod.exists, Option.pure_def] at ht
  all_goals first
    | exact absurd rfl (notCall _ _ _)
    | (obtain ⟨_, _, _, rfl⟩ := ht; exact immediateLines_straight _ _ _)
    | (obtain ⟨_, _, _, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, _, _, _, _, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, _, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, _, _, _, rfl⟩ := ht; simp [straight])
    | (obtain ⟨_, _, _, rfl⟩ := ht; simp [straight])
    | simp at ht

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem toMachine_pure (h : FrameHint) (inst : Instruction) (tmpl : List Inst)
    (ht : instTemplate h inst = some tmpl) (notCall : ∀ sym args res, inst ≠ .bl sym args res) :
    (inst.toMachine Loc.reg).pure = true := by
  cases inst <;> first
    | rfl
    | exact absurd rfl (notCall _ _ _)
    | simp [instTemplate] at ht

theorem evalInstruction_malformed (owner : Nat) (inst : Instruction) (st : Storage)
    (mem : Machine.Memory) (pure : (inst.toMachine Loc.reg).pure = true) :
    MalformedOnly (Alloc.evalInstruction owner inst st mem) := by
  unfold Alloc.evalInstruction
  apply bind_malformed _ _ (evalPure_malformed owner _ _ mem pure (operandReader_malformed st))
  intro _ _
  exact pure_malformed _

theorem inst_mem (f : Alloc.Function) (b : Alloc.Block) (i : Nat) (hi : i < b.insts.length)
    (hb : f.blocks.find? (·.id == b.id) = some b ∨ b ∈ f.blocks) :
    b.insts[i] ∈ instructionsOf f := by
  have mb : b ∈ f.blocks := by
    rcases hb with h | h
    · exact List.mem_of_find?_eq_some h
    · exact h
  simp only [instructionsOf, List.mem_flatMap]
  exact ⟨b, mb, List.getElem_mem hi⟩

theorem path_positive {step : α → Option α} {n : Nat} {s t : α} (path : Path step n s t)
    (pos : 0 < n) : Relation.TransGen (Predecessor step) t s := path.positive pos

/-- The chunk of a non-call instruction. -/
theorem advance_body {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    {s : Alloc.State} {t : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (c : Cursor ctx entry s t) (hi : c.a.pc < c.b.insts.length)
    (notCall : ∀ sym args res, c.b.insts[c.a.pc] ≠ .bl sym args res) :
    ∃ s' t', Relation.TransGen (Predecessor (Alloc.step ctx.p)) s' s ∧
      Relation.TransGen (Predecessor (Asm.step ctx.asm)) t' t ∧ Running ctx entry s' t' := by
  obtain ⟨a, cur, frs, pos, f, g, b, hs, ht, outer, ok, name, block, pc, at_, hl, need, chain⟩ := c
  simp only at hi notCall
  have hf : ctx.p.functions.find? (·.name == a.function) = some f := by
    rw [← name]; exact names_find ctx checked cur.index f ok.at_.1
  have hinst : b.insts[a.pc]? = some b.insts[a.pc] := List.getElem?_eq_getElem hi
  have hexec := execute_inst ctx.p s a f b _ hf block hinst notCall
  obtain ⟨tmpl, htmpl, hdrop, at'⟩ := At_inst cur.hint f g.body b a.pc pos at_ hi
  have mi := inst_mem f b a.pc hi (.inr (List.mem_of_find?_eq_some block))
  cases he : Alloc.evalInstruction a.owner b.insts[a.pc] a.storage s.memory with
  | error e =>
    obtain ⟨msg, rfl⟩ := evalInstruction_malformed a.owner _ a.storage s.memory
      (toMachine_pure cur.hint _ tmpl htmpl notCall) e he
    have hstep : Alloc.step ctx.p s = some {s with control := .halted (.error (.malformed msg))} := by
      simp [Alloc.step, hs, hexec, he, bind, Except.bind]
    exact absurd (no_malformed safe reach hstep msg rfl) id
  | ok result =>
    obtain ⟨st', mem'⟩ := result
    have hstep : Alloc.step ctx.p s =
        some {s with control := .running {a with pc := a.pc+1, storage := st'}, memory := mem'} := by
      simp [Alloc.step, hs, hexec, he, bind, Except.bind, pure, Except.pure]
    obtain ⟨rfl, t', hrun, hl', hfp⟩ := body_dispatch hl ok.layout _ tmpl htmpl notCall
      (fun n w m => ok.writable cur rfl _ mi n w m)
      (fun k src e => ok.outgoing k src (e ▸ mi)) he cur.index pos
    obtain ⟨straight, nonempty⟩ := template_straight cur.hint _ tmpl htmpl notCall
    obtain ⟨path, ctrl⟩ := run_path ctx.asm cur.index g ok.at_.2.2 tmpl pos _ t t' ht hdrop straight hrun
    refine ⟨_, t', .single hstep, path_positive path nonempty, ?_⟩
    have chain' : Chain ctx t'.low t'.high t'.memory cur t'.regs s.stack frs := by
      rw [hfp.low, hfp.high]
      exact chain_congr ctx t.low t.high s.stack frs cur t.memory t'.memory t.regs t'.regs
        hfp.above hfp.saves (fun r hr hn => hfp.regs r (.inr ⟨hr, hn⟩)) chain
    exact running_intro {a with pc := a.pc+1, storage := st'} cur frs (pos + tmpl.length) f g b
      rfl ctrl outer ok.at_ name block hi at' hl' (by rw [hfp.low]; exact need) chain'

end EvoIR.Asm.Proof
