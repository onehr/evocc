import EvoIR.Core.ValidatorStatementTransfer

/-! Actual branch scheduling, including an absent else and abrupt arms. Normal
join facts are selected only under the delivered arm's real falls-through flag. -/
namespace EvoIR.Core.Source

theorem validator_branch_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (condition : Expr) (yes : Stmt) (no : Option Stmt)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.ite condition yes no))
    (result : task.result = context.function.returnType) (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.ite condition yes no)) := by
  obtain ⟨fuel, facts, a, noFuel, b, fuelEq, accepted, yesAccepted, noAccepted, resultFacts⟩ :=
    checked_branch_statement callees task.result task.checked.fuel task.depth task.checkedInput condition yes no
      task.checked.facts task.checked.accepted
  let child : ValidatorExpressionTask callees condition :=
    ⟨task.environment, task.checkedInput, ⟨fuel, facts, accepted⟩,
      fun id => by simp [lookup, task.bindings], task.initialized⟩
  exact .running _ child.output checked (.expression callees condition context child _ models)
    (.frame (.branch state.functionId callees condition yes no context task result child.checked fuelEq) tail)

theorem validator_branch_value_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (condition : Expr) (yes : Stmt) (no : Option Stmt)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.ite condition yes no))
    (result : task.result = context.function.returnType)
    (checkedCondition : CheckedExpression callees task.checkedInput condition)
    (fuelEq : task.checked.fuel = checkedCondition.fuel + 1) (value : Value)
    (delivered : (ValidatorProtocol.value
      (task.environment.afterExpression checkedCondition.facts) condition.type).Models (.value value) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (resumeValue state (.branch yes no) value) := by
  obtain ⟨_, models⟩ := delivered
  obtain ⟨fuel, c, a, noFuel, b, extractedFuelEq, conditionAccepted, yesAccepted, noAccepted, resultFacts⟩ :=
    checked_branch_statement callees task.result task.checked.fuel task.depth task.checkedInput condition yes no
      task.checked.facts task.checked.accepted
  have sameFuel : fuel = checkedCondition.fuel := by omega
  subst fuel
  have sameFacts : c = checkedCondition.facts :=
    Except.ok.inj (conditionAccepted.symm.trans checkedCondition.accepted)
  subst c
  have selected (arm : Stmt)
      (certificate : CheckedStatement callees task.result task.depth
        {task.checkedInput with initialized := checkedCondition.facts.initialized} arm)
      (falls : certificate.facts.fallsThrough = true → task.checked.facts.fallsThrough = true)
      (initialized : certificate.facts.fallsThrough = true →
        ∀ id ∈ task.checked.facts.state.initialized, id ∈ certificate.facts.state.initialized) :
      ValidatorMachineInvariant program entryType {state with control := .statement arm} := by
    let environment := task.environment.afterExpression checkedCondition.facts
    let child := ValidatorStatementTask.ofChecked callees arm environment
      {task.checkedInput with initialized := checkedCondition.facts.initialized} task.bindings
      (fun id member => mem_validator_union id _ _ |>.mpr (Or.inr member)) task.result task.depth certificate
    have entails : child.output.Entails task.output := by
      apply ValidatorProtocol.restored_flow_entails_when _ _ rfl rfl
      · exact falls
      · intro normal
        refine ⟨?_, ?_, ?_, ?_⟩
        · intro id
          exact (child.output_entails.bindings id).trans (task.output_entails.bindings id).symm
        · exact child.outputLayout.trans task.outputLayout.symm
        · intro id member
          rw [task.outputInitialized] at member
          rw [child.outputInitialized]
          obtain old | fresh := mem_validator_union id _ _ |>.mp member
          · exact mem_validator_union id _ _ |>.mpr
              (Or.inl (mem_validator_union id _ _ |>.mpr (Or.inl old)))
          · exact mem_validator_union id _ _ |>.mpr (Or.inr (initialized normal id fresh))
        · intro id member
          simpa [child.outputFresh, task.outputFresh, child, ValidatorStatementTask.ofChecked,
            environment, ValidatorEnvironment.afterExpression] using member
      · exact task.environment.afterExpression_entails checkedCondition.facts
      · intro _
        rfl
      · rfl
      · rfl
    exact .running _ child.output checked (.statement callees arm context child result _ models)
      (.weaken entails tail)
  cases truth : value.truth with
  | true =>
    simpa [resumeValue, truth] using selected yes ⟨checkedCondition.fuel, a, yesAccepted⟩
      (by intro falls; change a.fallsThrough = true at falls; simp [resultFacts, falls])
      (by
        intro falls id member
        rw [resultFacts] at member
        exact validator_branch_yes_initialized a b falls id member)
  | false =>
    simpa [resumeValue, truth] using selected (no.getD .null) ⟨noFuel, b, noAccepted⟩
      (by intro falls; change b.fallsThrough = true at falls; simp [resultFacts, falls])
      (by
        intro falls id member
        rw [resultFacts] at member
        exact validator_branch_no_initialized a b falls id member)

end EvoIR.Core.Source
