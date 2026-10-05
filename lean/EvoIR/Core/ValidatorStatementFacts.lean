import EvoIR.Core.ValidatorFacts

/-! Exact successful statement-check equations used by actual statement entry.
No new source validator is introduced. -/
namespace EvoIR.Core.Source

def ValidatorStatementTask.ofChecked (callees : List Function) (statement : Stmt)
    (environment : ValidatorEnvironment) (input : LocalState)
    (bindings : environment.state.bindings = input.bindings)
    (initialized : ∀ id ∈ input.initialized, id ∈ environment.state.initialized)
    (result : QualifiedType) (depth : Nat)
    (checked : CheckedStatement callees result depth input statement) :
    ValidatorStatementTask callees statement := {
  environment
  checkedInput := input
  bindings
  initialized
  result
  depth
  checked
  outputEnvironment := ⟨⟨checked.facts.state.bindings,
    union environment.state.initialized checked.facts.state.initialized⟩,
    environment.layout, environment.freshConst⟩
  outputBindings := rfl
  outputInitialized := rfl
  outputLayout := rfl
  outputFresh := rfl
}

theorem ValidatorStatementTask.afterExpression_entails_output
    (task : ValidatorStatementTask callees statement) (facts : ExpressionFacts)
    (subset : ∀ id ∈ task.checked.facts.state.initialized,
      id ∈ task.environment.state.initialized ∨ id ∈ facts.initialized) :
    (task.environment.afterExpression facts).Entails task.outputEnvironment := by
  refine ⟨fun id => (task.output_entails.bindings id).symm, task.outputLayout.symm, ?_, ?_⟩
  · intro id member
    rw [task.outputInitialized] at member
    obtain old | fresh := mem_validator_union id _ _ |>.mp member
    · exact mem_validator_union id _ _ |>.mpr (Or.inl old)
    · exact mem_validator_union id _ _ |>.mpr (subset id fresh)
  · intro id member
    simpa [task.outputFresh, ValidatorEnvironment.afterExpression] using member

theorem ValidatorStatementTask.environment_entails_output
    (task : ValidatorStatementTask callees statement)
    (subset : ∀ id ∈ task.checked.facts.state.initialized, id ∈ task.environment.state.initialized) :
    task.environment.Entails task.outputEnvironment := by
  refine ⟨fun id => (task.output_entails.bindings id).symm, task.outputLayout.symm, ?_, ?_⟩
  · intro id member
    rw [task.outputInitialized] at member
    obtain old | fresh := mem_validator_union id _ _ |>.mp member
    · exact old
    · exact subset id fresh
  · intro id member
    simpa [task.outputFresh] using member

theorem ValidatorProtocol.restored_flow_entails_when (strong weak : ValidatorFlowProtocol)
    (strongRestored : strong.restoresLayout = true) (weakRestored : weak.restoresLayout = true)
    (normal : strong.fallsThrough = true → weak.fallsThrough = true)
    (normalModels : strong.fallsThrough = true → strong.normal.Entails weak.normal)
    (before : strong.before.Entails weak.before)
    (abrupt : strong.allowsAbrupt = true → weak.allowsAbrupt = true)
    (depth : strong.loopDepth = weak.loopDepth) (result : strong.resultType = weak.resultType) :
    (ValidatorProtocol.flow strong).Entails (.flow weak) := by
  intro control locals models
  cases control <;> try simp [ValidatorProtocol.Models] at models
  rename_i flow
  cases flow with
  | normal => exact ⟨normal models.1, models.2.weaken strong.normal weak.normal (normalModels models.1)⟩
  | «break» | «continue» =>
    obtain ⟨allowed, inLoop, localModels⟩ := models
    simp only [strongRestored, ↓reduceIte] at localModels
    refine ⟨abrupt allowed, by simpa [depth] using inLoop, ?_⟩
    simp only [weakRestored, ↓reduceIte]
    exact localModels.weaken strong.before weak.before before
  | returned value =>
    obtain ⟨allowed, typed, localModels⟩ := models
    simp only [strongRestored, ↓reduceIte] at localModels
    refine ⟨abrupt allowed, typed.trans result, ?_⟩
    simp only [weakRestored, ↓reduceIte]
    exact localModels.weaken strong.before weak.before before

