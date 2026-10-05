import EvoIR.MachinePrivateReturn
import EvoIR.MachineSelectionTokenFunction

/-!
Context-parametric private function records bind both actual entries and keep
the function's actual return open. The incoming source and machine memories
may differ in retired objects. Only the public empty-stack wrapper closes
the return boundary and forgets private storage in the scalar observation.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def PrivateEntryBodyDiagram (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank inputWidths) (type : Source.IntType) : Prop :=
  ∀ σ sourceMemory targetMemory owner nextFrame logicalInputs sourceStack targetStack observationType,
    sourceMemory.BeforeFrame owner → owner < nextFrame →
    EvoIR.Machine.PrivateMemory.LiveRelated sourceMemory targetMemory →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (PrivateReturnBoundary type sourceMemory owner sourceStack targetStack)
      (privateState σ [.memory sourceMemory, .world] sf sb.name 0 owner nextFrame bank.source sourceStack)
      (privateMachineState tf tb 0 (bankDenote σ bank.words) owner nextFrame
        logicalInputs targetStack targetMemory)

structure CheckedPrivateFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) where
  sourceFunction : IR.Function
  sourceBlock : IR.Block
  targetFunction : EvoIR.Machine.Function
  targetBlock : EvoIR.Machine.Block
  sourceFound : source.functions.find? (·.name == entry) = some sourceFunction
  targetFound : target.functions.find? (·.name == entry) = some targetFunction
  widths : List Nat
  bounded : ∀ width ∈ widths, width ≤ 64
  bank : PrivateBank inputWidths
  sourceEntry : ∀ σ memory owner,
    IR.enter sourceFunction (tokenInputValues σ (cutpointWords widths) memory) owner =
      .ok ⟨sourceFunction.name, sourceBlock.name, 0,
        bank.source.scope.denote σ (tokenEnvironment memory), owner⟩
  targetEntry : ∀ σ owner,
    EvoIR.Machine.enter targetFunction (machineInputs σ (cutpointWords widths)) owner =
      .ok ⟨targetFunction.name, targetBlock.id, 0, bankDenote σ bank.words,
        owner, machineInputs σ (cutpointWords widths)⟩
  body : PrivateEntryBodyDiagram source target sourceFunction sourceBlock
    targetFunction targetBlock bank type

