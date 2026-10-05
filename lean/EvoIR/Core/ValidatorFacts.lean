import EvoIR.Core.ValidatorInvariant

/-! Success inversion and concrete store entailment for the control/frame
preservation obligations of `ValidatorMachineInvariant`. All syntax premises
below are extracted from the actual `checkExpr`/`checkStmt` equations. -/
namespace EvoIR.Core.Source

theorem except_bind_success {α β : Type} (first : Except String α)
    (next : α → Except String β) (result : β)
    (accepted : (first >>= next) = .ok result) :
    ∃ value, first = .ok value ∧ next value = .ok result := by
  cases first with
  | error message => simp [bind, Except.bind] at accepted
  | ok value => exact ⟨value, rfl, by simpa [bind, Except.bind] using accepted⟩

theorem require_success (condition : Bool) (message : String)
    (accepted : require condition message = .ok ()) : condition = true := by
  cases condition <;> simp_all [require]

@[simp] theorem mem_validator_union (id : Nat) (left right : List Nat) :
    id ∈ union left right ↔ id ∈ left ∨ id ∈ right := by
  by_cases present : id ∈ left <;> simp [union, present]

@[simp] theorem mem_validator_intersection (id : Nat) (left right : List Nat) :
    id ∈ intersection left right ↔ id ∈ left ∧ id ∈ right := by
  simp [intersection]

theorem ValidatorEnvironment.afterExpression_entails (environment : ValidatorEnvironment)
    (facts : ExpressionFacts) : (environment.afterExpression facts).Entails environment := by
  refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
  intro id member
  exact mem_validator_union id _ _ |>.mpr (Or.inl member)

theorem ValidatorEnvironment.Entails.refl (environment : ValidatorEnvironment) :
    environment.Entails environment :=
  ⟨fun _ => rfl, rfl, fun _ member => member, fun _ member => member⟩

theorem ValidatorProtocol.value_entails (strong weak : ValidatorEnvironment) (type : IntType)
    (entails : strong.Entails weak) :
    (ValidatorProtocol.value strong type).Entails (.value weak type) := by
  intro control locals models
  cases control <;> try simp [ValidatorProtocol.Models] at models
  rename_i value
  exact ⟨models.1, models.2.weaken strong weak entails⟩

theorem ValidatorEnvironment.entails_afterExpression (environment : ValidatorEnvironment)
    (facts : ExpressionFacts) (subset : ∀ id ∈ facts.initialized, id ∈ environment.state.initialized) :
    environment.Entails (environment.afterExpression facts) := by
  refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
  intro id member
  obtain old | fresh := mem_validator_union id _ _ |>.mp member
  · exact old
  · exact subset id fresh

theorem ValidatorEnvironment.afterExpression_entails_afterExpression (environment : ValidatorEnvironment)
    (strong weak : ExpressionFacts)
    (subset : ∀ id ∈ weak.initialized, id ∈ environment.state.initialized ∨ id ∈ strong.initialized) :
    (environment.afterExpression strong).Entails (environment.afterExpression weak) := by
  refine ⟨fun _ => rfl, rfl, ?_, fun _ member => member⟩
  intro id member
  obtain old | added := mem_validator_union id _ _ |>.mp member
  · exact mem_validator_union id _ _ |>.mpr (Or.inl old)
  · exact mem_validator_union id _ _ |>.mpr (subset id added)

theorem ValidatorEnvironment.Models.checked_input
    (task : ValidatorExpressionTask callees expression) (models : task.environment.Models locals) :
    task.checkedInput.Realizes locals := by
  refine ⟨?_, models.realizes.values, ?_⟩
  · intro id
    exact (models.realizes.bindings id).trans (task.bindings id)
  · intro id member
    exact models.realizes.initialized id (task.initialized id member)

theorem checked_expression_state (callees : List Function) (fuel : Nat)
    (input output : LocalState) (expression : Expr)
    (accepted : expressionState callees fuel input expression = .ok output) :
    ∃ facts, checkExpr callees fuel input expression = .ok facts ∧
      output = {input with initialized := facts.initialized} := by
  obtain ⟨facts, checked, returned⟩ := except_bind_success
    (checkExpr callees fuel input expression) _ output accepted
  exact ⟨facts, checked, by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_literal_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (bits : Nat) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.literal type bits) = .ok facts) :
    bits < 2 ^ type.width ∧ facts = ⟨input.initialized, [], []⟩ := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    have checked := accepted
    simp only [checkExpr] at checked
    obtain ⟨unit, size, returned⟩ := except_bind_success _ _ _ checked
    cases unit
    exact ⟨by simpa using require_success _ _ size,
      by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_identifier_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (id : Nat) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.ident type id) = .ok facts) :
    facts.initialized = input.initialized := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨checkedUnit, _, returned⟩ := except_bind_success _ _ _ accepted
    cases checkedUnit
    have same : facts = ⟨input.initialized, [id], []⟩ := by
      simpa [pure, Except.pure] using returned.symm
    exact congrArg ExpressionFacts.initialized same

