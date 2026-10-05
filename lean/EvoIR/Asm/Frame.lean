import EvoIR.Asm.AllocFacts

/-!
Facts of a checked frame layout and the congruence of the relation under
memory and register changes. The layout facts come from `frameOk`: each area
is inside the frame and the areas do not overlap.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

/-! Disjoint regions. -/

def Disjoint (x y : Nat × Nat) : Prop := x.1 + x.2 ≤ y.1 ∨ y.1 + y.2 ≤ x.1

theorem regionsDisjoint_pair : ∀ (A B C : List (Nat × Nat)) (x y : Nat × Nat),
    regionsDisjoint (A ++ x :: (B ++ y :: C)) = true → Disjoint x y
  | [], B, C, x, y, h => by
    simp only [List.nil_append, regionsDisjoint, Bool.and_eq_true, List.all_eq_true] at h
    have := h.1 y (by simp)
    simp only [Bool.or_eq_true, decide_eq_true_eq] at this
    exact this
  | a :: A, B, C, x, y, h => by
    simp only [List.cons_append, regionsDisjoint, Bool.and_eq_true] at h
    exact regionsDisjoint_pair A B C x y h.2

theorem regionsDisjoint_mem (l : List (Nat × Nat)) (h : regionsDisjoint l = true)
    (xs ys : List (Nat × Nat)) (split : l = xs ++ ys) (x y : Nat × Nat) (hx : x ∈ xs) (hy : y ∈ ys) :
    Disjoint x y := by
  obtain ⟨A, B, rfl⟩ := List.append_of_mem hx
  obtain ⟨C, D, rfl⟩ := List.append_of_mem hy
  subst split
  exact regionsDisjoint_pair A (B ++ C) D x y (by simpa [List.append_assoc] using h)

/-- Two entries at different positions of a key-unique list are disjoint. -/
theorem regionsDisjoint_sub : ∀ (l : List (Nat × Nat)), regionsDisjoint l = true →
    ∀ (xs : List (Nat × Nat)), xs.Sublist l → regionsDisjoint xs = true
  | [], _, xs, sub => by rw [List.sublist_nil.mp sub]; rfl
  | a :: l, h, xs, sub => by
    simp only [regionsDisjoint, Bool.and_eq_true, List.all_eq_true] at h
    cases sub with
    | cons _ sub => exact regionsDisjoint_sub l h.2 xs sub
    | cons_cons _ sub =>
      simp only [regionsDisjoint, Bool.and_eq_true, List.all_eq_true]
      exact ⟨fun y m => h.1 y (sub.subset m), regionsDisjoint_sub l h.2 _ sub⟩

theorem disjoint_symm {x y : Nat × Nat} (h : Disjoint x y) : Disjoint y x := by
  unfold Disjoint at *; omega

theorem regionsDisjoint_pairwise : ∀ (l : List (Nat × Nat)), regionsDisjoint l = true →
    l.Pairwise Disjoint
  | [], _ => .nil
  | a :: l, h => by
    simp only [regionsDisjoint, Bool.and_eq_true, List.all_eq_true] at h
    refine List.Pairwise.cons (fun y m => ?_) (regionsDisjoint_pairwise l h.2)
    have := h.1 y m
    simp only [Bool.or_eq_true, decide_eq_true_eq] at this
    exact this

theorem pairwise_mem {α : Type} {R : α → α → Prop} (symm : ∀ a b, R a b → R b a) :
    ∀ {l : List α}, l.Pairwise R → ∀ {x y}, x ∈ l → y ∈ l → x ≠ y → R x y
  | [], _, _, _, hx, _, _ => by cases hx
  | a :: l, h, x, y, hx, hy, ne => by
    rcases List.pairwise_cons.mp h with ⟨ha, hl⟩
    cases hx with
    | head =>
      cases hy with
      | head => exact absurd rfl ne
      | tail _ hy => exact ha y hy
    | tail _ hx =>
      cases hy with
      | head => exact symm _ _ (ha x hx)
      | tail _ hy => exact pairwise_mem symm hl hx hy ne

theorem lookupOffset_mem (entries : List (Nat × Nat)) (k off : Nat)
    (h : lookupOffset entries k = some off) : (k, off) ∈ entries := by
  induction entries with
  | nil => simp [lookupOffset] at h
  | cons e rest ih =>
    obtain ⟨key, value⟩ := e
    simp only [lookupOffset, List.lookup] at h ih
    by_cases eq : k = key
    · subst eq
      simp at h
      simp [h]
    · have : (k == key) = false := by simpa using eq
      rw [this] at h
      exact List.mem_cons_of_mem _ (ih h)