def privateFunctionUsing (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType)
    (buildBody : (sf : IR.Function) → (sb : IR.Block) →
      (tf : EvoIR.Machine.Function) → (tb : EvoIR.Machine.Block) →
      (bank : PrivateBank inputWidths) →
      Option (PLift (PrivateEntryBodyDiagram source target sf sb tf tb bank type))) :
    Option (CheckedPrivateFunction source target entry type) := do
  let sf : {f // source.functions.find? (·.name == entry) = some f} ←
    match found : source.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let tf : {f // target.functions.find? (·.name == entry) = some f} ←
    match found : target.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let widths ← scalarTokenWidths sf.val.parameters
  if bounded : ∀ width ∈ widths, width ≤ 64 then
    let sourceEntry ← sourceTokenEntry sf.val (cutpointWords widths)
    let targetEntry ← machineEntry tf.val (cutpointWords widths)
    if inBank : sourceEntry.val.2.inBank 2 = true then
      let bank : PrivateBank inputWidths :=
        ⟨privateEntryConfig sourceEntry.val.2, targetEntry.val.2, []⟩
      let body ← buildBody sf.val sourceEntry.val.1 tf.val targetEntry.val.1 bank
      return {
        sourceFunction := sf.val, sourceBlock := sourceEntry.val.1,
        targetFunction := tf.val, targetBlock := targetEntry.val.1,
        sourceFound := sf.property, targetFound := tf.property,
        widths, bounded, bank,
        sourceEntry := sourceEntry.property,
        targetEntry := targetEntry.property,
        body := body.down }
    else none
  else none

def privateFunctionPairUsing (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (function : CheckedPrivateFunction source target entry type) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let args := (parameterWords parameters).map CWord.word
  if widths : function.widths = args.map Sigma.fst then
    if supported : type.width = 32 ∨ type.width = 64 then
      return ⟨by
        intro values typed
        let σ := inputSubst values
        let next := rebaseWords σ args
        let memory : IR.Memory := ⟨1, []⟩
        have bounded : ∀ word ∈ args, word.1 ≤ 64 := by
          intro word member
          apply function.bounded word.1
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
        have canonicalSource := function.sourceEntry next memory 1
        have sourceInputsRebased : (cutpointWords function.widths).map (Word.denote next) =
            values.map sourceValueIR := by
          dsimp only [next]
          rw [widths, cutpoint_words_rebase σ args bounded, sourceInputs]
        simp only [tokenInputValues, sourceInputsRebased] at canonicalSource
        have canonicalTarget := function.targetEntry next 1
        rw [widths, cutpoint_machine_rebase σ args bounded, targetInputs] at canonicalTarget
        let s := privateState next [.memory memory, .world] function.sourceFunction
          function.sourceBlock.name 0 1 2 function.bank.source []
        let t := privateMachineState function.targetFunction function.targetBlock 0
          (bankDenote next function.bank.words) 1 2 (values.map EvoIR.Machine.sourceArgument) [] memory
        refine ⟨s, t, ?_, ?_, ?_⟩
        · simp [IR.initial, function.sourceFound, canonicalSource, s, privateState,
            tokenEnvironment, memory, Bind.bind, Except.bind, Pure.pure, Except.pure]
        · simp [EvoIR.Machine.initial, function.targetFound, canonicalTarget, t, privateMachineState,
            memory, Bind.bind, Except.bind, Pure.pure, Except.pure]
        · have safe : memory.WellAllocated := by simp [memory, IR.Memory.WellAllocated]
          have fresh : memory.BeforeFrame 1 := by simp [memory, IR.Memory.BeforeFrame]
          have related := EvoIR.Machine.PrivateMemory.LiveRelated.refl memory safe
          exact (function.body next memory memory 1 2 (values.map EvoIR.Machine.sourceArgument)
            [] [] type fresh (by decide) related).finish
              (private_return_boundary_refines source target type memory 1 supported)⟩
    else none
  else none

structure PrivateCallee (source : IR.Program) (target : EvoIR.Machine.Program) where
  name : String
  type : Source.IntType
  checked : CheckedPrivateFunction source target name type

def PrivateCalleeArgumentsDiagram (function : CheckedPrivateFunction source target name type)
    (args : List (Word inputWidths)) : Prop :=
  ∀ σ sourceMemory targetMemory owner nextFrame sourceStack targetStack observationType
      (sourceActivation : IR.Activation) (targetActivation : EvoIR.Machine.Activation),
    sourceMemory.BeforeFrame owner → owner < nextFrame →
    EvoIR.Machine.PrivateMemory.LiveRelated sourceMemory targetMemory →
    IR.enter function.sourceFunction (tokenInputValues σ args sourceMemory) owner = .ok sourceActivation →
    EvoIR.Machine.enter function.targetFunction (machineInputs σ args) owner = .ok targetActivation →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (PrivateReturnBoundary type sourceMemory owner sourceStack targetStack)
      ⟨.running sourceActivation, sourceStack, nextFrame⟩
      ⟨.running targetActivation, targetStack, nextFrame, targetMemory⟩

theorem private_callee_arguments (function : CheckedPrivateFunction source target name type)
    (args : List (Word inputWidths)) (widths : function.widths = args.map Sigma.fst) :
    PrivateCalleeArgumentsDiagram function args := by
  intro σ sourceMemory targetMemory owner nextFrame sourceStack targetStack observationType
    sourceActivation targetActivation fresh future related sourceEntered targetEntered
  let next := rebaseWords σ args
  have bounded : ∀ word ∈ args, word.1 ≤ 64 := by
    intro word member
    apply function.bounded word.1
    rw [widths]
    exact List.mem_map.mpr ⟨word, member, rfl⟩
  have canonicalSource := function.sourceEntry next sourceMemory owner
  have sourceInputs : (cutpointWords function.widths).map (Word.denote next) =
      args.map (Word.denote σ) := by
    dsimp only [next]
    rw [widths, cutpoint_words_rebase σ args bounded]
  simp only [tokenInputValues, sourceInputs] at canonicalSource
  have sourceActual := Except.ok.inj (sourceEntered.symm.trans canonicalSource)
  have canonicalTarget := function.targetEntry next owner
  have targetInputs : machineInputs next (cutpointWords function.widths) = machineInputs σ args := by
    dsimp only [next]
    rw [widths, cutpoint_machine_rebase σ args bounded]
  rw [targetInputs] at canonicalTarget
  have targetActual := Except.ok.inj (targetEntered.symm.trans canonicalTarget)
  rw [sourceActual, targetActual]
  exact function.body next sourceMemory targetMemory owner nextFrame (machineInputs σ args)
    sourceStack targetStack observationType fresh future related

def privateCalleeArguments (function : CheckedPrivateFunction source target name type)
    (args : List (Word inputWidths)) : Option (PLift (PrivateCalleeArgumentsDiagram function args)) :=
  if widths : function.widths = args.map Sigma.fst then
    some ⟨private_callee_arguments function args widths⟩
  else none

end EvoIR.Core.Symbolic.MachineSelection
