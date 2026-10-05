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

theorem resumes_unary (p : Program) (locals : Locals) (fid : Nat)
    (type : IntType) (op : Unary) (value : Value)
    (typed : ∀ answer, unaryValue op value = .ok answer → answer.type = type) :
    ResumesContext p locals fid (.unary type op) value (locals, unaryValue op value) := by
  intro k
  cases result : unaryValue op value with
  | error fault =>
    exact ⟨locals, k, fid, .next (by simp [Step, step, resumeValue, valueResult, result, fail]) (.refl _)⟩
  | ok answer =>
    exact .next (by simp [Step, step, resumeValue, valueResult, result, typed answer result]) (.refl _)

theorem resumes_binary_right (p : Program) (locals : Locals) (fid : Nat)
    (type : IntType) (op : Binary) (left value : Value)
    (typed : ∀ answer, binaryValue op left value = .ok answer → answer.type = type) :
    ResumesContext p locals fid (.binaryRight type op left) value (locals, binaryValue op left value) := by
  intro k
  cases result : binaryValue op left value with
  | error fault =>
    exact ⟨locals, k, fid, .next (by simp [Step, step, resumeValue, valueResult, result, fail]) (.refl _)⟩
  | ok answer =>
    exact .next (by simp [Step, step, resumeValue, valueResult, result, typed answer result]) (.refl _)

theorem resumes_assignment (p : Program) (locals : Locals) (fid : Nat)
    (op : Assignment) (id : Nat) (destination : QualifiedType) (value : Value) :
    ResumesContext p locals fid (.assign destination.type op id destination) value
      (assignmentOutcome locals op id destination value) := by
  intro k
  have resume : resumeValue ⟨.value value, locals, k, fid⟩
      (.assign destination.type op id destination) value =
      match assignmentOutcome locals op id destination value with
      | (ls, .error fault) => ⟨.halted (.error fault), ls, k, fid⟩
      | (ls, .ok v) => ⟨.value v, ls, k, fid⟩ := by
    simp only [resumeValue, assignmentOutcome, assignmentCalculation]
    cases selected : op.binary with
    | none =>
      simp only [bind, Except.bind, pure, Except.pure]
      cases written : writeSlot locals id destination (value.cast destination.type) false <;>
        simp [fail, valueResult, Value.cast, Value.ofInt]
    | some binary =>
      cases read : readSlot locals id destination.type with
      | error fault => simp [bind, Except.bind, fail]
      | ok old =>
        simp only [bind, Except.bind]
        cases calculated : binaryValue binary old value with
        | error fault => simp [fail]
        | ok raw =>
          dsimp only
          cases written : writeSlot locals id destination (raw.cast destination.type) false <;>
            simp [pure, Except.pure, fail, valueResult, Value.cast, Value.ofInt]
  cases result : assignmentOutcome locals op id destination value with
  | mk updated answer =>
    cases answer with
    | error fault => exact ⟨updated, k, fid, .next (by simp [Step, step, resume, result]) (.refl _)⟩
    | ok answer => exact .next (by simp [Step, step, resume, result]) (.refl _)

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

theorem resumes_short {p : Program} {locals : Locals} {fid : Nat}
    (isOr : Bool) (right : Expr) (value : Value) (outcome : StateOutcome)
    (operand : EvaluatesContext p locals fid right outcome) :
    ResumesContext p locals fid (.binaryLeft .int (if isOr then .or else .and) right) value
      (if value.truth = isOr then (locals, .ok (.boolean isOr))
       else (outcome.1, outcome.2.map (fun v => Value.boolean v.truth))) := by
  by_cases skip : value.truth = isOr
  · intro k
    simp only [skip, ↓reduceIte]
    exact .next (by cases isOr <;>
      simp [Step, step, resumeValue, skip, valueResult, Value.boolean, Value.ofNat]) (.refl _)
  · have truth : value.truth = !isOr := by cases isOr <;> cases h : value.truth <;> simp_all
    have result (v : Value) : binaryValue (if isOr then .or else .and) value v =
        .ok (.boolean v.truth) := by
      cases isOr <;> simp [binaryValue, truth, pure, Except.pure]
    have path := resumes_binary .int (if isOr then .or else .and) right value outcome
      (by cases isOr <;> simp [truth]) operand
      (by intro v answer _ h; rw [result] at h; cases Except.ok.inj h; rfl)
    have same : (do let v ← outcome.2; binaryValue (if isOr then .or else .and) value v) =
        outcome.2.map (fun v => Value.boolean v.truth) := by
      cases outcome.2 <;> simp [bind, Except.bind, result, Except.map]
    simpa only [skip, ↓reduceIte, same] using path

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

