import EvoIR.Core.ValidatorInvariant

/-! Extract the safety interface from the actual independent validator.
The private callback abbreviations below are definitionally the callbacks in
`checkWellFormed` and `checkFunction`, not another admission algorithm. -/
namespace EvoIR.Core.Source

private theorem except_bind_success {α β : Type} (first : Except String α)
    (next : α → Except String β) (result : β)
    (accepted : first >>= next = .ok result) :
    ∃ value, first = .ok value ∧ next value = .ok result := by
  cases first with
  | error message => simp [Bind.bind, Except.bind] at accepted
  | ok value => exact ⟨value, rfl, accepted⟩

private theorem require_success (condition : Bool) (message : String)
    (accepted : require condition message = .ok ()) : condition = true := by
  cases condition <;> simp [require] at accepted ⊢

private def functionAdmissionStep (prior : List Function) (function : Function) :
    Except String (List Function) := do
  require (!(prior.any (fun previous =>
    previous.symbol == function.symbol || previous.name == function.name)))
    "duplicate function symbol or name"
  checkFunction prior function
  return prior ++ [function]

private def parameterAdmissionStep (callees : List Function) (functionId : Nat)
    (state : LocalState) (parameter : Parameter) : Except String LocalState := do
  require (parameter.storage != .auto) "auto storage is invalid for a parameter"
  require ((lookup state parameter.symbol).isNone && parameter.symbol != functionId &&
    !(callees.any (fun callee => callee.symbol == parameter.symbol)))
    "duplicate or nonlocal parameter symbol"
  return ⟨(parameter.symbol, parameter.type) :: state.bindings,
    parameter.symbol :: state.initialized⟩

private theorem checked_function_admission (program : Program) (function : Function)
    (prior following : List Function)
    (order : program.functions = prior ++ function :: following)
    (accepted : checkFunction prior function = .ok ()) :
    Nonempty (CheckedSourceFunction program function) := by
  unfold checkFunction at accepted
  obtain ⟨state, parameters, body⟩ := except_bind_success _ _ _ accepted
  obtain ⟨facts, checked, returns⟩ := except_bind_success _ _ _ body
  have returns := require_success _ _ returns
  refine ⟨{
    preceding := prior, following, order, parameterState := state,
    parameterCheck := parameters,
    body := ⟨1024, facts, checked⟩,
    returns := by simpa using returns }⟩

private theorem checked_function_fold (program : Program) (todo : List Function) :
    ∀ prior result,
      program.functions = prior ++ todo →
      todo.foldlM functionAdmissionStep prior = .ok result →
      (prior.map Function.symbol).Nodup →
      (∀ function ∈ todo, Nonempty (CheckedSourceFunction program function)) ∧
        (program.functions.map Function.symbol).Nodup := by
  induction todo with
  | nil =>
    intro prior result order accepted distinct
    refine ⟨?_, ?_⟩
    · intro function member
      cases member
    · simpa [order] using distinct
  | cons function rest ih =>
    intro prior result order accepted distinct
    simp only [List.foldlM_cons] at accepted
    obtain ⟨next, head, tail⟩ := except_bind_success _ _ _ accepted
    unfold functionAdmissionStep at head
    obtain ⟨guardResult, guardCheck, afterGuard⟩ := except_bind_success _ _ _ head
    obtain ⟨functionResult, functionCheck, nextEq⟩ := except_bind_success _ _ _ afterGuard
    cases guardResult
    cases functionResult
    have fresh := require_success _ _ guardCheck
    have nextEq : next = prior ++ [function] := by simpa [pure, Except.pure] using nextEq.symm
    subst next
    have noSymbol : function.symbol ∉ prior.map Function.symbol := by
      intro member
      obtain ⟨previous, member, same⟩ := List.mem_map.mp member
      have freshAny : prior.any (fun previous =>
          previous.symbol == function.symbol || previous.name == function.name) = false := by
        cases value : prior.any (fun previous =>
            previous.symbol == function.symbol || previous.name == function.name) <;>
          simp_all
      have absent := List.any_eq_false.mp freshAny previous member
      simp [same] at absent
    have extended : ((prior ++ [function]).map Function.symbol).Nodup := by
      simp only [List.map_append, List.map_cons, List.map_nil]
      apply List.nodup_append.mpr
      refine ⟨distinct, by simp, ?_⟩
      intro symbol member other singleton same
      have otherEq : other = function.symbol := by simpa using singleton
      exact noSymbol ((same.trans otherEq) ▸ member)
    have tailOrder : program.functions = (prior ++ [function]) ++ rest := by
      simpa [List.append_assoc] using order
    obtain ⟨remaining, symbols⟩ := ih (prior ++ [function]) result tailOrder tail extended
    refine ⟨?_, symbols⟩
    intro selected member
    rcases List.mem_cons.mp member with same | member
    · subst selected
      exact checked_function_admission program function prior rest order functionCheck
    · exact remaining selected member

