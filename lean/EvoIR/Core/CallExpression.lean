import EvoIR.Core.SourceCall

/-! Program-relative source diagrams. Calls keep their actual activation and
continuation; the numeric trees summarize results, not replacement source.
Traversal fuel bounds checker construction, never source execution. -/
namespace EvoIR.Core.Source

theorem context_cast {p : Program} {locals : Locals} {fid : Nat}
    (destination : QualifiedType) (expr : Expr) (outcome : StateOutcome)
    (path : EvaluatesContext p locals fid expr outcome) :
    EvaluatesContext p locals fid (.cast destination.type destination expr)
      (outcome.1, outcome.2.map (Value.cast destination.type)) := by
  cases result : outcome.2 with
  | error fault =>
    intro k
    simp only [EvaluatesContext, result] at path
    obtain ⟨ls, failedK, failedFid, steps⟩ := path (.convert destination.type :: k)
    exact ⟨ls, failedK, failedFid, .next (by
      simp [Step, step, enterExpression, wrappingConstant]) steps⟩
  | ok value =>
    have old : EvaluatesState p locals fid expr (outcome.1, .ok value) := by
      simpa only [EvaluatesContext, EvaluatesState, result] using path
    simpa only [result] using (state_cast destination expr _ old).inContext

theorem context_binary_error {p : Program} {locals finalLocals : Locals} {fid : Nat}
    (type : IntType) (op : Binary) (left right : Expr) (fault : Fault)
    (noWrap : wrappingConstant (.binary type op left right) = none)
    (path : EvaluatesContext p locals fid left (finalLocals, .error fault)) :
    EvaluatesContext p locals fid (.binary type op left right) (finalLocals, .error fault) := by
  intro k
  obtain ⟨ls, failedK, failedFid, steps⟩ := path (.binaryLeft type op right :: k)
  exact ⟨ls, failedK, failedFid, .next (by simp [Step, step, enterExpression, noWrap]) steps⟩

theorem context_binary_ok {p : Program} {locals middle : Locals} {fid : Nat}
    (type : IntType) (op : Binary) (left right : Expr) (value : Value) (outcome : StateOutcome)
    (noWrap : wrappingConstant (.binary type op left right) = none)
    (continueRight : (op == .and && !value.truth) = false ∧ (op == .or && value.truth) = false)
    (first : EvaluatesContext p locals fid left (middle, .ok value))
    (second : EvaluatesContext p middle fid right outcome)
    (typed : ∀ v result, outcome.2 = .ok v → binaryValue op value v = .ok result → result.type = type) :
    EvaluatesContext p locals fid (.binary type op left right)
      (outcome.1, do let v ← outcome.2; binaryValue op value v) := by
  cases result : outcome.2 with
  | ok v =>
    have old : EvaluatesState p middle fid right (outcome.1, .ok v) := by
      simpa only [EvaluatesContext, EvaluatesState, result] using second
    simpa only [result] using (state_binary_ok type op left right value _ noWrap continueRight
      first old (by simpa only [result] using typed)).inContext
  | error fault =>
    intro k
    simp only [EvaluatesContext, result] at second
    obtain ⟨ls, failedK, failedFid, tail⟩ := second (.binaryRight type op value :: k)
    have enter : Step p ⟨.expression (.binary type op left right), locals, k, fid⟩
        ⟨.expression left, locals, .binaryLeft type op right :: k, fid⟩ := by
      simp [Step, step, enterExpression, noWrap]
    have next : Step p ⟨.value value, middle, .binaryLeft type op right :: k, fid⟩
        ⟨.expression right, middle, .binaryRight type op value :: k, fid⟩ := by
      simp [Step, step, resumeValue, continueRight.1, continueRight.2]
    exact ⟨ls, failedK, failedFid,
      .next enter ((first (.binaryLeft type op right :: k)).trans (.next next tail))⟩

