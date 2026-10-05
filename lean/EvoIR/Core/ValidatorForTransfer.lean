import EvoIR.Core.ValidatorForFacts
import EvoIR.Core.ValidatorScopeFacts
import EvoIR.Core.ValidatorBranchTransfer

/-! Actual for-loop initializer, condition, increment and captured scope.
Initializer certificates are built from the exact successful checker fold. -/
namespace EvoIR.Core.Source

def validatorForInitialControl : ForInit → Control
  | .empty => .flow .normal
  | .expression expression => .statement (.expression expression)
  | .declarations objects => .items (objects.map BlockItem.declaration)

private theorem declaration_item_fold (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (objects : List Object) (flow : FlowFacts) :
    (objects.map BlockItem.declaration).foldlM (validatorItemStep callees result fuel depth) flow =
      (objects.foldlM (declare callees fuel) flow.state).map (fun state => {flow with state}) := by
  induction objects generalizing flow with
  | nil => simp [Except.map, pure, Except.pure]
  | cons object rest ih =>
    simp only [List.map_cons, List.foldlM_cons, validatorItemStep]
    cases declared : declare callees fuel flow.state object with
    | error message => simp [declared, bind, Except.bind, Except.map]
    | ok state =>
      simp only [declared, bind, Except.bind, pure, Except.pure]
      rw [ih]

theorem checked_declaration_items (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input output : LocalState) (objects : List Object)
    (accepted : objects.foldlM (declare callees fuel) input = .ok output) :
    checkItems callees result fuel depth input (objects.map BlockItem.declaration) = .ok ⟨output, true⟩ := by
  change (objects.map BlockItem.declaration).foldlM (validatorItemStep callees result fuel depth)
    ⟨input, true⟩ = _
  rw [declaration_item_fold, accepted]
  rfl

structure ValidatorForInitializer (program : Program) (entryType : IntType) (functionId : Nat)
    (callees : List Function) (environment : ValidatorEnvironment) (input : LocalState)
    (result : QualifiedType) (depth fuel : Nat) (init : ForInit) (start : LocalState)
    (locals : Locals) where
  normal : ValidatorEnvironment
  protocol : ValidatorFlowProtocol
  protocolNormal : protocol.normal = normal
  onlyNormal : protocol.allowsAbrupt = false
  typing : ValidatorControl program entryType functionId
    (validatorForInitialControl init) locals (.flow protocol)
  bindings : normal.state.bindings = start.bindings
  initialized : ∀ id ∈ start.initialized, id ∈ normal.state.initialized
  introduced : LocalLayout
  bindingsGrow : normal.state.bindings = introduced ++ environment.state.bindings
  layoutGrow : normal.layout = introduced ++ environment.layout
  separate : ∀ id, (lookup environment.state id).isSome = true →
    introduced.find? (fun pair => pair.1 == id) = none
  oldInitialized : ∀ id ∈ environment.state.initialized, id ∈ normal.state.initialized
  fresh : normal.freshConst = environment.freshConst

theorem validator_for_initializer (program : Program) (entryType : IntType) (functionId : Nat)
    (callees : List Function) (context : ValidatorFunctionContext program functionId callees)
    (environment : ValidatorEnvironment) (input : LocalState) (locals : Locals)
    (models : environment.Models locals) (bindings : environment.state.bindings = input.bindings)
    (initialized : ∀ id ∈ input.initialized, id ∈ environment.state.initialized)
    (result : QualifiedType) (functionResult : result = context.function.returnType)
    (depth fuel : Nat) (init : ForInit) (start : LocalState)
    (accepted : validatorForInitCheck callees fuel input init = .ok start) :
    Nonempty (ValidatorForInitializer program entryType functionId callees environment input result
      depth fuel init start locals) := by
  cases init with
  | empty =>
    have same : input = start := by simpa [validatorForInitCheck, pure, Except.pure] using accepted
    refine ⟨{
      normal := environment
      protocol := ⟨environment, environment, true, depth, result.type, true, false⟩
      protocolNormal := rfl
      onlyNormal := rfl
      typing := .delivered _ _ _ ⟨rfl, models⟩
      bindings := bindings.trans (congrArg LocalState.bindings same)
      initialized := by simpa [← same] using initialized
      introduced := []
      bindingsGrow := rfl
      layoutGrow := rfl
      separate := by simp
      oldInitialized := fun _ member => member
      fresh := rfl
    }⟩
  | expression expression =>
    have expressionChecked : expressionState callees fuel input expression = .ok start := accepted
    let certificate : CheckedStatement callees result depth input (.expression expression) :=
      ⟨fuel + 1, ⟨start, true⟩, by simp [checkStmt, expressionChecked, pure, Except.pure, bind, Except.bind]⟩
    let child := ValidatorStatementTask.ofChecked callees (.expression expression) environment input
      bindings initialized result depth certificate
    refine ⟨{
      normal := child.outputEnvironment
      protocol := ⟨environment, child.outputEnvironment, true, depth, result.type, true, false⟩
      protocolNormal := rfl
      onlyNormal := rfl
      typing := by
        simpa [validatorForInitialControl, ValidatorStatementTask.output, child,
          ValidatorStatementTask.ofChecked, certificate, statementAllowsAbrupt] using
          ValidatorControl.statement callees (.expression expression) context child functionResult locals models
      bindings := rfl
      initialized := by
        intro id member
        exact mem_validator_union id _ _ |>.mpr (Or.inr member)
      introduced := []
      bindingsGrow := by
        simpa [child, ValidatorStatementTask.ofChecked, certificate] using
          (checked_expression_state_bindings callees fuel input start expression expressionChecked).trans bindings.symm
      layoutGrow := rfl
      separate := by simp
      oldInitialized := by
        intro id member
        exact mem_validator_union id _ _ |>.mpr (Or.inl member)
      fresh := rfl
    }⟩
  | declarations objects =>
    have declared : objects.foldlM (declare callees fuel) input = .ok start := accepted
    have checked := checked_declaration_items callees result fuel depth input start objects declared
    obtain ⟨introduced, extension, separated⟩ := checked_items_prefix callees result fuel depth input
      (objects.map BlockItem.declaration) ⟨start, true⟩ checked
    let output : ValidatorEnvironment :=
      ⟨⟨start.bindings, union environment.state.initialized start.initialized⟩,
        introduced ++ environment.layout, environment.freshConst⟩
    let child : ValidatorItemsTask callees (objects.map BlockItem.declaration) := {
      environment
      initializedBound := models.initialized_bound environment locals
      freshBound := models.fresh_bound environment locals
      checkedInput := input
      bindings
      initialized
      result
      depth
      fuel
      checked := ⟨⟨start, true⟩, checked⟩
      introduced
      outputEnvironment := output
      outputBindings := rfl
      bindingsGrow := extension
      outputInitialized := rfl
      outputLayout := rfl
      outputFresh := rfl
    }
    have noAbrupt : (objects.map BlockItem.declaration).any (fun item =>
        match item with | .declaration _ => false | .statement _ => true) = false := by
      induction objects <;> simp_all
    refine ⟨{
      normal := output
      protocol := ⟨environment, output, true, depth, result.type, false, false⟩
      protocolNormal := rfl
      onlyNormal := rfl
      typing := by
        have typed : ValidatorControl program entryType functionId
            (.items (objects.map BlockItem.declaration)) locals child.output :=
          .items callees (objects.map BlockItem.declaration) context child functionResult locals models
        change ValidatorControl program entryType functionId (.items (objects.map BlockItem.declaration)) locals
          (.flow ⟨environment, output, true, depth, result.type, false,
            (objects.map BlockItem.declaration).any (fun item =>
              match item with | .declaration _ => false | .statement _ => true)⟩) at typed
        rw [noAbrupt] at typed
        exact typed
      bindings := rfl
      initialized := fun id member => mem_validator_union id _ _ |>.mpr (Or.inr member)
      introduced
      bindingsGrow := by simpa [output, bindings] using extension
      layoutGrow := rfl
      separate := by
        intro id bound
        exact separated id (by simpa [lookup, bindings] using bound)
      oldInitialized := fun id member => mem_validator_union id _ _ |>.mpr (Or.inl member)
      fresh := rfl
    }⟩

theorem checked_for_condition_expression (callees : List Function) (fuel : Nat)
    (start incoming : LocalState) (condition : Option Expr)
    (accepted : validatorForConditionCheck callees fuel start condition = .ok incoming) :
    ∃ conditionFuel facts,
      checkExpr callees conditionFuel start (condition.getD (.literal .int 1)) = .ok facts ∧
      incoming = {start with initialized := facts.initialized} := by
  cases condition with
  | none =>
    have same : start = incoming := by simpa [validatorForConditionCheck, pure, Except.pure] using accepted
    refine ⟨1, ⟨start.initialized, [], []⟩, ?_, same.symm⟩
    have canonical : 1 < 2 ^ IntType.int.width := by decide
    simp [checkExpr, require, canonical, Functor.map, Except.map, pure, Except.pure, bind, Except.bind]
  | some expression =>
    obtain ⟨facts, checked, incomingEq⟩ := checked_expression_state callees fuel start incoming expression accepted
    exact ⟨fuel, facts, checked, incomingEq⟩

theorem validator_for_control_preserves (program : Program) (entryType : IntType)
    (checked : CheckedSourceProgram program) (state : State) (callees : List Function)
    (init : ForInit) (condition increment : Option Expr) (body : Stmt)
    (context : ValidatorFunctionContext program state.functionId callees)
    (task : ValidatorStatementTask callees (.for init condition increment body))
    (result : task.result = context.function.returnType) (models : task.environment.Models state.locals)
    (tail : ValidatorContinuation program entryType state.functionId task.output state.continuation) :
    ValidatorMachineInvariant program entryType (enterStatement state (.for init condition increment body)) := by
  classical
  obtain ⟨fuel, start, incoming, bodyFacts, fuelEq, startChecked, conditionChecked,
    bodyChecked, incrementChecked, resultFacts⟩ := checked_for_statement callees task.result
      task.checked.fuel task.depth task.checkedInput init condition increment body task.checked.facts task.checked.accepted
  obtain ⟨initial⟩ := validator_for_initializer program entryType state.functionId callees context task.environment
    task.checkedInput state.locals models task.bindings task.initialized task.result result task.depth fuel init start startChecked
  obtain ⟨conditionFuel, conditionFacts, conditionAccepted, incomingEq⟩ :=
    checked_for_condition_expression callees fuel start incoming condition conditionChecked
  let conditionTask : ValidatorExpressionTask callees (condition.getD (.literal .int 1)) :=
    ⟨initial.normal, start, ⟨conditionFuel, conditionFacts, conditionAccepted⟩,
      fun id => by simp [lookup, initial.bindings], initial.initialized⟩
  let tested := initial.normal.afterExpression conditionFacts
  have testedBindings : tested.state.bindings = incoming.bindings := by
    simpa [tested, ValidatorEnvironment.afterExpression, incomingEq] using initial.bindings
  have testedInitialized : ∀ id ∈ incoming.initialized, id ∈ tested.state.initialized := by
    intro id member
    rw [incomingEq] at member
    exact mem_validator_union id _ _ |>.mpr (Or.inr member)
  let bodyTask := ValidatorStatementTask.ofChecked callees body tested incoming testedBindings testedInitialized
    task.result (task.depth + 1) ⟨fuel, bodyFacts, bodyChecked⟩
  have incrementFacts : ∀ expression, increment = some expression →
      ∃ facts, checkExpr callees fuel incoming expression = .ok facts := by
    intro expression present
    obtain ⟨out, accepted⟩ := incrementChecked expression present
    obtain ⟨facts, accepted, _⟩ := checked_expression_state callees fuel incoming out expression accepted
    exact ⟨facts, accepted⟩
  let incrementTask : ∀ expression, increment = some expression → ValidatorExpressionTask callees expression :=
    fun expression present =>
      ⟨tested, incoming,
        ⟨fuel, Classical.choose (incrementFacts expression present),
          Classical.choose_spec (incrementFacts expression present)⟩,
        fun id => by simp [lookup, testedBindings], testedInitialized⟩
  let loop : ValidatorLoop callees := {
    condition := condition.getD (.literal .int 1)
    increment
    body
    conditionTask
    bodyTask
    incrementTask
    before := initial.normal
    exit := tested
    result := task.result
    outerDepth := task.depth
    conditionBefore := ValidatorEnvironment.Entails.refl initial.normal
    testedExit := ValidatorEnvironment.Entails.refl tested
    testedBody := ValidatorEnvironment.Entails.refl tested
    bodyBefore := initial.normal.afterExpression_entails conditionFacts
    bodyExit := ValidatorEnvironment.Entails.refl tested
    bodyNormalBefore := bodyTask.output_entails.trans _ _ _
      (initial.normal.afterExpression_entails conditionFacts)
    bodyResult := rfl
    bodyDepth := rfl
    incrementBefore := fun _ _ => ValidatorEnvironment.Entails.refl tested
    incrementNormal := fun _ _ => bodyTask.output_entails
    incrementAfter := fun expression present =>
      (tested.afterExpression_entails (incrementTask expression present).checked.facts).trans _ _ _
        (initial.normal.afterExpression_entails conditionFacts)
  }
  let input : ValidatorFlowProtocol :=
    ⟨initial.normal, tested, true, task.depth, task.result.type, true, true⟩
  let output : ValidatorFlowProtocol :=
    ⟨task.environment, task.outputEnvironment, task.checked.facts.fallsThrough,
      task.depth, task.result.type, true, true⟩
  have scope : ValidatorScopeProtocol input output := {
    introduced := initial.introduced
    bindings := initial.bindingsGrow
    layout := initial.layoutGrow
    separate := initial.separate
    outputBindings := by
      rw [task.outputBindings, resultFacts]
      exact task.bindings.symm
    outputLayout := task.outputLayout
    initialized := by
      intro id member
      change id ∈ task.outputEnvironment.state.initialized at member
      change id ∈ tested.state.initialized ∧ (lookup task.environment.state id).isSome = true
      rw [task.outputInitialized] at member
      obtain old | fresh := mem_validator_union id _ _ |>.mp member
      · exact ⟨mem_validator_union id _ _ |>.mpr (Or.inl (initial.oldInitialized id old)),
          models.initialized_bound task.environment state.locals id old⟩
      · rw [resultFacts] at fresh
        obtain ⟨present, bound⟩ := List.mem_filter.mp fresh
        exact ⟨testedInitialized id present, by simpa [lookup, task.bindings] using bound⟩
    fresh := by
      intro id member
      change id ∈ task.outputEnvironment.freshConst at member
      change id ∈ tested.freshConst ∧ (lookup task.environment.state id).isSome = true
      have old : id ∈ task.environment.freshConst := by simpa [task.outputFresh] using member
      exact ⟨by simpa [tested, ValidatorEnvironment.afterExpression, initial.fresh] using old,
        models.fresh_bound task.environment state.locals id old⟩
    fallsThrough := by intro _; simp [output, resultFacts]
    abruptIntroduced := initial.introduced
    abruptBindings := initial.bindingsGrow
    abruptLayout := initial.layoutGrow
    abruptSeparate := initial.separate
    abruptInitialized := fun id member =>
      ⟨initial.oldInitialized id member, models.initialized_bound task.environment state.locals id member⟩
    abruptFresh := by
      intro id member
      change id ∈ task.environment.freshConst at member
      change id ∈ initial.normal.freshConst ∧ (lookup task.environment.state id).isSome = true
      exact ⟨by simpa [initial.fresh] using member,
        models.fresh_bound task.environment state.locals id member⟩
    depth := rfl
    result := rfl
    allowsAbrupt := fun _ => rfl
    restoresLayout := rfl
  }
  have startAfter : initial.protocol.normal.Entails loop.before := by
    rw [initial.protocolNormal]
    exact ValidatorEnvironment.Entails.refl initial.normal
  have continuation : ValidatorContinuation program entryType state.functionId (.flow initial.protocol)
      (.startLoop (condition.getD (.literal .int 1)) increment body ::
        .scope task.environment.layout.length :: state.continuation) :=
    .frame (.startLoop state.functionId callees context loop result initial.protocol startAfter initial.onlyNormal)
      (.frame (.scopeProtocol state.functionId input output scope) tail)
  have depth : state.locals.length = task.environment.layout.length := by
    simpa [localLayout] using congrArg List.length models.layout
  have entered : enterStatement state (.for init condition increment body) =
      {state with control := validatorForInitialControl init, continuation := .startLoop (condition.getD (.literal .int 1)) increment body :: .scope state.locals.length :: state.continuation} := by
    cases init <;> rfl
  rw [entered, depth]
  exact ValidatorMachineInvariant.running
    {state with control := validatorForInitialControl init, continuation := .startLoop (condition.getD (.literal .int 1)) increment body :: .scope task.environment.layout.length :: state.continuation}
    (.flow initial.protocol) checked initial.typing continuation

end EvoIR.Core.Source