theorem checked_source_program (program : Program)
    (accepted : checkWellFormed program = .ok ()) : Nonempty (CheckedSourceProgram program) := by
  classical
  have checked := accepted
  unfold checkWellFormed at checked
  obtain ⟨_, _, checked⟩ := except_bind_success _ _ _ checked
  obtain ⟨_, _, checked⟩ := except_bind_success _ _ _ checked
  obtain ⟨result, functions, _⟩ := except_bind_success _ _ _ checked
  have functions : program.functions.foldlM functionAdmissionStep [] = .ok result := functions
  obtain ⟨table, symbols⟩ := checked_function_fold program program.functions [] result
    (by simp) functions (by simp)
  exact ⟨⟨accepted, fun function member => Classical.choice (table function member), symbols⟩⟩

private theorem checked_parameter_fold (callees : List Function) (functionId : Nat)
    (parameters : List Parameter) : ∀ initial result,
      parameters.foldlM (parameterAdmissionStep callees functionId) initial = .ok result →
      (initial.bindings.map Prod.fst).Nodup →
      result.bindings = parameters.reverse.map (fun parameter => (parameter.symbol, parameter.type)) ++ initial.bindings ∧
      result.initialized = parameters.reverse.map Parameter.symbol ++ initial.initialized ∧
      (result.bindings.map Prod.fst).Nodup := by
  induction parameters with
  | nil =>
    intro initial result accepted distinct
    have same : initial = result := by simpa [pure, Except.pure] using accepted
    subst result
    exact ⟨rfl, rfl, distinct⟩
  | cons parameter rest ih =>
    intro initial result accepted distinct
    simp only [List.foldlM_cons] at accepted
    obtain ⟨next, step, tail⟩ := except_bind_success _ _ _ accepted
    unfold parameterAdmissionStep at step
    obtain ⟨_, _, step⟩ := except_bind_success _ _ _ step
    obtain ⟨_, fresh, nextEq⟩ := except_bind_success _ _ _ step
    have fresh := require_success _ _ fresh
    have absent : lookup initial parameter.symbol = none := by
      have absent : (lookup initial parameter.symbol).isNone = true := by
        exact (Bool.and_eq_true_iff.mp (Bool.and_eq_true_iff.mp fresh).1).1
      exact Option.isNone_iff_eq_none.mp absent
    have notMember : parameter.symbol ∉ initial.bindings.map Prod.fst := by
      intro member
      obtain ⟨binding, member, same⟩ := List.mem_map.mp member
      have missing : initial.bindings.find? (fun pair => pair.1 == parameter.symbol) = none :=
        Option.map_eq_none_iff.mp absent
      have noBinding := List.find?_eq_none.mp missing binding member
      simp [same] at noBinding
    have nextEq : next = ⟨(parameter.symbol, parameter.type) :: initial.bindings,
        parameter.symbol :: initial.initialized⟩ := by
      simpa [pure, Except.pure] using nextEq.symm
    subst next
    have extended : (((parameter.symbol, parameter.type) :: initial.bindings).map Prod.fst).Nodup := by
      simpa using List.nodup_cons.mpr ⟨notMember, distinct⟩
    obtain ⟨bindings, initialized, distinct⟩ := ih _ result tail extended
    refine ⟨?_, ?_, distinct⟩
    · simpa [List.reverse_cons, List.map_append, List.append_assoc] using bindings
    · simpa [List.reverse_cons, List.map_append, List.append_assoc] using initialized

private theorem typed_parameter_locals (parameters : List Parameter) (arguments : List Value)
    (typed : arguments.map Value.type = parameters.map (fun parameter => parameter.type.type))
    (distinct : (parameters.map Parameter.symbol).Nodup) :
    ∃ locals, parameterLocals parameters arguments = .ok locals ∧
      localLayout locals = parameters.map (fun parameter => (parameter.symbol, parameter.type)) ∧
      locals.map Slot.symbol = parameters.map Parameter.symbol ∧
      ∀ slot ∈ locals, ∃ value, slot.value = some value ∧ value.type = slot.type.type := by
  induction parameters generalizing arguments with
  | nil =>
    cases arguments with
    | nil => exact ⟨[], rfl, rfl, rfl, by intro slot member; cases member⟩
    | cons value rest => simp at typed
  | cons parameter rest ih =>
    cases arguments with
    | nil => simp at typed
    | cons value values =>
      obtain ⟨valueType, tailTypes⟩ := List.cons.inj typed
      obtain ⟨fresh, tailDistinct⟩ := List.nodup_cons.mp distinct
      obtain ⟨locals, bound, layout, symbols, initialized⟩ := ih values tailTypes tailDistinct
      have noDuplicate : locals.any (·.symbol == parameter.symbol) = false := by
        apply List.any_eq_false.mpr
        intro slot member same
        have represented : slot.symbol ∈ locals.map Slot.symbol := List.mem_map_of_mem member
        rw [symbols] at represented
        exact fresh ((beq_iff_eq.mp same) ▸ represented)
      refine ⟨⟨parameter.symbol, parameter.type, some value⟩ :: locals, ?_, ?_, ?_, ?_⟩
      · simp [parameterLocals, valueType, bound, noDuplicate, Bind.bind, Except.bind, pure, Except.pure]
      · simpa [localLayout] using congrArg (List.cons (parameter.symbol, parameter.type)) layout
      · simpa using congrArg (List.cons parameter.symbol) symbols
      · intro slot member
        rcases List.mem_cons.mp member with rfl | member
        · exact ⟨value, rfl, valueType⟩
        · exact initialized slot member

