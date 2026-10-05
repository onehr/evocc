import EvoIR.Core.ValidatorStatementFacts
import EvoIR.Core.ValidatorExpressionTransfer
import EvoIR.Core.ValidatorLoopTransfer

/-! Actual simple-statement entry and finite while/do-while packages. The loop
body, condition and backedge certificates are extracted from the existing check. -/
namespace EvoIR.Core.Source

theorem validator_null_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (task : ValidatorStatementTask callees .null) (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state .null) := by
  have facts := checked_null_statement callees task.result task.checked.fuel task.depth task.checkedInput
    task.checked.facts task.checked.accepted
  have after := models.weaken _ _ (task.environment_entails_output (by
    intro id member
    rw [facts] at member
    exact task.initialized id member))
  exact .running _ task.output checked (.delivered _ _ _
    ⟨by simp [ValidatorStatementTask.output, facts], after⟩) tail

theorem validator_abrupt_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (statement : Stmt) (abrupt : statement = .break ∨ statement = .continue)
    (task : ValidatorStatementTask callees statement) (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state statement) := by
  obtain ⟨inLoop, _⟩ := checked_abrupt_statement callees task.result task.checked.fuel task.depth
    task.checkedInput statement task.checked.facts abrupt task.checked.accepted
  rcases abrupt with rfl | rfl
  all_goals
    exact .running _ task.output checked (.delivered _ _ _ ⟨rfl, inLoop, models⟩) tail

theorem validator_expression_statement_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (expression : Expr) (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.expression expression))
    (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.expression expression)) := by
  obtain ⟨fuel, facts, fuelEq, accepted, resultFacts⟩ := checked_expression_statement callees task.result
    task.checked.fuel task.depth task.checkedInput expression task.checked.facts task.checked.accepted
  let child : ValidatorExpressionTask callees expression :=
    ⟨task.environment, task.checkedInput, ⟨fuel, facts, accepted⟩,
      fun id => by simp [lookup, task.bindings], task.initialized⟩
  let environment := task.environment.afterExpression facts
  have normal : environment.Entails task.outputEnvironment := task.afterExpression_entails_output facts (by
    intro id member
    rw [resultFacts] at member
    exact Or.inr member)
  have entails : (ValidatorProtocol.flow
      ⟨environment, environment, true, task.depth, task.result.type, true, false⟩).Entails task.output := by
    apply ValidatorProtocol.restored_flow_entails _ _ rfl rfl
    · intro _
      simp [ValidatorStatementTask.output, resultFacts]
    · exact normal
    · exact task.environment.afterExpression_entails facts
    · intro impossible
      cases impossible
    · rfl
    · rfl
  exact .running _ child.output checked (.expression callees expression context child _ models)
    (.frame (.discard state.functionId environment expression.type task.depth task.result)
      (.weaken entails tail))

theorem validator_return_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (destination : QualifiedType) (expression : Expr)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.return destination expression))
    (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.return destination expression)) := by
  obtain ⟨fuel, facts, fuelEq, signature, accepted, resultFacts⟩ := checked_return_statement callees task.result
    task.checked.fuel task.depth task.checkedInput destination expression task.checked.facts task.checked.accepted
  let child : ValidatorExpressionTask callees expression :=
    ⟨task.environment, task.checkedInput, ⟨fuel, facts, accepted⟩,
      fun id => by simp [lookup, task.bindings], task.initialized⟩
  let environment := task.environment.afterExpression facts
  have normal : environment.Entails task.outputEnvironment := task.afterExpression_entails_output facts (by
    intro id member
    rw [resultFacts] at member
    exact Or.inr member)
  have entails : (ValidatorProtocol.flow
      ⟨environment, environment, false, task.depth, destination.type, true, true⟩).Entails task.output := by
    apply ValidatorProtocol.restored_flow_entails _ _ rfl rfl
    · intro impossible
      cases impossible
    · exact normal
    · exact task.environment.afterExpression_entails facts
    · intro _
      rfl
    · rfl
    · exact congrArg QualifiedType.type signature
  exact .running _ child.output checked (.expression callees expression context child _ models)
    (.frame (.returned state.functionId environment expression.type task.depth destination)
      (.weaken entails tail))

