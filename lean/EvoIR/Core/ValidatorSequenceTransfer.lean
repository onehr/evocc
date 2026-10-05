import EvoIR.Core.ValidatorScopeTransfer

/-! Sequence resumption reconstructs the remaining checked item fold. Extra
runtime initialization is retained, and abruptly introduced locals are kept
only as a concrete prefix of the incoming scope. -/
namespace EvoIR.Core.Source

private def itemState : ValidatorItemTask callees item → LocalState
  | .declaration task => task.checked.state
  | .statement task => task.checked.facts.state

private def itemFalls : ValidatorItemTask callees item → Bool
  | .declaration _ => true
  | .statement task => task.checked.facts.fallsThrough

private def itemAfter : ValidatorItemTask callees item → ValidatorEnvironment
  | .declaration task => task.outputEnvironment
  | .statement task => task.outputEnvironment

private def itemIntroduced : ValidatorItemTask callees item → LocalLayout
  | .declaration (object := object) _ => [(object.symbol, object.type)]
  | .statement _ => []

private theorem item_checked (task : ValidatorItemTask callees item) :
    validatorItemStep callees task.result task.fuel task.depth ⟨task.checkedInput, true⟩ item =
      .ok ⟨itemState task, itemFalls task⟩ := by
  cases task with
  | declaration task =>
    simp [validatorItemStep, ValidatorItemTask.result, ValidatorItemTask.fuel,
      ValidatorItemTask.depth, ValidatorItemTask.checkedInput, itemState, itemFalls,
      task.checked.accepted, bind, Except.bind, pure, Except.pure]
  | statement task =>
    simp [validatorItemStep, ValidatorItemTask.result, ValidatorItemTask.fuel,
      ValidatorItemTask.depth, ValidatorItemTask.checkedInput, itemState, itemFalls,
      task.checked.accepted, bind, Except.bind, pure, Except.pure]

private theorem item_after_bindings (task : ValidatorItemTask callees item) :
    (itemAfter task).state.bindings = (itemState task).bindings := by
  cases task with
  | declaration task => exact task.outputBindings
  | statement task => exact task.outputBindings

private theorem item_after_initialized (task : ValidatorItemTask callees item) :
    (itemAfter task).state.initialized =
      union task.environment.state.initialized (itemState task).initialized := by
  cases task with
  | declaration task => exact task.outputInitialized
  | statement task => exact task.outputInitialized

private theorem item_after_layout (task : ValidatorItemTask callees item) :
    (itemAfter task).layout = itemIntroduced task ++ task.environment.layout := by
  cases task with
  | declaration task => exact task.outputLayout
  | statement task => exact task.outputLayout

private theorem item_after_fresh (task : ValidatorItemTask callees item) :
    (itemAfter task).freshConst = task.environment.freshConst := by
  cases task with
  | declaration task => exact task.outputFresh
  | statement task => exact task.outputFresh

private theorem item_bindings_grow (task : ValidatorItemTask callees item) :
    (itemState task).bindings = itemIntroduced task ++ task.checkedInput.bindings := by
  cases task with
  | declaration task =>
    exact (checked_declaration callees task.fuel task.checkedInput task.checked.state _
      task.checked.accepted).2.1
  | statement task =>
    exact checked_statement_bindings callees task.result task.checked.fuel task.depth
      task.checkedInput _ task.checked.facts task.checked.accepted

private theorem models_grown (environment : ValidatorEnvironment) (locals : Locals)
    (models : environment.Models locals) : environment.Grown locals :=
  ⟨[], locals, rfl, models, models.distinct, models.realizes.values⟩

private theorem item_normal_grown (parent : ValidatorItemsTask callees (first :: remaining))
    (child : ValidatorItemTask callees first) (environment : child.environment = parent.environment)
    (input : child.checkedInput = parent.checkedInput) (locals : Locals)
    (models : (itemAfter child).Models locals) : parent.environment.Grown locals := by
  have separate : ∀ id, (lookup parent.environment.state id).isSome = true →
      (itemIntroduced child).find? (fun pair => pair.1 == id) = none := by
    cases child with
    | @declaration object task =>
      have absent := (checked_declaration callees task.fuel task.checkedInput task.checked.state _
        task.checked.accepted).1
      change task.environment = parent.environment at environment
      change task.checkedInput = parent.checkedInput at input
      have absent : lookup parent.environment.state object.symbol = none := by
        simpa [lookup, ← environment, task.bindings] using absent
      intro id bound
      have different : object.symbol ≠ id := by
        intro same
        simp [← same, absent] at bound
      simpa [itemIntroduced, List.find?, beq_eq_false_iff_ne.mpr different]
    | statement task => simp [itemIntroduced]
  obtain ⟨introduced, retained, split, _, represented⟩ := validator_retained_models
    parent.environment (itemAfter child) parent.environment (itemIntroduced child) locals models
    (by rw [item_after_bindings, item_bindings_grow, input, ← parent.bindings])
    (by rw [item_after_layout, environment]) separate rfl rfl
    (by
      intro id member
      refine ⟨?_, parent.initializedBound id member⟩
      rw [item_after_initialized, environment]
      exact mem_validator_union id _ _ |>.mpr (Or.inl member))
    (by
      intro id member
      exact ⟨by simpa [item_after_fresh, environment] using member,
        parent.freshBound id member⟩)
  exact ⟨introduced, retained, split, represented, models.distinct, models.realizes.values⟩

