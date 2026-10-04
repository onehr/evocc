import EvoIR.Core.SourceCutpoint
import EvoIR.Core.Correspondence
import EvoIR.Core.CallSourceDiagram
import EvoIR.Core.SourceResumption

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
    (k : List Source.Frame) (tree : StateTree Γ) (context : Option SourceReturnContext) :
    ((tree.cutTree k).eval σ).map (fun cursor => cursor.denote σ fid context) =
      (tree.denote σ).2.map (fun v =>
        ⟨.value v, (tree.denote σ).1, sourceContextFrames context k, fid⟩) := by
  induction tree with
  | value locals word => cases context <;> rfl
  | checked locals guard tail ih =>
    simp only [cutTree, CutTree.eval, StateTree.denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => exact ih
  | branch condition yes no first second =>
    simp only [cutTree, CutTree.eval, StateTree.denote]
    split <;> assumption

theorem expression_cut_sound {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (expr : Source.Expr) (k : List Source.Frame) (tree : StateTree Γ)
    (diagram : ContextDiagram p locals expr tree) :
    SourceCutRuns p true ⟨.expression expr, locals, k⟩ (tree.cutTree k) := by
  intro σ fid context
  have path := diagram σ fid (sourceContextFrames context k)
  have same := tree.cut_eval σ fid k context
  have start : (SourceCursor.mk (.expression expr) locals k).denote σ fid context =
      ⟨.expression expr, locals.denote σ, sourceContextFrames context k, fid⟩ := by
    cases context <;> rfl
  rw [start]
  cases result : tree.denote σ with
  | mk ls value =>
    cases value with
    | error fault =>
      simp only [result] at path
      obtain ⟨failedLocals, failedK, failedFid, steps⟩ := path
      have failed : (tree.cutTree k).eval σ = .error fault := by
        cases h : (tree.cutTree k).eval σ <;> simp_all [Except.map]
      refine ⟨_, steps, ?_, ?_⟩
      · intro _
        exact steps.toReaches.positive_of_ne (by intro h; cases h)
      · simp [failed]
    | ok word =>
      simp only [result] at path
      have output : ∃ exit, (tree.cutTree k).eval σ = .ok exit ∧
          exit.denote σ fid context =
            ⟨.value word, ls, sourceContextFrames context k, fid⟩ := by
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
    (k : List Source.Frame) (tree : FlowTree Γ) (context : Option SourceReturnContext) :
    ((tree.cutTree k).eval σ).map (fun cursor => cursor.denote σ fid context) =
      (tree.denote σ).2.map (fun v =>
        ⟨.flow v, (tree.denote σ).1, sourceContextFrames context k, fid⟩) := by
  induction tree with
  | value locals flow => cases context <;> rfl
  | checked locals guard tail ih =>
    simp only [cutTree, CutTree.eval, FlowTree.denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok u => exact ih
  | branch condition yes no first second =>
    simp only [cutTree, CutTree.eval, FlowTree.denote]
    split <;> assumption

theorem object_cut_sound {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (object : Source.Object) (k : List Source.Frame) (tree : FlowTree Γ)
    (diagram : ContextFlowDiagram p locals (.object object) tree) :
    SourceCutRuns p true ⟨.object object, locals, k⟩ (tree.cutTree k) := by
  intro σ fid context
  have path := diagram σ fid (sourceContextFrames context k)
  have same := tree.cut_eval σ fid k context
  have start : (SourceCursor.mk (.object object) locals k).denote σ fid context =
      ⟨.object object, locals.denote σ, sourceContextFrames context k, fid⟩ := by
    cases context <;> rfl
  rw [start]
  cases result : tree.denote σ with
  | mk ls value =>
    cases value with
    | error fault =>
      simp only [result] at path
      obtain ⟨failedLocals, failedK, failedFid, steps⟩ := path
      have failed : (tree.cutTree k).eval σ = .error fault := by
        cases h : (tree.cutTree k).eval σ <;> simp_all [Except.map]
      refine ⟨_, steps, ?_, ?_⟩
      · intro _
        exact steps.toReaches.positive_of_ne (by intro h; cases h)
      · simp [failed]
    | ok flow =>
      simp only [result] at path
      have output : ∃ exit, (tree.cutTree k).eval σ = .ok exit ∧
          exit.denote σ fid context =
            ⟨.flow flow, ls, sourceContextFrames context k, fid⟩ := by
        cases h : (tree.cutTree k).eval σ with
        | error fault => simp [h, result, Except.map] at same
        | ok exit => exact ⟨exit, rfl, by simpa [h, result, Except.map] using same⟩
      obtain ⟨exit, success, represented⟩ := output
      refine ⟨_, path, ?_, ?_⟩
      · intro _
        exact path.toReaches.positive_of_ne (by intro h; cases h)
      · simpa [success] using represented.symm

theorem CursorStep.runs {Γ : Var → Nat} (p : Source.Program) {cursor : SourceCursor Γ}
    {tree : CutTree Γ (SourceCursor Γ)} (step : CursorStep cursor tree) :
    SourceCutRuns p true cursor tree := by
  intro σ fid context
  have actual := step σ p fid context
  cases result : tree.eval σ with
  | error fault => simp [result] at actual
  | ok exit =>
    simp only [result] at actual
    exact ⟨exit.denote σ fid context, .next actual (.refl _), fun _ => .single actual, rfl⟩

theorem source_cut_compose {Γ : Var → Nat} {p : Source.Program} {advance : Bool} {cursor : SourceCursor Γ}
    {before after : CutTree Γ (SourceCursor Γ)} (first : SourceCutRuns p advance cursor before)
    (continued : CutBind (SourceCutRuns p false) before after) : SourceCutRuns p advance cursor after := by
  intro σ fid context
  obtain ⟨middle, path, positive, outcome⟩ := first σ fid context
  rcases continued.outcome σ with ⟨fault, failed, output⟩ |
    ⟨exit, rest, success, resumed, output⟩
  · exact ⟨middle, path, positive, by simpa [output] using (by simpa [failed] using outcome :
        middle.control = .halted (.error fault))⟩
  · have same : middle = exit.denote σ fid context := by simpa [success] using outcome
    subst middle
    obtain ⟨final, suffix, _, endState⟩ := resumed σ fid context
    exact ⟨final, path.trans suffix,
      fun h => Transition.positive_then_reaches (positive h) suffix.toReaches,
      by simpa [output] using endState⟩

theorem SourceCutRuns.relax {Γ : Var → Nat} {p : Source.Program} {cursor : SourceCursor Γ}
    {tree : CutTree Γ (SourceCursor Γ)} (first : SourceCutRuns p true cursor tree) (advance : Bool) :
    SourceCutRuns p advance cursor tree := by
  intro σ fid context
  obtain ⟨final, path, positive, outcome⟩ := first σ fid context
  exact ⟨final, path, fun _ => positive rfl, outcome⟩

def cursorStep {Γ : Var → Nat} (cursor : SourceCursor Γ) :
    Option {tree // CursorStep cursor tree} :=
  match cursor with
  | ⟨.statement stmt, locals, k⟩ =>
    some ⟨.leaf (cursorStatement locals k stmt), cursor_statement_sound locals k stmt⟩
  | ⟨.items [], locals, k⟩ => some ⟨.leaf ⟨.flow .normal, locals, k⟩,
      by intro σ p fid context; cases context <;> rfl⟩
  | ⟨.items (.statement stmt :: rest), locals, k⟩ =>
    some ⟨.leaf ⟨.statement stmt, locals, .sequence rest :: k⟩,
      by intro σ p fid context; cases context <;> rfl⟩
  | ⟨.items (.declaration object :: rest), locals, k⟩ =>
    some ⟨.leaf ⟨.object object, locals, .sequence rest :: k⟩,
      by intro σ p fid context; cases context <;> rfl⟩
  | ⟨.flow flow, locals, k⟩ => cursorFlow locals k flow
  | ⟨.value word, locals, k⟩ => cursorValue locals k word
  | _ => none

theorem resumption_cut_sound {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (frame : Source.Frame) (word : CWord Γ) (k : List Source.Frame) (tree : StateTree Γ)
    (diagram : ResumptionDiagram p locals frame word tree) :
    SourceCutRuns p true ⟨.value word, locals, frame :: k⟩ (tree.cutTree k) := by
  intro σ fid context
  have path := diagram.runs σ fid (sourceContextFrames context k)
  have same := tree.cut_eval σ fid k context
  have start : (SourceCursor.mk (.value word) locals (frame :: k)).denote σ fid context =
      ⟨.value (word.denote σ), locals.denote σ, frame :: sourceContextFrames context k, fid⟩ := by
    cases context with
    | none => rfl
    | some caller =>
      simp only [SourceCursor.denote, CutControl.denote]
      rw [diagram.frames_stable]
  rw [start]
  cases result : tree.denote σ with
  | mk ls value =>
    cases value with
    | error fault =>
      simp only [result] at path
      obtain ⟨failedLocals, failedK, failedFid, steps⟩ := path
      have failed : (tree.cutTree k).eval σ = .error fault := by
        cases h : (tree.cutTree k).eval σ <;> simp_all [Except.map]
      refine ⟨_, steps, ?_, ?_⟩
      · intro _
        exact steps.toReaches.positive_of_ne (by intro h; cases h)
      · simp [failed]
    | ok answer =>
      simp only [result] at path
      have output : ∃ exit, (tree.cutTree k).eval σ = .ok exit ∧
          exit.denote σ fid context =
            ⟨.value answer, ls, sourceContextFrames context k, fid⟩ := by
        cases h : (tree.cutTree k).eval σ with
        | error fault => simp [h, result, Except.map] at same
        | ok exit => exact ⟨exit, rfl, by simpa [h, result, Except.map] using same⟩
      obtain ⟨exit, success, represented⟩ := output
      refine ⟨_, path, ?_, ?_⟩
      · intro _
        apply path.toReaches.positive_of_ne
        intro equal
        have lengths := congrArg (fun s : Source.State => s.continuation.length) equal
        simp only [List.length_cons] at lengths
        omega
      · simpa [success] using represented.symm

def sourceChunk {Γ : Var → Nat} (p : Source.Program) (budget : Nat) (cursor : SourceCursor Γ) :
    Option {tree // SourceCutRuns p true cursor tree} :=
  match cursor with
  | ⟨.expression expr, locals, k⟩ => do
    let e ← callExpression p budget locals expr
    return ⟨e.val.cutTree k, expression_cut_sound p locals expr k e.val e.property⟩
  | ⟨.object object, locals, k⟩ => do
    let e ← callFlow p budget locals (.object object)
    return ⟨e.val.cutTree k, object_cut_sound p locals object k e.val e.property⟩
  | ⟨.value word, locals, frame :: k⟩ =>
    match resumeExpression p budget locals frame word with
    | some e => some ⟨e.val.cutTree k, resumption_cut_sound p locals frame word k e.val e.property⟩
    | none => do
      let step ← cursorStep ⟨.value word, locals, frame :: k⟩
      return ⟨step.val, step.property.runs p⟩
  | cursor => do
    let step ← cursorStep cursor
    return ⟨step.val, step.property.runs p⟩

def sourceCutDiagram {Γ : Var → Nat} (p : Source.Program) (cutpoints : List SourceSite) (budget : Nat)
    (advance : Bool) (cursor : SourceCursor Γ) :
    Option {tree // SourceCutRuns p advance cursor tree} :=
  if stopped : advance = false ∧
      ((cursor.site.any (fun site => cutpoints.contains site)) ||
        (match cursor.control with | .returned _ => true | _ => false)) = true then
    some ⟨.leaf cursor, by
      intro σ fid context
      exact ⟨cursor.denote σ fid context, .refl _, by simp [stopped.1], rfl⟩⟩
  else match budget with
  | 0 => none
  | budget+1 => do
    let chunk ← sourceChunk p (budget+1) cursor
    let continued ← chunk.val.bindChecked (next := SourceCutRuns p false)
      (fun next => sourceCutDiagram p cutpoints budget false next)
    return ⟨continued.val, source_cut_compose (chunk.property.relax advance) continued.property⟩

theorem source_cut_diagram_sound {Γ : Var → Nat} (p : Source.Program) (cutpoints : List SourceSite)
    (budget : Nat) (advance : Bool) (cursor : SourceCursor Γ) (tree : CutTree Γ (SourceCursor Γ))
    (accepted : (sourceCutDiagram p cutpoints budget advance cursor).map Subtype.val = some tree) :
    SourceCutRuns p advance cursor tree := by
  cases h : sourceCutDiagram p cutpoints budget advance cursor with
  | none => simp [h] at accepted
  | some result =>
    simp only [h, Option.map_some, Option.some.injEq] at accepted
    exact accepted ▸ result.property

end EvoIR.Core.Symbolic
