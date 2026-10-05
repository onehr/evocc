import EvoIR.Core.ValidatorPrimitiveTransfer

/-! Actual loop tests, body exits, increments and unbounded backedges preserve
the concrete machine invariant. The finite loop package contains checker facts
and store entailments only; it assumes no correctness of a future iteration. -/
namespace EvoIR.Core.Source

theorem validator_loop_start_condition (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (loop : ValidatorLoop callees) (result : loop.result = context.function.returnType)
    (models : loop.before.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId loop.output state.continuation) :
    ValidatorMachineInvariant program entryType {state with
      control := .expression loop.condition,
      continuation := .loopTest loop.condition loop.increment loop.body :: state.continuation} := by
  exact .running _ loop.conditionTask.output checked
    (.expression callees loop.condition context loop.conditionTask _
      (models.weaken loop.before loop.conditionTask.environment loop.conditionBefore))
    (.frame (.loopTest state.functionId callees context loop result) tail)

theorem validator_loop_start_increment (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (loop : ValidatorLoop callees) (result : loop.result = context.function.returnType)
    (expression : Expr) (present : loop.increment = some expression)
    (models : (loop.incrementTask expression present).environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId loop.output state.continuation) :
    ValidatorMachineInvariant program entryType {state with
      control := .expression expression,
      continuation := .loopIncrement loop.condition expression loop.body :: state.continuation} := by
  exact .running _ (loop.incrementTask expression present).output checked
    (.expression callees expression context (loop.incrementTask expression present) _ models)
    (.frame (.loopIncrement state.functionId callees context loop result expression present) tail)

theorem validator_loop_test_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (loop : ValidatorLoop callees) (result : loop.result = context.function.returnType) (value : Value)
    (delivered : loop.conditionTask.output.Models (.value value) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId loop.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeValue state (.loopTest loop.condition loop.increment loop.body) value) := by
  change value.type = loop.condition.type ∧
    (loop.conditionTask.environment.afterExpression loop.conditionTask.checked.facts).Models state.locals
    at delivered
  by_cases truth : value.truth = true
  · simp only [resumeValue, truth, ↓reduceIte]
    exact .running _ loop.bodyTask.output checked
      (.statement callees loop.body context loop.bodyTask (loop.bodyResult.trans result) _
        (delivered.2.weaken _ _ loop.testedBody))
      (.frame (.loopBody state.functionId callees context loop result) tail)
  · simp only [resumeValue, truth, ↓reduceIte]
    exact .running _ loop.output checked
      (.delivered _ _ _ ⟨rfl, delivered.2.weaken _ _ loop.testedExit⟩) tail

theorem validator_loop_increment_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (loop : ValidatorLoop callees) (result : loop.result = context.function.returnType)
    (expression : Expr) (present : loop.increment = some expression) (value : Value)
    (delivered : (loop.incrementTask expression present).output.Models (.value value) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId loop.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeValue state (.loopIncrement loop.condition expression loop.body) value) := by
  change value.type = expression.type ∧
    ((loop.incrementTask expression present).environment.afterExpression
      (loop.incrementTask expression present).checked.facts).Models state.locals at delivered
  have models := delivered.2.weaken _ _ (loop.incrementAfter expression present)
  have transferred := validator_loop_start_condition program entryType checked state callees context
    loop result models tail
  simpa [resumeValue, present] using transferred

theorem validator_loop_body_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (loop : ValidatorLoop callees) (result : loop.result = context.function.returnType) (flow : Flow)
    (delivered : loop.bodyTask.output.Models (.flow flow) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId loop.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeFlow state (.loopBody loop.condition loop.increment loop.body) flow) := by
  cases flow with
  | normal =>
    change loop.bodyTask.checked.facts.fallsThrough = true ∧
      loop.bodyTask.outputEnvironment.Models state.locals at delivered
    cases present : loop.increment with
    | none =>
      have models := delivered.2.weaken _ _ loop.bodyNormalBefore
      simpa [resumeFlow, present] using validator_loop_start_condition program entryType checked state
        callees context loop result models tail
    | some expression =>
      have models := delivered.2.weaken _ _ (loop.incrementNormal expression present)
      simpa [resumeFlow, present] using validator_loop_start_increment program entryType checked state
        callees context loop result expression present models tail
  | «continue» =>
    change statementAllowsAbrupt loop.body = true ∧ loop.bodyTask.depth > 0 ∧
      loop.bodyTask.environment.Models state.locals at delivered
    cases present : loop.increment with
    | none =>
      have models := delivered.2.2.weaken _ _ loop.bodyBefore
      simpa [resumeFlow, present] using validator_loop_start_condition program entryType checked state
        callees context loop result models tail
    | some expression =>
      have models := delivered.2.2.weaken _ _ (loop.incrementBefore expression present)
      simpa [resumeFlow, present] using validator_loop_start_increment program entryType checked state
        callees context loop result expression present models tail
  | «break» =>
    change statementAllowsAbrupt loop.body = true ∧ loop.bodyTask.depth > 0 ∧
      loop.bodyTask.environment.Models state.locals at delivered
    exact .running _ loop.output checked
      (.delivered _ _ _ ⟨rfl, delivered.2.2.weaken _ _ loop.bodyExit⟩) tail
  | returned value =>
    change statementAllowsAbrupt loop.body = true ∧ value.type = loop.bodyTask.result.type ∧
      loop.bodyTask.environment.Models state.locals at delivered
    refine .running _ loop.output checked (.delivered _ _ _ ?_) tail
    exact ⟨rfl, delivered.2.1.trans (congrArg QualifiedType.type loop.bodyResult),
      delivered.2.2.weaken _ _ loop.bodyBefore⟩

theorem validator_start_loop_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (context : ValidatorFunctionContext program state.functionId callees)
    (loop : ValidatorLoop callees) (result : loop.result = context.function.returnType)
    (input : ValidatorFlowProtocol) (after : input.normal.Entails loop.before)
    (onlyNormal : input.allowsAbrupt = false) (flow : Flow)
    (delivered : input.Models flow state.locals)
    (tail : ValidatorContinuation program entryType state.functionId loop.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeFlow state (.startLoop loop.condition loop.increment loop.body) flow) := by
  cases flow with
  | normal =>
    change input.fallsThrough = true ∧ input.normal.Models state.locals at delivered
    exact validator_loop_start_condition program entryType checked state callees context loop result
      (delivered.2.weaken _ _ after) tail
  | «break» | «continue» | returned =>
    simp [ValidatorFlowProtocol.Models, onlyNormal] at delivered

end EvoIR.Core.Source