theorem validator_while_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (condition : Expr) (body : Stmt) (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.while condition body))
    (result : task.result = context.function.returnType) (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.while condition body)) := by
  obtain ⟨fuel, conditionFacts, bodyFacts, fuelEq, conditionAccepted, bodyAccepted, resultFacts⟩ :=
    checked_while_statement callees task.result task.checked.fuel task.depth task.checkedInput condition body
      task.checked.facts task.checked.accepted
  let conditionTask : ValidatorExpressionTask callees condition :=
    ⟨task.environment, task.checkedInput, ⟨fuel, conditionFacts, conditionAccepted⟩,
      fun id => by simp [lookup, task.bindings], task.initialized⟩
  let tested := task.environment.afterExpression conditionFacts
  let bodyTask := ValidatorStatementTask.ofChecked callees body tested
    {task.checkedInput with initialized := conditionFacts.initialized}
    task.bindings (fun id member => mem_validator_union id _ _ |>.mpr (Or.inr member))
    task.result (task.depth + 1) ⟨fuel, bodyFacts, bodyAccepted⟩
  let loop : ValidatorLoop callees := {
    condition
    increment := none
    body
    conditionTask
    bodyTask
    incrementTask := by intro expression present; cases present
    before := task.environment
    exit := tested
    result := task.result
    outerDepth := task.depth
    conditionBefore := ValidatorEnvironment.Entails.refl task.environment
    testedExit := ValidatorEnvironment.Entails.refl tested
    testedBody := ValidatorEnvironment.Entails.refl tested
    bodyBefore := task.environment.afterExpression_entails conditionFacts
    bodyExit := ValidatorEnvironment.Entails.refl tested
    bodyNormalBefore := bodyTask.output_entails.trans _ _ _
      (task.environment.afterExpression_entails conditionFacts)
    bodyResult := rfl
    bodyDepth := rfl
    incrementBefore := by intro expression present; cases present
    incrementNormal := by intro expression present; cases present
    incrementAfter := by intro expression present; cases present
  }
  have normal : tested.Entails task.outputEnvironment := task.afterExpression_entails_output conditionFacts (by
    intro id member
    rw [resultFacts] at member
    exact Or.inr member)
  have entails : loop.output.Entails task.output := by
    apply ValidatorProtocol.restored_flow_entails _ _ rfl rfl
    · intro _
      simp [ValidatorStatementTask.output, resultFacts]
    · exact normal
    · exact ValidatorEnvironment.Entails.refl task.environment
    · intro _
      rfl
    · rfl
    · rfl
  exact validator_loop_start_condition program entryType checked state callees context loop result models
    (.weaken entails tail)

theorem validator_do_while_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (body : Stmt) (condition : Expr) (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.doWhile body condition))
    (result : task.result = context.function.returnType) (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.doWhile body condition)) := by
  obtain ⟨fuel, bodyFacts, conditionFacts, fuelEq, bodyAccepted, conditionAccepted, resultFacts⟩ :=
    checked_do_while_statement callees task.result task.checked.fuel task.depth task.checkedInput body condition
      task.checked.facts task.checked.accepted
  let conditionTask : ValidatorExpressionTask callees condition :=
    ⟨task.environment, task.checkedInput, ⟨fuel, conditionFacts, conditionAccepted⟩,
      fun id => by simp [lookup, task.bindings], task.initialized⟩
  let bodyTask := ValidatorStatementTask.ofChecked callees body task.environment task.checkedInput
    task.bindings task.initialized task.result (task.depth + 1) ⟨fuel, bodyFacts, bodyAccepted⟩
  let loop : ValidatorLoop callees := {
    condition
    increment := none
    body
    conditionTask
    bodyTask
    incrementTask := by intro expression present; cases present
    before := task.environment
    exit := task.environment
    result := task.result
    outerDepth := task.depth
    conditionBefore := ValidatorEnvironment.Entails.refl task.environment
    testedExit := task.environment.afterExpression_entails conditionFacts
    testedBody := task.environment.afterExpression_entails conditionFacts
    bodyBefore := ValidatorEnvironment.Entails.refl task.environment
    bodyExit := ValidatorEnvironment.Entails.refl task.environment
    bodyNormalBefore := bodyTask.output_entails
    bodyResult := rfl
    bodyDepth := rfl
    incrementBefore := by intro expression present; cases present
    incrementNormal := by intro expression present; cases present
    incrementAfter := by intro expression present; cases present
  }
  have normal : task.environment.Entails task.outputEnvironment := task.environment_entails_output (by
    intro id member
    rw [resultFacts] at member
    exact task.initialized id member)
  have entails : loop.output.Entails task.output := by
    apply ValidatorProtocol.restored_flow_entails _ _ rfl rfl
    · intro _
      simp [ValidatorStatementTask.output, resultFacts]
    · exact normal
    · exact ValidatorEnvironment.Entails.refl task.environment
    · intro _
      rfl
    · rfl
    · rfl
  exact .running _ bodyTask.output checked
    (.statement callees body context bodyTask result _ models)
    (.frame (.loopBody state.functionId callees context loop result) (.weaken entails tail))

end EvoIR.Core.Source