theorem ValidatorProtocol.restored_flow_entails (strong weak : ValidatorFlowProtocol)
    (strongRestored : strong.restoresLayout = true) (weakRestored : weak.restoresLayout = true)
    (normal : strong.fallsThrough = true → weak.fallsThrough = true)
    (normalModels : strong.normal.Entails weak.normal)
    (before : strong.before.Entails weak.before)
    (abrupt : strong.allowsAbrupt = true → weak.allowsAbrupt = true)
    (depth : strong.loopDepth = weak.loopDepth) (result : strong.resultType = weak.resultType) :
    (ValidatorProtocol.flow strong).Entails (.flow weak) :=
  restored_flow_entails_when strong weak strongRestored weakRestored normal (fun _ => normalModels)
    before abrupt depth result

theorem checked_null_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input .null = .ok facts) :
    facts = ⟨input, true⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel => simpa [checkStmt, pure, Except.pure] using accepted.symm

theorem checked_abrupt_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (statement : Stmt) (facts : FlowFacts)
    (abrupt : statement = .break ∨ statement = .continue)
    (accepted : checkStmt callees result fuel depth input statement = .ok facts) :
    depth > 0 ∧ facts = ⟨input, false⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    rcases abrupt with rfl | rfl
    all_goals
      simp only [checkStmt] at accepted
      obtain ⟨checkedUnit, checked, returned⟩ := except_bind_success _ _ _ accepted
      cases checkedUnit
      exact ⟨by simpa using require_success _ _ checked,
        by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_expression_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (expression : Expr) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input (.expression expression) = .ok facts) :
    ∃ innerFuel expressionFacts, fuel = innerFuel + 1 ∧
      checkExpr callees innerFuel input expression = .ok expressionFacts ∧
      facts = ⟨{input with initialized := expressionFacts.initialized}, true⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    simp only [checkStmt] at accepted
    obtain ⟨state, checked, returned⟩ := except_bind_success _ _ _ accepted
    obtain ⟨expressionFacts, expressionChecked, rfl⟩ := checked_expression_state callees fuel input state expression checked
    exact ⟨fuel, expressionFacts, rfl, expressionChecked, by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_return_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (destination : QualifiedType)
    (expression : Expr) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input (.return destination expression) = .ok facts) :
    ∃ innerFuel expressionFacts, fuel = innerFuel + 1 ∧ destination = result ∧
      checkExpr callees innerFuel input expression = .ok expressionFacts ∧
      facts = ⟨{input with initialized := expressionFacts.initialized}, false⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    simp only [checkStmt] at accepted
    obtain ⟨checkedUnit, signature, remaining⟩ := except_bind_success _ _ _ accepted
    cases checkedUnit
    obtain ⟨state, checked, returned⟩ := except_bind_success _ _ _ remaining
    obtain ⟨expressionFacts, expressionChecked, rfl⟩ := checked_expression_state callees fuel input state expression checked
    exact ⟨fuel, expressionFacts, rfl, by simpa using require_success _ _ signature,
      expressionChecked, by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_while_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (condition : Expr) (body : Stmt) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input (.while condition body) = .ok facts) :
    ∃ innerFuel conditionFacts bodyFacts, fuel = innerFuel + 1 ∧
      checkExpr callees innerFuel input condition = .ok conditionFacts ∧
      checkStmt callees result innerFuel (depth + 1)
        {input with initialized := conditionFacts.initialized} body = .ok bodyFacts ∧
      facts = ⟨{input with initialized := conditionFacts.initialized}, true⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    simp only [checkStmt] at accepted
    obtain ⟨incoming, conditionChecked, remaining⟩ := except_bind_success _ _ _ accepted
    obtain ⟨conditionFacts, expressionChecked, rfl⟩ :=
      checked_expression_state callees fuel input incoming condition conditionChecked
    obtain ⟨bodyFacts, bodyChecked, returned⟩ := except_bind_success _ _ _ remaining
    exact ⟨fuel, conditionFacts, bodyFacts, rfl, expressionChecked, bodyChecked,
      by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_do_while_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (body : Stmt) (condition : Expr) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input (.doWhile body condition) = .ok facts) :
    ∃ innerFuel bodyFacts conditionFacts, fuel = innerFuel + 1 ∧
      checkStmt callees result innerFuel (depth + 1) input body = .ok bodyFacts ∧
      checkExpr callees innerFuel input condition = .ok conditionFacts ∧ facts = ⟨input, true⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    simp only [checkStmt] at accepted
    obtain ⟨bodyFacts, bodyChecked, remaining⟩ := except_bind_success _ _ _ accepted
    obtain ⟨conditionState, conditionChecked, returned⟩ := except_bind_success _ _ _ remaining
    obtain ⟨conditionFacts, expressionChecked, _⟩ :=
      checked_expression_state callees fuel input conditionState condition conditionChecked
    exact ⟨fuel, bodyFacts, conditionFacts, rfl, bodyChecked, expressionChecked,
      by simpa [pure, Except.pure] using returned.symm⟩

