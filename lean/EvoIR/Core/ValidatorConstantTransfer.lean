import EvoIR.Core.ValidatorFacts

/-! The source machine's literal-only wrapping shortcut must not invent a
type or definite initialization fact. Constant evaluation may skip an
unselected conditional arm, so this proof follows the executed constant path
while retaining the independent validator's conservative branch intersection.
-/
namespace EvoIR.Core.Source

private theorem constant_bind_success {α β : Type} (first : Option α)
    (next : α → Option β) (result : β) (accepted : first >>= next = some result) :
    ∃ value, first = some value ∧ next value = some result := by
  cases first with
  | none => simp [Bind.bind, Option.bind] at accepted
  | some value => exact ⟨value, rfl, accepted⟩

private theorem constant_result_type (type : IntType) (result : Result) (value : Value)
    (safe : ResultSafe type result) (accepted : result.toOption = some value) :
    value.type = type := by
  cases result with
  | error fault => cases accepted
  | ok actual =>
    have same : actual = value := Option.some.inj accepted
    exact same ▸ safe

private theorem binary_input_initialized (input : LocalState) (op : Binary)
    (left : ExpressionFacts)
    (bounded : ∀ id ∈ left.initialized, id ∈ input.initialized) :
    ∀ id ∈ (binaryRightInput input op left).initialized, id ∈ input.initialized := by
  intro id member
  unfold binaryRightInput at member
  split at member
  · exact bounded id member
  · exact member

private theorem binary_output_initialized (input : LocalState) (op : Binary)
    (left right : ExpressionFacts)
    (leftBound : ∀ id ∈ left.initialized, id ∈ input.initialized)
    (rightBound : ∀ id ∈ right.initialized, id ∈ (binaryRightInput input op left).initialized) :
    ∀ id ∈ binaryOutputInitialized op left right, id ∈ input.initialized := by
  intro id member
  have rightInput := binary_input_initialized input op left leftBound
  unfold binaryOutputInitialized at member
  split at member
  · split at member
    · exact rightInput id (rightBound id member)
    · exact leftBound id member
  · rcases (mem_validator_union id _ _).mp member with member | member
    · exact leftBound id member
    · exact rightInput id (rightBound id member)

