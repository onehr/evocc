import EvoIR.Core.ValidatorPrimitiveTransfer
import EvoIR.Core.ValidatorArguments
import EvoIR.Core.ValidatorConstantTransfer

/-! Actual expression entry and suspended expression transitions. Every child
certificate below is inverted from the successful original expression checker;
the runtime environment retains additional facts from already evaluated code. -/
namespace EvoIR.Core.Source

theorem validator_expression_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (expression : Expr) (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorExpressionTask callees expression)
    (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterExpression state expression) := by
  cases wrapped : wrappingConstant expression with
  | some value =>
    obtain ⟨typed, initialized⟩ := wrapping_constant_checked callees task.checked.fuel
      task.checkedInput expression task.checked.facts value task.checked.accepted wrapped
    have after := models.weaken task.environment (task.environment.afterExpression task.checked.facts)
      (task.environment.entails_afterExpression task.checked.facts
        (fun id member => task.initialized id (initialized id member)))
    simpa only [enterExpression, wrapped] using validator_result_transfer program entryType
      checked state _ after expression.type (.ok value) typed tail
  | none =>
    cases expression with
    | literal type bits =>
      obtain ⟨canonical, facts⟩ := checked_literal_facts callees task.checked.fuel task.checkedInput
        type bits task.checked.facts task.checked.accepted
      have after := models.weaken task.environment (task.environment.afterExpression task.checked.facts)
        (task.environment.entails_afterExpression task.checked.facts (by
          intro id member
          rw [facts] at member
          exact task.initialized id member))
      simpa only [enterExpression, wrapped, canonical, ↓reduceIte] using
        validator_result_transfer program entryType checked state _ after type
          (.ok (.ofNat type bits)) rfl tail
    | ident type id =>
      obtain ⟨value, typed, read⟩ := checked_identifier_read callees task.checked.fuel task.checkedInput
        state.locals type id task.checked.facts task.checked.accepted (models.checked_input task)
      have same := checked_identifier_facts callees task.checked.fuel task.checkedInput type id
        task.checked.facts task.checked.accepted
      have after := models.weaken task.environment (task.environment.afterExpression task.checked.facts)
        (task.environment.entails_afterExpression task.checked.facts (by
          intro other member
          rw [same] at member
          exact task.initialized other member))
      simpa only [enterExpression, wrapped, read] using validator_result_transfer program entryType
        checked state _ after type (.ok value) typed tail
    | unary type op operand =>
      obtain ⟨fuel, facts, fuelEq, accepted, signature, resultFacts⟩ := checked_unary_facts callees
        task.checked.fuel task.checkedInput type op operand task.checked.facts task.checked.accepted
      let child : ValidatorExpressionTask callees operand :=
        ⟨task.environment, task.checkedInput, ⟨fuel, facts, accepted⟩, task.bindings, task.initialized⟩
      have continuation : ValidatorContinuation program entryType state.functionId child.output
          (.unary type op :: state.continuation) := by
        rw [signature]
        refine .frame (.unary state.functionId (task.environment.afterExpression facts) operand.type op) ?_
        simpa [child, ValidatorExpressionTask.output, resultFacts, signature, Expr.type] using tail
      simpa only [enterExpression, wrapped] using
        ValidatorMachineInvariant.running
          {state with control := .expression operand, continuation := .unary type op :: state.continuation}
          child.output checked (.expression callees operand context child _ models) continuation
    | binary type op left right =>
      obtain ⟨fuel, a, b, fuelEq, leftAccepted, rightAccepted, signature, admitted, resultFacts⟩ :=
        checked_binary_facts callees task.checked.fuel task.checkedInput type op left right
          task.checked.facts task.checked.accepted
      let child : ValidatorExpressionTask callees left :=
        ⟨task.environment, task.checkedInput, ⟨fuel, a, leftAccepted⟩, task.bindings, task.initialized⟩
      have continuation : ValidatorContinuation program entryType state.functionId child.output
          (.binaryLeft type op right :: state.continuation) :=
        .frame (.binaryLeft state.functionId callees type op left right context task child.checked fuelEq) tail
      simpa only [enterExpression, wrapped] using
        ValidatorMachineInvariant.running
          {state with control := .expression left, continuation := .binaryLeft type op right :: state.continuation}
          child.output checked (.expression callees left context child _ models) continuation
    | conditional type condition yes no =>
      obtain ⟨fuel, c, a, b, fuelEq, conditionAccepted, yesAccepted, noAccepted, resultFacts⟩ :=
        checked_conditional_facts callees task.checked.fuel task.checkedInput type condition yes no
          task.checked.facts task.checked.accepted
      let child : ValidatorExpressionTask callees condition :=
        ⟨task.environment, task.checkedInput, ⟨fuel, c, conditionAccepted⟩, task.bindings, task.initialized⟩
      have continuation : ValidatorContinuation program entryType state.functionId child.output
          (.choice type yes no :: state.continuation) :=
        .frame (.choice state.functionId callees type condition yes no context task child.checked fuelEq) tail
      simpa only [enterExpression, wrapped] using
        ValidatorMachineInvariant.running
          {state with control := .expression condition, continuation := .choice type yes no :: state.continuation}
          child.output checked (.expression callees condition context child _ models) continuation
    | cast type destination operand =>
      obtain ⟨fuel, fuelEq, signature, accepted⟩ := checked_cast_facts callees task.checked.fuel
        task.checkedInput type destination operand task.checked.facts task.checked.accepted
      let child : ValidatorExpressionTask callees operand :=
        ⟨task.environment, task.checkedInput, ⟨fuel, task.checked.facts, accepted⟩,
          task.bindings, task.initialized⟩
      have continuation : ValidatorContinuation program entryType state.functionId child.output
          (.convert type :: state.continuation) :=
        .frame (.convert state.functionId (task.environment.afterExpression task.checked.facts) operand.type type) tail
      have entered : enterExpression state (.cast type destination operand) =
          {state with control := .expression operand, continuation := .convert type :: state.continuation} := by
        simp only [enterExpression, wrapped]
        simp [signature]
      rw [entered]
      exact .running _ child.output checked (.expression callees operand context child _ models) continuation
    | comma type left right =>
      obtain ⟨fuel, a, b, fuelEq, signature, leftAccepted, rightAccepted, resultFacts⟩ :=
        checked_comma_facts callees task.checked.fuel task.checkedInput type left right
          task.checked.facts task.checked.accepted
      let child : ValidatorExpressionTask callees left :=
        ⟨task.environment, task.checkedInput, ⟨fuel, a, leftAccepted⟩, task.bindings, task.initialized⟩
      have continuation : ValidatorContinuation program entryType state.functionId child.output
          (.binaryLeft type .comma right :: state.continuation) :=
        .frame (.commaLeft state.functionId callees type left right context task child.checked fuelEq) tail
      simpa only [enterExpression, wrapped] using
        ValidatorMachineInvariant.running
          {state with control := .expression left, continuation := .binaryLeft type .comma right :: state.continuation}
          child.output checked (.expression callees left context child _ models) continuation
    | assign type op id destination target rhs =>
      obtain ⟨signature, admitted, targetEq, binding, writable, initialized⟩ :=
        checked_assignment_signature callees task.checked.fuel task.checkedInput type op id destination
          target rhs task.checked.facts task.checked.accepted
      obtain ⟨fuel, facts, fuelEq, accepted, resultFacts⟩ := checked_assignment_facts callees
        task.checked.fuel task.checkedInput type op id destination target rhs task.checked.facts task.checked.accepted
      let child : ValidatorExpressionTask callees rhs :=
        ⟨task.environment, task.checkedInput, ⟨fuel, facts, accepted⟩, task.bindings, task.initialized⟩
      let environment := task.environment.afterExpression facts
      let after : ValidatorEnvironment :=
        {environment with state.initialized := id :: environment.state.initialized}
      have entails : after.Entails (task.environment.afterExpression task.checked.facts) := by
        refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
        intro other member
        obtain old | fresh := mem_validator_union other _ _ |>.mp member
        · exact List.mem_cons_of_mem _ (mem_validator_union other _ _ |>.mpr (Or.inl old))
        · rw [resultFacts] at fresh
          obtain rhsFact | written := mem_validator_union other _ _ |>.mp fresh
          · exact List.mem_cons_of_mem _ (mem_validator_union other _ _ |>.mpr (Or.inr rhsFact))
          · have same : other = id := by simpa using written
            exact same ▸ List.mem_cons_self
      have final : ValidatorContinuation program entryType state.functionId
          (.value after destination.type) state.continuation := by
        apply ValidatorContinuation.weaken (ValidatorProtocol.value_entails _ _ _ entails)
        simpa [ValidatorExpressionTask.output, Expr.type, signature] using tail
      have continuation : ValidatorContinuation program entryType state.functionId child.output
          (.assign type op id destination :: state.continuation) := by
        rw [signature]
        refine .frame (.assign state.functionId environment rhs.type op id destination ?_ writable ?_
          (by simpa [signature] using admitted)) final
        · exact (task.bindings id).trans binding
        · intro notSet
          exact mem_validator_union id _ _ |>.mpr (Or.inl (task.initialized id (initialized notSet)))
      rw [checked_assignment_enter callees task.checked.fuel task.checkedInput state type op id destination
        target rhs task.checked.facts task.checked.accepted (models.checked_input task)]
      exact .running _ child.output checked (.expression callees rhs context child _ models) continuation
    | update type op id destination target =>
      obtain ⟨signature, targetEq, binding, writable, initialized⟩ := checked_update_signature callees
        task.checked.fuel task.checkedInput type op id destination target task.checked.facts task.checked.accepted
      have factsEq := checked_update_facts callees task.checked.fuel task.checkedInput type op id
        destination target task.checked.facts task.checked.accepted
      have afterEntails := task.environment.entails_afterExpression task.checked.facts (by
        intro other member
        rw [factsEq] at member
        exact task.initialized other member)
      have transferred := validator_update_transfer task.environment state models op id destination
        ((task.bindings id).trans binding) writable (task.initialized id initialized)
      rw [signature, targetEq]
      rcases transferred with ⟨value, locals, typed, store, result⟩ | ⟨code, admitted, result⟩
      · rw [result]
        exact .running _ task.output checked (.delivered _ _ _
          ⟨by simpa [Expr.type, signature] using typed,
            store.weaken _ _ afterEntails⟩) tail
      · rw [result]
        exact validator_diagnostic_invariant program entryType state code admitted
    | call type callee arguments =>
      exact validator_call_expression_preserves program entryType checked state callees context type callee
        arguments task models tail

