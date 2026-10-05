import EvoIR.Core.ValidatorScopeFacts

/-! The actual scope-pop, item/declaration and sequence transitions preserve
the one validator-machine invariant. Scope certificates contain finite static
facts only; runtime suffixes are reconstructed from the current concrete store. -/
namespace EvoIR.Core.Source

private theorem scope_pop (state : State) (flow : Flow) (layout : LocalLayout)
    (introducedPrefix retained : Locals) (split : state.locals = introducedPrefix ++ retained)
    (retainedLayout : localLayout retained = layout) :
    resumeFlow state (.scope layout.length) flow =
      {state with control := .flow flow, locals := retained} := by
  have length : retained.length = layout.length := by
    simpa [localLayout] using congrArg List.length retainedLayout
  have safe : ¬introducedPrefix.length + retained.length < retained.length := by omega
  rw [← length]
  simp [resumeFlow, split, safe]

theorem validator_scope_protocol_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State)
    (input output : ValidatorFlowProtocol) (certificate : ValidatorScopeProtocol input output)
    (flow : Flow) (delivered : input.Models flow state.locals)
    (tail : ValidatorContinuation program entryType state.functionId
      (.flow output) state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeFlow state (.scope output.before.layout.length) flow) := by
  have retainedBefore (locals : Locals) (models : input.before.Models locals) :
      ∃ introducedPrefix retained, locals = introducedPrefix ++ retained ∧
        localLayout retained = output.before.layout ∧ output.before.Models retained :=
    validator_retained_models output.before input.before output.before certificate.abruptIntroduced
      locals models certificate.abruptBindings certificate.abruptLayout certificate.abruptSeparate
      rfl rfl certificate.abruptInitialized certificate.abruptFresh
  have abrupt (models : if input.restoresLayout then input.before.Models state.locals
      else input.before.Grown state.locals) :
      ∃ retained, resumeFlow state (.scope output.before.layout.length) flow =
        {state with control := .flow flow, locals := retained} ∧ output.before.Models retained := by
    cases restores : input.restoresLayout with
    | true =>
      obtain ⟨introducedPrefix, retained, split, layout, models⟩ := retainedBefore state.locals
        (by simpa [restores] using models)
      exact ⟨retained, scope_pop state flow output.before.layout introducedPrefix retained split layout, models⟩
    | false =>
      obtain ⟨introducedPrefix, retained, split, models, _, _⟩ :=
        (show input.before.Grown state.locals from by simpa [restores] using models)
      obtain ⟨nextPrefix, kept, nextSplit, layout, models⟩ := retainedBefore retained models
      have combined : state.locals = (introducedPrefix ++ nextPrefix) ++ kept := by
        simp [split, nextSplit, List.append_assoc]
      exact ⟨kept, scope_pop state flow output.before.layout _ kept combined layout, models⟩
  cases flow with
  | normal =>
    change input.fallsThrough = true ∧ input.normal.Models state.locals at delivered
    obtain ⟨introducedPrefix, retained, split, layout, models⟩ :=
      validator_retained_models output.before input.normal output.normal certificate.introduced
        state.locals delivered.2 certificate.bindings certificate.layout certificate.separate
        certificate.outputBindings certificate.outputLayout certificate.initialized certificate.fresh
    rw [scope_pop state .normal output.before.layout introducedPrefix retained split layout]
    exact .running _ (.flow output) checkedProgram
      (.delivered _ _ _ ⟨certificate.fallsThrough delivered.1, models⟩) tail
  | «break» =>
    change input.allowsAbrupt = true ∧ input.loopDepth > 0 ∧
      (if input.restoresLayout then input.before.Models state.locals else input.before.Grown state.locals)
      at delivered
    obtain ⟨retained, stepped, models⟩ := abrupt delivered.2.2
    rw [stepped]
    have depth : output.loopDepth > 0 := by simpa [← certificate.depth] using delivered.2.1
    refine .running _ (.flow output) checkedProgram (.delivered _ _ _ ?_) tail
    simpa [ValidatorProtocol.Models, ValidatorFlowProtocol.Models, certificate.restoresLayout]
      using And.intro (certificate.allowsAbrupt delivered.1)
        (And.intro depth models)
  | «continue» =>
    change input.allowsAbrupt = true ∧ input.loopDepth > 0 ∧
      (if input.restoresLayout then input.before.Models state.locals else input.before.Grown state.locals)
      at delivered
    obtain ⟨retained, stepped, models⟩ := abrupt delivered.2.2
    rw [stepped]
    have depth : output.loopDepth > 0 := by simpa [← certificate.depth] using delivered.2.1
    refine .running _ (.flow output) checkedProgram (.delivered _ _ _ ?_) tail
    simpa [ValidatorProtocol.Models, ValidatorFlowProtocol.Models, certificate.restoresLayout]
      using And.intro (certificate.allowsAbrupt delivered.1)
        (And.intro depth models)
  | returned value =>
    change input.allowsAbrupt = true ∧ value.type = input.resultType ∧
      (if input.restoresLayout then input.before.Models state.locals else input.before.Grown state.locals)
      at delivered
    obtain ⟨retained, stepped, models⟩ := abrupt delivered.2.2
    rw [stepped]
    refine .running _ (.flow output) checkedProgram (.delivered _ _ _ ?_) tail
    simpa [ValidatorProtocol.Models, ValidatorFlowProtocol.Models, certificate.restoresLayout]
      using And.intro (certificate.allowsAbrupt delivered.1)
        (And.intro (delivered.2.1.trans certificate.result) models)

