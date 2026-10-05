import EvoIR.Core.ValidatorState

/-! Preservation of the validator's local-state facts by the actual source
storage machine. These results do not assume a successful assignment: binding
and permission premises establish that the runtime write succeeds. -/
namespace EvoIR.Core.Source

theorem LocalState.Realizes.lookup (state : LocalState) (locals : Locals)
    (realized : state.Realizes locals) (id : Nat) (destination : QualifiedType)
    (binding : (state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd = some destination) :
    ∃ slot, locals.find? (·.symbol == id) = some slot ∧ slot.type = destination := by
  have same := (realized.bindings id).trans binding
  exact Option.map_eq_some_iff.mp same

theorem LocalState.Realizes.read (state : LocalState) (locals : Locals)
    (realized : state.Realizes locals) (id : Nat) (destination : QualifiedType)
    (binding : (state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd = some destination)
    (initialized : id ∈ state.initialized) :
    ∃ value, value.type = destination.type ∧ readSlot locals id destination.type = .ok value := by
  obtain ⟨slot, value, found, stored⟩ := realized.initialized id initialized
  have same := realized.bindings id
  rw [found, binding] at same
  have typed : slot.type = destination := Option.some.inj same
  have represented := realized.values slot (List.mem_of_find?_eq_some found) value stored
  refine ⟨value, represented.trans (congrArg QualifiedType.type typed), ?_⟩
  simp [readSlot, found, stored, represented, typed, pure, Except.pure]

theorem LocalState.Realizes.weaken (state : LocalState) (locals : Locals)
    (realized : state.Realizes locals) (initialized : List Nat)
    (subset : ∀ id ∈ initialized, id ∈ state.initialized) :
    (LocalState.mk state.bindings initialized).Realizes locals :=
  ⟨realized.bindings, realized.values, fun id member => realized.initialized id (subset id member)⟩

theorem LocalState.Realizes.declare (state : LocalState) (locals : Locals)
    (realized : state.Realizes locals) (id : Nat) (destination : QualifiedType)
    (absent : (state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd = none) :
    (LocalState.mk ((id, destination) :: state.bindings) state.initialized).Realizes
      (⟨id, destination, none⟩ :: locals) := by
  have noSlot : locals.find? (·.symbol == id) = none :=
    Option.map_eq_none_iff.mp ((realized.bindings id).trans absent)
  have uninitialized : id ∉ state.initialized := by
    intro member
    obtain ⟨slot, value, found, _⟩ := realized.initialized id member
    rw [noSlot] at found
    cases found
  refine ⟨?_, ?_, ?_⟩
  · intro other
    by_cases same : id = other
    · subst other; simp [List.find?]
    · have different : (id == other) = false := beq_eq_false_iff_ne.mpr same
      simpa [List.find?, different] using realized.bindings other
  · intro slot member value stored
    rcases List.mem_cons.mp member with rfl | member
    · cases stored
    · exact realized.values slot member value stored
  · intro other member
    have different : (id == other) = false := beq_eq_false_iff_ne.mpr (by
      intro same; exact uninitialized (same ▸ member))
    obtain ⟨slot, value, found, stored⟩ := realized.initialized other member
    exact ⟨slot, value, by simpa [List.find?, different] using found, stored⟩

/-- Calls restore the saved caller store, not a callee's equally named slots.
The return conversion is checked before exposing the value to the caller. -/
theorem caller_restore_safe (static : LocalState) (locals : Locals)
    (realized : static.Realizes locals) (state : State) (callerId : Nat)
    (type : IntType) (value : Value) (typed : value.type = type) :
    resumeFlow state (.function (some (callerId, locals)) type) (.returned value) =
      {state with control := .value value, locals, functionId := callerId} ∧
    static.Realizes (resumeFlow state (.function (some (callerId, locals)) type) (.returned value)).locals := by
  simp [resumeFlow, typed, realized]

/-- Initialization facts can grow only by the object actually written.
Other first bindings retain their old values, including hidden/uninitialized
objects; a same-width but different source rank is not a type equality. -/
theorem LocalState.Realizes.after_write (state : LocalState) (locals updated : Locals)
    (realized : state.Realizes locals) (id : Nat) (destination : QualifiedType)
    (value : Value) (initialization : Bool)
    (written : writeSlot locals id destination value initialization = .ok updated)
    (initialized : List Nat)
    (subset : ∀ other ∈ initialized, other = id ∨ other ∈ state.initialized) :
    (LocalState.mk state.bindings initialized).Realizes updated := by
  refine ⟨?_, write_slot_value_types locals updated id destination value initialization written realized.values, ?_⟩
  · intro other
    exact (write_slot_lookup_type locals updated id other destination value initialization written).trans
      (realized.bindings other)
  · intro other member
    by_cases same : other = id
    · subst other
      obtain ⟨slot, found, stored⟩ := read_slot_witness updated id destination.type value
        (read_written_slot locals updated id destination value initialization written)
      exact ⟨slot, value, found, stored⟩
    · have old := (subset other member).resolve_left same
      obtain ⟨slot, previous, found, stored⟩ := realized.initialized other old
      have typed := realized.values slot (List.mem_of_find?_eq_some found) previous stored
      have read : readSlot locals other slot.type.type = .ok previous := by
        simp [readSlot, found, stored, typed, pure, Except.pure]
      have kept := read_other_slot locals updated id other destination slot.type.type value
        initialization same written
      obtain ⟨current, selected, retained⟩ :=
        read_slot_witness updated other slot.type.type previous (kept.trans read)
      exact ⟨current, previous, selected, retained⟩

theorem LocalState.Realizes.assign (state : LocalState) (locals : Locals)
    (realized : state.Realizes locals) (id : Nat) (destination : QualifiedType)
    (binding : (state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd = some destination)
    (writable : destination.isConst = false) (value : Value)
    (typed : value.type = destination.type) :
    ∃ updated, writeSlot locals id destination value false = .ok updated ∧
      (LocalState.mk state.bindings (id :: state.initialized)).Realizes updated := by
  obtain ⟨slot, found, slotType⟩ := realized.lookup state locals id destination binding
  obtain ⟨updated, written⟩ := write_slot_exists locals id slot destination value false found
    slotType typed (by simp) (fun _ => writable)
  exact ⟨updated, written, realized.after_write state locals updated id destination value false written
    (id :: state.initialized) (by intro other member; simpa using member)⟩

theorem LocalState.Realizes.initialize (state : LocalState) (locals : Locals)
    (realized : state.Realizes locals) (id : Nat) (destination : QualifiedType)
    (slot : Slot) (found : locals.find? (·.symbol == id) = some slot)
    (slotType : slot.type = destination)
    (fresh : destination.isConst = true → slot.value = none) (value : Value)
    (typed : value.type = destination.type) :
    ∃ updated, writeSlot locals id destination value true = .ok updated ∧
      (LocalState.mk state.bindings (id :: state.initialized)).Realizes updated := by
  obtain ⟨updated, written⟩ := write_slot_exists locals id slot destination value true found
    slotType typed (fun _ => fresh) (by simp)
  exact ⟨updated, written, realized.after_write state locals updated id destination value true written
    (id :: state.initialized) (by intro other member; simpa using member)⟩

/-- A write expression either produces a typed value with the promised store
facts, or stops before modifying the store at the original checked diagnostic.
Continuation and caller identity are preserved in both alternatives. -/
def WriteOutcome (before : State) (after : LocalState) (type : IntType) (next : State) : Prop :=
  (∃ value locals, value.type = type ∧ after.Realizes locals ∧
    next = {before with control := .value value, locals}) ∨
  (∃ code, code ∈ [3002, 3003, 3004, 3005] ∧ next = fail before (.diagnostic code))

theorem assignment_continuation_safe (static : LocalState) (state : State)
    (realized : static.Realizes state.locals) (op : Assignment) (id : Nat)
    (destination : QualifiedType) (value : Value)
    (binding : (static.bindings.find? (fun pair => pair.1 == id)).map Prod.snd = some destination)
    (writable : destination.isConst = false)
    (initialized : op ≠ .set → id ∈ static.initialized)
    (admitted : !(op == .shl && destination.type.signed) = true) :
    WriteOutcome state ⟨static.bindings, id :: static.initialized⟩ destination.type
      (resumeValue state (.assign destination.type op id destination) value) := by
  have finish (result : Value) := realized.assign static state.locals id destination binding writable
    (result.cast destination.type) rfl
  cases operation : op.binary with
  | none =>
    obtain ⟨locals, written, preserved⟩ := finish value
    left
    refine ⟨value.cast destination.type, locals, rfl, preserved, ?_⟩
    simp only [resumeValue, operation, written, bind, Except.bind, pure, Except.pure]
    simp [valueResult, Value.cast, Value.ofInt]
  | some binary =>
    have notSet : op ≠ .set := by intro same; subst op; cases operation
    obtain ⟨previous, previousType, read⟩ := realized.read static state.locals id destination binding
      (initialized notSet)
    have binaryAdmitted : !(binary == .shl && previous.type.signed) = true := by
      rw [previousType]
      cases op <;> simp [Assignment.binary] at operation
      all_goals subst binary
      all_goals first | exact admitted | simp
    have safe := binary_result_safe binary previous value false binaryAdmitted
    cases result : binaryValue binary previous value with
    | error fault =>
      rw [result] at safe
      cases fault with
      | malformed message => exact False.elim safe
      | diagnostic code =>
        right
        refine ⟨code, safe, ?_⟩
        simp [resumeValue, operation, read, result, bind, Except.bind]
    | ok resultValue =>
      obtain ⟨locals, written, preserved⟩ := finish resultValue
      left
      refine ⟨resultValue.cast destination.type, locals, rfl, preserved, ?_⟩
      simp only [resumeValue, operation, read, result, written, bind, Except.bind, pure, Except.pure]
      simp [valueResult, Value.cast, Value.ofInt]

/-- The input check supplies the write permissions. Evaluating the RHS must
preserve the explicit incoming store invariant; proving that recursively is
the whole-expression preservation obligation, not an assumed write success. -/
theorem checked_assignment_continuation_safe (callees : List Function) (fuel : Nat)
    (static : LocalState) (state : State) (type : IntType) (op : Assignment)
    (id : Nat) (destination : QualifiedType) (target rhs : Expr) (facts : ExpressionFacts)
    (value : Value)
    (accepted : checkExpr callees fuel static (.assign type op id destination target rhs) = .ok facts)
    (realized : static.Realizes state.locals) :
    WriteOutcome state ⟨static.bindings, id :: static.initialized⟩ type
      (resumeValue state (.assign type op id destination) value) := by
  obtain ⟨rfl, admitted, _, binding, writable, initialized⟩ :=
    checked_assignment_signature callees fuel static type op id destination target rhs facts accepted
  exact assignment_continuation_safe static state realized op id destination value binding writable
    initialized admitted

theorem initialization_continuation_safe (static : LocalState) (state : State)
    (realized : static.Realizes state.locals) (id : Nat) (destination : QualifiedType)
    (slot : Slot) (found : state.locals.find? (·.symbol == id) = some slot)
    (slotType : slot.type = destination)
    (fresh : destination.isConst = true → slot.value = none) (value : Value) :
    ∃ locals, (LocalState.mk static.bindings (id :: static.initialized)).Realizes locals ∧
      resumeValue state (.initialize id destination) value =
        {state with control := .flow .normal, locals} := by
  obtain ⟨locals, written, preserved⟩ := realized.initialize static state.locals id destination
    slot found slotType fresh (value.cast destination.type) rfl
  exact ⟨locals, preserved, by simp [resumeValue, written]⟩

theorem LocalState.Realizes.place (static : LocalState) (state : State)
    (realized : static.Realizes state.locals) (id : Nat) (destination : QualifiedType)
    (binding : (static.bindings.find? (fun pair => pair.1 == id)).map Prod.snd = some destination)
    (writable : destination.isConst = false) :
    place state id destination (.ident destination.type id) = true := by
  obtain ⟨slot, found, typed⟩ := realized.lookup static state.locals id destination binding
  simp [Source.place, found, typed, writable]

theorem checked_assignment_enter (callees : List Function) (fuel : Nat)
    (static : LocalState) (state : State) (type : IntType) (op : Assignment)
    (id : Nat) (destination : QualifiedType) (target rhs : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel static (.assign type op id destination target rhs) = .ok facts)
    (realized : static.Realizes state.locals) :
    enterExpression state (.assign type op id destination target rhs) =
      {state with
        control := .expression rhs
        continuation := .assign type op id destination :: state.continuation} := by
  obtain ⟨_, _, rfl, binding, writable, _⟩ :=
    checked_assignment_signature callees fuel static type op id destination target rhs facts accepted
  have placeOk := realized.place static state id destination binding writable
  simp [enterExpression, wrappingConstant, placeOk]

theorem checked_update_safe (callees : List Function) (fuel : Nat)
    (static : LocalState) (state : State) (type : IntType) (op : Update)
    (id : Nat) (destination : QualifiedType) (target : Expr) (facts : ExpressionFacts)
    (accepted : checkExpr callees fuel static (.update type op id destination target) = .ok facts)
    (realized : static.Realizes state.locals) :
    WriteOutcome state static type (enterExpression state (.update type op id destination target)) := by
  obtain ⟨rfl, rfl, binding, writable, initialized⟩ :=
    checked_update_signature callees fuel static type op id destination target facts accepted
  have placeOk := realized.place static state id destination binding writable
  obtain ⟨previous, previousType, read⟩ := realized.read static state.locals id destination binding initialized
  let binary := if op == .preInc || op == .postInc then Binary.add else .sub
  have admitted : !(binary == .shl && previous.type.signed) = true := by
    dsimp [binary]; split <;> simp
  have safe := binary_result_safe binary previous (.ofNat .int 1) false admitted
  cases result : binaryValue binary previous (.ofNat .int 1) with
  | error fault =>
    rw [result] at safe
    cases fault with
    | malformed message => exact False.elim safe
    | diagnostic code =>
      right
      refine ⟨code, safe, ?_⟩
      change binaryValue (if op == .preInc || op == .postInc then Binary.add else .sub)
        previous (.ofNat .int 1) = .error (.diagnostic code) at result
      simp only [enterExpression, wrappingConstant, placeOk, Bool.not_true, Bool.false_eq_true,
        ↓reduceIte, read, result, bind, Except.bind]
  | ok resultValue =>
    obtain ⟨locals, written, preserved⟩ := realized.assign static state.locals id destination binding
      writable (resultValue.cast destination.type) rfl
    have retained : static.Realizes locals :=
      preserved.weaken _ locals static.initialized (by intro other member; simp [member])
    let returned := if op == .postInc || op == .postDec then previous else resultValue.cast destination.type
    have returnedType : returned.type = destination.type := by
      dsimp [returned]; split
      · exact previousType
      · rfl
    left
    refine ⟨returned, locals, returnedType, retained, ?_⟩
    simp only [enterExpression, wrappingConstant, placeOk, Bool.not_true, Bool.false_eq_true,
      ↓reduceIte, read, bind, Except.bind]
    change (match (do
        let after ← binaryValue binary previous (.ofNat .int 1)
        let locals ← writeSlot state.locals id destination (after.cast destination.type) false
        pure (if op == Update.postInc || op == Update.postDec then previous else after.cast destination.type, locals)) with
      | .error fault => fail state fault
      | .ok (v, locals) => valueResult {state with locals} destination.type (.ok v)) = _
    simp only [result, written, bind, Except.bind, pure, Except.pure]
    change valueResult {state with locals} destination.type (.ok returned) = _
    simp [valueResult, returnedType]

end EvoIR.Core.Source
