import EvoIR.Core.ConstantBranch

/-! Finite decision diagrams between cyclic cutpoints. Payloads are symbolic
states, not claims about the next iteration. Guards stay ordered and failed
paths never reach a payload or execute its continuation. -/
namespace EvoIR.Core.Symbolic

inductive CutTree (Γ : Var → Nat) (α : Type) where
  | leaf (value : α)
  | checked (guard : Guard Γ) (next : CutTree Γ α)
  | branch (condition : Term Γ 1) (yes no : CutTree Γ α)

def CutTree.eval {Γ : Var → Nat} {α : Type} (σ : Subst Γ) : CutTree Γ α → Except Source.Fault α
  | .leaf value => .ok value
  | .checked guard next => do guards σ [guard]; next.eval σ
  | .branch condition yes no =>
      if (denoteTerm σ condition).toNat != 0 then yes.eval σ else no.eval σ

def CutTree.prefix {Γ : Var → Nat} {α : Type}
    (checks : List (Guard Γ)) (tree : CutTree Γ α) : CutTree Γ α :=
  checks.foldr .checked tree

theorem CutTree.prefix_sound {Γ : Var → Nat} {α : Type} (σ : Subst Γ)
    (checks : List (Guard Γ)) (tree : CutTree Γ α) :
    (tree.prefix checks).eval σ = (do guards σ checks; tree.eval σ) := by
  induction checks with
  | nil => rfl
  | cons g gs ih =>
    simp only [CutTree.prefix, List.foldr_cons, eval, guards]
    split
    · rfl
    · exact ih

def CutTree.normalizeBranches {Γ : Var → Nat} {α : Type} : CutTree Γ α → CutTree Γ α
  | .leaf value => .leaf value
  | .checked guard next => .checked guard next.normalizeBranches
  | .branch condition yes no => match constantBranch condition with
    | some true => yes.normalizeBranches
    | some false => no.normalizeBranches
    | none => .branch condition yes.normalizeBranches no.normalizeBranches

