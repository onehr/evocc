import EvoIR.Asm.Relation

/-!
Code positions in a checked assembly body. The body is the prologue followed
by the lines of each block in layout order. The lemmas locate the template of
each instruction and terminator, the label of each block, and the label that
follows a block. They use only the definitions of the checker.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

/-- Labels in a list of lines. -/
def labels (lines : List Line) : List Nat := lines.filterMap fun
  | .label k => some k
  | .inst _ => none

@[simp] theorem labels_append (a b : List Line) : labels (a ++ b) = labels a ++ labels b := by
  simp [labels, List.filterMap_append]

@[simp] theorem labels_insts (insts : List Inst) : labels (insts.map .inst) = [] := by
  induction insts with
  | nil => rfl
  | cons i rest ih => simp [labels] at ih ⊢

theorem restLines_labels (h : FrameHint) (next : Option Nat) (b : Alloc.Block) (i : Nat)
    (code : List Line) (hc : restLines h next b i = some code) : labels code = [] := by
  simp only [restLines, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
    Option.some.injEq] at hc
  obtain ⟨_, _, _, _, rfl⟩ := hc
  exact labels_insts _

theorem blockLines_eq (h : FrameHint) (next : Option Nat) (b : Alloc.Block) (lines : List Line)
    (hl : blockLines h next b = some lines) :
    ∃ code, restLines h next b 0 = some code ∧ lines = .label b.id :: code := by
  simp only [blockLines, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
    Option.some.injEq] at hl
  obtain ⟨insts, hi, term, ht, rfl⟩ := hl
  refine ⟨_, ?_, rfl⟩
  simp [restLines, hi, ht]

theorem blocksLines_cons (h : FrameHint) (x : Alloc.Block) (xs : List Alloc.Block)
    (lines : List Line) (hl : blocksLines h (x :: xs) = some lines) :
    ∃ here tail, blockLines h (xs.head?.map (·.id)) x = some here ∧
      blocksLines h xs = some tail ∧ lines = here ++ tail := by
  simp only [blocksLines, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
    Option.some.injEq] at hl
  obtain ⟨here, hx, tail, ht, rfl⟩ := hl
  exact ⟨here, tail, hx, ht, rfl⟩

/-- The lines of the blocks split at one block. -/
theorem blocksLines_split (h : FrameHint) :
    ∀ (pre : List Alloc.Block) (b : Alloc.Block) (post : List Alloc.Block) (lines : List Line),
      blocksLines h (pre ++ b :: post) = some lines →
      ∃ A code C, lines = A ++ (.label b.id :: code) ++ C ∧
        restLines h (post.head?.map (·.id)) b 0 = some code ∧
        blocksLines h post = some C ∧ labels A = pre.map (·.id)
  | [], b, post, lines, hl => by
    obtain ⟨here, tail, hb, hp, rfl⟩ := blocksLines_cons h b post lines hl
    obtain ⟨code, hc, rfl⟩ := blockLines_eq h _ b here hb
    exact ⟨[], code, tail, by simp, hc, hp, rfl⟩
  | x :: pre, b, post, lines, hl => by
    obtain ⟨here, tail, hx, hr, rfl⟩ := blocksLines_cons h x (pre ++ b :: post) lines hl
    obtain ⟨A, code, C, htail, hc, hC, hA⟩ := blocksLines_split h pre b post tail hr
    obtain ⟨xcode, hxc, rfl⟩ := blockLines_eq h _ x here hx
    refine ⟨.label x.id :: xcode ++ A, code, C, ?_, hc, hC, ?_⟩
    · rw [htail]; simp
    · have hx0 := restLines_labels h _ x 0 xcode hxc
      have : labels (.label x.id :: xcode ++ A) = x.id :: (labels xcode ++ labels A) := by
        simp [labels]
      rw [this, hx0, hA]; rfl

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

theorem findIdx_label (xs ys : List Line) (k : Nat) (absent : k ∉ labels xs) :
    (xs ++ .label k :: ys).findIdx? (· == .label k) = some xs.length := by
  induction xs with
  | nil => simp [List.findIdx?_cons]
  | cons x xs ih =>
    have hx : (x == .label k) = false := by
      cases x with
      | label j =>
        have : j ≠ k := by intro e; subst e; simp [labels] at absent
        simp [this]
      | inst _ => rfl
    have rest : k ∉ labels xs := by
      intro m; apply absent; cases x <;> simp [labels] at m ⊢ <;> simp_all
    rw [List.cons_append, List.findIdx?_cons, hx, ih rest]
    simp

theorem labelIndex_split (P A rest : List Line) (k : Nat) (hP : labels P = [])
    (hA : k ∉ labels A) :
    labelIndex (P ++ (A ++ .label k :: rest)) k = some (P.length + A.length) := by
  unfold labelIndex
  rw [← List.append_assoc]
  have absent : k ∉ labels (P ++ A) := by simp [hP, hA]
  rw [findIdx_label (P ++ A) rest k absent]
  simp

end EvoIR.Asm.Proof

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

theorem keysUnique_append (xs ys : List Nat) (k : Nat) (h : keysUnique (xs ++ k :: ys) = true) :
    k ∉ xs := by
  induction xs with
  | nil => simp
  | cons x xs ih =>
    simp only [List.cons_append, keysUnique, Bool.and_eq_true, Bool.not_eq_true',
      decide_eq_false_iff_not] at h
    intro m
    rcases List.mem_cons.mp m with rfl | m
    · exact h.1 (by simp)
    · exact ih h.2 m

theorem drop_prefix {α : Type} (xs ys : List α) (n : Nat) :
    (xs ++ ys).drop (xs.length + n) = ys.drop n := by
  induction xs with
  | nil => simp
  | cons x xs ih => simpa [Nat.succ_add] using ih