theorem ValidatorItemsTask.separated (task : ValidatorItemsTask callees items) :
    ∀ id, (lookup task.environment.state id).isSome = true →
      task.introduced.find? (fun pair => pair.1 == id) = none := by
  obtain ⟨introduced, bindings, separate⟩ := checked_items_prefix callees task.result task.fuel
    task.depth task.checkedInput items task.checked.facts task.checked.accepted
  have same : task.introduced = introduced :=
    List.append_cancel_right (task.bindingsGrow.symm.trans bindings)
  subst introduced
  intro id bound
  exact separate id (by simpa [lookup, task.bindings] using bound)

theorem ValidatorItemsTask.output_models_grown (task : ValidatorItemsTask callees items)
    (locals : Locals) (models : task.outputEnvironment.Models locals) :
    task.environment.Grown locals := by
  obtain ⟨introducedPrefix, retained, split, _, represented⟩ := validator_retained_models
    task.environment task.outputEnvironment task.environment task.introduced locals models
    (by rw [task.outputBindings, task.bindingsGrow, ← task.bindings]) task.outputLayout
    task.separated rfl rfl
    (by
      intro id member
      exact ⟨by rw [task.outputInitialized]; exact mem_validator_union id _ _ |>.mpr (Or.inl member),
        task.initializedBound id member⟩)
    (by
      intro id member
      exact ⟨by simpa [task.outputFresh] using member, task.freshBound id member⟩)
  exact ⟨introducedPrefix, retained, split, represented, models.distinct, models.realizes.values⟩

private def itemsFlow (task : ValidatorItemsTask callees items) : ValidatorFlowProtocol :=
  ⟨task.environment, task.outputEnvironment, task.checked.facts.fallsThrough,
    task.depth, task.result.type, false, items.any (fun item =>
      match item with | .declaration _ => false | .statement _ => true)⟩

private def compoundFlow (task : ValidatorStatementTask callees (.compound items)) :
    ValidatorFlowProtocol :=
  ⟨task.environment, task.outputEnvironment, task.checked.facts.fallsThrough,
    task.depth, task.result.type, true, true⟩

private theorem checked_compound_facts (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (items : List BlockItem) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input (.compound items) = .ok facts) :
    ∃ innerFuel inner, fuel = innerFuel + 1 ∧
      checkItems callees result innerFuel depth input items = .ok inner ∧
      facts = {inner with state := restoreScope input inner.state} := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    simp only [checkStmt] at accepted
    obtain ⟨inner, checked, returned⟩ := except_bind_success _ _ _ accepted
    exact ⟨fuel, inner, rfl, checked, by simpa [pure, Except.pure] using returned.symm⟩