def ReturnsContext (p : Program) (locals : Locals) (fid : Nat) (body : Stmt)
    (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok value => Steps p ⟨.statement body, locals, k, fid⟩
      ⟨.flow (.returned value), outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFunction,
      Steps p ⟨.statement body, locals, k, fid⟩
        ⟨.halted (.error fault), failedLocals, failedK, failedFunction⟩

theorem ReturnsState.inContext {p : Program} {locals : Locals} {fid : Nat}
    {body : Stmt} {outcome : StateOutcome} (path : ReturnsState p locals fid body outcome) :
    ReturnsContext p locals fid body outcome := by
  intro k
  cases result : outcome.2 with
  | ok value => simpa only [ReturnsState, result] using path k
  | error fault =>
    simp only [ReturnsState, result] at path
    obtain ⟨failedK, steps⟩ := path k
    exact ⟨outcome.1, failedK, fid, steps⟩

theorem context_return {p : Program} {locals : Locals} {fid : Nat}
    (destination : QualifiedType) (expr : Expr) (outcome : StateOutcome)
    (path : EvaluatesContext p locals fid expr outcome) :
    ReturnsContext p locals fid (.return destination expr)
      (outcome.1, outcome.2.map (Value.cast destination.type)) := by
  intro k
  have enter : Step p ⟨.statement (.return destination expr), locals, k, fid⟩
      ⟨.expression expr, locals, .return destination :: k, fid⟩ := rfl
  cases result : outcome.2 with
  | error fault =>
    simp only [EvaluatesContext, result] at path
    obtain ⟨ls, failedK, failedFid, steps⟩ := path (.return destination :: k)
    exact ⟨ls, failedK, failedFid, .next enter steps⟩
  | ok value =>
    simp only [EvaluatesContext, result] at path
    exact .next enter ((path (.return destination :: k)).trans (.next rfl (.refl _)))

theorem invoke_context_body (p : Program) (callee : Function) (args : List Value)
    (calleeLocals callerLocals : Locals) (caller : Nat) (outcome : StateOutcome)
    (found : p.functions.find? (·.symbol == callee.symbol) = some callee)
    (parameters : parameterLocals callee.parameters args = .ok calleeLocals)
    (body : ReturnsContext p calleeLocals callee.symbol callee.body outcome)
    (typed : ∀ value, outcome.2 = .ok value → value.type = callee.returnType.type) :
    Invokes p callerLocals caller callee args outcome.2 := by
  intro k
  let frame : Frame := Frame.function (some (caller, callerLocals)) callee.returnType.type
  have enter : Step p
      ⟨.call callee.symbol callee.returnType.type args, callerLocals, k, caller⟩
      ⟨.statement callee.body, calleeLocals, frame :: k, callee.symbol⟩ := by
    simp [Step, step, enterFunction, found, parameters, frame]
  have path := body (frame :: k)
  cases result : outcome.2 with
  | error fault =>
    simp only [result] at path ⊢
    obtain ⟨ls, failedK, failedFid, steps⟩ := path
    exact ⟨ls, failedK, failedFid, .next enter steps⟩
  | ok value =>
    simp only [result] at path ⊢
    exact (Steps.next enter path).trans (.next (by
      simp [Step, step, resumeFlow, frame, typed value result]) (.refl _))

/-- Argument evaluation followed by the actual call. Failure is allowed in
either an argument activation or the final callee, without executing later args. -/
def CallsContext (p : Program) (locals : Locals) (fid : Nat) (type : IntType)
    (callee : Nat) (pending : List (QualifiedType × Expr)) (reverseValues : List Value)
    (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok value => Steps p
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ type callee pending reverseValues)
      ⟨.value value, outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFunction, Steps p
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ type callee pending reverseValues)
      ⟨.halted (.error fault), failedLocals, failedK, failedFunction⟩

theorem calls_cons_error {p : Program} {locals : Locals} {fid : Nat}
    (type : IntType) (callee : Nat) (destination : QualifiedType) (expr : Expr)
    (pending : List (QualifiedType × Expr)) (reverseValues : List Value)
    (faultLocals : Locals) (fault : Fault)
    (first : EvaluatesContext p locals fid expr (faultLocals, .error fault)) :
    CallsContext p locals fid type callee ((destination, expr) :: pending) reverseValues
      (faultLocals, .error fault) :=
  fun k => first (.callArgument type callee destination pending reverseValues :: k)

theorem calls_cons_ok {p : Program} {locals next : Locals} {fid : Nat}
    (type : IntType) (callee : Nat) (destination : QualifiedType) (expr : Expr)
    (pending : List (QualifiedType × Expr)) (reverseValues : List Value)
    (value : Value) (outcome : StateOutcome)
    (first : EvaluatesContext p locals fid expr (next, .ok value))
    (rest : CallsContext p next fid type callee pending
      (value.cast destination.type :: reverseValues) outcome) :
    CallsContext p locals fid type callee ((destination, expr) :: pending) reverseValues outcome := by
  intro k
  have headPath := first (.callArgument type callee destination pending reverseValues :: k)
  have resumed : Step p
      ⟨.value value, next, .callArgument type callee destination pending reverseValues :: k, fid⟩
      (beginArguments ⟨.flow .normal, next, k, fid⟩ type callee pending
        (value.cast destination.type :: reverseValues)) := by
    cases pending <;> rfl
  cases result : outcome.2 with
  | error fault =>
    simp only [CallsContext, result] at rest
    obtain ⟨ls, failedK, failedFid, path⟩ := rest k
    exact ⟨ls, failedK, failedFid, headPath.trans (.next resumed path)⟩
  | ok value =>
    simp only [CallsContext, result] at rest
    exact headPath.trans (.next resumed (rest k))

