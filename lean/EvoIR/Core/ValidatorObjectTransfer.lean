import EvoIR.Core.ValidatorObjectFacts

/-! Declaration entry constructs the actual fresh slot and initializer frame
from `declare` success. Neither a successful runtime write nor a future machine
closure is assumed. -/
namespace EvoIR.Core.Source

private theorem initializer_output_entails (task : ValidatorObjectTask callees object) :
    (ValidatorProtocol.flow ⟨task.outputEnvironment, task.outputEnvironment, true,
      task.depth, task.result.type, true, false⟩).Entails task.output := by
  intro control locals models
  cases control with
  | flow flow =>
    cases flow <;> simp_all [ValidatorProtocol.Models, ValidatorFlowProtocol.Models,
      ValidatorObjectTask.output]
  | _ => simp [ValidatorProtocol.Models] at models

theorem validator_object_control_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (object : Object) (context : ValidatorFunctionContext program state.functionId callees)
    (parent : ValidatorObjectTask callees object)
    (functionResult : parent.result = context.function.returnType)
    (models : parent.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId parent.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterObject state object) := by
  obtain ⟨absent, bindings, initializer⟩ := checked_declaration callees parent.fuel
    parent.checkedInput parent.checked.state object parent.checked.accepted
  have absent : lookup parent.environment.state object.symbol = none := by
    simpa [lookup, parent.bindings] using absent
  have duplicate := models.no_active_duplicate parent.environment state.locals object.symbol absent
  let fresh := parent.environment.declareObject object.symbol object.type
  have freshModels : fresh.Models (⟨object.symbol, object.type, none⟩ :: state.locals) :=
    models.declareObject parent.environment state.locals object.symbol object.type absent
  have oldFreshDifferent : ∀ id ∈ parent.environment.freshConst, id ≠ object.symbol := by
    intro id member same
    subst id
    have bound := models.fresh_bound parent.environment state.locals object.symbol member
    simp [absent] at bound
  cases initializerEq : object.initializer with
  | none =>
    have initialized : parent.checked.state.initialized = parent.checkedInput.initialized := by
      simpa [initializerEq] using initializer
    have entails : fresh.Entails parent.outputEnvironment := {
      bindings := by
        intro id
        simp only [lookup, fresh, ValidatorEnvironment.declareObject,
          parent.outputBindings, bindings, ← parent.bindings]
      layout := by simpa [fresh, ValidatorEnvironment.declareObject] using parent.outputLayout.symm
      initialized := by
        intro id member
        rw [parent.outputInitialized, initialized] at member
        rcases mem_validator_union id _ _ |>.mp member with member | member
        · exact member
        · exact parent.initialized id member
      fresh := by
        intro id member
        rw [parent.outputFresh] at member
        simp only [fresh, ValidatorEnvironment.declareObject]
        cases object.type.isConst <;> simp [member] }
    have represented := freshModels.weaken fresh parent.outputEnvironment entails
    simp only [enterObject, duplicate, Bool.false_eq_true, ↓reduceIte, initializerEq]
    exact .running _ parent.output checkedProgram
      (.delivered _ _ _ (by simpa [ValidatorObjectTask.output, ValidatorProtocol.Models,
        ValidatorFlowProtocol.Models] using represented)) tail
  | some pair =>
    rcases pair with ⟨destination, expression⟩
    obtain ⟨conversion, facts, accepted, initialized⟩ :=
      (show destination = object.type ∧ ∃ facts,
        checkExpr callees parent.fuel
          ⟨(object.symbol, object.type) :: parent.checkedInput.bindings,
            parent.checkedInput.initialized⟩ expression = .ok facts ∧
          parent.checked.state.initialized = union facts.initialized [object.symbol] from
        by simpa [initializerEq] using initializer)
    subst destination
    let child : ValidatorExpressionTask callees expression := {
      environment := fresh
      checkedInput := ⟨(object.symbol, object.type) :: parent.checkedInput.bindings,
        parent.checkedInput.initialized⟩
      checked := ⟨parent.fuel, facts, accepted⟩
      bindings := by
        intro id
        simp [fresh, ValidatorEnvironment.declareObject, lookup, parent.bindings]
      initialized := parent.initialized }
    let before := fresh.afterExpression facts
    have binding : lookup before.state object.symbol = some object.type := by
      simp [before, ValidatorEnvironment.afterExpression, fresh,
        ValidatorEnvironment.declareObject, lookup, List.find?]
    have isFresh : object.type.isConst = true → object.symbol ∈ before.freshConst := by
      intro const
      simp [before, ValidatorEnvironment.afterExpression, fresh,
        ValidatorEnvironment.declareObject, const]
    have afterBindings : parent.outputEnvironment.state.bindings = before.state.bindings := by
      simp [parent.outputBindings, bindings, before, ValidatorEnvironment.afterExpression,
        fresh, ValidatorEnvironment.declareObject, parent.bindings]
    have afterLayout : parent.outputEnvironment.layout = before.layout := by
      simpa [before, ValidatorEnvironment.afterExpression, fresh,
        ValidatorEnvironment.declareObject] using parent.outputLayout
    have afterInitialized : ∀ id ∈ parent.outputEnvironment.state.initialized,
        id = object.symbol ∨ id ∈ before.state.initialized := by
      intro id member
      rw [parent.outputInitialized, initialized] at member
      rcases mem_validator_union id _ _ |>.mp member with old | added
      · right
        exact mem_validator_union id _ _ |>.mpr (Or.inl old)
      · rcases mem_validator_union id _ _ |>.mp added with expressionInit | objectInit
        · right
          exact mem_validator_union id _ _ |>.mpr (Or.inr expressionInit)
        · left
          simpa using objectInit
    have afterFresh : ∀ id ∈ parent.outputEnvironment.freshConst,
        id ∈ before.freshConst ∧ id ≠ object.symbol := by
      intro id member
      rw [parent.outputFresh] at member
      refine ⟨?_, oldFreshDifferent id member⟩
      simp only [before, ValidatorEnvironment.afterExpression, fresh,
        ValidatorEnvironment.declareObject]
      cases object.type.isConst <;> simp [member]
    have continuation : ValidatorContinuation program entryType state.functionId child.output
        (.initialize object.symbol object.type :: state.continuation) :=
      .frame (.initialize state.functionId before expression.type object.symbol object.type
        binding isFresh parent.outputEnvironment afterBindings afterLayout afterInitialized
        afterFresh parent.depth parent.result) (.weaken (initializer_output_entails parent) tail)
    simp only [enterObject, duplicate, Bool.false_eq_true, ↓reduceIte, initializerEq,
      bne, beq_self_eq_true, Bool.not_true]
    exact .running _ child.output checkedProgram
      (.expression callees expression context child _ freshModels) continuation

end EvoIR.Core.Source
