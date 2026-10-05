import EvoIR.Core.LocalStorage
import EvoIR.Core.ScalarCorrectness

/-! Bridge the independent validator's facts to the actual source machine.
The store invariant is explicit; accepting an identifier is not a claim that
an arbitrary runtime store contains it. Global preservation through statements,
loops and calls remains the larger validator-safety obligation. -/
namespace EvoIR.Core.Source

/-- Lookup equality uses the first active binding on both sides. It allows
the validator and parameter machine to enumerate distinct bindings differently,
but neither an absent object nor a hidden same-ID value can supply a fact. -/
structure LocalState.Realizes (state : LocalState) (locals : Locals) : Prop where
  bindings : ∀ id, (locals.find? (·.symbol == id)).map Slot.type =
    (state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd
  values : ∀ slot ∈ locals, ∀ value, slot.value = some value → value.type = slot.type.type
  initialized : ∀ id ∈ state.initialized, ∃ slot value,
    locals.find? (·.symbol == id) = some slot ∧ slot.value = some value

theorem checked_identifier_read (callees : List Function) (fuel : Nat)
    (state : LocalState) (locals : Locals) (type : IntType) (id : Nat)
    (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel state (.ident type id) = .ok facts)
    (realized : state.Realizes locals) :
    ∃ value, value.type = type ∧ readSlot locals id type = .ok value := by
  obtain ⟨binding, found, typed, initialized⟩ :=
    checked_identifier callees fuel state type id facts accepted
  obtain ⟨slot, value, selected, stored⟩ := realized.initialized id initialized
  have same := realized.bindings id
  rw [selected, found] at same
  have bindingType : slot.type = binding := Option.some.inj same
  have valueType := realized.values slot (List.mem_of_find?_eq_some selected) value stored
  have typeEq : slot.type.type = type := bindingType ▸ typed
  refine ⟨value, valueType.trans typeEq, ?_⟩
  simp [readSlot, selected, typeEq, stored, valueType, pure, Except.pure]

theorem checked_identifier_step (program : Program) (callees : List Function) (fuel : Nat)
    (state : LocalState) (locals : Locals) (type : IntType) (id : Nat)
    (facts : ExpressionFacts) (continuation : List Frame) (functionId : Nat)
    (accepted : checkExpr callees fuel state (.ident type id) = .ok facts)
    (realized : state.Realizes locals) :
    ∃ value, value.type = type ∧
      Step program ⟨.expression (.ident type id), locals, continuation, functionId⟩
        ⟨.value value, locals, continuation, functionId⟩ := by
  obtain ⟨value, typed, read⟩ := checked_identifier_read callees fuel state locals type id facts accepted realized
  exact ⟨value, typed, by
    simp [Step, step, enterExpression, wrappingConstant, read, valueResult, typed]⟩

def SafeValueControl (type : IntType) : Control → Prop
  | .value value => value.type = type
  | .halted (.error (.diagnostic code)) => code ∈ [3002, 3003, 3004, 3005]
  | _ => False

theorem value_result_safe (state : State) (type : IntType) (result : Result)
    (safe : ResultSafe type result) : SafeValueControl type (valueResult state type result).control := by
  cases result with
  | ok value =>
    have typed : value.type = type := safe
    simp [valueResult, typed, SafeValueControl]
  | error fault =>
    cases fault with
    | diagnostic code => exact safe
    | malformed message => exact False.elim safe

/-- The scalar continuation consumes typed operands and either produces its
declared type or stops at a checked diagnostic. The shift exclusion is the
validator's existing rule, not a new semantic assumption. -/
theorem binary_continuation_safe (state : State) (op : Binary) (left right : Value)
    (admitted : !(op == .shl && left.type.signed) = true) :
    SafeValueControl (binaryType op left.type right.type)
      (resumeValue state (.binaryRight (binaryType op left.type right.type) op left) right).control :=
  value_result_safe state _ _ (binary_result_safe op left right false admitted)

theorem unary_continuation_safe (state : State) (op : Unary) (value : Value) :
    SafeValueControl (if op == .not then .int else value.type)
      (resumeValue state (.unary (if op == .not then .int else value.type) op) value).control :=
  value_result_safe state _ _ (unary_result_safe op value false)

/-- Connect the preceding primitive to the actual source admission check.
Preservation of the two operand-type premises by their evaluation is a later
whole-expression obligation, not something assumed of an arbitrary Value. -/
theorem checked_binary_continuation_safe (callees : List Function) (fuel : Nat)
    (static : LocalState) (state : State) (type : IntType) (op : Binary)
    (left right : Expr) (facts : ExpressionFacts) (a b : Value)
    (accepted : checkExpr callees fuel static (.binary type op left right) = .ok facts)
    (leftType : a.type = left.type) (rightType : b.type = right.type) :
    SafeValueControl type (resumeValue state (.binaryRight type op a) b).control := by
  obtain ⟨typed, admitted⟩ :=
    checked_binary_signature callees fuel static type op left right facts accepted
  have operands : type = binaryType op a.type b.type := by simpa [leftType, rightType] using typed
  rw [operands]
  exact binary_continuation_safe state op a b (by simpa [leftType] using admitted)

-- Invented initializedness cannot be realized by an empty store.
example : ¬(LocalState.mk [] [1]).Realizes [] := by
  intro claimed
  obtain ⟨slot, value, found, _⟩ := claimed.initialized 1 (by simp)
  cases found

-- An initialized hidden binding cannot justify reading an uninitialized head.
example : ¬(LocalState.mk [(1, ⟨.int, false⟩)] [1]).Realizes
    [⟨1, ⟨.int, false⟩, none⟩, ⟨1, ⟨.int, false⟩, some (.ofNat .int 7)⟩] := by
  intro claimed
  obtain ⟨slot, value, found, stored⟩ := claimed.initialized 1 (by simp)
  simp [List.find?] at found
  subst slot
  cases stored

-- Equal-width source ranks are still different types.
example : ¬(LocalState.mk [(1, ⟨.long, false⟩)] [1]).Realizes
    [⟨1, ⟨.longLong, false⟩, some (.ofNat .longLong 7)⟩] := by
  intro claimed
  have bindings := claimed.bindings 1
  simp [List.find?] at bindings

end EvoIR.Core.Source
