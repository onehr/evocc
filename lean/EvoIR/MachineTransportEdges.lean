import EvoIR.MachineTransportRoutes
import EvoIR.MachineTransportEntry

/-! The actual source edge captures its full argument vector before SSA
binding. Checked target routing realizes the demanded representatives, then
the new block's input valuation is reconstructed from those actual banks. -/
namespace EvoIR.Machine.Transport

theorem edgeParameterValid_needed (needed : List Nat) (mapping : UseMap)
    (source transported : Origins) (param arg : VReg)
    (accepted : edgeParameterValid needed mapping source transported param arg = true)
    (demanded : param.id ∈ needed) :
    param.class = arg.class ∧ ∃ representative expected,
      mapping.lookup param.id = some representative ∧ origin source arg = some expected ∧
      origin transported representative = some expected := by
  by_cases wrongClass : param.class ≠ arg.class
  · simp [edgeParameterValid, wrongClass] at accepted
  · have sameClass : param.class = arg.class := Classical.byContradiction wrongClass
    refine ⟨sameClass, ?_⟩
    cases representativeAt : mapping.lookup param.id with
    | none => simp [edgeParameterValid, wrongClass, demanded, representativeAt] at accepted
    | some representative =>
      cases sourceAt : origin source arg with
      | none => simp [edgeParameterValid, wrongClass, demanded, representativeAt, sourceAt] at accepted
      | some expected =>
        have targetAt : origin transported representative = some expected := by
          simpa [edgeParameterValid, wrongClass, demanded, representativeAt, sourceAt] using accepted
        exact ⟨representative, expected, rfl, rfl, targetAt⟩

theorem edgeValid_data (source target : Function) (plans : FunctionPlan) (env : BodyResult)
    (original actual : Nat × List VReg)
    (accepted : edgeValid source target plans env original actual = true) :
    ∃ destination plan transported,
      findBlock source original.1 = some destination ∧ findPlan plans original.1 = some plan ∧
      destination.args.length = original.2.length ∧ actual.2 = [] ∧
      routeOrigins source target plans original.1 (target.blocks.length+1) actual.1 env.target = some transported ∧
      ∀ pair ∈ destination.args.zip original.2,
        edgeParameterValid plans.needed plan.bindings env.source transported pair.1 pair.2 = true := by
  cases destinationAt : findBlock source original.1 with
  | none => simp [edgeValid, destinationAt] at accepted
  | some destination =>
    cases planAt : findPlan plans original.1 with
    | none => simp [edgeValid, destinationAt, planAt] at accepted
    | some plan =>
      by_cases wrongArity : destination.args.length ≠ original.2.length
      · simp [edgeValid, destinationAt, planAt, wrongArity] at accepted
      · by_cases uncleared : actual.2 ≠ []
        · simp [edgeValid, destinationAt, planAt, wrongArity, uncleared] at accepted
        · have arity : destination.args.length = original.2.length := by omega
          have empty : actual.2 = [] := Classical.byContradiction uncleared
          cases routed : routeOrigins source target plans original.1 (target.blocks.length+1) actual.1 env.target with
          | none => simp [edgeValid, destinationAt, planAt, wrongArity, empty, routed] at accepted
          | some transported =>
            have pairs : ∀ pair ∈ destination.args.zip original.2,
                edgeParameterValid plans.needed plan.bindings env.source transported pair.1 pair.2 = true := by
              simpa [edgeValid, destinationAt, planAt, wrongArity, empty, routed, List.all_eq_true] using accepted
            exact ⟨destination, plan, transported, rfl, rfl, arity, empty, rfl, pairs⟩

theorem origin_value_normalized (valuation : Origin → Slot) (normalized : OriginsNormalized valuation)
    (key : Origin) (regClass : RegClass) (sameClass : key.class = regClass) (value : Value)
    (slot : valuation key = .value value) : normalize regClass value = value := by
  by_cases general : key.class ≠ .flags
  · obtain ⟨concrete, concreteSlot, concreteNormalized⟩ := normalized key general
    have same : concrete = value := Slot.value.inj (concreteSlot.symm.trans slot)
    simpa only [sameClass, same] using concreteNormalized
  · have flags : regClass = .flags := Classical.byContradiction (fun h => general (by simpa only [sameClass] using h))
    rw [flags]
    cases value <;> rfl