private theorem item_grown_grown (parent : ValidatorItemsTask callees (first :: remaining))
    (child : ValidatorItemTask callees first) (environment : child.environment = parent.environment)
    (input : child.checkedInput = parent.checkedInput) (locals : Locals)
    (models : (itemAfter child).Grown locals) : parent.environment.Grown locals := by
  obtain ⟨introduced, retained, split, represented, distinct, values⟩ := models
  obtain ⟨next, kept, nextSplit, keptModels, _, _⟩ :=
    item_normal_grown parent child environment input retained represented
  refine ⟨introduced ++ next, kept, ?_, keptModels, distinct, values⟩
  simp [split, nextSplit, List.append_assoc]

theorem validator_sequence_frame_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (first : BlockItem) (remaining : List BlockItem)
    (context : ValidatorFunctionContext program state.functionId callees)
    (parent : ValidatorItemsTask callees (first :: remaining))
    (functionResult : parent.result = context.function.returnType)
    (child : ValidatorItemTask callees first) (environment : child.environment = parent.environment)
    (input : child.checkedInput = parent.checkedInput) (result : child.result = parent.result)
    (depth : child.depth = parent.depth) (fuel : child.fuel = parent.fuel)
    (flow : Flow) (delivered : child.output.Models (.flow flow) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId parent.output state.continuation) :
    ValidatorMachineInvariant program entryType (resumeFlow state (.sequence remaining) flow) := by
  cases flow with
  | normal =>
    have delivered : itemFalls child = true ∧ (itemAfter child).Models state.locals := by
      cases child <;> simpa [ValidatorItemTask.output, ValidatorObjectTask.output,
        ValidatorStatementTask.output, ValidatorProtocol.Models, ValidatorFlowProtocol.Models,
        itemFalls, itemAfter] using delivered
    obtain ⟨head, suffixFacts, headChecked, suffixChecked, finalState, finalFalls⟩ :=
      checked_items_split callees parent.result parent.fuel parent.depth parent.checkedInput
        first remaining parent.checked.facts parent.checked.accepted
    have checked := item_checked child
    rw [result, fuel, depth, input] at checked
    have headEq : head = ⟨itemState child, itemFalls child⟩ :=
      Except.ok.inj (headChecked.symm.trans checked)
    subst head
    obtain ⟨introduced, growth, _⟩ := checked_items_prefix callees parent.result parent.fuel
      parent.depth (itemState child) remaining suffixFacts suffixChecked
    let suffix : ValidatorItemsTask callees remaining := {
      environment := itemAfter child
      initializedBound := delivered.2.initialized_bound _ state.locals
      freshBound := delivered.2.fresh_bound _ state.locals
      checkedInput := itemState child
      bindings := item_after_bindings child
      initialized := by
        intro id member
        rw [item_after_initialized]
        exact mem_validator_union id _ _ |>.mpr (Or.inr member)
      result := parent.result
      depth := parent.depth
      fuel := parent.fuel
      checked := ⟨suffixFacts, suffixChecked⟩
      introduced
      outputEnvironment := {
        state := ⟨suffixFacts.state.bindings,
          union (itemAfter child).state.initialized suffixFacts.state.initialized⟩
        layout := introduced ++ (itemAfter child).layout
        freshConst := (itemAfter child).freshConst }
      outputBindings := rfl
      bindingsGrow := growth
      outputInitialized := rfl
      outputLayout := rfl
      outputFresh := rfl }
    have prefixEq : parent.introduced = introduced ++ itemIntroduced child :=
      List.append_cancel_right (by
        rw [← parent.bindingsGrow, finalState, growth, item_bindings_grow, input]
        simp [List.append_assoc])
    have normal : suffix.outputEnvironment.Entails parent.outputEnvironment := {
      bindings := by
        intro id
        simp [lookup, suffix, parent.outputBindings, finalState]
      layout := by
        simp [suffix, parent.outputLayout, prefixEq, item_after_layout,
          environment, List.append_assoc]
      initialized := by
        intro id member
        rw [parent.outputInitialized, finalState] at member
        rcases mem_validator_union id _ _ |>.mp member with incoming | finalInit
        · apply mem_validator_union id _ _ |>.mpr
          left
          rw [item_after_initialized, environment]
          exact mem_validator_union id _ _ |>.mpr (Or.inl incoming)
        · exact mem_validator_union id _ _ |>.mpr (Or.inr finalInit)
      fresh := by
        intro id member
        simpa [suffix, parent.outputFresh, item_after_fresh, environment] using member }
    have entails : suffix.output.Entails parent.output := by
      intro control locals represented
      cases control with
      | flow flow =>
        cases flow with
        | normal =>
          change suffix.checked.facts.fallsThrough = true ∧ suffix.outputEnvironment.Models locals
            at represented
          refine ⟨?_, represented.2.weaken _ _ normal⟩
          simpa [ValidatorItemsTask.output, suffix, finalFalls, delivered.1] using represented.1
        | «break» =>
          change (remaining.any _) = true ∧ parent.depth > 0 ∧ (itemAfter child).Grown locals at represented
          refine ⟨?_, represented.2.1,
            item_grown_grown parent child environment input locals represented.2.2⟩
          simp [List.any_cons, represented.1]
        | «continue» =>
          change (remaining.any _) = true ∧ parent.depth > 0 ∧ (itemAfter child).Grown locals at represented
          refine ⟨?_, represented.2.1,
            item_grown_grown parent child environment input locals represented.2.2⟩
          simp [List.any_cons, represented.1]
        | returned value =>
          change (remaining.any _) = true ∧ value.type = parent.result.type ∧
            (itemAfter child).Grown locals at represented
          refine ⟨?_, represented.2.1,
            item_grown_grown parent child environment input locals represented.2.2⟩
          simp [List.any_cons, represented.1]
      | _ => simp [ValidatorItemsTask.output, ValidatorProtocol.Models] at represented
    exact .running _ suffix.output checkedProgram
      (.items callees remaining context suffix functionResult state.locals delivered.2)
      (.weaken entails tail)
  | «break» =>
    cases child with
    | declaration task =>
      simp [ValidatorItemTask.output, ValidatorObjectTask.output,
        ValidatorProtocol.Models, ValidatorFlowProtocol.Models] at delivered
    | statement task =>
      change task.environment = parent.environment at environment
      change statementAllowsAbrupt _ = true ∧ task.depth > 0 ∧ task.environment.Models state.locals at delivered
      simp only [resumeFlow]
      refine .running _ parent.output checkedProgram (.delivered _ _ _ ?_) tail
      refine ⟨by simp [ValidatorItemsTask.output, List.any_cons], ?_, ?_⟩
      · simpa [ValidatorItemTask.depth] using depth ▸ delivered.2.1
      · exact models_grown parent.environment state.locals (environment ▸ delivered.2.2)
  | «continue» =>
    cases child with
    | declaration task =>
      simp [ValidatorItemTask.output, ValidatorObjectTask.output,
        ValidatorProtocol.Models, ValidatorFlowProtocol.Models] at delivered
    | statement task =>
      change task.environment = parent.environment at environment
      change statementAllowsAbrupt _ = true ∧ task.depth > 0 ∧ task.environment.Models state.locals at delivered
      simp only [resumeFlow]
      refine .running _ parent.output checkedProgram (.delivered _ _ _ ?_) tail
      refine ⟨by simp [ValidatorItemsTask.output, List.any_cons], ?_, ?_⟩
      · simpa [ValidatorItemTask.depth] using depth ▸ delivered.2.1
      · exact models_grown parent.environment state.locals (environment ▸ delivered.2.2)
  | returned value =>
    cases child with
    | declaration task =>
      simp [ValidatorItemTask.output, ValidatorObjectTask.output,
        ValidatorProtocol.Models, ValidatorFlowProtocol.Models] at delivered
    | statement task =>
      change task.environment = parent.environment at environment
      change statementAllowsAbrupt _ = true ∧ value.type = task.result.type ∧
        task.environment.Models state.locals at delivered
      simp only [resumeFlow]
      refine .running _ parent.output checkedProgram (.delivered _ _ _ ?_) tail
      refine ⟨by simp [ValidatorItemsTask.output, List.any_cons], ?_, ?_⟩
      · simpa [ValidatorItemTask.result] using delivered.2.1.trans (congrArg QualifiedType.type result)
      · exact models_grown parent.environment state.locals (environment ▸ delivered.2.2)

end EvoIR.Core.Source
