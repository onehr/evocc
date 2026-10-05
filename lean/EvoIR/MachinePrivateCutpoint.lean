import EvoIR.MachinePrivateBinding
import EvoIR.MachinePrivateReturn
import EvoIR.MachineTransportBindings
import EvoIR.Core.PrivateCheckpointBank
import EvoIR.Core.PrivateProject

/-!
Private cutpoint proposals describe scalar parameters and live cell values.
The projection is a finite syntax bank, not another source execution. The
checker validates the actual full IR binding and the actual positional MIR
binding, retaining every pointer identity as a Nat-valued private token.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def privatePointInputs {Γ : Var → Nat} (config : PrivateConfig Γ) :
    (parameters : List IR.Binding) → (regs : List EvoIR.Machine.VReg) →
    Option {inputs : List (PrivateInput Γ) // inputs.map PrivateInput.target = regs ∧
      ∀ σ values incoming owner frame, config.Realizes σ values incoming owner frame →
        PrivateInputsBound inputs values (config.scope.denote σ (privateToken values))}
  | [], [] => some ⟨[], rfl, by
      intro σ values incoming owner frame models name reg key member
      cases member⟩
  | parameter :: parameters, regs =>
    match parameter.type with
    | .memory | .world => privatePointInputs config parameters regs
    | .bits _ =>
      match regsShape : regs with
      | [] => none
      | reg :: rest => do
        let word ← config.scope.word parameter.id
        let tail ← privatePointInputs config parameters rest
        return ⟨.word parameter.id reg word :: tail.val, by
          simp only [List.map_cons, PrivateInput.target, tail.property.1, regsShape], by
          intro σ values incoming owner frame models name target key member
          rcases List.mem_cons.mp member with impossible | member
          · cases impossible
          · exact tail.property.2 σ values incoming owner frame models name target key member⟩
    | .pointer =>
      match regsShape : regs with
      | [] => none
      | reg :: rest => do
        let key : {key // config.scope.lookup parameter.id = some (.opaque key)} ←
          match found : config.scope.lookup parameter.id with
          | some (.opaque key) => some ⟨key, found⟩
          | _ => none
        let cell : {cell // config.cells.find? (·.pointer == key.val) = some cell} ←
          match found : config.cells.find? (·.pointer == key.val) with
          | none => none
          | some cell => some ⟨cell, found⟩
        let tail ← privatePointInputs config parameters rest
        return ⟨.pointer parameter.id reg key.val :: tail.val, by
          simp only [List.map_cons, PrivateInput.target, tail.property.1, regsShape], by
          intro σ values incoming owner frame models name target pointerKey member
          rcases List.mem_cons.mp member with equal | member
          · cases equal
            have pointerName : cell.val.pointer = key.val := by simpa using List.find?_some cell.property
            obtain ⟨_, _, _, _, _, _, _, cells⟩ := models.2.2.2.2
            have initialized := (cells cell.val (List.mem_of_find?_eq_some cell.property)).1
            have actual : privateToken values key.val = .pointer (cell.val.id values) 0 := by
              simpa only [pointerName] using initialized
            refine ⟨cell.val.id values, ?_, actual⟩
            simp [IR.lookup, lookup_denote, key.property, Slot.denote, actual]
          · exact tail.property.2 σ values incoming owner frame models name target pointerKey member⟩
  | _, _ => none

structure PrivatePointProposal where
  layout : CutpointLayout
  target : Nat

def privateParameterNames (parameters : List IR.Binding) : List Nat :=
  parameters.filterMap fun parameter => match parameter.type with
    | .bits _ | .pointer => some parameter.id
    | .memory | .world => none

structure PrivateCutpoint (sf : IR.Function) (tf : EvoIR.Machine.Function) where
  source : PrivateLayoutPoint sf
  targetBlock : EvoIR.Machine.Block
  targetFound : tf.blocks.find? (·.id == targetBlock.id) = some targetBlock
  inputs : List (PrivateInput inputWidths)
  targetNames : inputs.map PrivateInput.target = targetBlock.args
  sourceNames : inputs.map PrivateInput.source = privateParameterNames source.block.parameters
  inputMatches : ∀ input ∈ inputs, input.followsScope source.config.scope
  bank : PrivateBank inputWidths
  sourceBank : bank.source = source.config
  words : bank.words = privateBindingWords inputs
  pointers : bank.pointers = privateBindingPointers inputs
  sourceSlots : List (Slot inputWidths)
  sourceRead : source.block.parameters.mapM (fun parameter => bank.source.scope.lookup parameter.id) =
    some sourceSlots
  sourceBinding : ∀ σ values,
    values.map IR.Value.type = bank.source.tokens →
    IR.bind source.block.parameters (sourceSlots.map (Slot.denote σ (privateToken values))) =
      .ok (bank.source.scope.denote σ (privateToken values))
  targetBinding : ∀ σ values incoming owner frame,
    bank.source.Realizes σ values incoming owner frame →
    ∃ regs,
      EvoIR.Machine.bind [] targetBlock.args (inputs.map (PrivateInput.value σ values)) = .ok regs ∧
      bank.words.Models σ regs ∧
      PrivatePointer.Models bank.pointers (bank.source.scope.denote σ (privateToken values)) regs

def privateCutpoint (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (proposal : PrivatePointProposal) : Option (PrivateCutpoint sf tf) := do
  let point ← privateLayoutPoint sf proposal.layout
  let block : {block // tf.blocks.find? (·.id == proposal.target) = some block} ←
    match found : tf.blocks.find? (·.id == proposal.target) with
    | none => none
    | some block => some ⟨block, found⟩
  let inputs ← privatePointInputs point.config point.block.parameters block.val.args
  if unique : (block.val.args.map EvoIR.Machine.VReg.id).eraseDups.length = block.val.args.length then
    let slots : {slots // point.block.parameters.mapM (fun parameter => point.config.scope.lookup parameter.id) = some slots} ←
      match found : point.block.parameters.mapM (fun parameter => point.config.scope.lookup parameter.id) with
      | none => none
      | some slots => some ⟨slots, found⟩
    if sourceBinding : bindSlots (privateTokenType point.config.tokens)
        point.block.parameters slots.val = some point.config.scope ∧
        inputs.val.map PrivateInput.source = privateParameterNames point.block.parameters ∧
        inputs.val.all (fun input => decide (input.followsScope point.config.scope)) = true then
      let bank : PrivateBank inputWidths :=
        ⟨point.config, privateBindingWords inputs.val, privateBindingPointers inputs.val⟩
      have targetId : block.val.id = proposal.target := by simpa using List.find?_some block.property
      return {
        source := point, targetBlock := block.val,
        targetFound := by simpa only [targetId] using block.property,
        inputs := inputs.val, targetNames := inputs.property.1,
        sourceNames := sourceBinding.2.1,
        inputMatches := by
          intro input member
          exact of_decide_eq_true (List.all_eq_true.mp sourceBinding.2.2 input member),
        bank, sourceBank := rfl, words := rfl, pointers := rfl,
        sourceSlots := slots.val, sourceRead := slots.property,
        sourceBinding := by
          intro σ values typed
          exact bind_slots_sound σ (privateToken values) (privateTokenType point.config.tokens)
            (private_token_typed values point.config.tokens typed)
            point.block.parameters slots.val point.config.scope sourceBinding.1,
        targetBinding := by
          intro σ values incoming owner frame models
          let regs := EvoIR.Machine.bindValues [] (privateBindingValues σ values inputs.val)
          have distinct : (inputs.val.map (fun input => input.target.id)).Nodup := by
            have nodup : (block.val.args.map EvoIR.Machine.VReg.id).Nodup :=
              EvoIR.Machine.Transport.nodup_of_eraseDups_length _
                (by simpa only [List.length_map] using unique)
            have named : inputs.val.map (fun input => input.target.id) =
                block.val.args.map EvoIR.Machine.VReg.id := by
              simpa only [List.map_map, Function.comp_def] using
                congrArg (List.map EvoIR.Machine.VReg.id) inputs.property.1
            exact Eq.mpr (congrArg List.Nodup named) nodup
          obtain ⟨words, pointers⟩ := private_binding_models σ values inputs.val
            (point.config.scope.denote σ (privateToken values)) distinct
            (inputs.property.2 σ values incoming owner frame models)
          refine ⟨regs, ?_, words, pointers⟩
          have arity : block.val.args.length = (inputs.val.map (PrivateInput.value σ values)).length := by
            simpa only [List.length_map] using (congrArg List.length inputs.property.1).symm
          have bound := EvoIR.Machine.bind_exact [] block.val.args
            (inputs.val.map (PrivateInput.value σ values)) arity unique
          rw [bound]
          apply congrArg Except.ok
          congr 1
          have canonical : (inputs.val.map PrivateInput.target).zip
              (inputs.val.map (PrivateInput.value σ values)) = privateBindingValues σ values inputs.val := by
            exact List.zip_map'
          exact (congrArg (fun params => params.zip (inputs.val.map (PrivateInput.value σ values)))
            inputs.property.1).symm.trans canonical }
    else none
  else none

def PrivateBoundary {sf : IR.Function} {tf : EvoIR.Machine.Function}
    (table : List (PrivateCutpoint sf tf)) (incoming : IR.Memory) (owner : Nat)
    (sourceStack : List IR.ReturnFrame) (targetStack : List EvoIR.Machine.ReturnFrame)
    (s : IR.State) (t : EvoIR.Machine.State) : Prop :=
  ∃ point ∈ table, ∃ σ values frame targetMemory regs logicalInputs,
    point.bank.Models σ values incoming owner frame targetMemory regs ∧
    s = privateState σ values sf point.source.block.name 0 owner frame point.bank.source sourceStack ∧
    t = privateMachineState tf point.targetBlock 0 regs owner frame logicalInputs targetStack targetMemory

end EvoIR.Core.Symbolic.MachineSelection
