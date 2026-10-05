import EvoIR.Core.ValidatorFacts

/-! Store/layout/freshness transfer for the write and initializer cases of
the single validator-machine preservation proof. -/
namespace EvoIR.Core.Source

theorem write_slot_other_lookup (locals updated : Locals) (symbol other : Nat)
    (destination : QualifiedType) (value : Value) (initialization : Bool)
    (different : other ≠ symbol)
    (written : writeSlot locals symbol destination value initialization = .ok updated) :
    updated.find? (·.symbol == other) = locals.find? (·.symbol == other) := by
  obtain ⟨introduced, slot, suffix, rfl, rfl, selected, _⟩ :=
    write_slot_decompose locals updated symbol destination value initialization written
  have skips : (slot.symbol == other) = false := by
    simp [selected, Ne.symm different]
  simp [List.find?_append, List.find?, skips]

theorem ValidatorEnvironment.Models.after_write
    (before after : ValidatorEnvironment) (locals updated : Locals)
    (models : before.Models locals) (id : Nat) (destination : QualifiedType)
    (value : Value) (initialization : Bool)
    (written : writeSlot locals id destination value initialization = .ok updated)
    (bindings : after.state.bindings = before.state.bindings)
    (layout : after.layout = before.layout)
    (initialized : ∀ other ∈ after.state.initialized,
      other = id ∨ other ∈ before.state.initialized)
    (fresh : ∀ other ∈ after.freshConst, other ∈ before.freshConst ∧ other ≠ id) :
    after.Models updated := by
  have represented := models.realizes.after_write before.state locals updated id destination value
    initialization written after.state.initialized initialized
  have sameLayout := write_slot_binding_types locals updated id destination value initialization written
  refine ⟨?_, sameLayout.trans (models.layout.trans layout.symm), ?_, ?_⟩
  · change (LocalState.mk after.state.bindings after.state.initialized).Realizes updated
    rw [bindings]
    exact represented
  · have sameSymbols := congrArg (List.map Prod.fst) sameLayout
    have symbols : updated.map Slot.symbol = locals.map Slot.symbol := by
      simpa [List.map_map, Function.comp_def] using sameSymbols
    simpa [symbols] using models.distinct
  · intro other member
    obtain ⟨previous, different⟩ := fresh other member
    obtain ⟨slot, found, const, empty⟩ := models.fresh other previous
    exact ⟨slot, (write_slot_other_lookup locals updated id other destination value
      initialization different written).trans found, const, empty⟩

theorem ValidatorEnvironment.Models.writable_not_fresh
    (environment : ValidatorEnvironment) (locals : Locals)
    (models : environment.Models locals) (id : Nat) (destination : QualifiedType)
    (binding : lookup environment.state id = some destination)
    (writable : destination.isConst = false) : id ∉ environment.freshConst := by
  intro member
  obtain ⟨slot, found, typed⟩ := models.realizes.lookup environment.state locals id destination binding
  obtain ⟨freshSlot, freshFound, const, _⟩ := models.fresh id member
  have same : slot = freshSlot := Option.some.inj (found.symm.trans freshFound)
  subst freshSlot
  simp [typed, writable] at const

theorem ValidatorEnvironment.Models.assign
    (environment : ValidatorEnvironment) (locals : Locals)
    (models : environment.Models locals) (id : Nat) (destination : QualifiedType)
    (binding : lookup environment.state id = some destination)
    (writable : destination.isConst = false) (value : Value)
    (typed : value.type = destination.type) :
    ∃ updated, writeSlot locals id destination value false = .ok updated ∧
      ({environment with state.initialized := id :: environment.state.initialized}).Models updated := by
  obtain ⟨updated, written, _⟩ := models.realizes.assign environment.state locals id destination
    binding writable value typed
  refine ⟨updated, written, models.after_write environment _ locals updated id destination value false
    written rfl rfl (by intro other member; simpa using member) ?_⟩
  intro other member
  refine ⟨member, ?_⟩
  intro same
  subst other
  exact models.writable_not_fresh environment locals id destination binding writable member

