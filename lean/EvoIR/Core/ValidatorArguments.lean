import EvoIR.Core.ValidatorCallTransfer
import EvoIR.Core.ValidatorFacts

/-! Preservation of the actual call-expression and call-argument transitions.
Argument syntax is checked independently in the original validator input;
the concrete machine evaluates it in order, retaining earlier initialization. -/
namespace EvoIR.Core.Source

private theorem checked_argument_step (callees : List Function) (fuel : Nat)
    (input : LocalState) (accumulated output : ExpressionFacts)
    (destination : QualifiedType) (expression : Expr) (parameter : Parameter)
    (accepted : validatorArgumentStep callees fuel input accumulated
      ((destination, expression), parameter) = .ok output) :
    ∃ current, destination = parameter.type ∧
      checkExpr callees fuel input expression = .ok current ∧
      output = accessUnion accumulated current (union accumulated.initialized current.initialized) := by
  unfold validatorArgumentStep at accepted
  obtain ⟨checkedUnit, conversion, remaining⟩ := except_bind_success _ _ _ accepted
  cases checkedUnit
  obtain ⟨current, checked, remaining⟩ := except_bind_success _ _ _ remaining
  obtain ⟨checkedUnit, _, returned⟩ := except_bind_success _ _ _ remaining
  cases checkedUnit
  exact ⟨current, by simpa using require_success _ _ conversion, checked,
    by simpa [pure, Except.pure] using returned.symm⟩

private theorem checked_call_facts (callees : List Function) (fuel : Nat)
    (input : LocalState) (type : IntType) (callee : Nat)
    (arguments : List (QualifiedType × Expr)) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.call type callee arguments) = .ok facts) :
    ∃ innerFuel function, fuel = innerFuel + 1 ∧
      callees.find? (·.symbol == callee) = some function ∧
      type = function.returnType.type ∧ arguments.length = function.parameters.length ∧
      (arguments.zip function.parameters).foldlM
        (validatorArgumentStep callees innerFuel input) ⟨input.initialized, [], []⟩ = .ok facts := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    cases found : callees.find? (·.symbol == callee) with
    | none =>
      simp [checkExpr, found, throw, throwThe, MonadExceptOf.throw] at accepted
    | some function =>
      simp only [checkExpr, found] at accepted
      obtain ⟨checkedUnit, _, remaining⟩ := except_bind_success _ _ _ accepted
      cases checkedUnit
      obtain ⟨checkedUnit, signature, fold⟩ := except_bind_success _ _ _ remaining
      cases checkedUnit
      have signature : type = function.returnType.type ∧
          arguments.length = function.parameters.length := by
        simpa using require_success _ _ signature
      have folded : (arguments.zip function.parameters).foldlM
          (validatorArgumentStep callees fuel input) ⟨input.initialized, [], []⟩ = .ok facts := fold
      exact ⟨fuel, function, rfl, rfl, signature.1, signature.2, folded⟩

private theorem source_function_lookup (functions : List Function) (function : Function)
    (distinct : (functions.map Function.symbol).Nodup) (member : function ∈ functions) :
    functions.find? (·.symbol == function.symbol) = some function := by
  induction functions with
  | nil => cases member
  | cons first rest ih =>
    obtain ⟨fresh, distinct⟩ := List.nodup_cons.mp distinct
    rcases List.mem_cons.mp member with same | member
    · subst function
      simp [List.find?]
    · have different : first.symbol ≠ function.symbol := by
        intro same
        have present : function.symbol ∈ rest.map Function.symbol := List.mem_map_of_mem member
        exact fresh (same.symm ▸ present)
      simpa [List.find?, beq_eq_false_iff_ne.mpr different] using ih distinct member

