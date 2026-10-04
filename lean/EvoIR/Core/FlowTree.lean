import EvoIR.Core.StateEntry

/-! Symbolic statement exits. Normal completion, return and loop control are
distinct, and a fault retains its exact store without executing a continuation.
These are finite statement diagrams, not a bounded semantics for loops. -/
namespace EvoIR.Core.Source

abbrev FlowOutcome := Locals × Except Fault Flow

def RunsFlow (p : Program) (locals : Locals) (fid : Nat)
    (control : Control) (outcome : FlowOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok flow => Steps p ⟨control, locals, k, fid⟩
      ⟨.flow flow, outcome.1, k, fid⟩
  | .error fault => ∃ failedK, Steps p ⟨control, locals, k, fid⟩
      ⟨.halted (.error fault), outcome.1, failedK, fid⟩

def ResumesFlow (p : Program) (locals : Locals) (fid : Nat)
    (frame : Frame) (value : Value) (outcome : FlowOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok flow => Steps p ⟨.value value, locals, frame :: k, fid⟩
      ⟨.flow flow, outcome.1, k, fid⟩
  | .error fault => ∃ failedK, Steps p ⟨.value value, locals, frame :: k, fid⟩
      ⟨.halted (.error fault), outcome.1, failedK, fid⟩

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

inductive SymbolicFlow (Γ : Var → Nat) where
  | normal | break | continue
  | returned (word : CWord Γ)

def SymbolicFlow.denote {Γ : Var → Nat} (σ : Subst Γ) : SymbolicFlow Γ → Source.Flow
  | .normal => .normal
  | .break => .break
  | .continue => .continue
  | .returned word => .returned (word.denote σ)

inductive FlowTree (Γ : Var → Nat) where
  | value (locals : LocalStore Γ) (flow : SymbolicFlow Γ)
  | checked (locals : LocalStore Γ) (guard : Guard Γ) (next : FlowTree Γ)
  | branch (condition : Term Γ 1) (yes no : FlowTree Γ)

def FlowTree.denote {Γ : Var → Nat} (σ : Subst Γ) : FlowTree Γ → Source.FlowOutcome
  | .value locals flow => (locals.denote σ, .ok (flow.denote σ))
  | .checked locals guard next => match guards σ [guard] with
    | .error fault => (locals.denote σ, .error fault)
    | .ok _ => next.denote σ
  | .branch condition yes no =>
    if (denoteTerm σ condition).toNat != 0 then yes.denote σ else no.denote σ

def FlowDiagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (control : Source.Control) (tree : FlowTree Γ) : Prop :=
  ∀ σ p fid, Source.RunsFlow p (locals.denote σ) fid control (tree.denote σ)

def FrameDiagram {Γ : Var → Nat} (locals : LocalStore Γ)
    (frame : Source.Frame) (word : CWord Γ) (tree : FlowTree Γ) : Prop :=
  ∀ σ p fid, Source.ResumesFlow p (locals.denote σ) fid frame
    (word.denote σ) (tree.denote σ)

/-- Substitute a statement continuation at successful expression leaves only. -/
inductive ExpressionFlowBind {Γ : Var → Nat}
    (next : LocalStore Γ → CWord Γ → FlowTree Γ → Prop) :
    StateTree Γ → FlowTree Γ → Prop where
  | value {locals word output} (continued : next locals word output) :
      ExpressionFlowBind next (.value locals word) output
  | checked {locals guard before after} (inner : ExpressionFlowBind next before after) :
      ExpressionFlowBind next (.checked locals guard before) (.checked locals guard after)
  | branch {condition yes no yes' no'}
      (first : ExpressionFlowBind next yes yes') (second : ExpressionFlowBind next no no') :
      ExpressionFlowBind next (.branch condition yes no) (.branch condition yes' no')

theorem ExpressionFlowBind.outcome {Γ : Var → Nat}
    {next : LocalStore Γ → CWord Γ → FlowTree Γ → Prop} {before : StateTree Γ} {after : FlowTree Γ}
    (diagram : ExpressionFlowBind next before after) (σ : Subst Γ) :
    (∃ locals fault, before.denote σ = (locals, .error fault) ∧
      after.denote σ = (locals, .error fault)) ∨
    (∃ locals word output, next locals word output ∧
      before.denote σ = (locals.denote σ, .ok (word.denote σ)) ∧
      after.denote σ = output.denote σ) := by
  induction diagram with
  | value continued => exact .inr ⟨_, _, _, continued, rfl, rfl⟩
  | checked inner ih =>
    simp only [StateTree.denote, FlowTree.denote]
    cases guards σ [_] with
    | error fault => exact .inl ⟨_, fault, rfl, rfl⟩
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [StateTree.denote, FlowTree.denote]
    split <;> assumption

def StateTree.bindFlow? {Γ : Var → Nat}
    {next : LocalStore Γ → CWord Γ → FlowTree Γ → Prop}
    (build : ∀ locals word, Option {output // next locals word output})
    (tree : StateTree Γ) : Option {output // ExpressionFlowBind next tree output} :=
  match tree with
  | .value locals word => do
    let r ← build locals word
    return ⟨r.val, .value r.property⟩
  | .checked locals guard inner => do
    let r ← inner.bindFlow? build
    return ⟨.checked locals guard r.val, .checked r.property⟩
  | .branch condition yes no => do
    let a ← yes.bindFlow? build
    let b ← no.bindFlow? build
    return ⟨.branch condition a.val b.val, .branch a.property b.property⟩

theorem expression_flow_sound {Γ : Var → Nat} (locals : LocalStore Γ)
    (control : Source.Control) (expr : Source.Expr) (frame : Source.Frame)
    (before : StateTree Γ) (after : FlowTree Γ)
    (start : ∀ σ p fid k, Source.Step p ⟨control, locals.denote σ, k, fid⟩
      ⟨.expression expr, locals.denote σ, frame :: k, fid⟩)
    (evaluated : StateDiagram locals expr before)
    (continued : ExpressionFlowBind (fun ls w t => FrameDiagram ls frame w t) before after) :
    FlowDiagram locals control after := by
  intro σ p fid k
  have eval := evaluated σ p fid
  rcases continued.outcome σ with ⟨ls, fault, failed, output⟩ |
    ⟨ls, word, tree, resume, success, output⟩
  · rw [output]
    simp only [Source.EvaluatesState, failed] at eval
    obtain ⟨failedK, path⟩ := eval (frame :: k)
    exact ⟨failedK, .next (start σ p fid k) path⟩
  · rw [output]
    simp only [Source.EvaluatesState, success] at eval
    have path := Source.Steps.next (start σ p fid k) (eval (frame :: k))
    have tail := resume σ p fid k
    cases result : (tree.denote σ).2 with
    | error fault =>
      simp only [result] at tail ⊢
      obtain ⟨failedK, steps⟩ := tail
      exact ⟨failedK, path.trans steps⟩
    | ok flow =>
      simp only [result] at tail ⊢
      exact path.trans tail

/-- Only normal flow executes the remainder of a block. -/
inductive NormalBind {Γ : Var → Nat} (next : LocalStore Γ → FlowTree Γ → Prop) :
    FlowTree Γ → FlowTree Γ → Prop where
  | normal {locals output} (continued : next locals output) :
      NormalBind next (.value locals .normal) output
  | break (locals) : NormalBind next (.value locals .break) (.value locals .break)
  | continue (locals) : NormalBind next (.value locals .continue) (.value locals .continue)
  | returned (locals word) :
      NormalBind next (.value locals (.returned word)) (.value locals (.returned word))
  | checked {locals guard before after} (inner : NormalBind next before after) :
      NormalBind next (.checked locals guard before) (.checked locals guard after)
  | branch {condition yes no yes' no'}
      (first : NormalBind next yes yes') (second : NormalBind next no no') :
      NormalBind next (.branch condition yes no) (.branch condition yes' no')

theorem NormalBind.outcome {Γ : Var → Nat}
    {next : LocalStore Γ → FlowTree Γ → Prop} {before after : FlowTree Γ}
    (diagram : NormalBind next before after) (σ : Subst Γ) :
    (before.denote σ = after.denote σ ∧
      ∀ ls, before.denote σ ≠ (ls, .ok .normal)) ∨
    (∃ locals output, next locals output ∧
      before.denote σ = (locals.denote σ, .ok .normal) ∧
      after.denote σ = output.denote σ) := by
  induction diagram with
  | normal continued => exact .inr ⟨_, _, continued, rfl, rfl⟩
  | «break» => exact .inl ⟨rfl, by intro ls; simp [FlowTree.denote, SymbolicFlow.denote]⟩
  | «continue» => exact .inl ⟨rfl, by intro ls; simp [FlowTree.denote, SymbolicFlow.denote]⟩
  | returned => exact .inl ⟨rfl, by intro ls; simp [FlowTree.denote, SymbolicFlow.denote]⟩
  | checked inner ih =>
    simp only [FlowTree.denote]
    cases guards σ [_] with
    | error fault => exact .inl ⟨rfl, by simp⟩
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [FlowTree.denote]
    split <;> assumption

def FlowTree.bindNormal? {Γ : Var → Nat}
    {next : LocalStore Γ → FlowTree Γ → Prop}
    (build : ∀ locals, Option {output // next locals output})
    (tree : FlowTree Γ) : Option {output // NormalBind next tree output} :=
  match tree with
  | .value locals .normal => do
    let r ← build locals
    return ⟨r.val, .normal r.property⟩
  | .value locals .break => some ⟨.value locals .break, .break locals⟩
  | .value locals .continue => some ⟨.value locals .continue, .continue locals⟩
  | .value locals (.returned word) => some ⟨.value locals (.returned word), .returned locals word⟩
  | .checked locals guard inner => do
    let r ← inner.bindNormal? build
    return ⟨.checked locals guard r.val, .checked r.property⟩
  | .branch condition yes no => do
    let a ← yes.bindNormal? build
    let b ← no.bindNormal? build
    return ⟨.branch condition a.val b.val, .branch a.property b.property⟩

/-- A checked operation on each successful statement exit, including return
and loop-control exits. Failed exits never perform scope unwinding. -/
inductive FlowMap {Γ : Var → Nat}
    (next : LocalStore Γ → SymbolicFlow Γ → FlowTree Γ → Prop) :
    FlowTree Γ → FlowTree Γ → Prop where
  | value {locals flow output} (continued : next locals flow output) :
      FlowMap next (.value locals flow) output
  | checked {locals guard before after} (inner : FlowMap next before after) :
      FlowMap next (.checked locals guard before) (.checked locals guard after)
  | branch {condition yes no yes' no'}
      (first : FlowMap next yes yes') (second : FlowMap next no no') :
      FlowMap next (.branch condition yes no) (.branch condition yes' no')

theorem FlowMap.outcome {Γ : Var → Nat}
    {next : LocalStore Γ → SymbolicFlow Γ → FlowTree Γ → Prop} {before after : FlowTree Γ}
    (diagram : FlowMap next before after) (σ : Subst Γ) :
    (∃ locals fault, before.denote σ = (locals, .error fault) ∧
      after.denote σ = (locals, .error fault)) ∨
    (∃ locals flow output, next locals flow output ∧
      before.denote σ = (locals.denote σ, .ok (flow.denote σ)) ∧
      after.denote σ = output.denote σ) := by
  induction diagram with
  | value continued => exact .inr ⟨_, _, _, continued, rfl, rfl⟩
  | checked inner ih =>
    simp only [FlowTree.denote]
    cases guards σ [_] with
    | error fault => exact .inl ⟨_, fault, rfl, rfl⟩
    | ok _ => exact ih
  | branch first second ih ih' =>
    simp only [FlowTree.denote]
    split <;> assumption

def FlowTree.mapLeaves? {Γ : Var → Nat}
    {next : LocalStore Γ → SymbolicFlow Γ → FlowTree Γ → Prop}
    (build : ∀ locals flow, Option {output // next locals flow output})
    (tree : FlowTree Γ) : Option {output // FlowMap next tree output} :=
  match tree with
  | .value locals flow => do
    let r ← build locals flow
    return ⟨r.val, .value r.property⟩
  | .checked locals guard inner => do
    let r ← inner.mapLeaves? build
    return ⟨.checked locals guard r.val, .checked r.property⟩
  | .branch condition yes no => do
    let a ← yes.mapLeaves? build
    let b ← no.mapLeaves? build
    return ⟨.branch condition a.val b.val, .branch a.property b.property⟩

end EvoIR.Core.Symbolic
