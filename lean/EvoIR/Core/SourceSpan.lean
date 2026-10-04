import EvoIR.Core.SourceCutpoint
import EvoIR.Core.Correspondence

/-! Checked finite source spans between cyclic cutpoints. The exploration
bound is not an execution bound: a span must end at a checked successor or
terminal, and its proof covers every valuation of the incoming symbolic state. -/
namespace EvoIR.Core.Symbolic

def StateTree.cutTree {Γ : Var → Nat} (k : List Source.Frame) :
    StateTree Γ → CutTree Γ (SourceCursor Γ)
  | .value locals word => .leaf ⟨.value word, locals, k⟩
  | .checked _ guard tail => .checked guard (tail.cutTree k)
  | .branch condition yes no => .branch condition (yes.cutTree k) (no.cutTree k)

theorem StateTree.cut_eval {Γ : Var → Nat} (σ : Subst Γ) (fid : Nat)
    (k : List Source.Frame) (tree : StateTree Γ) :
    ((tree.cutTree k).eval σ).map (SourceCursor.denote σ fid) =
      (tree.denote σ).2.map (fun v => ⟨.value v, (tree.denote σ).1, k, fid⟩) := by
  induction tree with
  | value locals word => rfl
  | checked locals guard tail ih =>
    simp only [cutTree, CutTree.eval, StateTree.denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => exact ih
  | branch condition yes no first second =>
    simp only [cutTree, CutTree.eval, StateTree.denote]
    split <;> assumption

theorem expression_cut_sound {Γ : Var → Nat} (locals : LocalStore Γ)
    (expr : Source.Expr) (k : List Source.Frame) (tree : StateTree Γ)
    (diagram : StateDiagram locals expr tree) :
    SourceCutRuns true ⟨.expression expr, locals, k⟩ (tree.cutTree k) := by
  intro σ p fid
  have path := diagram σ p fid k
  have same := tree.cut_eval σ fid k
  cases result : tree.denote σ with
  | mk ls value =>
    cases value with
    | error fault =>
      simp only [result] at path
      obtain ⟨failedK, steps⟩ := path
      have failed : (tree.cutTree k).eval σ = .error fault := by
        cases h : (tree.cutTree k).eval σ <;> simp_all [Except.map]
      refine ⟨_, steps, ?_, ?_⟩
      · intro _
        exact steps.toReaches.positive_of_ne (by intro h; cases h)
      · simp [failed]
    | ok word =>
      simp only [result] at path
      have output : ∃ exit, (tree.cutTree k).eval σ = .ok exit ∧
          exit.denote σ fid = ⟨.value word, ls, k, fid⟩ := by
        cases h : (tree.cutTree k).eval σ with
        | error fault => simp [h, result, Except.map] at same
        | ok exit => exact ⟨exit, rfl, by simpa [h, result, Except.map] using same⟩
      obtain ⟨exit, success, represented⟩ := output
      refine ⟨_, path, ?_, ?_⟩
      · intro _
        exact path.toReaches.positive_of_ne (by intro h; cases h)
      · simpa [success] using represented.symm

def FlowTree.cutTree {Γ : Var → Nat} (k : List Source.Frame) :
    FlowTree Γ → CutTree Γ (SourceCursor Γ)
  | .value locals flow => .leaf ⟨.flow flow, locals, k⟩
  | .checked _ guard tail => .checked guard (tail.cutTree k)
  | .branch condition yes no => .branch condition (yes.cutTree k) (no.cutTree k)

theorem FlowTree.cut_eval {Γ : Var → Nat} (σ : Subst Γ) (fid : Nat)
    (k : List Source.Frame) (tree : FlowTree Γ) :
    ((tree.cutTree k).eval σ).map (SourceCursor.denote σ fid) =
      (tree.denote σ).2.map (fun v => ⟨.flow v, (tree.denote σ).1, k, fid⟩) := by
  induction tree with
  | value locals flow => rfl
  | checked locals guard tail ih =>
    simp only [cutTree, CutTree.eval, FlowTree.denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => exact ih
  | branch condition yes no first second =>
    simp only [cutTree, CutTree.eval, FlowTree.denote]
    split <;> assumption

theorem object_cut_sound {Γ : Var → Nat} (locals : LocalStore Γ)
    (object : Source.Object) (k : List Source.Frame) (tree : FlowTree Γ)
    (diagram : FlowDiagram locals (.object object) tree) :
    SourceCutRuns true ⟨.object object, locals, k⟩ (tree.cutTree k) := by
  intro σ p fid
  have path := diagram σ p fid k
  have same := tree.cut_eval σ fid k
  cases result : tree.denote σ with
  | mk ls value =>
    cases value with
    | error fault =>
      simp only [result] at path
      obtain ⟨failedK, steps⟩ := path
      have failed : (tree.cutTree k).eval σ = .error fault := by
        cases h : (tree.cutTree k).eval σ <;> simp_all [Except.map]
      refine ⟨_, steps, ?_, ?_⟩
      · intro _
        exact steps.toReaches.positive_of_ne (by intro h; cases h)
      · simp [failed]
    | ok flow =>
      simp only [result] at path
      have output : ∃ exit, (tree.cutTree k).eval σ = .ok exit ∧
          exit.denote σ fid = ⟨.flow flow, ls, k, fid⟩ := by
        cases h : (tree.cutTree k).eval σ with
        | error fault => simp [h, result, Except.map] at same
        | ok exit => exact ⟨exit, rfl, by simpa [h, result, Except.map] using same⟩
      obtain ⟨exit, success, represented⟩ := output
      refine ⟨_, path, ?_, ?_⟩
      · intro _
        exact path.toReaches.positive_of_ne (by intro h; cases h)
      · simpa [success] using represented.symm

theorem CursorStep.runs {Γ : Var → Nat} {cursor : SourceCursor Γ}
    {tree : CutTree Γ (SourceCursor Γ)} (step : CursorStep cursor tree) :
    SourceCutRuns true cursor tree := by
  intro σ p fid
  have actual := step σ p fid
  cases result : tree.eval σ with
  | error fault => simp [result] at actual
  | ok exit =>
    simp only [result] at actual
    exact ⟨exit.denote σ fid, .next actual (.refl _), fun _ => .single actual, rfl⟩

theorem source_cut_compose {Γ : Var → Nat} {advance : Bool} {cursor : SourceCursor Γ}
    {before after : CutTree Γ (SourceCursor Γ)} (first : SourceCutRuns advance cursor before)
    (continued : CutBind (SourceCutRuns false) before after) : SourceCutRuns advance cursor after := by
  intro σ p fid
  obtain ⟨middle, path, positive, outcome⟩ := first σ p fid
  rcases continued.outcome σ with ⟨fault, failed, output⟩ |
    ⟨exit, rest, success, resumed, output⟩
  · exact ⟨middle, path, positive, by simpa [output] using (by simpa [failed] using outcome :
        middle.control = .halted (.error fault))⟩
  · have same : middle = exit.denote σ fid := by simpa [success] using outcome
    subst middle
    obtain ⟨final, suffix, _, endState⟩ := resumed σ p fid
    exact ⟨final, path.trans suffix,
      fun h => Transition.positive_then_reaches (positive h) suffix.toReaches,
      by simpa [output] using endState⟩

theorem SourceCutRuns.relax {Γ : Var → Nat} {cursor : SourceCursor Γ}
    {tree : CutTree Γ (SourceCursor Γ)} (first : SourceCutRuns true cursor tree) (advance : Bool) :
    SourceCutRuns advance cursor tree := by
  intro σ p fid
  obtain ⟨final, path, positive, outcome⟩ := first σ p fid
  exact ⟨final, path, fun _ => positive rfl, outcome⟩

def cursorStep {Γ : Var → Nat} (cursor : SourceCursor Γ) :
    Option {tree // CursorStep cursor tree} :=
  match cursor with
  | ⟨.statement stmt, locals, k⟩ =>
    some ⟨.leaf (cursorStatement locals k stmt), cursor_statement_sound locals k stmt⟩
  | ⟨.items [], locals, k⟩ => some ⟨.leaf ⟨.flow .normal, locals, k⟩, by intro σ p fid; rfl⟩
  | ⟨.items (.statement stmt :: rest), locals, k⟩ =>
    some ⟨.leaf ⟨.statement stmt, locals, .sequence rest :: k⟩, by intro σ p fid; rfl⟩
  | ⟨.items (.declaration object :: rest), locals, k⟩ =>
    some ⟨.leaf ⟨.object object, locals, .sequence rest :: k⟩, by intro σ p fid; rfl⟩
  | ⟨.flow flow, locals, k⟩ => cursorFlow locals k flow
  | ⟨.value word, locals, k⟩ => cursorValue locals k word
  | _ => none

def sourceChunk {Γ : Var → Nat} (cursor : SourceCursor Γ) :
    Option {tree // SourceCutRuns true cursor tree} :=
  match cursor with
  | ⟨.expression expr, locals, k⟩ => do
    let e ← stateExpression (Source.expressionDepth expr) locals expr
    return ⟨e.val.cutTree k, expression_cut_sound locals expr k e.val e.property⟩
  | ⟨.object object, locals, k⟩ => do
    let e ← objectDiagram locals object
    return ⟨e.val.cutTree k, object_cut_sound locals object k e.val e.property⟩
  | cursor => do
    let step ← cursorStep cursor
    return ⟨step.val, step.property.runs⟩

def sourceCutDiagram {Γ : Var → Nat} (cutpoints : List SourceSite) (budget : Nat)
    (advance : Bool) (cursor : SourceCursor Γ) :
    Option {tree // SourceCutRuns advance cursor tree} :=
  if stopped : advance = false ∧
      ((cursor.site.any (fun site => cutpoints.contains site)) ||
        (match cursor.control with | .returned _ => true | _ => false)) = true then
    some ⟨.leaf cursor, by
      intro σ p fid
      exact ⟨cursor.denote σ fid, .refl _, by simp [stopped.1], rfl⟩⟩
  else match budget with
  | 0 => none
  | budget+1 => do
    let chunk ← sourceChunk cursor
    let continued ← chunk.val.bindChecked (next := SourceCutRuns false)
      (fun next => sourceCutDiagram cutpoints budget false next)
    return ⟨continued.val, source_cut_compose (chunk.property.relax advance) continued.property⟩

theorem source_cut_diagram_sound {Γ : Var → Nat} (cutpoints : List SourceSite)
    (budget : Nat) (advance : Bool) (cursor : SourceCursor Γ) (tree : CutTree Γ (SourceCursor Γ))
    (accepted : (sourceCutDiagram cutpoints budget advance cursor).map Subtype.val = some tree) :
    SourceCutRuns advance cursor tree := by
  cases h : sourceCutDiagram cutpoints budget advance cursor with
  | none => simp [h] at accepted
  | some result =>
    simp only [h, Option.map_some, Option.some.injEq] at accepted
    exact accepted ▸ result.property

end EvoIR.Core.Symbolic