private theorem arguments_head (program : Program) (callees : List Function)
    (type : IntType) (callee : Nat) (destination : QualifiedType) (expression : Expr)
    (remaining : List (QualifiedType × Expr)) (reverseValues : List Value)
    (task : ValidatorArgumentsTask program callees type callee
      ((destination, expression) :: remaining) reverseValues) :
    ∃ parameter parameters current,
      task.remainingParameters = parameter :: parameters ∧
      destination = parameter.type ∧
      checkExpr callees task.fuel task.checkedInput expression = .ok current ∧
      (remaining.zip parameters).foldlM
        (validatorArgumentStep callees task.fuel task.checkedInput)
        (accessUnion task.accumulated current
          (union task.accumulated.initialized current.initialized)) = .ok task.facts := by
  cases parameters : task.remainingParameters with
  | nil =>
    have arity := task.arity
    simp [parameters] at arity
  | cons parameter rest =>
    have checked := task.accepted
    rw [parameters] at checked
    simp only [List.zip_cons_cons, List.foldlM_cons] at checked
    obtain ⟨next, step, tail⟩ := except_bind_success _ _ _ checked
    obtain ⟨current, conversion, checked, nextEq⟩ := checked_argument_step
      callees task.fuel task.checkedInput task.accumulated next destination expression parameter step
    rw [nextEq] at tail
    exact ⟨parameter, rest, current, rfl, conversion, checked, tail⟩