theorem context_call {p : Program} {locals : Locals} {fid : Nat}
    (type : IntType) (callee : Nat) (arguments : List (QualifiedType × Expr))
    (outcome : StateOutcome)
    (called : CallsContext p locals fid type callee arguments [] outcome) :
    EvaluatesContext p locals fid (.call type callee arguments) outcome := by
  intro k
  have enter : Step p ⟨.expression (.call type callee arguments), locals, k, fid⟩
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ type callee arguments []) := by
    cases arguments <;> rfl
  cases result : outcome.2 with
  | error fault =>
    simp only [CallsContext, result] at called
    obtain ⟨ls, failedK, failedFid, path⟩ := called k
    exact ⟨ls, failedK, failedFid, .next enter path⟩
  | ok value =>
    simp only [CallsContext, result] at called
    exact .next enter (called k)

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

def ContextDiagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (expr : Source.Expr) (tree : StateTree Γ) : Prop :=
  ∀ σ fid, Source.EvaluatesContext p (locals.denote σ) fid expr (tree.denote σ)

theorem context_binary_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (op : Source.Binary) (left right : Source.Expr)
    (first output : StateTree Γ)
    (noWrap : Source.wrappingConstant (.binary type op left right) = none)
    (notAnd : op ≠ .and) (notOr : op ≠ .or)
    (a : ContextDiagram p locals left first)
    (b : TreeBind (fun next word tree => ∃ middle, ContextDiagram p next right middle ∧
      TreeBind (BinaryContinuation type op word) middle tree) first output) :
    ContextDiagram p locals (.binary type op left right) output := by
  intro σ fid
  rcases b.outcome σ with ⟨finalLocals, fault, before, after⟩ |
      ⟨next, word, tree, ⟨middle, diagram, applied⟩, before, after⟩
  · rw [after]
    exact Source.context_binary_error type op left right fault noWrap
      (by simpa only [before] using a σ fid)
  · rw [after]
    obtain ⟨result, typed⟩ := binary_continuation_result σ type op word middle tree applied
    rw [result]
    exact Source.context_binary_ok type op left right (word.denote σ) (middle.denote σ) noWrap
      (by simp [notAnd, notOr]) (by simpa only [before] using a σ fid) (diagram σ fid) typed

def InvocationDiagram {Γ : Var → Nat} (p : Source.Program) (f : Source.Function)
    (args : List (CWord Γ)) (tree : ValueTree Γ) : Prop :=
  ∀ σ locals fid, Source.Invokes p locals fid f (args.map (CWord.denote σ)) (tree.denote σ)

def ArgumentsDiagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (callee : Nat) (pending : List (Source.QualifiedType × Source.Expr))
    (reverseWords : List (CWord Γ)) (tree : StateTree Γ) : Prop :=
  ∀ σ fid, Source.CallsContext p (locals.denote σ) fid type callee pending
    (reverseWords.map (CWord.denote σ)) (tree.denote σ)

def ValueTree.withLocals {Γ : Var → Nat} (locals : LocalStore Γ) : ValueTree Γ → StateTree Γ
  | .value word => .value locals word
  | .checked guard next => .checked locals guard (next.withLocals locals)
  | .branch condition yes no => .branch condition (yes.withLocals locals) (no.withLocals locals)

theorem ValueTree.withLocals_sound {Γ : Var → Nat} (σ : Subst Γ) (locals : LocalStore Γ)
    (tree : ValueTree Γ) : (tree.withLocals locals).denote σ = (locals.denote σ, tree.denote σ) := by
  induction tree with
  | value word => rfl
  | checked guard next ih =>
    simp only [withLocals, StateTree.denote, denote]
    cases guards σ [guard] with
    | error fault => rfl
    | ok _ => exact ih
  | branch condition yes no a b =>
    simp only [withLocals, StateTree.denote, denote]
    split <;> assumption

theorem arguments_cons_diagram {Γ : Var → Nat} (p : Source.Program) (locals : LocalStore Γ)
    (type : Source.IntType) (callee : Nat) (destination : Source.QualifiedType) (expr : Source.Expr)
    (pending : List (Source.QualifiedType × Source.Expr)) (reverseWords : List (CWord Γ))
    (first output : StateTree Γ) (a : ContextDiagram p locals expr first)
    (b : TreeBind (fun next word tree => ArgumentsDiagram p next type callee pending
      (word.cast destination.type :: reverseWords) tree) first output) :
    ArgumentsDiagram p locals type callee ((destination, expr) :: pending) reverseWords output := by
  intro σ fid
  rcases b.outcome σ with ⟨ls, fault, before, after⟩ |
      ⟨next, word, tree, continuation, before, after⟩
  · rw [after]
    exact Source.calls_cons_error type callee destination expr pending _ ls fault
      (by simpa only [before] using a σ fid)
  · rw [after]
    apply Source.calls_cons_ok type callee destination expr pending _ (word.denote σ)
      (tree.denote σ) (by simpa only [before] using a σ fid)
    simpa only [List.map_cons, cast_sound] using continuation σ fid

end EvoIR.Core.Symbolic