def validatorBranchInitialized (yes no : FlowFacts) : List Nat :=
  if yes.fallsThrough then
    if no.fallsThrough then intersection yes.state.initialized no.state.initialized
    else yes.state.initialized
  else no.state.initialized

theorem checked_branch_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (condition : Expr) (yes : Stmt) (no : Option Stmt)
    (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input (.ite condition yes no) = .ok facts) :
    ∃ innerFuel conditionFacts yesFacts noFuel noFacts,
      fuel = innerFuel + 1 ∧ checkExpr callees innerFuel input condition = .ok conditionFacts ∧
      checkStmt callees result innerFuel depth
        {input with initialized := conditionFacts.initialized} yes = .ok yesFacts ∧
      checkStmt callees result noFuel depth
        {input with initialized := conditionFacts.initialized} (no.getD .null) = .ok noFacts ∧
      facts = ⟨{input with initialized := validatorBranchInitialized yesFacts noFacts},
        yesFacts.fallsThrough || noFacts.fallsThrough⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    cases no with
    | none =>
      simp only [checkStmt] at accepted
      obtain ⟨incoming, conditionChecked, remaining⟩ := except_bind_success _ _ _ accepted
      obtain ⟨conditionFacts, expressionChecked, rfl⟩ :=
        checked_expression_state callees fuel input incoming condition conditionChecked
      obtain ⟨yesFacts, yesChecked, returned⟩ := except_bind_success _ _ _ remaining
      refine ⟨fuel, conditionFacts, yesFacts, 1,
        ⟨{input with initialized := conditionFacts.initialized}, true⟩,
        rfl, expressionChecked, yesChecked, rfl, ?_⟩
      simpa [pure, Except.pure, bind, Except.bind, validatorBranchInitialized] using returned.symm
    | some no =>
      simp only [checkStmt] at accepted
      obtain ⟨incoming, conditionChecked, remaining⟩ := except_bind_success _ _ _ accepted
      obtain ⟨conditionFacts, expressionChecked, rfl⟩ :=
        checked_expression_state callees fuel input incoming condition conditionChecked
      obtain ⟨yesFacts, yesChecked, remaining⟩ := except_bind_success _ _ _ remaining
      obtain ⟨noFacts, noChecked, returned⟩ := except_bind_success _ _ _ remaining
      exact ⟨fuel, conditionFacts, yesFacts, fuel, noFacts, rfl, expressionChecked, yesChecked,
        noChecked, by simpa [pure, Except.pure, validatorBranchInitialized] using returned.symm⟩

theorem validator_branch_yes_initialized (yes no : FlowFacts) (falls : yes.fallsThrough = true)
    (id : Nat) (member : id ∈ validatorBranchInitialized yes no) : id ∈ yes.state.initialized := by
  simp only [validatorBranchInitialized, falls, ↓reduceIte] at member
  split at member
  · exact (mem_validator_intersection id _ _ |>.mp member).1
  · exact member

theorem validator_branch_no_initialized (yes no : FlowFacts) (falls : no.fallsThrough = true)
    (id : Nat) (member : id ∈ validatorBranchInitialized yes no) : id ∈ no.state.initialized := by
  simp only [validatorBranchInitialized, falls, ↓reduceIte] at member
  split at member
  · exact (mem_validator_intersection id _ _ |>.mp member).2
  · exact member

end EvoIR.Core.Source
