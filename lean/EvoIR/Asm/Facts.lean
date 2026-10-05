import EvoIR.Asm.Slots

/-!
The facts that `checkAsm` gives for one function, in the form the
simulation uses.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

/-- The checked facts of the function at index `i`. -/
structure FunOk (ctx : Ctx) (i : Nat) (f : Alloc.Function) (h : FrameHint) (g : Asm.Function) :
    Prop where
  at_ : FunAt ctx i f h g
  name : g.name = f.name
  body : expectedBody h f = some g.body
  frame : frameOk f h = true
  layout : LayoutOk h
  regs : registersOk f = true
  entry : entryOk f = true
  stack : stackOk ctx.p ctx.hints i f h = true
  need : frameBytes h ≤ needOf ctx.p ctx.hints f.name
  span : g.body.length < functionSpan
  index : i < 2^30
  count : ctx.p.functions.length = ctx.asm.functions.length

theorem funOk_of_check (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true)
    (i : Nat) (f : Alloc.Function) (hf : ctx.p.functions[i]? = some f) :
    ∃ h g, FunOk ctx i f h g := by
  simp only [checkAsm, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq, List.all_eq_true,
    List.mem_range] at checked
  obtain ⟨⟨⟨⟨_, count⟩, _⟩, small⟩, all⟩ := checked
  have hi : i < ctx.p.functions.length := by
    rcases Nat.lt_or_ge i ctx.p.functions.length with h | h
    · exact h
    · rw [List.getElem?_eq_none h] at hf; cases hf
  have := all i hi
  rw [hf] at this
  split at this
  · rename_i f' h g hf' hh hg
    cases hf'
    simp only [checkFunction, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at this
    obtain ⟨⟨⟨⟨⟨⟨⟨⟨name, body⟩, frame⟩, regs⟩, _⟩, entry⟩, stack⟩, need⟩, span⟩ := this
    exact ⟨h, g, ⟨⟨hf, hh, hg⟩, name, by simpa using body, frame, layoutOk_of_frameOk f h frame,
      regs, entry, stack, need, span, by omega, count⟩⟩
  · cases this

theorem keysUnique_nodup : ∀ (l : List Nat), keysUnique l = true → l.Nodup
  | [], _ => List.nodup_nil
  | k :: rest, h => by
    simp only [keysUnique, Bool.and_eq_true, Bool.not_eq_true', decide_eq_false_iff_not] at h
    exact List.nodup_cons.mpr ⟨h.1, keysUnique_nodup rest h.2⟩

theorem namesUnique_nodup : ∀ (l : List String), namesUnique.keysUnique' l = true → l.Nodup
  | [], _ => List.nodup_nil
  | k :: rest, h => by
    simp only [namesUnique.keysUnique', Bool.and_eq_true, Bool.not_eq_true',
      decide_eq_false_iff_not] at h
    exact List.nodup_cons.mpr ⟨h.1, namesUnique_nodup rest h.2⟩

theorem find_unique {α β : Type} [BEq β] [LawfulBEq β] (k : α → β) :
    ∀ (l : List α), (l.map k).Nodup → ∀ x, x ∈ l → l.find? (fun y => k y == k x) = some x
  | [], _, _, m => by cases m
  | y :: rest, nd, x, m => by
    rcases List.nodup_cons.mp nd with ⟨notIn, nd'⟩
    rcases List.mem_cons.mp m with rfl | m
    · simp
    · have ne : k y ≠ k x := by
        intro e; apply notIn; rw [e]; exact List.mem_map.mpr ⟨x, m, rfl⟩
      rw [List.find?_cons, beq_false_of_ne ne]
      exact find_unique k rest nd' x m

theorem names_find (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true) (i : Nat)
    (f : Alloc.Function) (hf : ctx.p.functions[i]? = some f) :
    ctx.p.functions.find? (·.name == f.name) = some f := by
  simp only [checkAsm, Bool.and_eq_true] at checked
  have nd := namesUnique_nodup _ checked.1.1.1.1
  exact find_unique (·.name) _ nd f (List.mem_of_getElem? hf)

theorem blocks_find (f : Alloc.Function) (unique : keysUnique (f.blocks.map (·.id)) = true)
    (b : Alloc.Block) (m : b ∈ f.blocks) : f.blocks.find? (·.id == b.id) = some b :=
  find_unique (·.id) _ (keysUnique_nodup _ unique) b m

theorem FunOk.unique {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) : keysUnique (f.blocks.map (·.id)) = true := by
  have := ok.entry
  simp only [entryOk, Bool.and_eq_true] at this
  exact this.1.1.1.2

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

theorem written_locs (inst : Instruction) (l : Loc) (m : l ∈ instWritten inst) : l ∈ instLocs inst := by
  cases inst <;> simp only [instWritten, instLocs, List.mem_cons, List.mem_append,
    List.not_mem_nil, or_false] at m ⊢
  all_goals first
    | (subst m; simp)
    | (rcases m with rfl | rfl <;> simp)
    | exact .inr m

theorem lookupOffset_isSome (entries : List (Nat × Nat)) (k : Nat) (m : k ∈ entries.map Prod.fst) :
    (lookupOffset entries k).isSome = true := by
  induction entries with
  | nil => cases m
  | cons e rest ih =>
    obtain ⟨key, value⟩ := e
    simp only [lookupOffset, List.lookup] at ih ⊢
    by_cases eq : k = key
    · subst eq; simp
    · have : (k == key) = false := by simpa using eq
      rw [this]
      apply ih
      simp only [List.map_cons, List.mem_cons] at m
      rcases m with h | h
      · exact absurd h eq
      · exact h

theorem FunOk.writable {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) (cur : Frame) (hh : cur.hint = h) (inst : Instruction)
    (mi : inst ∈ instructionsOf f) (n : Nat) (w : Machine.Width) (mw : Loc.gpr n w ∈ instWritten inst) :
    Writable cur n := by
  have regs := ok.regs
  simp only [registersOk, List.all_eq_true] at regs
  have hloc : Loc.gpr n w ∈ locsOf f := by
    simp only [locsOf, List.mem_append, List.mem_flatMap]
    exact .inl ⟨inst, mi, written_locs inst _ mw⟩
  have alloc := regs _ hloc
  refine ⟨by simpa using alloc, fun cs => ?_⟩
  have frame := ok.frame
  simp only [frameOk, Bool.and_eq_true, List.all_eq_true] at frame
  have hw := frame.1.1.1.2
  rw [hh]
  apply lookupOffset_isSome
  apply of_decide_eq_true
  apply hw
  simp only [calleeSavedWritten, List.mem_flatMap, List.mem_filterMap]
  exact ⟨inst, mi, _, mw, by simp [cs]⟩

theorem FunOk.outgoing {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) (k : Nat) (src : Loc) (mi : Instruction.outgoingArg k src ∈ instructionsOf f) :
    8 * k + 8 ≤ h.outgoing := by
  have frame := ok.frame
  simp only [frameOk, Bool.and_eq_true, List.all_eq_true, decide_eq_true_eq] at frame
  apply frame.2
  simp only [outgoingSlotsUsed, List.mem_filterMap]
  exact ⟨_, mi, rfl⟩

end EvoIR.Asm.Proof