theorem CutTree.normalize_branches_sound {Γ : Var → Nat} {α : Type}
    (σ : Subst Γ) (tree : CutTree Γ α) : tree.normalizeBranches.eval σ = tree.eval σ := by
  induction tree with
  | leaf value => rfl
  | checked guard next ih => simp only [normalizeBranches, eval, ih]
  | branch condition yes no ih ih' =>
    simp only [normalizeBranches]
    cases h : constantBranch condition with
    | none => simp only [eval, ih, ih']
    | some value =>
      have same := constant_branch_sound σ condition value h
      cases value <;> simp [eval, same, ih, ih']

/-- Repeated intrinsic traps and explicit obligations agree within one span.
No passed-check facts are transported to a successor cutpoint. -/
def CutTree.normalizeChecks {Γ : Var → Nat} {α : Type} (seen : List (Guard Γ)) :
    CutTree Γ α → CutTree Γ α
  | .leaf value => .leaf value
  | .checked guard next =>
    if guard ∈ seen then next.normalizeChecks seen
    else .checked guard (next.normalizeChecks (guard :: seen))
  | .branch condition yes no =>
    .branch condition (yes.normalizeChecks seen) (no.normalizeChecks seen)

theorem CutTree.normalize_checks_sound {Γ : Var → Nat} {α : Type} (σ : Subst Γ)
    (tree : CutTree Γ α) (seen : List (Guard Γ))
    (passed : ∀ g ∈ seen, (denoteTerm σ g.condition).toNat ≠ 0) :
    (tree.normalizeChecks seen).eval σ = tree.eval σ := by
  induction tree generalizing seen with
  | leaf value => rfl
  | checked guard next ih =>
    simp only [normalizeChecks]
    split
    · rename_i member
      rw [ih seen passed]
      simp [eval, guards, passed guard member, bind, Except.bind]
    · simp only [eval, guards]
      by_cases zero : (denoteTerm σ guard.condition).toNat = 0
      · simp [zero, bind, Except.bind]
      · simp only [beq_iff_eq, zero, ↓reduceIte, bind, Except.bind]
        exact ih (guard :: seen) (by
          intro g member
          rcases List.mem_cons.mp member with same | old
          · subst g; exact zero
          · exact passed g old)
  | branch condition yes no first second =>
    simp only [normalizeChecks, eval]
    split
    · exact first seen passed
    · exact second seen passed

def CutTree.normalizeTrace {Γ : Var → Nat} {α : Type} (tree : CutTree Γ α) : CutTree Γ α :=
  (tree.normalizeChecks []).normalizeBranches

theorem CutTree.normalize_trace_sound {Γ : Var → Nat} {α : Type}
    (σ : Subst Γ) (tree : CutTree Γ α) : tree.normalizeTrace.eval σ = tree.eval σ := by
  rw [normalizeTrace, normalize_branches_sound, normalize_checks_sound σ tree [] (by simp)]

/-- The checker must establish the leaf relation independently. Matching a
branch shape or observing the same diagnostic on a few inputs is insufficient. -/
inductive CutMatch {Γ : Var → Nat} {α β : Type} (related : α → β → Prop) :
    CutTree Γ α → CutTree Γ β → Prop where
  | leaf {a b} (relation : related a b) : CutMatch related (.leaf a) (.leaf b)
  | checked {guard a b} (tail : CutMatch related a b) :
      CutMatch related (.checked guard a) (.checked guard b)
  | branch {condition yes no yes' no'} (first : CutMatch related yes yes')
      (second : CutMatch related no no') :
      CutMatch related (.branch condition yes no) (.branch condition yes' no')

theorem CutMatch.sound {Γ : Var → Nat} {α β : Type} {related : α → β → Prop}
    {a : CutTree Γ α} {b : CutTree Γ β} (matched : CutMatch related a b) (σ : Subst Γ) :
    (∃ fault, a.eval σ = .error fault ∧ b.eval σ = .error fault) ∨
    (∃ av bv, a.eval σ = .ok av ∧ b.eval σ = .ok bv ∧ related av bv) := by
  induction matched with
  | leaf relation => exact .inr ⟨_, _, rfl, rfl, relation⟩
  | checked tail ih =>
    simp only [CutTree.eval]
    cases guards σ [_] with
    | error fault => exact .inl ⟨_, rfl, rfl⟩
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [CutTree.eval]
    split <;> assumption

def cutMatch {Γ : Var → Nat} {α β : Type} {related : α → β → Prop}
    (check : ∀ a b, Option (PLift (related a b))) :
    (a : CutTree Γ α) → (b : CutTree Γ β) → Option (PLift (CutMatch related a b))
  | .leaf a, .leaf b => do return ⟨.leaf (← check a b).down⟩
  | .checked ga a, .checked gb b =>
      if same : ga = gb then do
        let tail ← cutMatch check a b
        return ⟨same ▸ CutMatch.checked tail.down⟩
      else none
  | .branch ca yes no, .branch cb yes' no' =>
      if same : ca = cb then do
        let first ← cutMatch check yes yes'
        let second ← cutMatch check no no'
        return ⟨same ▸ CutMatch.branch first.down second.down⟩
      else none
  | _, _ => none

/-- Checked substitution at successful leaves only. A failing guard never
executes the substituted continuation. -/
inductive CutBind {Γ : Var → Nat} {α β : Type}
    (next : α → CutTree Γ β → Prop) : CutTree Γ α → CutTree Γ β → Prop where
  | leaf {a out} : next a out → CutBind next (.leaf a) out
  | checked {guard a b} : CutBind next a b → CutBind next (.checked guard a) (.checked guard b)
  | branch {condition a b a' b'} : CutBind next a a' → CutBind next b b' →
      CutBind next (.branch condition a b) (.branch condition a' b')

def CutTree.bindChecked {Γ : Var → Nat} {α β : Type}
    {next : α → CutTree Γ β → Prop} (build : ∀ a, Option {out // next a out}) :
    (tree : CutTree Γ α) → Option {out // CutBind next tree out}
  | .leaf a => do
    let out ← build a
    return ⟨out.val, .leaf out.property⟩
  | .checked guard tail => do
    let out ← tail.bindChecked build
    return ⟨.checked guard out.val, .checked out.property⟩
  | .branch condition a b => do
    let first ← a.bindChecked build
    let second ← b.bindChecked build
    return ⟨.branch condition first.val second.val, .branch first.property second.property⟩

theorem CutBind.outcome {Γ : Var → Nat} {α β : Type} {next : α → CutTree Γ β → Prop}
    {before : CutTree Γ α} {after : CutTree Γ β} (bound : CutBind next before after)
    (σ : Subst Γ) :
    (∃ fault, before.eval σ = .error fault ∧ after.eval σ = .error fault) ∨
    (∃ a out, before.eval σ = .ok a ∧ next a out ∧ after.eval σ = out.eval σ) := by
  induction bound with
  | leaf continued => exact .inr ⟨_, _, rfl, continued, rfl⟩
  | checked _ ih =>
    simp only [CutTree.eval]
    cases guards σ [_] with
    | error fault => exact .inl ⟨fault, rfl, rfl⟩
    | ok u => simpa only [bind, Except.bind] using ih
  | branch _ _ first second =>
    simp only [CutTree.eval]
    split <;> assumption

end EvoIR.Core.Symbolic
