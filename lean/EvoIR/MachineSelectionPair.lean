import EvoIR.MachineSelectionEntry

/-!
Whole-entry selection result for the actual IR and SSA MIR pair. The first
builder handles straight-line scalar bodies. Other builders must return the
same all-argument proposition; a successful instruction quote or a finite
execution sample is not an instance of this result.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

structure ScalarSelectionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) : Type where
  checked : ∀ values : List Source.Value,
    values.map Source.Value.type = parameters.map (fun p => p.type.type) →
    ∃ sourceEntry targetEntry,
      IR.initial source entry (values.map sourceValueIR) = .ok sourceEntry ∧
      EvoIR.Machine.initial target entry (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source target type sourceEntry targetEntry

def straightLinePair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (ScalarSelectionPair source target entry parameters type) := do
  let sf : {f // source.functions.find? (·.name == entry) = some f} ←
    match found : source.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let tf : {f // target.functions.find? (·.name == entry) = some f} ←
    match found : target.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let sb : {b // sf.val.blocks.head? = some b} ←
    match found : sf.val.blocks.head? with
    | none => none
    | some b => some ⟨b, found⟩
  let args := (parameterWords parameters).map CWord.word
  if signature : sf.val.parameters = args.map (fun word => .bits word.1) then
    if blockTypes : sb.val.parameters.map IR.Binding.type = sf.val.parameters then
      let scope : {scope // bindSlots (fun _ => .world) sb.val.parameters
          (args.map Slot.word) = some scope} ←
        match bound : bindSlots (fun _ => .world) sb.val.parameters (args.map Slot.word) with
        | none => none
        | some scope => some ⟨scope, bound⟩
      let machine ← machineEntry tf.val args
      let sourceBody ← sourceSpan source sf.val sb.val sb.val.instructions.length 0 scope.val
      if noGuards : sourceBody.val.checks = [] then
        let targetBody ← machineSpan target tf.val machine.val.1 machine.val.1.insts.length 0 machine.val.2
        let finished ← scalarReturn source target sf.val sb.val sourceBody.val.scope
          tf.val machine.val.1 targetBody.val type
        return ⟨by
          intro values typed
          let σ := inputSubst values
          let ρ : Nat → IR.Value := fun _ => .world
          have allInputs := parameter_words_cover parameters values typed
          have sourceInputs : args.map (Word.denote σ) = values.map sourceValueIR := by
            rw [← allInputs]
            simp only [args, List.map_map, Function.comp_def]
            rfl
          have targetInputs : machineInputs σ args = values.map EvoIR.Machine.sourceArgument := by
            rw [← allInputs]
            simp only [args, machineInputs, List.map_map, Function.comp_def]
            rfl
          have binding := bind_slots_sound σ ρ (fun _ => .world) (by intro id; rfl)
            sb.val.parameters (args.map Slot.word) scope.val scope.property
          have sourceBind : IR.bind sb.val.parameters (values.map sourceValueIR) =
              .ok (scope.val.denote σ ρ) := by
            simpa only [List.map_map, Function.comp_def, Slot.denote, ← sourceInputs] using binding
          have sourceTypes : (values.map sourceValueIR).map IR.Value.type = sf.val.parameters := by
            rw [← sourceInputs, List.map_map, signature]
            rfl
          have entered : IR.enter sf.val (values.map sourceValueIR) 1 =
              .ok ⟨sf.val.name, sb.val.name, 0, scope.val.denote σ ρ, 1⟩ := by
            simp [IR.enter, sourceTypes, sb.property, blockTypes, sourceBind,
              Bind.bind, Except.bind, Pure.pure, Except.pure]
          let s := sourceState σ ρ sf.val sb.val 0 scope.val 1 2 []
          let t := machineState σ tf.val machine.val.1 0 machine.val.2 1 2
            (machineInputs σ args) [] ⟨1, []⟩
          refine ⟨s, t, ?_, ?_, ?_⟩
          · simp [IR.initial, sf.property, entered, s, sourceState,
              Bind.bind, Except.bind, Pure.pure, Except.pure]
          · rw [← targetInputs]
            simp [EvoIR.Machine.initial, tf.property, machine.property σ 1, t, machineState,
              Bind.bind, Except.bind, Pure.pure, Except.pure]
          · obtain ⟨last, path, outcome⟩ := sourceBody.property σ ρ 1 2 []
            have lastState : last = sourceState σ ρ sf.val sb.val sb.val.instructions.length
                sourceBody.val.scope 1 2 [] := by
              simpa only [noGuards, guards, pure, Except.pure, Nat.zero_add] using outcome
            rw [lastState] at path
            have targetPath := targetBody.property σ 1 2 (machineInputs σ args) [] ⟨1, []⟩
            have tail := finished.down σ ρ 1 2 (machineInputs σ args) ⟨1, []⟩
            exact tail.prepend path.toReaches (by simpa only [Nat.zero_add] using targetPath.reaches)⟩
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
