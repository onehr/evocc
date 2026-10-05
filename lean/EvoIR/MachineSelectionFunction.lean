import EvoIR.MachineSelectionCFG

/-!
Admission of a scalar function connects the closed CFG table to the actual
entry dispatchers and every typed argument vector. A table with no admitted
entry cannot produce a function certificate, even if its closure is vacuous.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def scalarTable (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (proposal : List (String × Nat)) : Option (List (ScalarCutpoint sf tf)) :=
  proposal.mapM fun (label, id) => scalarCutpoint sf tf label id

def scalarFunctionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (proposal : List (String × Nat)) :
    Option (ScalarSelectionPair source target entry parameters type) := do
  let sf : {f // source.functions.find? (·.name == entry) = some f} ←
    match found : source.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let tf : {f // target.functions.find? (·.name == entry) = some f} ←
    match found : target.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let table ← scalarTable sf.val tf.val proposal
  let body ← scalarCFG source target sf.val tf.val type table
  let first : {point // table.find? (fun point =>
      decide (sf.val.blocks.head? = some point.sourceBlock) && point.targetBlock.id == 0) = some point} ←
    match found : table.find? (fun point =>
        decide (sf.val.blocks.head? = some point.sourceBlock) && point.targetBlock.id == 0) with
    | none => none
    | some point => some ⟨point, found⟩
  let args := (parameterWords parameters).map CWord.word
  if signature : sf.val.parameters = args.map (fun word => .bits word.1) then
    if blockTypes : first.val.sourceBlock.parameters.map IR.Binding.type = sf.val.parameters then
      if widths : first.val.widths = args.map Sigma.fst then
        return ⟨by
          intro values typed
          let σ := inputSubst values
          let next := rebaseWords σ args
          have entryShape : sf.val.blocks.head? = some first.val.sourceBlock ∧ first.val.targetBlock.id = 0 := by
            simpa only [Bool.and_eq_true, decide_eq_true_eq, beq_iff_eq] using List.find?_some first.property
          have bounded : ∀ word ∈ args, word.1 ≤ 64 := by
            intro word member
            apply first.val.bounded word.1
            rw [widths]
            exact List.mem_map.mpr ⟨word, member, rfl⟩
          have allInputs := parameter_words_cover parameters values typed
          have sourceInputs : args.map (Word.denote σ) = values.map sourceValueIR := by
            rw [← allInputs]
            simp only [args, List.map_map, Function.comp_def]
            rfl
          have targetInputs : machineInputs σ args = values.map EvoIR.Machine.sourceArgument := by
            rw [← allInputs]
            simp only [args, machineInputs, List.map_map, Function.comp_def]
            rfl
          have sourceBind := first.val.sourceBinding next
          rw [widths, cutpoint_words_rebase σ args bounded, sourceInputs] at sourceBind
          have targetBind := first.val.targetBinding next
          rw [widths, cutpoint_machine_rebase σ args bounded, targetInputs] at targetBind
          have sourceTypes : (values.map sourceValueIR).map IR.Value.type = sf.val.parameters := by
            rw [← sourceInputs, List.map_map, signature]
            rfl
          have sourceEntered : IR.enter sf.val (values.map sourceValueIR) 1 =
              .ok ⟨sf.val.name, first.val.sourceBlock.name, 0,
                first.val.scope.denote next (fun _ => .world), 1⟩ := by
            simp [IR.enter, sourceTypes, entryShape.1, blockTypes, sourceBind,
              bind, Except.bind, pure, Except.pure]
          have targetFound : tf.val.blocks.find? (·.id == 0) = some first.val.targetBlock := by
            simpa only [entryShape.2] using first.val.targetFound
          have targetEntered : EvoIR.Machine.enter tf.val (values.map EvoIR.Machine.sourceArgument) 1 =
              .ok ⟨tf.val.name, first.val.targetBlock.id, 0, bankDenote next first.val.bank,
                1, values.map EvoIR.Machine.sourceArgument⟩ := by
            simp [EvoIR.Machine.enter, targetFound, targetBind, bind, Except.bind, pure, Except.pure]
          let s := sourceState next (fun _ => .world) sf.val first.val.sourceBlock 0 first.val.scope 1 2 []
          let t := machineState next tf.val first.val.targetBlock 0 first.val.bank 1 2
            (values.map EvoIR.Machine.sourceArgument) [] ⟨1, []⟩
          refine ⟨s, t, ?_, ?_, ?_⟩
          · simp [IR.initial, sf.property, sourceEntered, s, sourceState,
              bind, Except.bind, pure, Except.pure]
          · simp [EvoIR.Machine.initial, tf.property, targetEntered, t, machineState,
              bind, Except.bind, pure, Except.pure]
          · exact body.checked s t
              ⟨first.val, List.mem_of_find?_eq_some first.property, next, 1, 2,
                values.map EvoIR.Machine.sourceArgument, ⟨1, []⟩, rfl, rfl⟩⟩
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