theorem prologue_labels (h : FrameHint) : labels ((prologue h).map .inst) = [] := labels_insts _

/-- The checked body is the prologue followed by the lines of the blocks. -/
theorem body_lines (h : FrameHint) (f : Alloc.Function) (body : List Line)
    (he : expectedBody h f = some body) :
    ∃ lines, blocksLines h f.blocks = some lines ∧ body = (prologue h).map .inst ++ lines := by
  simp only [expectedBody, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
    Option.some.injEq] at he
  obtain ⟨lines, hl, rfl⟩ := he
  exact ⟨lines, hl, rfl⟩

/-- Each block has its label in the body, and its code follows the label. -/
theorem At_block (h : FrameHint) (f : Alloc.Function) (body : List Line)
    (he : expectedBody h f = some body) (unique : keysUnique (f.blocks.map (·.id)) = true)
    (b : Alloc.Block) (mem : b ∈ f.blocks) :
    ∃ L, labelIndex body b.id = some L ∧ body[L]? = some (.label b.id) ∧ At h f body b 0 (L + 1) := by
  obtain ⟨lines, hl, rfl⟩ := body_lines h f body he
  obtain ⟨pre, post, split⟩ := List.append_of_mem mem
  rw [split] at hl
  obtain ⟨A, code, C, rfl, hc, hC, hA⟩ := blocksLines_split h pre b post lines hl
  have absent : b.id ∉ labels A := by
    rw [hA]
    apply keysUnique_append _ (post.map (·.id))
    simpa [split] using unique
  refine ⟨(prologue h).length + A.length, ?_, ?_, ?_⟩
  · have := labelIndex_split ((prologue h).map .inst) A (code ++ C) b.id (prologue_labels h) absent
    simpa [List.append_assoc] using this
  · simp [List.append_assoc]
  · refine ⟨pre, post, code, C, split, hc, hC, ?_⟩
    have := drop_prefix ((prologue h).map .inst ++ A) (Line.label b.id :: (code ++ C)) 1
    simpa [List.append_assoc, Nat.add_assoc] using this

/-- The template of instruction `i` starts at the position of `i`, and the
code of instruction `i+1` follows it. -/
theorem At_inst (h : FrameHint) (f : Alloc.Function) (body : List Line) (b : Alloc.Block)
    (i pos : Nat) (at_ : At h f body b i pos) (hi : i < b.insts.length) :
    ∃ tmpl, instTemplate h b.insts[i] = some tmpl ∧
      body.drop pos = tmpl.map .inst ++ body.drop (pos + tmpl.length) ∧
      At h f body b (i+1) (pos + tmpl.length) := by
  obtain ⟨pre, post, code, tail, split, hc, ht, hd⟩ := at_
  simp only [restLines, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
    Option.some.injEq] at hc
  obtain ⟨insts, hi', term, hterm, rfl⟩ := hc
  rw [List.drop_eq_getElem_cons hi] at hi'
  simp only [List.mapM_cons, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def,
    Option.some.injEq] at hi'
  obtain ⟨tmpl, htmpl, rest, hrest, rfl⟩ := hi'
  have hdrop : body.drop (pos + tmpl.length) = (rest.flatten ++ term).map .inst ++ tail := by
    rw [← List.drop_drop, hd]
    simp [List.map_append, List.append_assoc]
  refine ⟨tmpl, htmpl, ?_, pre, post, _, tail, split, ?_, ht, hdrop⟩
  · rw [hdrop, hd]; simp [List.map_append, List.append_assoc]
  · simp [restLines, hrest, hterm]

/-- At the end of the instructions, the terminator template starts, and the
lines of the following blocks follow it. -/
theorem At_term (h : FrameHint) (f : Alloc.Function) (body : List Line) (b : Alloc.Block)
    (i pos : Nat) (at_ : At h f body b i pos) (hi : b.insts.length ≤ i) :
    ∃ pre post tmpl tail, f.blocks = pre ++ b :: post ∧
      termTemplate h (post.head?.map (·.id)) b.term = some tmpl ∧
      blocksLines h post = some tail ∧ body.drop pos = tmpl.map .inst ++ tail := by
  obtain ⟨pre, post, code, tail, split, hc, ht, hd⟩ := at_
  simp only [restLines, List.drop_eq_nil_of_le hi, List.mapM_nil, Option.bind_eq_bind,
    Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at hc
  obtain ⟨_, rfl, term, hterm, rfl⟩ := hc
  exact ⟨pre, post, term, tail, split, hterm, ht, by simpa using hd⟩

/-- After the code of a block comes the label of the next block and the code
of that block. -/
theorem At_next (h : FrameHint) (f : Alloc.Function) (body : List Line) (pre : List Alloc.Block)
    (b bk : Alloc.Block) (post : List Alloc.Block) (tail : List Line) (q : Nat)
    (split : f.blocks = pre ++ b :: bk :: post) (ht : blocksLines h (bk :: post) = some tail)
    (hd : body.drop q = tail) :
    body[q]? = some (.label bk.id) ∧ At h f body bk 0 (q + 1) := by
  obtain ⟨here, rest, hb, hr, rfl⟩ := blocksLines_cons h bk post tail ht
  obtain ⟨code, hc, rfl⟩ := blockLines_eq h _ bk here hb
  constructor
  · have := congrArg (·[0]?) hd
    simpa using this
  · refine ⟨pre ++ [b], post, code, rest, by simp [split], hc, hr, ?_⟩
    rw [← List.drop_drop, hd]; simp

end EvoIR.Asm.Proof