private theorem compound_scope_certificate (parent : ValidatorStatementTask callees (.compound items))
    (child : ValidatorItemsTask callees items) (environment : child.environment = parent.environment)
    (input : child.checkedInput = parent.checkedInput) (result : child.result = parent.result)
    (depth : child.depth = parent.depth) (fuel : parent.checked.fuel = child.fuel + 1) :
    Nonempty (ValidatorScopeProtocol (itemsFlow child) (compoundFlow parent)) := by
  obtain ⟨innerFuel, facts, checkedFuel, checked, factsEq⟩ := checked_compound_facts
    callees parent.result parent.checked.fuel parent.depth parent.checkedInput items
    parent.checked.facts parent.checked.accepted
  have sameFuel : innerFuel = child.fuel := by omega
  subst innerFuel
  have childCheck := child.checked.accepted
  simp only [input, result, depth] at childCheck
  have same : facts = child.checked.facts := Except.ok.inj (checked.symm.trans childCheck)
  subst facts
  refine ⟨{
    introduced := child.introduced
    bindings := by
      change child.outputEnvironment.state.bindings = child.introduced ++ parent.environment.state.bindings
      rw [child.outputBindings, child.bindingsGrow, ← child.bindings, environment]
    layout := by simpa [itemsFlow, compoundFlow, environment] using child.outputLayout
    separate := by simpa [compoundFlow, environment] using child.separated
    outputBindings := by
      change parent.outputEnvironment.state.bindings = parent.environment.state.bindings
      simp only [parent.outputBindings, factsEq, restoreScope, parent.bindings]
    outputLayout := parent.outputLayout
    initialized := ?_
    fresh := ?_
    fallsThrough := by intro truth; simpa [itemsFlow, compoundFlow, factsEq] using truth
    abruptIntroduced := []
    abruptBindings := by simp [itemsFlow, compoundFlow, environment]
    abruptLayout := by simp [itemsFlow, compoundFlow, environment]
    abruptSeparate := by simp
    abruptInitialized := by
      intro id member
      change id ∈ parent.environment.state.initialized at member
      exact ⟨by simpa [itemsFlow, compoundFlow, environment] using member,
        by simpa [compoundFlow, environment] using
          (child.initializedBound id (by simpa [environment] using member))⟩
    abruptFresh := by
      intro id member
      change id ∈ parent.environment.freshConst at member
      exact ⟨by simpa [itemsFlow, compoundFlow, environment] using member,
        by simpa [compoundFlow, environment] using
          (child.freshBound id (by simpa [environment] using member))⟩
    depth := depth
    result := congrArg QualifiedType.type result
    allowsAbrupt := by intro _; rfl
    restoresLayout := rfl }⟩
  · intro id member
    change id ∈ parent.outputEnvironment.state.initialized at member
    rw [parent.outputInitialized, factsEq] at member
    rcases mem_validator_union id _ _ |>.mp member with incoming | filtered
    · refine ⟨?_, ?_⟩
      · change id ∈ child.outputEnvironment.state.initialized
        rw [child.outputInitialized]
        exact mem_validator_union id _ _ |>.mpr (Or.inl (by simpa [environment] using incoming))
      · simpa [compoundFlow, environment] using
          (child.initializedBound id (by simpa [environment] using incoming))
    · obtain ⟨initialized, bound⟩ := List.mem_filter.mp filtered
      refine ⟨?_, ?_⟩
      · change id ∈ child.outputEnvironment.state.initialized
        rw [child.outputInitialized]
        exact mem_validator_union id _ _ |>.mpr (Or.inr initialized)
      · simpa [compoundFlow, lookup, parent.bindings] using bound
  · intro id member
    change id ∈ parent.outputEnvironment.freshConst at member
    rw [parent.outputFresh] at member
    refine ⟨?_, ?_⟩
    · change id ∈ child.outputEnvironment.freshConst
      simpa [child.outputFresh, environment] using member
    · simpa [compoundFlow, environment] using
        (child.freshBound id (by simpa [environment] using member))