theorem edge_entryReady (destination : Block) (plan : BlockPlan) (needed : List Nat)
    (args : List VReg) (captured : List Value) (source target : RegisterFile)
    (sourceAfter : RegisterFile) (sourceEnv transported : Origins) (valuation : Origin → Slot)
    (arity : destination.args.length = args.length)
    (bindings : bindingsValid destination plan needed = true)
    (parameters : ∀ pair ∈ destination.args.zip args,
      edgeParameterValid needed plan.bindings sourceEnv transported pair.1 pair.2 = true)
    (sourceReads : args.mapM (read source) = .ok captured)
    (sourceBound : bind [] destination.args captured = .ok sourceAfter)
    (sourceReal : Realizes sourceEnv valuation source)
    (targetReal : Realizes transported valuation target)
    (normalized : OriginsNormalized valuation) :
    EntryReady destination.args plan.bindings sourceAfter target := by
  have sourceReady := bind_normalized_reads [] destination.args captured sourceAfter sourceBound
  have parameterValue : ∀ param ∈ destination.args, param.id ∈ needed →
      ∃ representative value,
        plan.bindings.lookup param.id = some representative ∧
        readOperand sourceAfter param = .ok (.value value) ∧ normalize param.class value = value ∧
        readOperand target representative = .ok (.value value) ∧ normalize representative.class value = value := by
    intro param member demanded
    have firsts : (destination.args.zip args).map Prod.fst = destination.args := List.map_fst_zip (by omega)
    have pairMember : param ∈ (destination.args.zip args).map Prod.fst := by rw [firsts]; exact member
    obtain ⟨pair, paired, pairFirst⟩ := List.mem_map.mp pairMember
    rcases pair with ⟨param', arg⟩
    change param' = param at pairFirst
    subst param'
    obtain ⟨sameClass, representative, key, representativeAt, sourceAt, targetAt⟩ :=
      edgeParameterValid_needed needed plan.bindings sourceEnv transported param arg
        (parameters _ paired) demanded
    obtain ⟨value, valueInBinding, readArg⟩ := reads_zip_pairs destination.args args source captured sourceReads
      param arg paired
    have sourceSlot := sourceReal arg key sourceAt
    have slot : valuation key = .value value := by
      cases concrete : valuation key with
      | flags flags =>
        have wrong := read_of_operand_ok source arg (.flags flags) (by simpa only [concrete] using sourceSlot)
        rw [readArg] at wrong
        simp [slotValue] at wrong
      | value actualValue =>
        have readActual := read_of_operand_ok source arg (.value actualValue) (by simpa only [concrete] using sourceSlot)
        have valuesEqual : actualValue = value := by
          simpa only [slotValue, readArg, Except.ok.injEq] using readActual.symm
        exact congrArg Slot.value valuesEqual
    have keyClass : key.class = param.class := (origin_class sourceEnv arg key sourceAt).trans sameClass.symm
    have normalParam := origin_value_normalized valuation normalized key param.class keyClass value slot
    have representativeClass := origin_class transported representative key targetAt
    have normalRepresentative := origin_value_normalized valuation normalized key representative.class representativeClass value slot
    have sourceAfterRead := bind_parameter_read [] destination.args captured sourceAfter sourceBound param value valueInBinding
    rw [normalParam] at sourceAfterRead
    refine ⟨representative, value, representativeAt, sourceAfterRead, normalParam, ?_, normalRepresentative⟩
    rw [targetReal representative key targetAt, slot]
  constructor
  · intro param member
    obtain ⟨value, readable, normal⟩ := sourceReady param member
    refine ⟨value, readable, normal, ?_⟩
    intro representative mapped
    obtain ⟨before, after, lookupShape, _⟩ := List.lookup_eq_some_iff.mp mapped
    have inMapping : (param.id, representative) ∈ plan.bindings := by rw [lookupShape]; simp
    have demanded := bindingsValid_needed destination plan needed bindings _ inMapping
    obtain ⟨actual, actualValue, actualAt, sourceRead, _, targetRead, targetNormal⟩ := parameterValue param member demanded
    have sameRepresentative : actual = representative := Option.some.inj (actualAt.symm.trans mapped)
    have sameValue : actualValue = value := Slot.value.inj (Except.ok.inj (sourceRead.symm.trans readable))
    simpa only [sameRepresentative, sameValue] using And.intro targetRead targetNormal
  · intro entry member
    obtain ⟨param, paramMember, sameId, _⟩ := bindingsValid_member destination plan needed bindings entry member
    have demanded : param.id ∈ needed := by rw [sameId]; exact bindingsValid_needed destination plan needed bindings entry member
    obtain ⟨representative, value, representativeAt, _, _, targetRead, targetNormal⟩ := parameterValue param paramMember demanded
    have entryAt : plan.bindings.lookup param.id = some entry.2 := by
      rw [sameId]
      exact lookup_mem_unique plan.bindings (bindingsValid_unique destination plan needed bindings) entry member
    have same : representative = entry.2 := Option.some.inj (representativeAt.symm.trans entryAt)
    exact ⟨value, by simpa only [same] using targetRead, by simpa only [same] using targetNormal⟩

end EvoIR.Machine.Transport