structure ResumptionDiagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (frame : Source.Frame (CWord Γ)) (word : CWord Γ) (tree : StateTree Γ) : Prop where
  frames_stable : ∀ σ context rest, sourceContextFrames context (frame.map (CWord.denote σ) :: rest) =
    frame.map (CWord.denote σ) :: sourceContextFrames context rest
  runs : ∀ σ fid, Source.ResumesContext p (locals.denote σ) fid (frame.map (CWord.denote σ)) (word.denote σ) (tree.denote σ)

def resumeShort {Γ : Var → Nat} (p : Source.Program) (budget : Nat)
    (locals : LocalStore Γ) (isOr : Bool) (right : Source.Expr) (word : CWord Γ) :
    Option {tree // ResumptionDiagram p locals
      (.binaryLeft .int (if isOr then .or else .and) right) word tree} := do
  let r ← callExpression p budget locals right
  return ⟨shortTree isOr locals word r.val, ⟨by intro σ context rest; cases context <;> rfl, by
    intro σ fid
    have path := Source.resumes_short isOr right (word.denote σ) (r.val.denote σ) (r.property σ fid)
    have constant (b : Bool) :
        (CWord.constant (Γ := Γ) (.boolean b)).denote σ = Source.Value.boolean b := by
      cases b <;> rfl
    cases isOr <;> cases truth : (word.denote σ).truth <;>
      simpa [shortTree, StateTree.denote, truth_term_sound, truth,
        StateTree.booleanize_sound, constant, Source.Frame.map] using path⟩⟩

def resumeExpression {Γ : Var → Nat} (p : Source.Program) (budget : Nat)
    (locals : LocalStore Γ) (frame : Source.Frame (CWord Γ)) (word : CWord Γ) :
    Option {tree // ResumptionDiagram p locals frame word tree} :=
  match frame with
  | .binaryLeft .int .and right => resumeShort p budget locals false right word
  | .binaryLeft .int .or right => resumeShort p budget locals true right word
  | .binaryRight type op left => do
    let r ← binaryResult op left word
    if typed : r.val.2.type = type then
      return ⟨.linear locals r.val, ⟨by intro σ context rest; cases context <;> rfl, by
        intro σ fid
        rw [StateTree.linear_sound, r.property σ]
        apply Source.resumes_binary_right
        intro answer calculated
        rw [← r.property σ] at calculated
        rw [expression_result_value σ r.val answer calculated]
        exact typed⟩⟩
    else none
  | .unary type op =>
    if typed : (unary op word).2.type = type then
      some ⟨.linear locals (unary op word), ⟨by intro σ context rest; cases context <;> rfl, by
        intro σ fid
        have result : ExpressionResult.denote σ (unary op word) = Source.unaryValue op (word.denote σ) :=
          unary_sound σ op word
        rw [StateTree.linear_sound, result]
        apply Source.resumes_unary
        intro answer calculated
        rw [← result] at calculated
        rw [expression_result_value σ (unary op word) answer calculated]
        exact typed⟩⟩
    else none
  | .assign type op id destination => do
    if typed : type = destination.type then
      let r ← assignmentResult locals op id destination word
      match written : locals.write id destination (r.val.2.cast destination.type) false with
      | none => none
      | some updated =>
        let tree := (StateTree.value updated (r.val.2.cast destination.type)).prefix locals r.val.1
        have continuation : AssignmentContinuation op id destination locals word tree :=
          ⟨r.val, updated, r.property, written, rfl⟩
        return ⟨tree, ⟨by intro σ context rest; cases context <;> rfl, by
          intro σ fid
          rw [assignment_continuation_sound σ continuation, typed]
          exact Source.resumes_assignment p (locals.denote σ) fid op id destination (word.denote σ)⟩⟩
    else none
  | .binaryLeft type op right => do
    if notAnd : op ≠ .and then
      if notOr : op ≠ .or then
        let r ← callExpression p budget locals right
        let applied ← r.val.bind? (next := BinaryContinuation type op word) (fun ls v => do
          let result ← binaryResult op word v
          if typed : result.val.2.type = type then
            return ⟨.linear ls result.val, result.val, result.property, typed, rfl⟩
          else none)
        return ⟨applied.val, ⟨by intro σ context rest; cases context <;> rfl, by
          intro σ fid
          obtain ⟨result, typed⟩ := binary_continuation_result σ type op word r.val applied.val applied.property
          rw [result]
          exact Source.resumes_binary type op right (word.denote σ) (r.val.denote σ)
            (by simp [notAnd, notOr]) (r.property σ fid) typed⟩⟩
      else none
    else none
  | .convert type => some ⟨.value locals (word.cast type), ⟨
      by intro σ context rest; cases context <;> rfl,
      by intro σ fid k; simp only [StateTree.denote, cast_sound]; exact .next rfl (.refl _)⟩⟩
  | _ => none

end EvoIR.Core.Symbolic