theorem binary_output_initialized_subset (op : Binary) (left right : ExpressionFacts)
    (id : Nat) (member : id ∈ binaryOutputInitialized op left right) :
    id ∈ left.initialized ∨ id ∈ right.initialized := by
  unfold binaryOutputInitialized at member
  split at member
  · split at member
    · exact Or.inr member
    · exact Or.inl member
  · exact mem_validator_union id _ _ |>.mp member

theorem validator_binary_left_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (type : IntType) (op : Binary) (left right : Expr)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorExpressionTask callees (.binary type op left right))
    (checkedLeft : CheckedExpression callees task.checkedInput left)
    (fuelEq : task.checked.fuel = checkedLeft.fuel + 1) (value : Value)
    (delivered : (ValidatorProtocol.value
      (task.environment.afterExpression checkedLeft.facts) left.type).Models (.value value) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (resumeValue state (.binaryLeft type op right) value) := by
  obtain ⟨valueType, models⟩ := delivered
  obtain ⟨fuel, a, b, extractedFuelEq, leftAccepted, rightAccepted, signature, admitted, resultFacts⟩ :=
    checked_binary_facts callees task.checked.fuel task.checkedInput type op left right
      task.checked.facts task.checked.accepted
  have sameFuel : fuel = checkedLeft.fuel := by omega
  subst fuel
  have sameFacts : a = checkedLeft.facts := Except.ok.inj (leftAccepted.symm.trans checkedLeft.accepted)
  subst a
  have shortModels (operation : op = .and ∨ op = .or) :
      (task.environment.afterExpression task.checked.facts).Models state.locals := by
    apply models.weaken _ _
    apply task.environment.afterExpression_entails_afterExpression checkedLeft.facts task.checked.facts
    intro id member
    rw [resultFacts] at member
    rcases operation with operation | operation
    all_goals
      simp only [accessUnion, binaryOutputInitialized, binarySequenced, operation, BEq.rfl,
        Bool.true_or, ↓reduceIte] at member
      exact Or.inr member
  have continued : ValidatorMachineInvariant program entryType
      {state with control := .expression right, continuation := .binaryRight type op value :: state.continuation} := by
    let environment := task.environment.afterExpression checkedLeft.facts
    let child : ValidatorExpressionTask callees right := {
      environment
      checkedInput := binaryRightInput task.checkedInput op checkedLeft.facts
      checked := ⟨checkedLeft.fuel, b, rightAccepted⟩
      bindings := by
        intro id
        unfold binaryRightInput
        split <;> exact task.bindings id
      initialized := by
        intro id member
        unfold binaryRightInput at member
        split at member
        · exact mem_validator_union id _ _ |>.mpr (Or.inr member)
        · exact mem_validator_union id _ _ |>.mpr (Or.inl (task.initialized id member))
    }
    have entails : (environment.afterExpression b).Entails
        (task.environment.afterExpression task.checked.facts) := by
      refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
      intro id member
      obtain old | fresh := mem_validator_union id _ _ |>.mp member
      · exact mem_validator_union id _ _ |>.mpr
          (Or.inl (mem_validator_union id _ _ |>.mpr (Or.inl old)))
      · rw [resultFacts] at fresh
        obtain leftFact | rightFact := binary_output_initialized_subset op checkedLeft.facts b id fresh
        · exact mem_validator_union id _ _ |>.mpr
            (Or.inl (mem_validator_union id _ _ |>.mpr (Or.inr leftFact)))
        · exact mem_validator_union id _ _ |>.mpr (Or.inr rightFact)
    have resultType : type = binaryType op value.type right.type := by simpa [valueType] using signature
    have allowed : !(op == .shl && value.type.signed) = true := by simpa [valueType] using admitted
    have final : ValidatorContinuation program entryType state.functionId
        (.value (environment.afterExpression b) type) state.continuation :=
      .weaken (ValidatorProtocol.value_entails _ _ type entails) tail
    have continuation : ValidatorContinuation program entryType state.functionId child.output
        (.binaryRight type op value :: state.continuation) := by
      rw [resultType] at final ⊢
      exact .frame (.binaryRight state.functionId (environment.afterExpression b) op value right.type allowed) final
    exact .running _ child.output checked (.expression callees right context child _ models) continuation
  by_cases shortAnd : (op == .and && !value.truth) = true
  · have operation : op = .and := by
      have parts := shortAnd
      simp only [Bool.and_eq_true] at parts
      simpa using parts.1
    have typed : (Value.boolean false).type = type := by
      change IntType.int = type
      simpa [binaryType, operation] using signature.symm
    simpa [resumeValue, shortAnd] using validator_result_transfer program entryType
      checked state _ (shortModels (Or.inl operation)) type (.ok (.boolean false)) typed tail
  · by_cases shortOr : (op == .or && value.truth) = true
    · have operation : op = .or := by
        have parts := shortOr
        simp only [Bool.and_eq_true] at parts
        simpa using parts.1
      have typed : (Value.boolean true).type = type := by
        change IntType.int = type
        simpa [binaryType, operation] using signature.symm
      simpa [resumeValue, shortAnd, shortOr] using validator_result_transfer program entryType
        checked state _ (shortModels (Or.inr operation)) type (.ok (.boolean true)) typed tail
    · simpa [resumeValue, shortAnd, shortOr] using continued

theorem validator_choice_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (type : IntType) (condition yes no : Expr)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorExpressionTask callees (.conditional type condition yes no))
    (checkedCondition : CheckedExpression callees task.checkedInput condition)
    (fuelEq : task.checked.fuel = checkedCondition.fuel + 1) (value : Value)
    (delivered : (ValidatorProtocol.value
      (task.environment.afterExpression checkedCondition.facts) condition.type).Models (.value value) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (resumeValue state (.choice type yes no) value) := by
  obtain ⟨_, models⟩ := delivered
  obtain ⟨fuel, c, a, b, extractedFuelEq, conditionAccepted, yesAccepted, noAccepted, resultFacts⟩ :=
    checked_conditional_facts callees task.checked.fuel task.checkedInput type condition yes no
      task.checked.facts task.checked.accepted
  have sameFuel : fuel = checkedCondition.fuel := by omega
  subst fuel
  have sameFacts : c = checkedCondition.facts :=
    Except.ok.inj (conditionAccepted.symm.trans checkedCondition.accepted)
  subst c
  have selected (arm : Expr) (facts : ExpressionFacts)
      (accepted : checkExpr callees checkedCondition.fuel
        {task.checkedInput with initialized := checkedCondition.facts.initialized} arm = .ok facts)
      (subset : ∀ id ∈ task.checked.facts.initialized, id ∈ facts.initialized) :
      ValidatorMachineInvariant program entryType
        {state with control := .expression arm, continuation := .convert type :: state.continuation} := by
    let environment := task.environment.afterExpression checkedCondition.facts
    let child : ValidatorExpressionTask callees arm := {
      environment
      checkedInput := {task.checkedInput with initialized := checkedCondition.facts.initialized}
      checked := ⟨checkedCondition.fuel, facts, accepted⟩
      bindings := task.bindings
      initialized := fun id member => mem_validator_union id _ _ |>.mpr (Or.inr member)
    }
    have entails : (environment.afterExpression facts).Entails
        (task.environment.afterExpression task.checked.facts) := by
      refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
      intro id member
      obtain old | fresh := mem_validator_union id _ _ |>.mp member
      · exact mem_validator_union id _ _ |>.mpr
          (Or.inl (mem_validator_union id _ _ |>.mpr (Or.inl old)))
      · exact mem_validator_union id _ _ |>.mpr (Or.inr (subset id fresh))
    have final : ValidatorContinuation program entryType state.functionId
        (.value (environment.afterExpression facts) type) state.continuation :=
      .weaken (ValidatorProtocol.value_entails _ _ type entails) tail
    exact .running _ child.output checked (.expression callees arm context child _ models)
      (.frame (.convert state.functionId (environment.afterExpression facts) arm.type type) final)
  cases truth : value.truth with
  | true =>
    simpa [resumeValue, truth] using selected yes a yesAccepted (by
      intro id member
      rw [resultFacts] at member
      exact (mem_validator_intersection id _ _ |>.mp member).1)
  | false =>
    simpa [resumeValue, truth] using selected no b noAccepted (by
      intro id member
      rw [resultFacts] at member
      exact (mem_validator_intersection id _ _ |>.mp member).2)

theorem validator_comma_left_transfer (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (type : IntType) (left right : Expr)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorExpressionTask callees (.comma type left right))
    (checkedLeft : CheckedExpression callees task.checkedInput left)
    (fuelEq : task.checked.fuel = checkedLeft.fuel + 1) (value : Value)
    (delivered : (ValidatorProtocol.value
      (task.environment.afterExpression checkedLeft.facts) left.type).Models (.value value) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (resumeValue state (.binaryLeft type .comma right) value) := by
  obtain ⟨_, models⟩ := delivered
  obtain ⟨fuel, a, b, extractedFuelEq, signature, leftAccepted, rightAccepted, resultFacts⟩ :=
    checked_comma_facts callees task.checked.fuel task.checkedInput type left right
      task.checked.facts task.checked.accepted
  have sameFuel : fuel = checkedLeft.fuel := by omega
  subst fuel
  have sameFacts : a = checkedLeft.facts := Except.ok.inj (leftAccepted.symm.trans checkedLeft.accepted)
  subst a
  let environment := task.environment.afterExpression checkedLeft.facts
  let child : ValidatorExpressionTask callees right := {
    environment
    checkedInput := {task.checkedInput with initialized := checkedLeft.facts.initialized}
    checked := ⟨checkedLeft.fuel, b, rightAccepted⟩
    bindings := task.bindings
    initialized := fun id member => mem_validator_union id _ _ |>.mpr (Or.inr member)
  }
  have entails : (environment.afterExpression b).Entails
      (task.environment.afterExpression task.checked.facts) := by
    refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
    intro id member
    obtain old | fresh := mem_validator_union id _ _ |>.mp member
    · exact mem_validator_union id _ _ |>.mpr
        (Or.inl (mem_validator_union id _ _ |>.mpr (Or.inl old)))
    · rw [resultFacts] at fresh
      exact mem_validator_union id _ _ |>.mpr (Or.inr fresh)
  have final : ValidatorContinuation program entryType state.functionId
      (.value (environment.afterExpression b) type) state.continuation :=
    .weaken (ValidatorProtocol.value_entails _ _ type entails) tail
  have continuation : ValidatorContinuation program entryType state.functionId child.output
      (.binaryRight type .comma value :: state.continuation) := by
    rw [signature] at final ⊢
    simpa [binaryType, child, ValidatorExpressionTask.output, Expr.type] using ValidatorContinuation.frame
      (.binaryRight state.functionId (environment.afterExpression b) .comma value right.type (by simp)) final
  simpa [resumeValue] using ValidatorMachineInvariant.running
    {state with control := .expression right, continuation := .binaryRight type .comma value :: state.continuation}
    child.output checked (.expression callees right context child _ models) continuation

end EvoIR.Core.Source
