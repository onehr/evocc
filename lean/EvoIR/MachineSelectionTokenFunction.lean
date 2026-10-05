import EvoIR.MachineSelectionTokenCFG
import EvoIR.MachineSelectionFunction

/-!
The function record keeps its open return boundary. A caller can instantiate
it at a real suspended frame without assuming that the continuation is sound.
The public entry wrapper closes that boundary only for the empty call stack.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenTable (sf : IR.Function) (tf : EvoIR.Machine.Function)
    (proposal : List (String × Nat)) : Option (List (TokenCutpoint sf tf)) :=
  proposal.mapM fun (label, id) => tokenCutpoint sf tf label id

structure CheckedTokenFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) where
  sourceFunction : IR.Function
  targetFunction : EvoIR.Machine.Function
  sourceFound : source.functions.find? (·.name == entry) = some sourceFunction
  targetFound : target.functions.find? (·.name == entry) = some targetFunction
  table : List (TokenCutpoint sourceFunction targetFunction)
  first : TokenCutpoint sourceFunction targetFunction
  member : first ∈ table
  body : CheckedTokenCFG source target sourceFunction targetFunction type table
  signature : sourceFunction.parameters = tokenInputTypes (cutpointWords first.widths)
  sourceEntry : ∀ σ memory owner,
    IR.enter sourceFunction (tokenInputValues σ (cutpointWords first.widths) memory) owner =
      .ok ⟨sourceFunction.name, first.sourceBlock.name, 0,
        first.scope.denote σ (tokenEnvironment memory), owner⟩
  targetEntry : ∀ σ owner,
    EvoIR.Machine.enter targetFunction (machineInputs σ (cutpointWords first.widths)) owner =
      .ok ⟨targetFunction.name, first.targetBlock.id, 0, bankDenote σ first.bank,
        owner, machineInputs σ (cutpointWords first.widths)⟩

def tokenFunctionUsing (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) (proposal : List (String × Nat))
    (buildBody : (sf : IR.Function) → (tf : EvoIR.Machine.Function) →
      (table : List (TokenCutpoint sf tf)) →
      Option (CheckedTokenCFG source target sf tf type table)) :
    Option (CheckedTokenFunction source target entry type) := do
  let sf : {f // source.functions.find? (·.name == entry) = some f} ←
    match found : source.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let tf : {f // target.functions.find? (·.name == entry) = some f} ←
    match found : target.functions.find? (·.name == entry) with
    | none => none
    | some f => some ⟨f, found⟩
  let table ← tokenTable sf.val tf.val proposal
  let body ← buildBody sf.val tf.val table
  let first : {point // table.find? (fun point =>
      decide (sf.val.blocks.head? = some point.sourceBlock) && point.targetBlock.id == 0) = some point} ←
    match found : table.find? (fun point =>
        decide (sf.val.blocks.head? = some point.sourceBlock) && point.targetBlock.id == 0) with
    | none => none
    | some point => some ⟨point, found⟩
  if signature : sf.val.parameters = tokenInputTypes (cutpointWords first.val.widths) then
    if blockTypes : first.val.sourceBlock.parameters.map IR.Binding.type = sf.val.parameters then
      if slots : first.val.slots = tokenInputSlots (cutpointWords first.val.widths) then
        have shape : sf.val.blocks.head? = some first.val.sourceBlock ∧ first.val.targetBlock.id = 0 := by
          simpa only [Bool.and_eq_true, decide_eq_true_eq, beq_iff_eq] using List.find?_some first.property
        return {
          sourceFunction := sf.val, targetFunction := tf.val
          sourceFound := sf.property, targetFound := tf.property
          table, first := first.val, member := List.mem_of_find?_eq_some first.property
          body, signature
          sourceEntry := by
            intro σ memory owner
            have binding := first.val.sourceBinding σ memory
            rw [slots, token_input_denote] at binding
            have types := (token_input_types σ (cutpointWords first.val.widths) memory).trans signature.symm
            simp [IR.enter, types, shape.1, blockTypes, binding,
              Bind.bind, Except.bind, Pure.pure, Except.pure]
          targetEntry := by
            intro σ owner
            have found : tf.val.blocks.find? (·.id == 0) = some first.val.targetBlock := by
              simpa only [shape.2] using first.val.targetFound
            simp [EvoIR.Machine.enter, found, first.val.targetBinding σ,
              Bind.bind, Except.bind, Pure.pure, Except.pure] }
      else none
    else none
  else none

def tokenFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) (proposal : List (String × Nat)) :
    Option (CheckedTokenFunction source target entry type) :=
  tokenFunctionUsing source target entry type proposal
    (fun sf tf table => tokenCFG source target sf tf type table)