/-- The layout facts of a checked frame hint. -/
structure LayoutOk (h : FrameHint) : Prop where
  size16 : h.size % 16 = 0
  outgoing : h.outgoing ≤ h.size
  spill : ∀ k a, (k, a) ∈ h.spills → a + 8 ≤ h.size ∧ h.outgoing ≤ a
  save : ∀ r a, (r, a) ∈ h.saved → a + 8 ≤ h.size ∧ h.outgoing ≤ a ∧ isCalleeSaved r = true
  spillSpill : ∀ k a k' b, (k, a) ∈ h.spills → (k', b) ∈ h.spills → k ≠ k' → Disjoint (a, 8) (b, 8)
  saveSave : ∀ r a r' b, (r, a) ∈ h.saved → (r', b) ∈ h.saved → r ≠ r' → Disjoint (a, 8) (b, 8)
  spillSave : ∀ k a r b, (k, a) ∈ h.spills → (r, b) ∈ h.saved → Disjoint (a, 8) (b, 8)

theorem layoutOk_of_frameOk (f : Alloc.Function) (h : FrameHint) (ok : frameOk f h = true) :
    LayoutOk h := by
  simp only [frameOk, Bool.and_eq_true, decide_eq_true_eq, List.all_eq_true] at ok
  obtain ⟨⟨⟨⟨⟨⟨⟨⟨⟨⟨⟨⟨size16, _⟩, inside⟩, disjoint⟩, _⟩, _⟩, _⟩, _⟩, savedCallee⟩, _⟩, _⟩, _⟩, _⟩ := ok
  have pw := regionsDisjoint_pairwise _ disjoint
  have inside' : ∀ x ∈ frameRegions f h, x.1 + x.2 ≤ h.size := by
    intro x m
    have := inside x m
    exact this.2
  let O : List (Nat × Nat) := if h.outgoing = 0 then [] else [(0, h.outgoing)]
  let S := h.spills.map (fun p : Nat × Nat => (p.2, 8))
  let J := (frameObjects f).filterMap (fun (origin, size) =>
      (lookupOffset h.slots origin).map (fun off => (off, size)))
  let V := h.saved.map (fun p : Nat × Nat => (p.2, 8))
  have split : frameRegions f h = ((O ++ S) ++ J) ++ V := rfl
  rw [split] at pw inside'
  simp only [List.pairwise_append, List.mem_append] at pw
  obtain ⟨⟨⟨_, spillsPw, outSpill⟩, _, _⟩, savedPw, cross⟩ := pw
  have outBelow : ∀ x, x ∈ S ∨ x ∈ V → h.outgoing ≤ x.1 := by
    intro x m
    by_cases zero : h.outgoing = 0
    · omega
    · have hO : (0, h.outgoing) ∈ O := by simp [O, zero]
      rcases m with m | m
      · have := outSpill _ hO x m
        obtain ⟨p, _, rfl⟩ := List.mem_map.mp m
        unfold Disjoint at this; simp at this; omega
      · have := cross _ (Or.inl (Or.inl hO)) x m
        obtain ⟨p, _, rfl⟩ := List.mem_map.mp m
        unfold Disjoint at this; simp at this; omega
  have spillPair : ∀ k a, (k, a) ∈ h.spills → ((a, 8) : Nat × Nat) ∈ S := by
    intro k a m; exact List.mem_map.mpr ⟨(k, a), m, rfl⟩
  have savePair : ∀ r a, (r, a) ∈ h.saved → ((a, 8) : Nat × Nat) ∈ V := by
    intro r a m; exact List.mem_map.mpr ⟨(r, a), m, rfl⟩
  refine ⟨size16, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · by_cases zero : h.outgoing = 0
    · omega
    · have := inside' (0, h.outgoing) (List.mem_append_left _ (List.mem_append_left _
        (List.mem_append_left _ (by simp [O, zero]))))
      simpa using this
  · intro k a m
    exact ⟨inside' (a, 8) (List.mem_append_left _ (List.mem_append_left _
      (List.mem_append_right _ (spillPair k a m)))), outBelow _ (.inl (spillPair k a m))⟩
  · intro r a m
    refine ⟨inside' (a, 8) (List.mem_append_right _ (savePair r a m)),
      outBelow _ (.inr (savePair r a m)),
      savedCallee (r, a) m⟩
  · intro k a k' b ma mb ne
    have spw := List.pairwise_map.mp spillsPw
    exact pairwise_mem (R := fun x y : Nat × Nat => Disjoint (x.2, 8) (y.2, 8))
      (fun _ _ d => disjoint_symm d) spw ma mb (by intro e; cases e; exact ne rfl)
  · intro r a r' b ma mb ne
    have spw := List.pairwise_map.mp savedPw
    exact pairwise_mem (R := fun x y : Nat × Nat => Disjoint (x.2, 8) (y.2, 8))
      (fun _ _ d => disjoint_symm d) spw ma mb (by intro e; cases e; exact ne rfl)
  · intro k a r b ma mb
    exact cross _ (Or.inl (Or.inr (spillPair k a ma))) _ (savePair r b mb)

end EvoIR.Asm.Proof
