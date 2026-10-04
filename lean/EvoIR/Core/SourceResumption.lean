import EvoIR.Core.CallSourceDiagram
import EvoIR.Core.SourceCutpoint

/-! Source expression resumption after an actual call return. Unlike an
expression diagram, the starting value has already been computed. The pending
operand executes with the actual saved continuation, local store and callee
fault behavior. No source expression is substituted for the returned value. -/
namespace EvoIR.Core.Source

def ResumesContext (p : Program) (locals : Locals) (fid : Nat)
    (frame : Frame) (value : Value) (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok answer => Steps p ⟨.value value, locals, frame :: k, fid⟩
      ⟨.value answer, outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFunction,
      Steps p ⟨.value value, locals, frame :: k, fid⟩
        ⟨.halted (.error fault), failedLocals, failedK, failedFunction⟩

theorem resumes_binary {p : Program} {locals : Locals} {fid : Nat}
    (type : IntType) (op : Binary) (right : Expr) (value : Value) (outcome : StateOutcome)
    (continueRight : (op == .and && !value.truth) = false ∧ (op == .or && value.truth) = false)
    (operand : EvaluatesContext p locals fid right outcome)
    (typed : ∀ v result, outcome.2 = .ok v → binaryValue op value v = .ok result → result.type = type) :
    ResumesContext p locals fid (.binaryLeft type op right) value
      (outcome.1, do let v ← outcome.2; binaryValue op value v) := by
  intro k
  have start : Step p ⟨.value value, locals, .binaryLeft type op right :: k, fid⟩
      ⟨.expression right, locals, .binaryRight type op value :: k, fid⟩ := by
    simp [Step, step, resumeValue, continueRight.1, continueRight.2]
  cases result : outcome.2 with
  | error fault =>
    simp only [EvaluatesContext, result] at operand
    obtain ⟨ls, failedK, failedFid, steps⟩ := operand (.binaryRight type op value :: k)
    exact ⟨ls, failedK, failedFid, .next start steps⟩
  | ok v =>
    simp only [EvaluatesContext, result] at operand
    have path := Steps.next start (operand (.binaryRight type op value :: k))
    cases calculated : binaryValue op value v with
    | error fault =>
      simp only [bind, Except.bind, calculated]
      exact ⟨outcome.1, k, fid, path.trans (.next (by
        simp [Step, step, resumeValue, valueResult, calculated, fail]) (.refl _))⟩
    | ok answer =>
      simp only [bind, Except.bind, calculated]
      exact path.trans (.next (by
        simp [Step, step, resumeValue, valueResult, calculated, typed v answer result calculated]) (.refl _))

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

structure ResumptionDiagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (frame : Source.Frame) (word : CWord Γ) (tree : StateTree Γ) : Prop where
  frames_stable : ∀ context rest, sourceContextFrames context (frame :: rest) =
    frame :: sourceContextFrames context rest
  runs : ∀ σ fid, Source.ResumesContext p (locals.denote σ) fid frame (word.denote σ) (tree.denote σ)

def resumeExpression {Γ : Var → Nat} (p : Source.Program) (budget : Nat)
    (locals : LocalStore Γ) (frame : Source.Frame) (word : CWord Γ) :
    Option {tree // ResumptionDiagram p locals frame word tree} :=
  match frame with
  | .binaryLeft type op right => do
    if notAnd : op ≠ .and then
      if notOr : op ≠ .or then
        let r ← callExpression p budget locals right
        let applied ← r.val.bind? (next := BinaryContinuation type op word) (fun ls v => do
          let result ← binaryResult op word v
          if typed : result.val.2.type = type then
            return ⟨.linear ls result.val, result.val, result.property, typed, rfl⟩
          else none)
        return ⟨applied.val, ⟨by intro context rest; cases context <;> rfl, by
          intro σ fid
          obtain ⟨result, typed⟩ := binary_continuation_result σ type op word r.val applied.val applied.property
          rw [result]
          exact Source.resumes_binary type op right (word.denote σ) (r.val.denote σ)
            (by simp [notAnd, notOr]) (r.property σ fid) typed⟩⟩
      else none
    else none
  | .convert type => some ⟨.value locals (word.cast type), ⟨
      by intro context rest; cases context <;> rfl,
      by intro σ fid k; simp only [StateTree.denote, cast_sound]; exact .next rfl (.refl _)⟩⟩
  | _ => none

end EvoIR.Core.Symbolic