private theorem binding_lookup_reverse (bindings : List (Nat × QualifiedType))
    (distinct : (bindings.map Prod.fst).Nodup) (id : Nat) :
    (bindings.reverse.find? (fun pair => pair.1 == id)).map Prod.snd =
      (bindings.find? (fun pair => pair.1 == id)).map Prod.snd := by
  induction bindings with
  | nil => rfl
  | cons binding rest ih =>
    obtain ⟨fresh, tailDistinct⟩ := List.nodup_cons.mp distinct
    by_cases selected : binding.1 = id
    · have absent : rest.find? (fun pair => pair.1 == id) = none := by
        apply List.find?_eq_none.mpr
        intro pair member same
        have represented : pair.1 ∈ rest.map Prod.fst := List.mem_map_of_mem member
        have same : pair.1 = binding.1 := (beq_iff_eq.mp same).trans selected.symm
        exact fresh (same ▸ represented)
      have reverseAbsent : rest.reverse.find? (fun pair => pair.1 == id) = none := by
        simpa [List.find?_eq_none] using absent
      simp [List.reverse_cons, List.find?_append, List.find?, selected, reverseAbsent]
    · have different : (binding.1 == id) = false := beq_eq_false_iff_ne.mpr selected
      simpa [List.reverse_cons, List.find?_append, List.find?, different] using ih tailDistinct

theorem checked_parameters_models (program : Program) (function : Function)
    (checked : CheckedSourceFunction program function) (arguments : List Value)
    (typed : arguments.map Value.type = function.parameters.map (fun parameter => parameter.type.type)) :
    ∃ locals, parameterLocals function.parameters arguments = .ok locals ∧
      (ValidatorEnvironment.mk checked.parameterState (localLayout locals) []).Models locals := by
  have parameters := checked.parameterCheck
  change function.parameters.foldlM (parameterAdmissionStep checked.preceding function.symbol)
    ⟨[], []⟩ = .ok checked.parameterState at parameters
  obtain ⟨bindings, initialized, stateDistinct⟩ := checked_parameter_fold checked.preceding
    function.symbol function.parameters ⟨[], []⟩ checked.parameterState parameters (by simp)
  simp only [List.append_nil] at bindings initialized
  have distinct : (function.parameters.map Parameter.symbol).Nodup := by
    have reversed : (function.parameters.map Parameter.symbol).reverse.Nodup := by
      simpa [bindings, List.map_map, Function.comp_def, List.map_reverse] using stateDistinct
    exact (List.pairwise_reverse.mp reversed).imp (fun different same => different same.symm)
  obtain ⟨locals, bound, layout, symbols, values⟩ := typed_parameter_locals function.parameters arguments typed distinct
  have bindingsEq : checked.parameterState.bindings = (localLayout locals).reverse := by
    simpa [layout, List.map_reverse] using bindings
  have initializedEq : checked.parameterState.initialized = (locals.map Slot.symbol).reverse := by
    simpa [symbols, List.map_reverse] using initialized
  have localDistinct : (locals.map Slot.symbol).Nodup := by simpa [symbols] using distinct
  refine ⟨locals, bound, ⟨⟨?_, ?_, ?_⟩, rfl, localDistinct, by simp⟩⟩
  · intro id
    have lookupEq := binding_lookup_reverse (localLayout locals)
      (by simpa [localLayout, List.map_map, Function.comp_def] using localDistinct) id
    rw [bindingsEq]
    simpa [localLayout, List.find?_map, Function.comp_def, Option.map_map] using lookupEq.symm
  · intro slot member value stored
    obtain ⟨represented, same, valueType⟩ := values slot member
    have same : represented = value := Option.some.inj (same.symm.trans stored)
    simpa [← same] using valueType
  · intro id member
    have represented : id ∈ locals.map Slot.symbol := by simpa [initializedEq] using member
    cases found : locals.find? (·.symbol == id) with
    | none =>
      obtain ⟨slot, member, same⟩ := List.mem_map.mp represented
      have absent := List.find?_eq_none.mp found slot member
      simp [same] at absent
    | some slot =>
      obtain ⟨value, stored, _⟩ := values slot (List.mem_of_find?_eq_some found)
      exact ⟨slot, value, rfl, stored⟩

end EvoIR.Core.Source
