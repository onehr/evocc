import EvoIR.Core.ReturnCorrespondence

/-! The source call boundary uses the existing operational call stack. A
successful callee restores the caller's private locals and function identity;
a fault remains in the callee and never executes the caller continuation.
These are composition lemmas, not whole-call-closure admission. -/
namespace EvoIR.Core.Source

/-- Calls may halt inside a different activation. Successful expression exits
still preserve the caller identity and exact post-expression local store. -/
def EvaluatesContext (p : Program) (locals : Locals) (fid : Nat)
    (expr : Expr) (outcome : StateOutcome) : Prop :=
  ∀ k, match outcome.2 with
  | .ok value => Steps p ⟨.expression expr, locals, k, fid⟩
      ⟨.value value, outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFunction,
      Steps p ⟨.expression expr, locals, k, fid⟩
        ⟨.halted (.error fault), failedLocals, failedK, failedFunction⟩

theorem EvaluatesState.inContext {p : Program} {locals : Locals} {fid : Nat}
    {expr : Expr} {outcome : StateOutcome} (path : EvaluatesState p locals fid expr outcome) :
    EvaluatesContext p locals fid expr outcome := by
  intro k
  cases result : outcome.2 with
  | ok value => simpa only [EvaluatesState, result] using path k
  | error fault =>
    simp only [EvaluatesState, result] at path
    obtain ⟨failedK, steps⟩ := path k
    exact ⟨outcome.1, failedK, fid, steps⟩

def ArgumentsRun (p : Program) (locals : Locals) (fid : Nat) (type : IntType)
    (callee : Nat) (pending : List (QualifiedType × Expr)) (reverseValues : List Value)
    (outcome : Locals × Except Fault (List Value)) : Prop :=
  ∀ k, match outcome.2 with
  | .ok values => Steps p
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ type callee pending reverseValues)
      ⟨.call callee type values, outcome.1, k, fid⟩
  | .error fault => ∃ failedLocals failedK failedFunction, Steps p
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ type callee pending reverseValues)
      ⟨.halted (.error fault), failedLocals, failedK, failedFunction⟩

theorem arguments_nil (p : Program) (locals : Locals) (fid : Nat)
    (type : IntType) (callee : Nat) (reverseValues : List Value) :
    ArgumentsRun p locals fid type callee [] reverseValues (locals, .ok reverseValues.reverse) :=
  fun _ => .refl _

theorem arguments_cons_error {p : Program} {locals : Locals} {fid : Nat}
    (type : IntType) (callee : Nat) (destination : QualifiedType) (expr : Expr)
    (pending : List (QualifiedType × Expr)) (reverseValues : List Value)
    (faultLocals : Locals) (fault : Fault)
    (first : EvaluatesContext p locals fid expr (faultLocals, .error fault)) :
    ArgumentsRun p locals fid type callee ((destination, expr) :: pending) reverseValues
      (faultLocals, .error fault) := by
  intro k
  exact first (.callArgument type callee destination pending reverseValues :: k)

theorem arguments_cons_ok {p : Program} {locals next : Locals} {fid : Nat}
    (type : IntType) (callee : Nat) (destination : QualifiedType) (expr : Expr)
    (pending : List (QualifiedType × Expr)) (reverseValues : List Value)
    (value : Value) (outcome : Locals × Except Fault (List Value))
    (first : EvaluatesContext p locals fid expr (next, .ok value))
    (rest : ArgumentsRun p next fid type callee pending
      (value.cast destination.type :: reverseValues) outcome) :
    ArgumentsRun p locals fid type callee ((destination, expr) :: pending) reverseValues outcome := by
  intro k
  have prefixPath := first (.callArgument type callee destination pending reverseValues :: k)
  have resumed : Step p
      ⟨.value value, next, .callArgument type callee destination pending reverseValues :: k, fid⟩
      (beginArguments ⟨.flow .normal, next, k, fid⟩ type callee pending
        (value.cast destination.type :: reverseValues)) := by
    cases pending <;> rfl
  cases result : outcome.2 with
  | error fault =>
    simp only [ArgumentsRun, result] at rest
    obtain ⟨ls, failedK, failedFid, path⟩ := rest k
    exact ⟨ls, failedK, failedFid, prefixPath.trans (.next resumed path)⟩
  | ok values =>
    simp only [ArgumentsRun, result] at rest
    exact prefixPath.trans (.next resumed (rest k))

