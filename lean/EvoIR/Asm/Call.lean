import EvoIR.Asm.Prologue

/-!
The call chunk: the allocated `bl` step and the assembly `bl`, the prologue
of the callee and the label of its entry block. The callee frame lies below
the stack pointer of the caller; the outgoing area of the caller is the
incoming area of the callee.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

theorem names_map (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true) :
    ctx.asm.functions.map (·.name) = ctx.p.functions.map (·.name) := by
  apply List.ext_getElem?
  intro i
  simp only [List.getElem?_map]
  cases hf : ctx.p.functions[i]? with
  | none =>
    have : ctx.p.functions.length ≤ i := by
      rcases Nat.lt_or_ge i ctx.p.functions.length with h | h
      · rw [List.getElem?_eq_getElem h] at hf; cases hf
      · exact h
    have count : ctx.p.functions.length = ctx.asm.functions.length := by
      simp only [checkAsm, Bool.and_eq_true, beq_iff_eq] at checked; exact checked.1.1.1.2
    rw [List.getElem?_eq_none (by omega)]; rfl
  | some f =>
    obtain ⟨h, g, ok⟩ := funOk_of_check ctx checked i f hf
    rw [ok.at_.2.2]; simp [ok.name]

/-- The callee of a call: its index in both programs. -/
theorem callee_index (ctx : Ctx) (checked : checkAsm ctx.p ctx.asm ctx.hints = true) (sym : String)
    (callee : Alloc.Function) (hfind : ctx.p.functions.find? (·.name == sym) = some callee) :
    ∃ j, ctx.p.functions[j]? = some callee ∧ functionIndex ctx.asm sym = some j := by
  obtain ⟨hp, i, hi, hget, before⟩ := List.find?_eq_some_iff_getElem.mp hfind
  have hidx : ctx.p.functions.findIdx? (·.name == sym) = some i := by
    rw [List.findIdx?_eq_some_iff_getElem]
    refine ⟨hi, by rw [hget]; exact hp, fun j hj => ?_⟩
    have := before j hj
    simpa using this
  refine ⟨i, by rw [List.getElem?_eq_getElem hi, hget], ?_⟩
  have e1 : functionIndex ctx.asm sym = (ctx.asm.functions.map (·.name)).findIdx? (· == sym) := by
    simp only [functionIndex, List.findIdx?_map]; rfl
  have e2 : ctx.p.functions.findIdx? (·.name == sym) =
      (ctx.p.functions.map (·.name)).findIdx? (· == sym) := by
    simp only [List.findIdx?_map]; rfl
  rw [e1, names_map ctx checked, ← e2, hidx]

theorem callStorage_shape (st : Storage) (count : Nat) (shape : Shape st) (small : count ≤ 8) :
    Shape (callStorage st count) := by
  refine ⟨fun place v d h => ?_, fun n hn => ?_⟩
  · cases place with
    | gpr n =>
      simp only [callStorage] at h
      split at h
      · exact shape.1 _ v d h
      · cases h
    | incoming k => exact shape.1 _ v d h
    | _ => cases h
  · simp only [callStorage]
    have : ¬ n < count := by
      intro lt; simp [allocatable, isCalleeSaved] at hn; omega
    simp [this]

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check
open Core.Transition (Reaches Predecessor Path)

/-- The entry block is the first block; its label follows the prologue. -/
theorem entry_block {ctx : Ctx} {i : Nat} {f : Alloc.Function} {h : FrameHint} {g : Asm.Function}
    (ok : FunOk ctx i f h g) :
    ∃ b0 rest, f.blocks = b0 :: rest ∧ b0.id = 0 ∧ f.blocks.find? (·.id == 0) = some b0 ∧
      g.body.drop 0 = (prologue h).map .inst ++ g.body.drop (prologue h).length ∧
      g.body[(prologue h).length]? = some (.label 0) ∧ At h f g.body b0 0 ((prologue h).length + 1) := by
  have entry := ok.entry
  simp only [entryOk, Bool.and_eq_true] at entry
  obtain ⟨⟨⟨⟨head, unique⟩, _⟩, _⟩, _⟩ := entry
  cases hb : f.blocks with
  | nil => rw [hb] at head; simp at head
  | cons b0 rest =>
    rw [hb] at head
    have id0 : b0.id = 0 := by simpa using head
    obtain ⟨lines, hl, hbody⟩ := body_lines h f g.body ok.body
    rw [hb] at hl
    obtain ⟨here, C, hhere, hC, hlines⟩ := blocksLines_cons h b0 rest lines hl
    obtain ⟨code, hc, rfl⟩ := blockLines_eq h _ b0 here hhere
    refine ⟨b0, rest, rfl, id0, ?_, ?_, ?_, ?_⟩
    · rw [← id0, ← hb]; exact blocks_find f ok.unique b0 (by rw [hb]; simp)
    · rw [hbody, hlines]; simp
    · rw [hbody, hlines, id0]; simp
    · refine ⟨[], rest, code, C, hb, hc, hC, ?_⟩
      rw [hbody, hlines]
      have := drop_prefix ((prologue h).map Line.inst) (Line.label b0.id :: code ++ C) 1
      simpa using this

end EvoIR.Asm.Proof