theorem constant_value_checked (callees : List Function) (fuel : Nat)
    (input : LocalState) (expression : Expr) (facts : ExpressionFacts) (value : Value)
    (accepted : checkExpr callees fuel input expression = .ok facts)
    (constant : constantValue expression = some value) :
    value.type = expression.type ∧ ∀ id ∈ facts.initialized, id ∈ input.initialized := by
  induction fuel generalizing input expression facts value with
  | zero => simp [checkExpr] at accepted
  | succ fuel ih =>
    cases expression with
    | literal type bits =>
      obtain ⟨_, sameFacts⟩ := checked_literal_facts callees (fuel+1) input type bits facts accepted
      have same : Value.ofNat type bits = value := Option.some.inj constant
      subst value
      refine ⟨rfl, ?_⟩
      simpa [sameFacts]
    | unary type op operand =>
      obtain ⟨innerFuel, inner, fuelEq, checked, typeEq, factsEq⟩ :=
        checked_unary_facts callees (fuel+1) input type op operand facts accepted
      have sameFuel : innerFuel = fuel := by omega
      subst innerFuel
      obtain ⟨actual, evaluated, result⟩ := constant_bind_success (constantValue operand) _ value constant
      obtain ⟨operandType, initialized⟩ := ih input operand inner actual checked evaluated
      have actualType := constant_result_type _ _ value (unary_result_safe op actual true) result
      refine ⟨?_, ?_⟩
      · simpa only [Expr.type, typeEq, operandType] using actualType
      · simpa only [factsEq] using initialized
    | binary type op left right =>
      obtain ⟨innerFuel, a, b, fuelEq, leftChecked, rightChecked, typeEq, admitted, factsEq⟩ :=
        checked_binary_facts callees (fuel+1) input type op left right facts accepted
      have sameFuel : innerFuel = fuel := by omega
      subst innerFuel
      have notComma : op ≠ .comma := by
        intro same
        simp [constantValue, same] at constant
      have start : (do
          let x ← constantValue left
          if op == .and && !x.truth then return Value.boolean false
          if op == .or && x.truth then return Value.boolean true
          (binaryValue op x (← constantValue right) true).toOption) = some value := by
        simpa only [constantValue, beq_eq_false_iff_ne.mpr notComma, Bool.false_eq_true,
          ↓reduceIte] using constant
      obtain ⟨x, leftConstant, continued⟩ := constant_bind_success (constantValue left) _ value start
      obtain ⟨leftType, leftInit⟩ := ih input left a x leftChecked leftConstant
      by_cases shortAnd : (op == .and && !x.truth) = true
      · have selected : op = .and := by simpa using (Bool.and_eq_true_iff.mp shortAnd).1
        have returned : Value.boolean false = value := by
          simpa only [shortAnd, ↓reduceIte, pure, Option.some.injEq] using continued
        subst value
        refine ⟨?_, ?_⟩
        · simp [Expr.type, typeEq, selected, binaryType, Value.boolean, Value.ofNat]
        · simpa [factsEq, binaryOutputInitialized, binarySequenced, accessUnion, selected] using leftInit
      · by_cases shortOr : (op == .or && x.truth) = true
        · have selected : op = .or := by simpa using (Bool.and_eq_true_iff.mp shortOr).1
          have returned : Value.boolean true = value := by
            simpa only [shortAnd, shortOr, Bool.false_eq_true, ↓reduceIte, pure, Option.some.injEq] using continued
          subst value
          refine ⟨?_, ?_⟩
          · simp [Expr.type, typeEq, selected, binaryType, Value.boolean, Value.ofNat]
          · simpa [factsEq, binaryOutputInitialized, binarySequenced, accessUnion, selected] using leftInit
        · have continued : (do (binaryValue op x (← constantValue right) true).toOption) = some value := by
            simpa only [shortAnd, shortOr, Bool.false_eq_true, ↓reduceIte] using continued
          obtain ⟨y, rightConstant, result⟩ := constant_bind_success (constantValue right) _ value continued
          obtain ⟨rightType, rightInit⟩ := ih (binaryRightInput input op a) right b y rightChecked rightConstant
          have actualType := constant_result_type _ _ value
            (binary_result_safe op x y true (by simpa only [leftType] using admitted)) result
          refine ⟨?_, ?_⟩
          · simpa only [Expr.type, typeEq, leftType, rightType] using actualType
          · simpa only [factsEq, accessUnion] using
              binary_output_initialized input op a b leftInit rightInit
    | conditional type condition yes no =>
      obtain ⟨innerFuel, c, a, b, fuelEq, conditionChecked, yesChecked, noChecked, factsEq⟩ :=
        checked_conditional_facts callees (fuel+1) input type condition yes no facts accepted
      have sameFuel : innerFuel = fuel := by omega
      subst innerFuel
      obtain ⟨test, tested, remaining⟩ := constant_bind_success (constantValue condition) _ value constant
      obtain ⟨_, conditionInit⟩ := ih input condition c test conditionChecked tested
      by_cases chosen : test.truth = true
      · have remaining : (do return (← constantValue yes).cast type) = some value := by
          simpa only [chosen, Bool.false_eq_true, ↓reduceIte] using remaining
        obtain ⟨branch, evaluated, returned⟩ := constant_bind_success (constantValue yes) _ value remaining
        obtain ⟨_, branchInit⟩ := ih {input with initialized := c.initialized} yes a branch yesChecked evaluated
        have returned : branch.cast type = value := Option.some.inj returned
        subst value
        refine ⟨rfl, ?_⟩
        intro id member
        have both : id ∈ a.initialized ∧ id ∈ b.initialized := by
          simpa only [factsEq, accessUnion, mem_validator_intersection] using member
        exact conditionInit id (branchInit id both.1)
      · have remaining : (do return (← constantValue no).cast type) = some value := by
          simpa only [chosen, Bool.false_eq_true, ↓reduceIte] using remaining
        obtain ⟨branch, evaluated, returned⟩ := constant_bind_success (constantValue no) _ value remaining
        obtain ⟨_, branchInit⟩ := ih {input with initialized := c.initialized} no b branch noChecked evaluated
        have returned : branch.cast type = value := Option.some.inj returned
        subst value
        refine ⟨rfl, ?_⟩
        intro id member
        have both : id ∈ a.initialized ∧ id ∈ b.initialized := by
          simpa only [factsEq, accessUnion, mem_validator_intersection] using member
        exact conditionInit id (branchInit id both.2)
    | cast type destination operand =>
      obtain ⟨innerFuel, fuelEq, typeEq, checked⟩ :=
        checked_cast_facts callees (fuel+1) input type destination operand facts accepted
      have sameFuel : innerFuel = fuel := by omega
      subst innerFuel
      obtain ⟨actual, evaluated, returned⟩ := constant_bind_success (constantValue operand) _ value constant
      obtain ⟨_, initialized⟩ := ih input operand facts actual checked evaluated
      have returned : actual.cast destination.type = value := Option.some.inj returned
      subst value
      exact ⟨typeEq.symm, initialized⟩
    | _ => simp [constantValue] at constant

theorem wrapping_constant_is_constant (expression : Expr) (value : Value)
    (wrapped : wrappingConstant expression = some value) : constantValue expression = some value := by
  cases expression with
  | unary type op operand =>
    cases op <;> try simp [wrappingConstant] at wrapped
    case neg =>
      cases inner : constantValue operand with
      | none => simp [wrappingConstant, inner] at wrapped
      | some actual =>
        by_cases overflows : unaryValue .neg actual = .error (.diagnostic 3004)
        · simpa [wrappingConstant, constantValue, inner, overflows] using wrapped
        · simp [wrappingConstant, inner, overflows] at wrapped
  | binary type op left right =>
    have allowed : op = .add ∨ op = .sub ∨ op = .mul := by
      by_cases admitted : op = .add ∨ op = .sub ∨ op = .mul
      · exact admitted
      · have notAdd : op ≠ .add := fun same => admitted (Or.inl same)
        have notSub : op ≠ .sub := fun same => admitted (Or.inr (Or.inl same))
        have notMul : op ≠ .mul := fun same => admitted (Or.inr (Or.inr same))
        simp [wrappingConstant, notAdd, notSub, notMul] at wrapped
    rcases allowed with rfl | rfl | rfl
    all_goals
      cases a : constantValue left <;> cases b : constantValue right <;>
        simp_all [wrappingConstant, constantValue, Bind.bind, Option.bind]
    all_goals split at wrapped <;> simp_all
  | _ => simp [wrappingConstant] at wrapped

theorem wrapping_constant_checked (callees : List Function) (fuel : Nat)
    (input : LocalState) (expression : Expr) (facts : ExpressionFacts) (value : Value)
    (accepted : checkExpr callees fuel input expression = .ok facts)
    (wrapped : wrappingConstant expression = some value) :
    value.type = expression.type ∧ ∀ id ∈ facts.initialized, id ∈ input.initialized :=
  constant_value_checked callees fuel input expression facts value accepted
    (wrapping_constant_is_constant expression value wrapped)

end EvoIR.Core.Source