theorem validator_scope_frame_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (items : List BlockItem) (context : ValidatorFunctionContext program state.functionId callees)
    (parent : ValidatorStatementTask callees (.compound items))
    (functionResult : parent.result = context.function.returnType)
    (child : ValidatorItemsTask callees items) (environment : child.environment = parent.environment)
    (input : child.checkedInput = parent.checkedInput) (result : child.result = parent.result)
    (depth : child.depth = parent.depth) (fuel : parent.checked.fuel = child.fuel + 1)
    (flow : Flow) (delivered : child.output.Models (.flow flow) state.locals)
    (tail : ValidatorContinuation program entryType state.functionId parent.output state.continuation) :
    ValidatorMachineInvariant program entryType
      (resumeFlow state (.scope parent.environment.layout.length) flow) := by
  obtain ⟨certificate⟩ := compound_scope_certificate parent child environment input result depth fuel
  exact validator_scope_protocol_preserves program entryType checkedProgram state
    (itemsFlow child) (compoundFlow parent) certificate flow delivered tail

theorem validator_compound_control_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (items : List BlockItem) (context : ValidatorFunctionContext program state.functionId callees)
    (parent : ValidatorStatementTask callees (.compound items))
    (functionResult : parent.result = context.function.returnType)
    (models : parent.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId parent.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.compound items)) := by
  obtain ⟨fuel, facts, fuelEq, checked, _⟩ := checked_compound_facts callees parent.result
    parent.checked.fuel parent.depth parent.checkedInput items parent.checked.facts parent.checked.accepted
  obtain ⟨introduced, growth, _⟩ := checked_items_prefix callees parent.result fuel parent.depth
    parent.checkedInput items facts checked
  let child : ValidatorItemsTask callees items := {
    environment := parent.environment
    initializedBound := models.initialized_bound parent.environment state.locals
    freshBound := models.fresh_bound parent.environment state.locals
    checkedInput := parent.checkedInput
    bindings := parent.bindings
    initialized := parent.initialized
    result := parent.result
    depth := parent.depth
    fuel
    checked := ⟨facts, checked⟩
    introduced
    outputEnvironment := {
      state := ⟨facts.state.bindings, union parent.environment.state.initialized facts.state.initialized⟩
      layout := introduced ++ parent.environment.layout
      freshConst := parent.environment.freshConst }
    outputBindings := rfl
    bindingsGrow := growth
    outputInitialized := rfl
    outputLayout := rfl
    outputFresh := rfl }
  have length : state.locals.length = parent.environment.layout.length := by
    simpa [localLayout] using congrArg List.length models.layout
  simp only [enterStatement]
  rw [length]
  exact .running _ child.output checkedProgram
    (.items callees items context child functionResult state.locals models)
    (.frame (.scope state.functionId callees items context parent functionResult child rfl rfl rfl rfl fuelEq) tail)