theorem checked_unary_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (op : Unary) (operand : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.unary type op operand) = .ok facts) :
    ∃ innerFuel inner,
      fuel = innerFuel + 1 ∧ checkExpr callees innerFuel input operand = .ok inner ∧
      type = (if op == .not then .int else operand.type) ∧ facts = inner := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨inner, checked, remaining⟩ := except_bind_success _ _ _ accepted
    obtain ⟨unit, typed, returned⟩ := except_bind_success _ _ _ remaining
    cases unit
    exact ⟨fuel, inner, rfl, checked, by simpa using require_success _ _ typed,
      by simpa [pure, Except.pure] using returned.symm⟩

def binarySequenced (op : Binary) : Bool := op == .comma || op == .and || op == .or

def binaryRightInput (input : LocalState) (op : Binary) (left : ExpressionFacts) : LocalState :=
  if binarySequenced op then {input with initialized := left.initialized} else input

def binaryOutputInitialized (op : Binary) (left right : ExpressionFacts) : List Nat :=
  if binarySequenced op then if op == .comma then right.initialized else left.initialized
  else union left.initialized right.initialized

theorem checked_binary_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (op : Binary) (left right : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.binary type op left right) = .ok facts) :
    ∃ innerFuel a b,
      fuel = innerFuel + 1 ∧ checkExpr callees innerFuel input left = .ok a ∧
      checkExpr callees innerFuel (binaryRightInput input op a) right = .ok b ∧
      type = binaryType op left.type right.type ∧
      !(op == .shl && left.type.signed) = true ∧
      facts = accessUnion a b (binaryOutputInitialized op a b) := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨unit, typed, remaining⟩ := except_bind_success _ _ _ accepted
    cases unit
    have signature : type = binaryType op left.type right.type := by
      simpa using require_success _ _ typed
    obtain ⟨unit, admitted, remaining⟩ := except_bind_success _ _ _ remaining
    cases unit
    have allowed := require_success _ _ admitted
    obtain ⟨a, leftChecked, remaining⟩ := except_bind_success _ _ _ remaining
    by_cases sequenced : binarySequenced op = true
    · simp only [binarySequenced] at sequenced
      simp only [sequenced, ↓reduceIte] at remaining
      obtain ⟨b, rightChecked, returned⟩ := except_bind_success _ _ _ remaining
      refine ⟨fuel, a, b, rfl, leftChecked, ?_, signature, by simpa using allowed, ?_⟩
      · simpa [binaryRightInput, binarySequenced, sequenced] using rightChecked
      · simpa [pure, Except.pure, binaryOutputInitialized, binarySequenced, sequenced]
          using returned.symm
    · have unsequenced : (op == .comma || op == .and || op == .or) = false :=
        Bool.eq_false_iff.mpr sequenced
      simp only [unsequenced, Bool.false_eq_true, ↓reduceIte] at remaining
      obtain ⟨b, rightChecked, remaining⟩ := except_bind_success _ _ _ remaining
      obtain ⟨unit, _, returned⟩ := except_bind_success _ _ _ remaining
      cases unit
      refine ⟨fuel, a, b, rfl, leftChecked, ?_, signature, by simpa using allowed, ?_⟩
      · simpa [binaryRightInput, binarySequenced, unsequenced] using rightChecked
      · simpa [pure, Except.pure, binaryOutputInitialized, binarySequenced, unsequenced]
          using returned.symm

theorem checked_conditional_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (condition yes no : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.conditional type condition yes no) = .ok facts) :
    ∃ innerFuel c a b,
      fuel = innerFuel + 1 ∧ checkExpr callees innerFuel input condition = .ok c ∧
      checkExpr callees innerFuel {input with initialized := c.initialized} yes = .ok a ∧
      checkExpr callees innerFuel {input with initialized := c.initialized} no = .ok b ∧
      facts = accessUnion c (accessUnion a b (intersection a.initialized b.initialized))
        (intersection a.initialized b.initialized) := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨unit, _, remaining⟩ := except_bind_success _ _ _ accepted
    cases unit
    obtain ⟨c, conditionChecked, remaining⟩ := except_bind_success _ _ _ remaining
    obtain ⟨a, yesChecked, remaining⟩ := except_bind_success _ _ _ remaining
    obtain ⟨b, noChecked, returned⟩ := except_bind_success _ _ _ remaining
    exact ⟨fuel, c, a, b, rfl, conditionChecked, yesChecked, noChecked,
      by simpa [pure, Except.pure, accessUnion] using returned.symm⟩