def Invokes (p : Program) (callerLocals : Locals) (caller : Nat)
    (callee : Function) (args : List Value) (result : Result) : Prop :=
  ∀ k, match result with
  | .ok value => Steps p
      ⟨.call callee.symbol callee.returnType.type args, callerLocals, k, caller⟩
      ⟨.value value, callerLocals, k, caller⟩
  | .error fault => ∃ failedLocals failedK failedFunction, Steps p
      ⟨.call callee.symbol callee.returnType.type args, callerLocals, k, caller⟩
      ⟨.halted (.error fault), failedLocals, failedK, failedFunction⟩

theorem invoke_body (p : Program) (callee : Function) (args : List Value)
    (calleeLocals callerLocals : Locals) (caller : Nat) (outcome : StateOutcome)
    (found : p.functions.find? (·.symbol == callee.symbol) = some callee)
    (parameters : parameterLocals callee.parameters args = .ok calleeLocals)
    (body : ReturnsState p calleeLocals callee.symbol callee.body outcome)
    (typed : ∀ value, outcome.2 = .ok value → value.type = callee.returnType.type) :
    Invokes p callerLocals caller callee args outcome.2 := by
  intro k
  let frame := Frame.function (some (caller, callerLocals)) callee.returnType.type
  have entry : Step p
      ⟨.call callee.symbol callee.returnType.type args, callerLocals, k, caller⟩
      ⟨.statement callee.body, calleeLocals, frame :: k, callee.symbol⟩ := by
    simp [Step, step, enterFunction, found, parameters, frame]
  have path := body (frame :: k)
  cases result : outcome.2 with
  | error fault =>
    simp only [result] at path ⊢
    obtain ⟨failedK, failed⟩ := path
    exact ⟨outcome.1, failedK, callee.symbol, .next entry failed⟩
  | ok value =>
    simp only [result] at path ⊢
    exact (Steps.next entry path).trans (.next (by
      simp [Step, step, resumeFlow, frame, typed value result]) (.refl _))

theorem evaluates_call {p : Program} {locals next : Locals} {fid : Nat}
    (callee : Function) (arguments : List (QualifiedType × Expr)) (values : List Value)
    (result : Result)
    (evaluated : ArgumentsRun p locals fid callee.returnType.type callee.symbol arguments []
      (next, .ok values))
    (invoked : Invokes p next fid callee values result) :
    EvaluatesContext p locals fid (.call callee.returnType.type callee.symbol arguments)
      (next, result) := by
  intro k
  have start : Step p
      ⟨.expression (.call callee.returnType.type callee.symbol arguments), locals, k, fid⟩
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ callee.returnType.type callee.symbol arguments []) := by
    cases arguments <;> rfl
  have prefixPath := Steps.next start (evaluated k)
  cases result with
  | ok value => exact prefixPath.trans (invoked k)
  | error fault =>
    obtain ⟨ls, failedK, failedFid, path⟩ := invoked k
    exact ⟨ls, failedK, failedFid, prefixPath.trans path⟩

theorem evaluates_call_argument_error {p : Program} {locals : Locals} {fid : Nat}
    (type : IntType) (callee : Nat) (arguments : List (QualifiedType × Expr))
    (faultLocals : Locals) (fault : Fault)
    (evaluated : ArgumentsRun p locals fid type callee arguments [] (faultLocals, .error fault)) :
    EvaluatesContext p locals fid (.call type callee arguments) (faultLocals, .error fault) := by
  intro k
  have start : Step p ⟨.expression (.call type callee arguments), locals, k, fid⟩
      (beginArguments ⟨.flow .normal, locals, k, fid⟩ type callee arguments []) := by
    cases arguments <;> rfl
  obtain ⟨ls, failedK, failedFid, path⟩ := evaluated k
  exact ⟨ls, failedK, failedFid, .next start path⟩

end EvoIR.Core.Source

namespace EvoIR.Core.Symbolic

/-- Reuse a proved source body under a real caller continuation. This does not
erase the call, replace the original source, or claim a target call proof. -/
theorem ReturnDiagram.invokes {Γ : Var → Nat} (σ : Subst Γ)
    {source : Source.Function} {target : IR.Function} {args : List (CWord Γ)}
    (diagram : ReturnDiagram source target args) (p : Source.Program)
    (found : p.functions.find? (·.symbol == source.symbol) = some source)
    (callerLocals : Source.Locals) (caller : Nat) :
    Source.Invokes p callerLocals caller source (args.map (CWord.denote σ))
      (diagram.tree.denote σ).2 :=
  Source.invoke_body p source (args.map (CWord.denote σ)) (diagram.locals.denote σ)
    callerLocals caller (diagram.tree.denote σ) found
    (parameter_store_sound σ source.parameters args diagram.locals diagram.storage)
    (diagram.body σ p source.symbol) (diagram.typed σ)

end EvoIR.Core.Symbolic