theorem validator_begin_arguments_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (type : IntType) (callee : Nat) (remaining : List (QualifiedType × Expr))
    (reverseValues : List Value)
    (task : ValidatorArgumentsTask program callees type callee remaining reverseValues)
    (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (beginArguments state type callee remaining reverseValues) := by
  cases remaining with
  | nil =>
    have parameters : task.remainingParameters = [] := List.eq_nil_of_length_eq_zero (by
      simpa using task.arity.symm)
    have facts : task.facts = task.accumulated := by
      have accepted := task.accepted
      simp only [List.zip_nil_left, List.foldlM_nil] at accepted
      exact Except.ok.inj accepted.symm
    have entails : task.environment.Entails (task.base.afterExpression task.facts) := by
      refine ⟨task.preserves.bindings, task.preserves.layout, ?_, task.preserves.fresh⟩
      intro id member
      rcases mem_validator_union id _ _ |>.mp member with incoming | accumulated
      · exact task.preserves.initialized id incoming
      · exact task.accumulatedInitialized id (facts ▸ accumulated)
    have outgoing := models.weaken task.environment (task.base.afterExpression task.facts) entails
    let callTask : ValidatorCallTask program callee type reverseValues.reverse := {
      function := task.function
      defined := task.defined
      checked := task.checked
      result := task.result
      typedArguments := by simpa [parameters] using task.typedPrefix
      environment := task.base.afterExpression task.facts }
    exact .running _ task.output checkedProgram
      (.call callee type reverseValues.reverse callTask state.locals outgoing) tail
  | cons argument remaining =>
    rcases argument with ⟨destination, expression⟩
    obtain ⟨_, _, current, _, _, checked, _⟩ := arguments_head
      program callees type callee destination expression remaining reverseValues task
    let checkedArgument : CheckedExpression callees task.checkedInput expression :=
      ⟨task.fuel, current, checked⟩
    let child : ValidatorExpressionTask callees expression := {
      environment := task.environment
      checkedInput := task.checkedInput
      checked := checkedArgument
      bindings := fun id => (task.preserves.bindings id).trans (task.bindings id)
      initialized := fun id member => task.preserves.initialized id (task.initialized id member) }
    exact .running _ child.output checkedProgram
      (.expression callees expression context child state.locals models)
      (.frame (.argument state.functionId callees type callee destination expression remaining
        reverseValues context task checkedArgument rfl) tail)

theorem validator_call_argument_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (type : IntType) (callee : Nat) (destination : QualifiedType) (expression : Expr)
    (remaining : List (QualifiedType × Expr)) (reverseValues : List Value)
    (task : ValidatorArgumentsTask program callees type callee
      ((destination, expression) :: remaining) reverseValues)
    (checkedArgument : CheckedExpression callees task.checkedInput expression)
    (fuel : checkedArgument.fuel = task.fuel) (value : Value)
    (models : (task.environment.afterExpression checkedArgument.facts).Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeValue state (.callArgument type callee destination remaining reverseValues) value) := by
  obtain ⟨parameter, parameters, current, parameterList, conversion, checked, remainingCheck⟩ :=
    arguments_head program callees type callee destination expression remaining reverseValues task
  have headChecked := checkedArgument.accepted
  rw [fuel] at headChecked
  have same : current = checkedArgument.facts := Except.ok.inj (checked.symm.trans headChecked)
  subst current
  let after := task.environment.afterExpression checkedArgument.facts
  let next : ValidatorArgumentsTask program callees type callee remaining
      (value.cast destination.type :: reverseValues) := {
    function := task.function
    defined := task.defined
    checked := task.checked
    result := task.result
    remainingParameters := parameters
    arity := by simpa [parameterList] using task.arity
    typedPrefix := by
      have typed := task.typedPrefix
      rw [parameterList] at typed
      simpa [List.reverse_cons, List.map_append, List.append_assoc, conversion,
        Value.cast, Value.ofInt] using typed
    checkedInput := task.checkedInput
    fuel := task.fuel
    accumulated := accessUnion task.accumulated checkedArgument.facts
      (union task.accumulated.initialized checkedArgument.facts.initialized)
    facts := task.facts
    accepted := remainingCheck
    base := task.base
    environment := after
    preserves := {
      bindings := task.preserves.bindings
      layout := task.preserves.layout
      initialized := fun id member => mem_validator_union id _ _ |>.mpr
        (Or.inl (task.preserves.initialized id member))
      fresh := task.preserves.fresh }
    bindings := task.bindings
    initialized := task.initialized
    accumulatedInitialized := by
      intro id member
      rcases mem_validator_union id _ _ |>.mp member with prior | current
      · exact mem_validator_union id _ _ |>.mpr (Or.inl (task.accumulatedInitialized id prior))
      · exact mem_validator_union id _ _ |>.mpr (Or.inr current) }
  exact validator_begin_arguments_preserves program entryType checkedProgram state callees context
    type callee remaining (value.cast destination.type :: reverseValues) next models tail

theorem validator_call_expression_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (type : IntType) (callee : Nat) (arguments : List (QualifiedType × Expr))
    (parent : ValidatorExpressionTask callees (.call type callee arguments))
    (models : parent.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId parent.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (enterExpression state (.call type callee arguments)) := by
  obtain ⟨fuel, function, _, found, result, arity, accepted⟩ := checked_call_facts callees
    parent.checked.fuel parent.checkedInput type callee arguments parent.checked.facts parent.checked.accepted
  have member : function ∈ program.functions := by
    rw [context.checked.order]
    apply List.mem_append_left
    rw [context.preceding]
    exact List.mem_of_find?_eq_some found
  have symbol : function.symbol = callee := by simpa using List.find?_some found
  have defined : program.functions.find? (·.symbol == callee) = some function := by
    simpa only [symbol] using source_function_lookup program.functions function checkedProgram.symbols member
  let task : ValidatorArgumentsTask program callees type callee arguments [] := {
    function
    defined
    checked := checkedProgram.functions function member
    result := result.symm
    remainingParameters := function.parameters
    arity
    typedPrefix := by simp
    checkedInput := parent.checkedInput
    fuel
    accumulated := ⟨parent.checkedInput.initialized, [], []⟩
    facts := parent.checked.facts
    accepted
    base := parent.environment
    environment := parent.environment
    preserves := ⟨fun _ => rfl, rfl, fun _ member => member, fun _ member => member⟩
    bindings := parent.bindings
    initialized := parent.initialized
    accumulatedInitialized := parent.initialized }
  have preserved := validator_begin_arguments_preserves program entryType checkedProgram state
    callees context type callee arguments [] task models tail
  simpa only [enterExpression, wrappingConstant] using preserved

end EvoIR.Core.Source