theorem checked_cast_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (destination : QualifiedType) (operand : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.cast type destination operand) = .ok facts) :
    ∃ innerFuel, fuel = innerFuel + 1 ∧ type = destination.type ∧
      checkExpr callees innerFuel input operand = .ok facts := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨unit, typed, checked⟩ := except_bind_success _ _ _ accepted
    cases unit
    exact ⟨fuel, rfl, by simpa using require_success _ _ typed, checked⟩

theorem checked_comma_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (left right : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.comma type left right) = .ok facts) :
    ∃ innerFuel a b,
      fuel = innerFuel + 1 ∧ type = right.type ∧
      checkExpr callees innerFuel input left = .ok a ∧
      checkExpr callees innerFuel {input with initialized := a.initialized} right = .ok b ∧
      facts = accessUnion a b b.initialized := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨unit, typed, remaining⟩ := except_bind_success _ _ _ accepted
    cases unit
    obtain ⟨a, leftChecked, remaining⟩ := except_bind_success _ _ _ remaining
    obtain ⟨b, rightChecked, returned⟩ := except_bind_success _ _ _ remaining
    exact ⟨fuel, a, b, rfl, by simpa using require_success _ _ typed,
      leftChecked, rightChecked, by simpa [pure, Except.pure] using returned.symm⟩

theorem checked_assignment_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (op : Assignment) (id : Nat) (destination : QualifiedType)
    (target value : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.assign type op id destination target value) = .ok facts) :
    ∃ innerFuel rhs,
      fuel = innerFuel + 1 ∧ checkExpr callees innerFuel input value = .ok rhs ∧
      facts.initialized = union rhs.initialized [id] := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨unit, _, remaining⟩ := except_bind_success _ _ _ accepted
    cases unit
    obtain ⟨unit, _, remaining⟩ := except_bind_success _ _ _ remaining
    cases unit
    obtain ⟨unit, _, remaining⟩ := except_bind_success _ _ _ remaining
    cases unit
    obtain ⟨rhs, rhsChecked, remaining⟩ := except_bind_success _ _ _ remaining
    obtain ⟨unit, _, returned⟩ := except_bind_success _ _ _ remaining
    cases unit
    have result : facts = ⟨union rhs.initialized [id],
        if op == .set then rhs.reads else union rhs.reads [id], union rhs.writes [id]⟩ := by
      simpa [pure, Except.pure] using returned.symm
    exact ⟨fuel, rhs, rfl, rhsChecked, congrArg ExpressionFacts.initialized result⟩

theorem checked_update_facts (callees : List Function) (fuel : Nat) (input : LocalState)
    (type : IntType) (op : Update) (id : Nat) (destination : QualifiedType)
    (target : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel input (.update type op id destination target) = .ok facts) :
    facts.initialized = input.initialized := by
  cases fuel with
  | zero => simp [checkExpr] at accepted
  | succ fuel =>
    simp only [checkExpr] at accepted
    obtain ⟨unit, _, remaining⟩ := except_bind_success _ _ _ accepted
    cases unit
    obtain ⟨unit, _, returned⟩ := except_bind_success _ _ _ remaining
    cases unit
    have result : facts = ⟨input.initialized, [id], [id]⟩ := by
      simpa [pure, Except.pure] using returned.symm
    exact congrArg ExpressionFacts.initialized result

theorem checked_expression_state_bindings (callees : List Function) (fuel : Nat)
    (input output : LocalState) (expression : Expr)
    (accepted : expressionState callees fuel input expression = .ok output) :
    output.bindings = input.bindings := by
  obtain ⟨_, _, rfl⟩ := checked_expression_state callees fuel input output expression accepted
  rfl