theorem token_return_boundary_refines (source : IR.Program) (target : EvoIR.Machine.Program)
    (type : Source.IntType) (owner : Nat) (supported : type.width = 32 ∨ type.width = 64)
    (s : IR.State) (t : EvoIR.Machine.State)
    (boundary : TokenReturnBoundary type owner [] [] s t) :
    EvoIR.Machine.SelectionRefines source target type s t := by
  obtain ⟨σ, word, memory, nextFrame, typed, rfl, rfl⟩ := boundary
  apply Transition.refines_of_terminals (.refl _) (.refl _) rfl rfl
    (o := Observation.returned type (denoteTerm σ word.2).toNat)
  · simp [tokenSourceReturnState, cutResume, observeTarget, Word.denote, typed]
  · have bound : word.1 ≤ 64 := by omega
    have retained : (((denoteTerm σ word.2).setWidth 64).setWidth type.width).toNat =
        (denoteTerm σ word.2).toNat := by
      rw [← typed, BitVec.setWidth_setWidth_of_le _ bound]
      simp only [BitVec.setWidth_eq]
    have admitted : (type.width == 32 || type.width == 64) = true := by
      rcases supported with h | h <;> simp [h]
    simp only [machineReturnState, machineInputs, List.map_cons, List.map_nil,
      EvoIR.Machine.observe, admitted, ↓reduceIte]
    exact congrArg (fun bits => some (Observation.returned type bits)) retained

structure TokenSelectionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) : Type where
  checked : ∀ values : List Source.Value,
    values.map Source.Value.type = parameters.map (fun p => p.type.type) →
    ∃ sourceEntry targetEntry,
      IR.initial source entry (values.map sourceValueIR ++ [.memory ⟨1, []⟩, .world]) = .ok sourceEntry ∧
      EvoIR.Machine.initial target entry (values.map EvoIR.Machine.sourceArgument) = .ok targetEntry ∧
      EvoIR.Machine.SelectionRefines source target type sourceEntry targetEntry

def tokenFunctionPairUsing (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (function : CheckedTokenFunction source target entry type) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let args := (parameterWords parameters).map CWord.word
  if widths : function.first.widths = args.map Sigma.fst then
    if supported : type.width = 32 ∨ type.width = 64 then
      return ⟨by
        intro values typed
        let σ := inputSubst values
        let next := rebaseWords σ args
        let memory : IR.Memory := ⟨1, []⟩
        have bounded : ∀ word ∈ args, word.1 ≤ 64 := by
          intro word member
          apply function.first.bounded word.1
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
        have sourceEntered := function.sourceEntry next memory 1
        have canonicalInputs : (cutpointWords function.first.widths).map (Word.denote next) =
            values.map sourceValueIR := by
          dsimp only [next]
          rw [widths, cutpoint_words_rebase σ args bounded, sourceInputs]
        simp only [tokenInputValues, canonicalInputs] at sourceEntered
        have targetEntered := function.targetEntry next 1
        rw [widths, cutpoint_machine_rebase σ args bounded, targetInputs] at targetEntered
        let s := sourceState next (tokenEnvironment memory) function.sourceFunction
          function.first.sourceBlock 0 function.first.scope 1 2 []
        let t := machineState next function.targetFunction function.first.targetBlock 0
          function.first.bank 1 2 (values.map EvoIR.Machine.sourceArgument) [] memory
        refine ⟨s, t, ?_, ?_, ?_⟩
        · simp [IR.initial, function.sourceFound, sourceEntered, memory, s, sourceState,
            Bind.bind, Except.bind, Pure.pure, Except.pure]
        · simp [EvoIR.Machine.initial, function.targetFound, targetEntered, memory, t, machineState,
            Bind.bind, Except.bind, Pure.pure, Except.pure]
        · apply (function.body.openBody 1 [] [] type s t
            ⟨function.first, function.member, next, 2,
              values.map EvoIR.Machine.sourceArgument, memory, rfl, rfl⟩).finish
          exact token_return_boundary_refines source target type 1 supported⟩
    else none
  else none

def tokenFunctionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType)
    (proposal : List (String × Nat)) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← tokenFunction source target entry type proposal
  tokenFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
