import EvoIR.Core.CallOperators

/-! Program-relative statement execution. A callee fault retains its actual
activation; successful statements restore the caller and its continuation.
The existing symbolic flow trees and scope operations are reused unchanged. -/
namespace EvoIR.Core.Source

def RunsContextFlow (p : Program) (locals : Locals) (fid : Nat)
    (control : Control) (outcome : FlowOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok flow => Steps p ⟨control, locals, k, fid⟩
      ⟨.flow flow, outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFid,
      Steps p ⟨control, locals, k, fid⟩ ⟨.halted (.error fault), failedLocals, failedK, failedFid⟩

def ResumesContextFlow (p : Program) (locals : Locals) (fid : Nat)
    (frame : Frame) (value : Value) (outcome : FlowOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok flow => Steps p ⟨.value value, locals, frame :: k, fid⟩
      ⟨.flow flow, outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFid,
      Steps p ⟨.value value, locals, frame :: k, fid⟩
        ⟨.halted (.error fault), failedLocals, failedK, failedFid⟩

theorem RunsFlow.inContext {p locals fid control outcome}
    (path : RunsFlow p locals fid control outcome) :
    RunsContextFlow p locals fid control outcome := by
  intro k
  have steps := path k
  cases result : outcome.2 with
  | error fault =>
    simp only [result] at steps ⊢
    obtain ⟨failedK, steps⟩ := steps
    exact ⟨outcome.1, failedK, fid, steps⟩
  | ok flow => simpa only [result] using steps

theorem ResumesFlow.inContext {p locals fid frame value outcome}
    (path : ResumesFlow p locals fid frame value outcome) :
    ResumesContextFlow p locals fid frame value outcome := by
  intro k
  have steps := path k
  cases result : outcome.2 with
  | error fault =>
    simp only [result] at steps ⊢
    obtain ⟨failedK, steps⟩ := steps
    exact ⟨outcome.1, failedK, fid, steps⟩
  | ok flow => simpa only [result] using steps

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

def ContextFlowDiagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (control : Source.Control) (tree : FlowTree Γ) : Prop :=
  ∀ σ fid, Source.RunsContextFlow p (locals.denote σ) fid control (tree.denote σ)

def ContextFrameDiagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (frame : Source.Frame) (word : CWord Γ) (tree : FlowTree Γ) : Prop :=
  ∀ σ fid, Source.ResumesContextFlow p (locals.denote σ) fid frame
    (word.denote σ) (tree.denote σ)

theorem context_expression_flow {Γ : Var → Nat} (p : Source.Program)
    (locals fresh : LocalStore Γ) (control : Source.Control) (expr : Source.Expr)
    (frame : Source.Frame) (before : StateTree Γ) (after : FlowTree Γ)
    (start : ∀ σ fid k, Source.Step p ⟨control, locals.denote σ, k, fid⟩
      ⟨.expression expr, fresh.denote σ, frame :: k, fid⟩)
    (evaluated : ContextDiagram p fresh expr before)
    (continued : ExpressionFlowBind (ContextFrameDiagram p · frame · ·) before after) :
    ContextFlowDiagram p locals control after := by
  intro σ fid k
  have eval := evaluated σ fid (frame :: k)
  rcases continued.outcome σ with ⟨ls, fault, failed, output⟩ |
    ⟨ls, word, tree, resume, success, output⟩
  · rw [output]
    simp only [failed] at eval
    obtain ⟨failedLocals, failedK, failedFid, path⟩ := eval
    exact ⟨failedLocals, failedK, failedFid, .next (start σ fid k) path⟩
  · rw [output]
    simp only [success] at eval
    have path := Source.Steps.next (start σ fid k) eval
    have tail := resume σ fid k
    cases result : (tree.denote σ).2 with
    | error fault =>
      simp only [result] at tail ⊢
      obtain ⟨failedLocals, failedK, failedFid, steps⟩ := tail
      exact ⟨failedLocals, failedK, failedFid, path.trans steps⟩
    | ok flow =>
      simp only [result] at tail ⊢
      exact path.trans tail

theorem context_sequence {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (item : Source.BlockItem) (rest : List Source.BlockItem) (before after : FlowTree Γ)
    (first : ContextFlowDiagram p locals (itemControl item) before)
    (next : NormalBind (fun ls tree => ContextFlowDiagram p ls (.items rest) tree) before after) :
    ContextFlowDiagram p locals (.items (item :: rest)) after := by
  intro σ fid k
  have start : Source.Step p ⟨.items (item :: rest), locals.denote σ, k, fid⟩
      ⟨itemControl item, locals.denote σ, .sequence rest :: k, fid⟩ := by
    cases item <;> rfl
  have path := first σ fid (.sequence rest :: k)
  rcases next.outcome σ with ⟨same, notNormal⟩ | ⟨ls, tree, tail, success, output⟩
  · rw [← same]
    cases result : (before.denote σ).2 with
    | error fault =>
      simp only [result] at path ⊢
      obtain ⟨failedLocals, failedK, failedFid, steps⟩ := path
      exact ⟨failedLocals, failedK, failedFid, .next start steps⟩
    | ok flow =>
      simp only [result] at path ⊢
      have notNormalFlow : flow ≠ .normal := by
        intro sameFlow
        apply notNormal (before.denote σ).1
        exact Prod.ext rfl (result.trans (congrArg Except.ok sameFlow))
      exact .next start (path.trans (.next (by
        cases flow <;> simp_all [Source.Step, Source.step, Source.resumeFlow]) (.refl _)))
  · rw [output]
    simp only [success] at path
    have prefixPath := (Source.Steps.next start path).trans (.next rfl (.refl _))
    have finish := tail σ fid k
    cases result : (tree.denote σ).2 with
    | error fault =>
      simp only [result] at finish ⊢
      obtain ⟨failedLocals, failedK, failedFid, steps⟩ := finish
      exact ⟨failedLocals, failedK, failedFid, prefixPath.trans steps⟩
    | ok flow =>
      simp only [result] at finish ⊢
      exact prefixPath.trans finish

theorem context_scope {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (items : List Source.BlockItem) (before after : FlowTree Γ)
    (body : ContextFlowDiagram p locals (.items items) before)
    (scope : FlowMap (ScopeExit locals.length) before after) :
    ContextFlowDiagram p locals (.statement (.compound items)) after := by
  intro σ fid k
  have length : (locals.denote σ).length = locals.length := by simp [LocalStore.denote]
  have start : Source.Step p ⟨.statement (.compound items), locals.denote σ, k, fid⟩
      ⟨.items items, locals.denote σ, .scope locals.length :: k, fid⟩ := by
    simp [Source.Step, Source.step, Source.enterStatement, length]
  have path := body σ fid (.scope locals.length :: k)
  rcases scope.outcome σ with ⟨ls, fault, failed, output⟩ |
    ⟨ls, flow, tree, ⟨updated, exit, treeEq⟩, success, output⟩
  · rw [output]
    simp only [failed] at path
    obtain ⟨failedLocals, failedK, failedFid, steps⟩ := path
    exact ⟨failedLocals, failedK, failedFid, .next start steps⟩
  · rw [output, treeEq]
    simp only [success] at path
    have resume := local_scope_exit_sound σ ls updated locals.length exit
      ⟨.flow (flow.denote σ), ls.denote σ, k, fid⟩ rfl (flow.denote σ)
    exact .next start (path.trans (.next (congrArg some resume) (.refl _)))

theorem context_branch_frame {Γ : Var → Nat} (p : Source.Program)
    (locals : LocalStore Γ) (word : CWord Γ) (yes : Source.Stmt) (no : Option Source.Stmt)
    (a b : FlowTree Γ) (first : ContextFlowDiagram p locals (.statement yes) a)
    (second : ContextFlowDiagram p locals (.statement (no.getD .null)) b) :
    ContextFrameDiagram p locals (.branch yes no) word (.branch word.truthTerm a b) := by
  intro σ fid k
  simp only [FlowTree.denote, truth_term_sound]
  cases truth : (word.denote σ).truth with
  | false =>
    simp only [Bool.false_eq_true, ↓reduceIte]
    have path := second σ fid k
    cases result : (b.denote σ).2 with
    | error fault =>
      simp only [result] at path ⊢
      obtain ⟨failedLocals, failedK, failedFid, tail⟩ := path
      exact ⟨failedLocals, failedK, failedFid,
        .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) tail⟩
    | ok flow =>
      simp only [result] at path ⊢
      exact .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) path
  | true =>
    simp only [↓reduceIte]
    have path := first σ fid k
    cases result : (a.denote σ).2 with
    | error fault =>
      simp only [result] at path ⊢
      obtain ⟨failedLocals, failedK, failedFid, tail⟩ := path
      exact ⟨failedLocals, failedK, failedFid,
        .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) tail⟩
    | ok flow =>
      simp only [result] at path ⊢
      exact .next (by simp [Source.Step, Source.step, Source.resumeValue, truth]) path

theorem context_initialized_object {Γ : Var → Nat} (p : Source.Program)
    (locals fresh : LocalStore Γ) (object : Source.Object) (expr : Source.Expr)
    (before : StateTree Γ) (after : FlowTree Γ)
    (initializer : object.initializer = some (object.type, expr))
    (declared : locals.declare object.symbol object.type = some fresh)
    (value : ContextDiagram p fresh expr before)
    (continued : ExpressionFlowBind
      (ContextFrameDiagram p · (.initialize object.symbol object.type) · ·) before after) :
    ContextFlowDiagram p locals (.object object) after := by
  apply context_expression_flow p locals fresh (.object object) expr
    (.initialize object.symbol object.type) before after ?_ value continued
  intro σ fid k
  unfold LocalStore.declare at declared
  split at declared
  · contradiction
  · rename_i unique
    cases Option.some.inj declared
    have duplicate : (locals.denote σ).any (·.symbol == object.symbol) = false := by
      simpa [LocalStore.denote, List.any_map, Function.comp_def, LocalSlot.denote]
        using Bool.eq_false_iff.mpr unique
    simp only [Source.Step, Source.step, Source.enterObject, duplicate,
      Bool.false_eq_true, ↓reduceIte, initializer, bne_self_eq_false]
    rfl

end EvoIR.Core.Symbolic
