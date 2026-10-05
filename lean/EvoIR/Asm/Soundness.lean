import EvoIR.Asm.Return

/-!
Soundness of the frame and emission checker.

`checkAsm_sound`: an accepted allocated program `p`, assembly file `asm` and
frame hints give a refinement of the allocated machine by the assembly
machine from related entry states. The refinement is
`Core.Transition.Refines` with the observations of `Alloc.observe` and
`Asm.observe`: every terminal assembly state has the observation of a
terminal allocated state, every infinite assembly execution has an infinite
allocated execution, and no reachable assembly state is stuck without an
observation.

The explicit hypotheses are:

* `AbiEntry`: a valid ABI entry state. The control is at the first line of
  the entry function, the argument registers and the incoming stack slots
  hold the integer arguments, the stack pointer is 16-byte aligned, the
  return address in `x30` is outside the program, and the stack region has
  enough bytes below the stack pointer for the deepest call chain of the
  entry function (`needOf`). The stack region ends below `2^64`.
* `ResultView`: the result type is not wider than the view of the returned
  register of the entry function.
* `Safe`: each reachable allocated state steps or has an observation. The
  allocation stage proves this for its target program; the composition in
  `EvoIR.Asm.Baseline` discharges it.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path Refines CheckpointSimulation)

/-- The chunk of `brk`. -/
theorem advance_brk {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    {s : Alloc.State} {t : Asm.State} (reach : Reaches (Alloc.step ctx.p) s0 s)
    (c : Cursor ctx entry s t) (hpc : c.a.pc = c.b.insts.length) (imm : Nat)
    (hterm : c.b.term = .brk imm) : Advance ctx entry type s0 s t := by
  obtain ⟨a, cur, frs, pos, f, g, b, hs, ht, outer, ok, name, block, pc, at_, hl, need, chain⟩ := c
  simp only at hpc hterm
  have hf : ctx.p.functions.find? (·.name == a.function) = some f := by
    rw [← name]; exact names_find ctx checked cur.index f ok.at_.1
  have hexec : Alloc.execute ctx.p s a = .error (.diagnostic imm) := by
    rw [execute_term_eq ctx.p s a f b hf block hpc, hterm]; rfl
  let sf : Alloc.State := {s with control := .halted (.error (.diagnostic imm))}
  have hstep : Alloc.step ctx.p s = some sf := by simp [Alloc.step, hs, hexec, sf]
  obtain ⟨pre, post, tmpl, tail, split, htmpl, htail, hd⟩ :=
    At_term cur.hint f g.body b a.pc pos at_ (by omega)
  rw [hterm] at htmpl
  simp only [termTemplate, Option.some.injEq] at htmpl
  subst htmpl
  let tf : Asm.State := {t with control := .halted (.error (.diagnostic imm))}
  have hstep2 : Asm.step ctx.asm t = some tf := by
    rw [step_at ctx.asm t cur.index pos g (.brk imm) tail ht ok.at_.2.2 (by simpa using hd)]
    rfl
  obtain ⟨o, ho⟩ : ∃ o, Alloc.observe type sf = some o := by
    rcases safe sf (reach.trans (.next hstep (.refl _))) with ⟨_, h⟩ | h
    · simp [Alloc.step, sf] at h
    · exact h
  refine .inl ⟨sf, tf, o, .next hstep (.refl _), .next hstep2 (.refl _),
    by simp [Alloc.step, sf], halted_stuck ctx.asm tf _ rfl, ho, ?_⟩
  simp only [Alloc.observe, sf] at ho
  simp only [Asm.observe, tf]
  exact ho

/-- Every related pair advances. -/
theorem advance {ctx : Ctx} {entry : String} (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    {type : Core.Source.IntType} {s0 : Alloc.State} (safe : Safe ctx.p type s0)
    (rv : ResultView ctx.p entry type) {s : Alloc.State} {t : Asm.State}
    (reach : Reaches (Alloc.step ctx.p) s0 s) (rel : Running ctx entry s t) :
    Advance ctx entry type s0 s t := by
  obtain ⟨c⟩ := cursor_of_running checked rel
  by_cases hi : c.a.pc < c.b.insts.length
  · by_cases call : ∃ sym args res, c.b.insts[c.a.pc] = .bl sym args res
    · obtain ⟨sym, args, res, hinst⟩ := call
      obtain ⟨s', t', hs, ht, rel'⟩ := advance_call checked safe reach c hi sym args res hinst
      exact .inr ⟨s', t', hs, ht, reach.trans (Core.Transition.reaches_of_positive hs), rel'⟩
    · have notCall : ∀ sym args res, c.b.insts[c.a.pc] ≠ .bl sym args res := by
        intro sym args res e; exact call ⟨sym, args, res, e⟩
      obtain ⟨s', t', hs, ht, rel'⟩ := advance_body checked safe reach c hi notCall
      exact .inr ⟨s', t', hs, ht, reach.trans (Core.Transition.reaches_of_positive hs), rel'⟩
  · have hpc : c.a.pc = c.b.insts.length := by have := c.pc; omega
    cases hterm : c.b.term with
    | b k => exact advance_branch checked safe reach c hpc (.inl ⟨k, hterm⟩)
    | bCond fl cond tk un =>
      exact advance_branch checked safe reach c hpc (.inr (.inl ⟨fl, cond, tk, un, hterm⟩))
    | cb nz src tk un =>
      exact advance_branch checked safe reach c hpc (.inr (.inr (.inl ⟨nz, src, tk, un, hterm⟩)))
    | tb nz src bit tk un =>
      exact advance_branch checked safe reach c hpc
        (.inr (.inr (.inr ⟨nz, src, bit, tk, un, hterm⟩)))
    | ret locs => exact advance_ret checked safe rv reach c hpc locs hterm
    | brk imm => exact advance_brk checked safe reach c hpc imm hterm

end EvoIR.Asm.Proof