theorem checked_statement_bindings (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (statement : Stmt) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input statement = .ok facts) :
    facts.state.bindings = input.bindings := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    cases statement with
    | null =>
      have same : facts = ⟨input, true⟩ := by
        simpa [checkStmt, pure, Except.pure] using accepted.symm
      simp [same]
    | «break» | «continue» =>
      simp only [checkStmt] at accepted
      obtain ⟨checkedUnit, _, returned⟩ := except_bind_success _ _ _ accepted
      cases checkedUnit
      have same : facts = ⟨input, false⟩ := by simpa [pure, Except.pure] using returned.symm
      simp [same]
    | expression expression =>
      simp only [checkStmt] at accepted
      obtain ⟨state, checked, returned⟩ := except_bind_success _ _ _ accepted
      have same : facts = ⟨state, true⟩ := by simpa [pure, Except.pure] using returned.symm
      simpa [same] using checked_expression_state_bindings callees fuel input state expression checked
    | «return» destination expression =>
      simp only [checkStmt] at accepted
      obtain ⟨checkedUnit, _, remaining⟩ := except_bind_success _ _ _ accepted
      cases checkedUnit
      obtain ⟨state, checked, returned⟩ := except_bind_success _ _ _ remaining
      have same : facts = ⟨state, false⟩ := by simpa [pure, Except.pure] using returned.symm
      simpa [same] using checked_expression_state_bindings callees fuel input state expression checked
    | compound items =>
      simp only [checkStmt] at accepted
      obtain ⟨out, _, returned⟩ := except_bind_success _ _ _ accepted
      have same : facts = {out with state := restoreScope input out.state} := by
        simpa [pure, Except.pure] using returned.symm
      simp [same, restoreScope]
    | ite condition yes no =>
      cases no <;> simp only [checkStmt] at accepted
      all_goals
        obtain ⟨branch, conditionChecked, remaining⟩ := except_bind_success _ _ _ accepted
        obtain ⟨a, _, remaining⟩ := except_bind_success _ _ _ remaining
        obtain ⟨b, _, returned⟩ := except_bind_success _ _ _ remaining
        have same : facts = ⟨{branch with initialized := if a.fallsThrough then
            if b.fallsThrough then intersection a.state.initialized b.state.initialized
            else a.state.initialized else b.state.initialized}, a.fallsThrough || b.fallsThrough⟩ := by
          simpa [pure, Except.pure] using returned.symm
        simpa [same] using checked_expression_state_bindings callees fuel input branch condition conditionChecked
    | «while» condition body =>
      simp only [checkStmt] at accepted
      obtain ⟨incoming, conditionChecked, remaining⟩ := except_bind_success _ _ _ accepted
      obtain ⟨_, _, returned⟩ := except_bind_success _ _ _ remaining
      have same : facts = ⟨incoming, true⟩ := by simpa [pure, Except.pure] using returned.symm
      simpa [same] using checked_expression_state_bindings callees fuel input incoming condition conditionChecked
    | doWhile body condition =>
      simp only [checkStmt] at accepted
      obtain ⟨_, _, remaining⟩ := except_bind_success _ _ _ accepted
      obtain ⟨_, _, returned⟩ := except_bind_success _ _ _ remaining
      have same : facts = ⟨input, true⟩ := by simpa [pure, Except.pure] using returned.symm
      simp [same]
    | «for» init condition increment body =>
      cases increment with
      | none =>
        cases init <;> cases condition <;> simp only [checkStmt] at accepted
        all_goals
          obtain ⟨start, _, remaining⟩ := except_bind_success _ _ _ accepted
          obtain ⟨incoming, _, remaining⟩ := except_bind_success _ _ _ remaining
          obtain ⟨_, _, returned⟩ := except_bind_success _ _ _ remaining
          have same : facts = ⟨restoreScope input incoming, true⟩ := by
            simpa [pure, Except.pure] using returned.symm
          simp [same, restoreScope]
      | some expression =>
        cases init <;> cases condition <;> simp only [checkStmt] at accepted
        all_goals
          obtain ⟨start, _, remaining⟩ := except_bind_success _ _ _ accepted
          obtain ⟨incoming, _, remaining⟩ := except_bind_success _ _ _ remaining
          obtain ⟨_, _, remaining⟩ := except_bind_success _ _ _ remaining
          obtain ⟨_, _, returned⟩ := except_bind_success _ _ _ remaining
          have same : facts = ⟨restoreScope input incoming, true⟩ := by
            simpa [pure, Except.pure] using returned.symm
          simp [same, restoreScope]

theorem ValidatorEnvironment.Entails.trans (a b c : ValidatorEnvironment)
    (ab : a.Entails b) (bc : b.Entails c) : a.Entails c :=
  ⟨fun id => (ab.bindings id).trans (bc.bindings id), ab.layout.trans bc.layout,
    fun id member => ab.initialized id (bc.initialized id member),
    fun id member => ab.fresh id (bc.fresh id member)⟩

theorem ValidatorStatementTask.output_entails (task : ValidatorStatementTask callees statement) :
    task.outputEnvironment.Entails task.environment := by
  refine ⟨?_, task.outputLayout, ?_, ?_⟩
  · intro id
    simp only [lookup, task.outputBindings,
      checked_statement_bindings callees task.result task.checked.fuel task.depth task.checkedInput
        statement task.checked.facts task.checked.accepted, task.bindings]
  · intro id member
    rw [task.outputInitialized]
    exact mem_validator_union id _ _ |>.mpr (Or.inl member)
  · intro id member
    simpa [task.outputFresh] using member

end EvoIR.Core.Source
