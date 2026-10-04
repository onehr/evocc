import EvoIR.Core.SourceSpan

/-! Evaluate the actual call arguments, stopping before callee entry. Neither
callee termination nor a chosen return value is needed to prove this prefix. -/
namespace EvoIR.Core.Symbolic

inductive StateCutBind {Γ : Var → Nat} {α : Type}
    (next : LocalStore Γ → CWord Γ → CutTree Γ α → Prop) :
    StateTree Γ → CutTree Γ α → Prop where
  | value {locals word out} : next locals word out → StateCutBind next (.value locals word) out
  | checked {locals guard a b} : StateCutBind next a b →
      StateCutBind next (.checked locals guard a) (.checked guard b)
  | branch {condition a b a' b'} : StateCutBind next a a' → StateCutBind next b b' →
      StateCutBind next (.branch condition a b) (.branch condition a' b')

def StateTree.bindCut {Γ : Var → Nat} {α : Type}
    {next : LocalStore Γ → CWord Γ → CutTree Γ α → Prop}
    (build : ∀ locals word, Option {out // next locals word out}) :
    (tree : StateTree Γ) → Option {out // StateCutBind next tree out}
  | .value locals word => do
    let out ← build locals word
    return ⟨out.val, .value out.property⟩
  | .checked _ guard tail => do
    let out ← tail.bindCut build
    return ⟨.checked guard out.val, .checked out.property⟩
  | .branch condition a b => do
    let first ← a.bindCut build
    let second ← b.bindCut build
    return ⟨.branch condition first.val second.val, .branch first.property second.property⟩

theorem StateCutBind.outcome {Γ : Var → Nat} {α : Type}
    {next : LocalStore Γ → CWord Γ → CutTree Γ α → Prop}
    {before : StateTree Γ} {after : CutTree Γ α} (bound : StateCutBind next before after)
    (σ : Subst Γ) :
    (∃ fault, (before.denote σ).2 = .error fault ∧ after.eval σ = .error fault) ∨
    (∃ locals word out, before.denote σ = (locals.denote σ, .ok (word.denote σ)) ∧
      next locals word out ∧ after.eval σ = out.eval σ) := by
  induction bound with
  | value continued => exact .inr ⟨_, _, _, rfl, continued, rfl⟩
  | @checked locals guard a b _ ih =>
    simp only [StateTree.denote, CutTree.eval]
    cases guards σ [guard] with
    | error fault => exact .inl ⟨fault, rfl, rfl⟩
    | ok u => exact ih
  | branch _ _ first second =>
    simp only [StateTree.denote, CutTree.eval]
    split <;> assumption

abbrev ArgumentExit (Γ : Var → Nat) := LocalStore Γ × List (CWord Γ)

def argumentOutcome {Γ : Var → Nat} (σ : Subst Γ)
    (tree : CutTree Γ (ArgumentExit Γ)) : Source.Locals × Except Source.Fault (List Source.Value) :=
  match tree.eval σ with
  | .error fault => ([], .error fault)
  | .ok (locals, words) => (locals.denote σ, .ok (words.map (CWord.denote σ)))

def ArgumentFrontierRuns {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (callee : Nat) (pending : List (Source.QualifiedType × Source.Expr))
    (reverseWords : List (CWord Γ)) (tree : CutTree Γ (ArgumentExit Γ)) : Prop :=
  ∀ σ fid, Source.ArgumentsRun p (locals.denote σ) fid type callee pending
    (reverseWords.map (CWord.denote σ)) (argumentOutcome σ tree)

theorem argument_frontier_cons {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (callee : Nat) (destination : Source.QualifiedType) (expr : Source.Expr)
    (pending : List (Source.QualifiedType × Source.Expr)) (reverseWords : List (CWord Γ))
    (head : StateTree Γ) (tail : CutTree Γ (ArgumentExit Γ))
    (first : ContextDiagram p locals expr head)
    (continued : StateCutBind (fun next word out => ArgumentFrontierRuns p next type callee pending
      (word.cast destination.type :: reverseWords) out) head tail) :
    ArgumentFrontierRuns p locals type callee ((destination, expr) :: pending) reverseWords tail := by
  intro σ fid
  rcases continued.outcome σ with ⟨fault, failed, output⟩ |
    ⟨next, word, out, success, resumed, output⟩
  · have expression := first σ fid
    have failedExpression : Source.EvaluatesContext p (locals.denote σ) fid expr ([], .error fault) := by
      intro k
      simpa only [Source.EvaluatesContext, failed] using expression k
    simpa only [argumentOutcome, output] using Source.arguments_cons_error type callee destination expr
      pending (reverseWords.map (CWord.denote σ)) [] fault failedExpression
  · have expression := first σ fid
    rw [success] at expression
    have rest := resumed σ fid
    have result := Source.arguments_cons_ok type callee destination expr pending
      (reverseWords.map (CWord.denote σ)) (word.denote σ) (argumentOutcome σ out) expression
      (by simpa only [List.map_cons, cast_sound] using rest)
    simpa only [argumentOutcome, output] using result

def argumentFrontier {Γ : Var → Nat} (p : Source.Program) : (budget : Nat) →
    (locals : LocalStore Γ) → (type : Source.IntType) → (callee : Nat) →
    (pending : List (Source.QualifiedType × Source.Expr)) → (reverseWords : List (CWord Γ)) →
    Option {tree // ArgumentFrontierRuns p locals type callee pending reverseWords tree}
  | 0, _, _, _, _, _ => none
  | _+1, locals, type, callee, [], reverseWords => some ⟨.leaf (locals, reverseWords.reverse), by
      intro σ fid
      simpa only [argumentOutcome, CutTree.eval, List.map_reverse] using
        Source.arguments_nil p (locals.denote σ) fid type callee (reverseWords.map (CWord.denote σ))⟩
  | budget+1, locals, type, callee, (destination, expr) :: pending, reverseWords => do
      let head ← callExpression p budget locals expr
      let tail ← head.val.bindCut (next := fun next word tree => ArgumentFrontierRuns p next type callee
        pending (word.cast destination.type :: reverseWords) tree)
        (fun next word => argumentFrontier p budget next type callee pending
          (word.cast destination.type :: reverseWords))
      return ⟨tail.val, argument_frontier_cons p locals type callee destination expr pending
        reverseWords head.val tail.val head.property tail.property⟩

def argumentCallTree {Γ : Var → Nat} (type : Source.IntType) (callee : Nat) (k : List Source.Frame) :
    CutTree Γ (ArgumentExit Γ) → CutTree Γ (SourceCursor Γ)
  | .leaf (locals, words) => .leaf ⟨.call callee type words, locals, k⟩
  | .checked guard tail => .checked guard (argumentCallTree type callee k tail)
  | .branch condition a b => .branch condition (argumentCallTree type callee k a) (argumentCallTree type callee k b)

theorem argument_call_eval {Γ : Var → Nat} (σ : Subst Γ) (type : Source.IntType) (callee : Nat)
    (k : List Source.Frame) (tree : CutTree Γ (ArgumentExit Γ)) :
    (argumentCallTree type callee k tree).eval σ =
      (tree.eval σ).map (fun (locals, words) => ⟨.call callee type words, locals, k⟩) := by
  induction tree with
  | leaf value => cases value; rfl
  | checked guard tail ih =>
    simp only [argumentCallTree, CutTree.eval]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => exact ih
  | branch condition a b ih ih' => simp only [argumentCallTree, CutTree.eval]; split <;> assumption

theorem call_frontier_sound {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (callee : Nat) (arguments : List (Source.QualifiedType × Source.Expr))
    (k : List Source.Frame) (tree : CutTree Γ (ArgumentExit Γ))
    (built : ArgumentFrontierRuns p locals type callee arguments [] tree) :
    SourceCutRuns p true ⟨.expression (.call type callee arguments), locals, k⟩
      (argumentCallTree type callee k tree) := by
  intro σ fid context
  have prefixPath := built σ fid (sourceContextFrames context k)
  let start := (⟨.expression (.call type callee arguments), locals, k⟩ : SourceCursor Γ).denote σ fid context
  have step : Source.Step p start
      (Source.beginArguments ⟨.flow .normal, locals.denote σ, sourceContextFrames context k, fid⟩
        type callee arguments []) := by
    cases context <;> cases arguments <;> rfl
  rw [argument_call_eval]
  cases outcome : tree.eval σ with
  | error fault =>
    simp only [argumentOutcome, outcome] at prefixPath
    obtain ⟨ls, frames, id, path⟩ := prefixPath
    exact ⟨_, .next step path, fun _ => path.toReaches.after_step step, rfl⟩
  | ok exit =>
    rcases exit with ⟨next, words⟩
    simp only [argumentOutcome, outcome, List.map_nil] at prefixPath
    refine ⟨_, .next step prefixPath, fun _ => prefixPath.toReaches.after_step step, ?_⟩
    cases context <;> rfl

end EvoIR.Core.Symbolic
