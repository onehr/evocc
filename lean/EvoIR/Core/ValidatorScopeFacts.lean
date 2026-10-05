import EvoIR.Core.ValidatorFacts

/-! Actual declaration/item-fold inversion and retention of a concrete scope
suffix. Prefix separation comes from checker freshness, and the retained store
comes from the live output store, never from a saved runtime ScopeShape. -/
namespace EvoIR.Core.Source

theorem ValidatorEnvironment.Models.initialized_bound
    (environment : ValidatorEnvironment) (locals : Locals) (models : environment.Models locals) :
    ∀ id ∈ environment.state.initialized, (lookup environment.state id).isSome = true := by
  intro id member
  obtain ⟨slot, _, found, _⟩ := models.realizes.initialized id member
  have same := models.realizes.bindings id
  rw [found] at same
  simp only [Option.map_some] at same
  change ((environment.state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd).isSome = true
  rw [← same]
  rfl

theorem ValidatorEnvironment.Models.fresh_bound
    (environment : ValidatorEnvironment) (locals : Locals) (models : environment.Models locals) :
    ∀ id ∈ environment.freshConst, (lookup environment.state id).isSome = true := by
  intro id member
  obtain ⟨slot, found, _, _⟩ := models.fresh id member
  have same := models.realizes.bindings id
  rw [found] at same
  simp only [Option.map_some] at same
  change ((environment.state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd).isSome = true
  rw [← same]
  rfl

theorem checked_declaration (callees : List Function) (fuel : Nat) (input output : LocalState)
    (object : Object) (accepted : declare callees fuel input object = .ok output) :
    lookup input object.symbol = none ∧
      output.bindings = (object.symbol, object.type) :: input.bindings ∧
      match object.initializer with
      | none => output.initialized = input.initialized
      | some (destination, expression) => destination = object.type ∧
          ∃ facts, checkExpr callees fuel
            ⟨(object.symbol, object.type) :: input.bindings, input.initialized⟩ expression = .ok facts ∧
            output.initialized = union facts.initialized [object.symbol] := by
  unfold declare at accepted
  obtain ⟨checkedUnit, absent, remaining⟩ := except_bind_success _ _ _ accepted
  cases checkedUnit
  have absent : lookup input object.symbol = none :=
    Option.isNone_iff_eq_none.mp (require_success _ _ absent)
  obtain ⟨checkedUnit, _, remaining⟩ := except_bind_success _ _ _ remaining
  cases checkedUnit
  cases initializerEq : object.initializer with
  | none =>
    simp only [initializerEq] at remaining
    have same : output = ⟨(object.symbol, object.type) :: input.bindings, input.initialized⟩ := by
      simpa [pure, Except.pure] using remaining.symm
    exact ⟨absent, by simp [same], by simp [same]⟩
  | some pair =>
    rcases pair with ⟨destination, expression⟩
    simp only [initializerEq] at remaining
    obtain ⟨checkedUnit, conversion, remaining⟩ := except_bind_success _ _ _ remaining
    cases checkedUnit
    obtain ⟨state, checked, returned⟩ := except_bind_success _ _ _ remaining
    obtain ⟨facts, expressionChecked, stateEq⟩ := checked_expression_state callees fuel _ state expression checked
    have outputEq : output = {state with initialized := union state.initialized [object.symbol]} := by
      simpa [pure, Except.pure] using returned.symm
    exact ⟨absent, by simp [outputEq, stateEq],
      by simpa using require_success _ _ conversion,
      facts, expressionChecked, by simp [outputEq, stateEq]⟩

def validatorItemStep (callees : List Function) (result : QualifiedType) (fuel depth : Nat)
    (flow : FlowFacts) (item : BlockItem) : Except String FlowFacts := do
  match item with
  | .declaration object => return {flow with state := ← declare callees fuel flow.state object}
  | .statement statement =>
    let next ← checkStmt callees result fuel depth flow.state statement
    return {next with fallsThrough := flow.fallsThrough && next.fallsThrough}

private theorem item_fold_flag (callees : List Function) (result : QualifiedType) (fuel depth : Nat)
    (items : List BlockItem) : ∀ input flag,
    items.foldlM (validatorItemStep callees result fuel depth) ⟨input, flag⟩ =
      (items.foldlM (validatorItemStep callees result fuel depth) ⟨input, true⟩).map
        (fun facts => {facts with fallsThrough := flag && facts.fallsThrough}) := by
  induction items with
  | nil => intro input flag; simp [pure, Except.pure, Except.map]
  | cons item rest ih =>
    intro input flag
    cases item with
    | declaration object =>
      simp only [List.foldlM_cons, validatorItemStep]
      cases declared : declare callees fuel input object with
      | error message => simp [declared, bind, Except.bind, Except.map]
      | ok output =>
        simpa [declared, bind, Except.bind, pure, Except.pure] using ih output flag
    | statement statement =>
      simp only [List.foldlM_cons, validatorItemStep]
      cases checked : checkStmt callees result fuel depth input statement with
      | error message => simp [checked, bind, Except.bind, Except.map]
      | ok output =>
        simp only [checked, bind, Except.bind, pure, Except.pure, Bool.true_and]
        rw [ih output.state (flag && output.fallsThrough), ih output.state output.fallsThrough]
        cases tail : rest.foldlM (validatorItemStep callees result fuel depth) ⟨output.state, true⟩ <;>
          simp [Except.map, Bool.and_assoc]

theorem checked_items_split (callees : List Function) (result : QualifiedType) (fuel depth : Nat)
    (input : LocalState) (first : BlockItem) (remaining : List BlockItem) (facts : FlowFacts)
    (accepted : checkItems callees result fuel depth input (first :: remaining) = .ok facts) :
    ∃ head tail,
      validatorItemStep callees result fuel depth ⟨input, true⟩ first = .ok head ∧
      checkItems callees result fuel depth head.state remaining = .ok tail ∧
      facts.state = tail.state ∧ facts.fallsThrough = (head.fallsThrough && tail.fallsThrough) := by
  change (first :: remaining).foldlM (validatorItemStep callees result fuel depth)
    ⟨input, true⟩ = .ok facts at accepted
  simp only [List.foldlM_cons] at accepted
  obtain ⟨head, checked, remainingCheck⟩ := except_bind_success _ _ _ accepted
  rw [item_fold_flag] at remainingCheck
  cases tailCheck : remaining.foldlM (validatorItemStep callees result fuel depth) ⟨head.state, true⟩ with
  | error message => simp [tailCheck, Except.map] at remainingCheck
  | ok tail =>
    have same : facts = {tail with fallsThrough := head.fallsThrough && tail.fallsThrough} := by
      simpa [tailCheck, Except.map] using remainingCheck.symm
    have tailAccepted : checkItems callees result fuel depth head.state remaining = .ok tail := tailCheck
    exact ⟨head, tail, checked, tailAccepted,
      by simpa only using congrArg FlowFacts.state same,
      by simpa only using congrArg FlowFacts.fallsThrough same⟩

private theorem item_fold_prefix (callees : List Function) (result : QualifiedType) (fuel depth : Nat)
    (items : List BlockItem) : ∀ input facts,
      items.foldlM (validatorItemStep callees result fuel depth) input = .ok facts →
      ∃ introduced, facts.state.bindings = introduced ++ input.state.bindings ∧
        ∀ id, (lookup input.state id).isSome = true →
          introduced.find? (fun pair => pair.1 == id) = none := by
  induction items with
  | nil =>
    intro input facts accepted
    have same : input = facts := by simpa [pure, Except.pure] using accepted
    subst facts
    exact ⟨[], rfl, by simp⟩
  | cons item rest ih =>
    intro input facts accepted
    simp only [List.foldlM_cons] at accepted
    obtain ⟨next, step, tail⟩ := except_bind_success _ _ _ accepted
    cases item with
    | declaration object =>
      unfold validatorItemStep at step
      obtain ⟨state, declared, returned⟩ := except_bind_success _ _ _ step
      have nextEq : next = {input with state} := by simpa [pure, Except.pure] using returned.symm
      obtain ⟨absent, bindings, _⟩ := checked_declaration callees fuel input.state state object declared
      subst next
      obtain ⟨introduced, extension, separate⟩ := ih _ facts tail
      refine ⟨introduced ++ [(object.symbol, object.type)], ?_, ?_⟩
      · simpa [bindings, List.append_assoc] using extension
      · intro id bound
        have different : object.symbol ≠ id := by
          intro same
          simp [← same, absent] at bound
        have boundNext : (lookup state id).isSome = true := by
          simpa [lookup, bindings, List.find?, beq_eq_false_iff_ne.mpr different] using bound
        simp [List.find?_append, separate id boundNext, List.find?, beq_eq_false_iff_ne.mpr different]
    | statement statement =>
      unfold validatorItemStep at step
      obtain ⟨state, checked, returned⟩ := except_bind_success _ _ _ step
      have nextEq : next = {state with fallsThrough := input.fallsThrough && state.fallsThrough} := by
        simpa [pure, Except.pure] using returned.symm
      have bindings := checked_statement_bindings callees result fuel depth input.state statement state checked
      subst next
      obtain ⟨introduced, extension, separate⟩ := ih _ facts tail
      exact ⟨introduced, by simpa [bindings] using extension,
        fun id bound => separate id (by simpa [lookup, bindings] using bound)⟩

theorem checked_items_prefix (callees : List Function) (result : QualifiedType) (fuel depth : Nat)
    (input : LocalState) (items : List BlockItem) (facts : FlowFacts)
    (accepted : checkItems callees result fuel depth input items = .ok facts) :
    ∃ introduced, facts.state.bindings = introduced ++ input.bindings ∧
      ∀ id, (lookup input id).isSome = true →
        introduced.find? (fun pair => pair.1 == id) = none :=
  item_fold_prefix callees result fuel depth items ⟨input, true⟩ facts accepted

private theorem layout_lookup (locals : Locals) (layout : LocalLayout)
    (same : localLayout locals = layout) (id : Nat) :
    (locals.find? (·.symbol == id)).map Slot.type =
      (layout.find? (fun pair => pair.1 == id)).map Prod.snd := by
  rw [← same]
  simp [localLayout, List.find?_map, Option.map_map, Function.comp_def]

theorem validator_retained_models (before after retainedEnvironment : ValidatorEnvironment)
    (introduced : LocalLayout) (locals : Locals) (models : after.Models locals)
    (bindings : after.state.bindings = introduced ++ before.state.bindings)
    (layout : after.layout = introduced ++ before.layout)
    (separate : ∀ id, (lookup before.state id).isSome = true →
      introduced.find? (fun pair => pair.1 == id) = none)
    (retainedBindings : retainedEnvironment.state.bindings = before.state.bindings)
    (retainedLayout : retainedEnvironment.layout = before.layout)
    (initialized : ∀ id ∈ retainedEnvironment.state.initialized,
      id ∈ after.state.initialized ∧ (lookup before.state id).isSome = true)
    (fresh : ∀ id ∈ retainedEnvironment.freshConst,
      id ∈ after.freshConst ∧ (lookup before.state id).isSome = true) :
    ∃ introducedPrefix retained, locals = introducedPrefix ++ retained ∧
      localLayout retained = before.layout ∧ retainedEnvironment.Models retained := by
  have splitLayout : localLayout locals = introduced ++ before.layout := models.layout.trans layout
  obtain ⟨introducedPrefix, retained, split, prefixLayout, suffixLayout⟩ := List.map_eq_append_iff.mp splitLayout
  have distinct : (introducedPrefix.map Slot.symbol ++ retained.map Slot.symbol).Nodup := by
    simpa [split] using models.distinct
  obtain ⟨_, retainedDistinct, different⟩ := List.nodup_append.mp distinct
  have prefixAbsent (id : Nat) (bound : (lookup before.state id).isSome = true) :
      introducedPrefix.find? (·.symbol == id) = none := by
    have same := layout_lookup introducedPrefix introduced prefixLayout id
    rw [separate id bound] at same
    exact Option.map_eq_none_iff.mp same
  have retainedTypes (id : Nat) : (retained.find? (·.symbol == id)).map Slot.type =
      (before.state.bindings.find? (fun pair => pair.1 == id)).map Prod.snd := by
    cases selected : introducedPrefix.find? (·.symbol == id) with
    | none =>
      have staticAbsent : introduced.find? (fun pair => pair.1 == id) = none := by
        have same := layout_lookup introducedPrefix introduced prefixLayout id
        rw [selected] at same
        exact Option.map_eq_none_iff.mp same.symm
      have allTypes := models.realizes.bindings id
      simpa [split, bindings, List.find?_append, selected, staticAbsent] using allTypes
    | some slot =>
      have absent : retained.find? (·.symbol == id) = none := by
        apply List.find?_eq_none.mpr
        intro other member same
        have slotMember := List.mem_map_of_mem (f := Slot.symbol) (List.mem_of_find?_eq_some selected)
        have otherMember := List.mem_map_of_mem (f := Slot.symbol) member
        have slotSymbol : slot.symbol = id := by simpa using List.find?_some selected
        have otherSymbol : other.symbol = id := by simpa using same
        exact different slot.symbol slotMember other.symbol otherMember (slotSymbol.trans otherSymbol.symm)
      have unbound : lookup before.state id = none := by
        cases found : lookup before.state id with
        | none => rfl
        | some type =>
          have missing := prefixAbsent id (by simp [found])
          rw [selected] at missing
          cases missing
      simpa [lookup, absent] using unbound.symm
  refine ⟨introducedPrefix, retained, split, suffixLayout, ⟨⟨?_, ?_, ?_⟩, ?_, retainedDistinct, ?_⟩⟩
  · intro id
    simpa [retainedBindings] using retainedTypes id
  · intro slot member value stored
    exact models.realizes.values slot (by rw [split]; exact List.mem_append_right _ member) value stored
  · intro id member
    obtain ⟨present, bound⟩ := initialized id member
    obtain ⟨slot, value, found, stored⟩ := models.realizes.initialized id present
    exact ⟨slot, value, by simpa [split, List.find?_append, prefixAbsent id bound] using found, stored⟩
  · exact suffixLayout.trans retainedLayout.symm
  · intro id member
    obtain ⟨present, bound⟩ := fresh id member
    obtain ⟨slot, found, const, empty⟩ := models.fresh id present
    exact ⟨slot, by simpa [split, List.find?_append, prefixAbsent id bound] using found, const, empty⟩

end EvoIR.Core.Source