theorem ValidatorEnvironment.Models.initialize
    (before after : ValidatorEnvironment) (locals : Locals)
    (models : before.Models locals) (id : Nat) (destination : QualifiedType)
    (binding : lookup before.state id = some destination)
    (fresh : destination.isConst = true → id ∈ before.freshConst)
    (bindings : after.state.bindings = before.state.bindings)
    (layout : after.layout = before.layout)
    (initialized : ∀ other ∈ after.state.initialized,
      other = id ∨ other ∈ before.state.initialized)
    (freshAfter : ∀ other ∈ after.freshConst, other ∈ before.freshConst ∧ other ≠ id)
    (value : Value) :
    ∃ updated, writeSlot locals id destination (value.cast destination.type) true = .ok updated ∧
      after.Models updated := by
  obtain ⟨slot, found, typed⟩ := models.realizes.lookup before.state locals id destination binding
  have uninitialized : destination.isConst = true → slot.value = none := by
    intro const
    obtain ⟨freshSlot, freshFound, _, empty⟩ := models.fresh id (fresh const)
    have same : slot = freshSlot := Option.some.inj (found.symm.trans freshFound)
    simpa [same] using empty
  obtain ⟨updated, written, _⟩ := models.realizes.initialize before.state locals id destination
    slot found typed uninitialized (value.cast destination.type) rfl
  exact ⟨updated, written, models.after_write before after locals updated id destination
    (value.cast destination.type) true written bindings layout initialized freshAfter⟩

def ValidatorWriteResult (before : State) (after : ValidatorEnvironment) (type : IntType)
    (next : State) : Prop :=
  (∃ value locals, value.type = type ∧ after.Models locals ∧
    next = {before with control := .value value, locals}) ∨
  (∃ code, code ∈ [3002, 3003, 3004, 3005] ∧ next = fail before (.diagnostic code))

theorem validator_assignment_transfer (environment : ValidatorEnvironment) (state : State)
    (models : environment.Models state.locals) (op : Assignment) (id : Nat)
    (destination : QualifiedType) (value : Value)
    (binding : lookup environment.state id = some destination)
    (writable : destination.isConst = false)
    (initialized : op ≠ .set → id ∈ environment.state.initialized)
    (admitted : !(op == .shl && destination.type.signed) = true) :
    ValidatorWriteResult state {environment with state.initialized := id :: environment.state.initialized}
      destination.type (resumeValue state (.assign destination.type op id destination) value) := by
  have finish (result : Value) := models.assign environment state.locals id destination binding
    writable (result.cast destination.type) rfl
  cases operation : op.binary with
  | none =>
    obtain ⟨locals, written, preserved⟩ := finish value
    left
    refine ⟨value.cast destination.type, locals, rfl, preserved, ?_⟩
    simp only [resumeValue, operation, written, bind, Except.bind, pure, Except.pure]
    simp [valueResult, Value.cast, Value.ofInt]
  | some binary =>
    have notSet : op ≠ .set := by intro same; subst op; cases operation
    obtain ⟨previous, previousType, read⟩ := models.realizes.read environment.state state.locals
      id destination binding (initialized notSet)
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
        exact ⟨code, safe, by simp [resumeValue, operation, read, result, bind, Except.bind]⟩
    | ok resultValue =>
      obtain ⟨locals, written, preserved⟩ := finish resultValue
      left
      refine ⟨resultValue.cast destination.type, locals, rfl, preserved, ?_⟩
      simp only [resumeValue, operation, read, result, written, bind, Except.bind, pure, Except.pure]
      simp [valueResult, Value.cast, Value.ofInt]

theorem validator_update_transfer (environment : ValidatorEnvironment) (state : State)
    (models : environment.Models state.locals) (op : Update) (id : Nat)
    (destination : QualifiedType)
    (binding : lookup environment.state id = some destination)
    (writable : destination.isConst = false) (initialized : id ∈ environment.state.initialized) :
    ValidatorWriteResult state environment destination.type
      (enterExpression state (.update destination.type op id destination
        (.ident destination.type id))) := by
  have placeOk := models.realizes.place environment.state state id destination binding writable
  obtain ⟨previous, previousType, read⟩ := models.realizes.read environment.state state.locals
    id destination binding initialized
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
    obtain ⟨locals, written, preserved⟩ := models.assign environment state.locals id destination binding
      writable (resultValue.cast destination.type) rfl
    have retained : environment.Models locals := preserved.weaken _ environment
      ⟨fun _ => rfl, rfl, by intro other member; exact List.mem_cons_of_mem _ member,
        fun _ member => member⟩
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