private theorem item_declaration_checked (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (object : Object) (facts : FlowFacts)
    (accepted : validatorItemStep callees result fuel depth ⟨input, true⟩
      (.declaration object) = .ok facts) :
    ∃ output, declare callees fuel input object = .ok output ∧ facts = ⟨output, true⟩ := by
  unfold validatorItemStep at accepted
  obtain ⟨output, checked, same⟩ := except_bind_success _ _ _ accepted
  exact ⟨output, checked, by simpa [pure, Except.pure] using same.symm⟩

private theorem item_statement_checked (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (statement : Stmt) (facts : FlowFacts)
    (accepted : validatorItemStep callees result fuel depth ⟨input, true⟩
      (.statement statement) = .ok facts) :
    checkStmt callees result fuel depth input statement = .ok facts := by
  unfold validatorItemStep at accepted
  obtain ⟨output, checked, same⟩ := except_bind_success _ _ _ accepted
  have same : facts = output := by
    simpa [pure, Except.pure] using same.symm
  simpa [same] using checked

theorem validator_items_control_preserves (program : Program) (entryType : IntType)
    (checkedProgram : CheckedSourceProgram program) (state : State) (callees : List Function)
    (items : List BlockItem) (context : ValidatorFunctionContext program state.functionId callees)
    (parent : ValidatorItemsTask callees items)
    (functionResult : parent.result = context.function.returnType)
    (models : parent.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId parent.output state.continuation)
    (next : State) (stepped : step program {state with control := .items items} = some next) :
    ValidatorMachineInvariant program entryType next := by
  cases items with
  | nil =>
    have facts : parent.checked.facts = ⟨parent.checkedInput, true⟩ := by
      simpa [checkItems, pure, Except.pure] using parent.checked.accepted.symm
    have introduced : parent.introduced = [] := List.append_cancel_right
      (bs := parent.checkedInput.bindings)
      (show parent.introduced ++ parent.checkedInput.bindings = [] ++ parent.checkedInput.bindings from
        by simpa [facts] using parent.bindingsGrow.symm)
    have entails : parent.environment.Entails parent.outputEnvironment := {
      bindings := by intro id; simp [lookup, parent.outputBindings, facts, parent.bindings]
      layout := by simpa [introduced] using parent.outputLayout.symm
      initialized := by
        intro id member
        rw [parent.outputInitialized, facts] at member
        rcases mem_validator_union id _ _ |>.mp member with member | member
        · exact member
        · exact parent.initialized id member
      fresh := by intro id member; simpa [parent.outputFresh] using member }
    have represented := models.weaken parent.environment parent.outputEnvironment entails
    have same : next = {state with control := .flow .normal} := by
      simpa [step] using stepped.symm
    rw [same]
    exact .running _ parent.output checkedProgram
      (.delivered _ _ _ (by simpa [ValidatorItemsTask.output, ValidatorProtocol.Models,
        ValidatorFlowProtocol.Models, facts] using represented)) tail
  | cons first remaining =>
    obtain ⟨head, suffix, headChecked, _, _⟩ := checked_items_split callees parent.result
      parent.fuel parent.depth parent.checkedInput first remaining parent.checked.facts parent.checked.accepted
    cases first with
    | declaration object =>
      have same : next = {state with
          control := .object object
          continuation := .sequence remaining :: state.continuation} := by
        simpa [step] using stepped.symm
      rw [same]
      obtain ⟨output, accepted, _⟩ := item_declaration_checked callees parent.result parent.fuel
        parent.depth parent.checkedInput object head headChecked
      let child : ValidatorObjectTask callees object := {
        environment := parent.environment
        checkedInput := parent.checkedInput
        bindings := parent.bindings
        initialized := parent.initialized
        result := parent.result
        depth := parent.depth
        fuel := parent.fuel
        checked := ⟨output, accepted⟩
        outputEnvironment := {
          state := ⟨output.bindings, union parent.environment.state.initialized output.initialized⟩
          layout := (object.symbol, object.type) :: parent.environment.layout
          freshConst := parent.environment.freshConst }
        outputBindings := rfl
        outputInitialized := rfl
        outputLayout := rfl
        outputFresh := rfl }
      exact .running _ child.output checkedProgram
        (.object callees object context child functionResult state.locals models)
        (.frame (.sequence state.functionId callees (.declaration object) remaining context
          parent functionResult (.declaration child) rfl rfl rfl rfl rfl) tail)
    | statement statement =>
      have same : next = {state with
          control := .statement statement
          continuation := .sequence remaining :: state.continuation} := by
        simpa [step] using stepped.symm
      rw [same]
      have accepted := item_statement_checked callees parent.result parent.fuel parent.depth
        parent.checkedInput statement head headChecked
      let child : ValidatorStatementTask callees statement := {
        environment := parent.environment
        checkedInput := parent.checkedInput
        bindings := parent.bindings
        initialized := parent.initialized
        result := parent.result
        depth := parent.depth
        checked := ⟨parent.fuel, head, accepted⟩
        outputEnvironment := {
          state := ⟨head.state.bindings, union parent.environment.state.initialized head.state.initialized⟩
          layout := parent.environment.layout
          freshConst := parent.environment.freshConst }
        outputBindings := rfl
        outputInitialized := rfl
        outputLayout := rfl
        outputFresh := rfl }
      exact .running _ child.output checkedProgram
        (.statement callees statement context child functionResult state.locals models)
        (.frame (.sequence state.functionId callees (.statement statement) remaining context
          parent functionResult (.statement child) rfl rfl rfl rfl rfl) tail)

end EvoIR.Core.Source
