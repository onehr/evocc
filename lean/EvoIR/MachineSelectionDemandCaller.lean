import EvoIR.MachineSelectionCallerDemand
import EvoIR.MachineSelectionCaller

/-! Executable caller checking with proved demand fallback. Ordinary calls
still check full equality first. Only a previously checked demand certificate
permits a positional literal-zero difference, with BOTH actual auxiliary
callee progress premises proved by the all-argument function certificate. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenDemandCallerSuffix (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    (constructionDepth : Nat) → (checks : List (Guard inputWidths)) →
    (types : List IR.Ty) → (memoryKey worldKey : Nat) →
    (sb : IR.Block) → (spc : Nat) → (scope : Scope inputWidths) →
    (tb : EvoIR.Machine.Block) → (tpc : Nat) → (bank : Bank inputWidths) →
    Option (PLift (TokenSuffixDiagram source target sf tf type table sb spc scope tb tpc bank
      types memoryKey worldKey checks))
  | 0, _, _, _, _, _, _, _, _, _, _ => none
  | depth+1, checks, types, memoryKey, worldKey, sb, spc, scope, tb, tpc, bank => do
    let count := sourcePurePrefix (sb.instructions.drop spc)
    let sourceBody ← sourceSpan source sf sb count spc scope
    let targetBody ← guardChainUnder target tf tb tpc bank checks sourceBody.val.checks
    let allChecks := checks ++ sourceBody.val.checks
    let finish : PLift (TokenSuffixDiagram source target sf tf type table
        sb (spc+count) sourceBody.val.scope targetBody.val.block targetBody.val.pc targetBody.val.bank
        types memoryKey worldKey allChecks) ←
      if sourceEnd : spc+count = sb.instructions.length then
        if targetEnd : targetBody.val.pc = targetBody.val.block.insts.length then do
          let final ← tokenControl source target sf tf type table sb sourceBody.val.scope
            targetBody.val.block targetBody.val.bank allChecks memoryKey worldKey
          pure ⟨by
            simpa only [sourceEnd, targetEnd] using token_suffix_control source target sf tf type table
              sb sourceBody.val.scope targetBody.val.block targetBody.val.bank types memoryKey worldKey allChecks final.down⟩
        else none
      else do
        let inst ← sb.instructions[spc+count]?
        let .call name := inst.op | none
        let callee ← callees.find? (·.name == name)
        let full := do
          let call ← tokenCallEntry source target sf sb (spc+count) sourceBody.val.scope
            tf targetBody.val.block targetBody.val.pc targetBody.val.bank
            callee.checked.sourceFunction callee.checked.targetFunction allChecks memoryKey worldKey
          let calleeBody ← tokenCalleeArguments callee.checked call.val.2.2.arguments
          let binding ← resumeBindings types sourceBody.val.scope targetBody.val.bank
            call.val.1.results call.val.2.1 callee.type
          let suffix ← tokenDemandCallerSuffix source target sf tf type table callees plan demand depth
            (guardsForReturn allChecks) (types ++ [IR.Ty.memory, IR.Ty.world]) types.length (types.length+1)
            sb (spc+count+1) binding.scope targetBody.val.block (targetBody.val.pc+1) binding.bank
          pure (⟨token_suffix_call source target sf tf type table sb (spc+count) sourceBody.val.scope
            targetBody.val.block targetBody.val.pc targetBody.val.bank types memoryKey worldKey allChecks
            callee call.val.1 call.val.2.1 call.val.2.2 call.property calleeBody.down binding suffix.down⟩ :
            PLift (TokenSuffixDiagram source target sf tf type table sb (spc+count) sourceBody.val.scope
              targetBody.val.block targetBody.val.pc targetBody.val.bank types memoryKey worldKey allChecks))
        match full with
        | some matched => pure matched
        | none => do
          let call ← tokenDemandCallEntry source target sf sb (spc+count) sourceBody.val.scope
            tf targetBody.val.block targetBody.val.pc targetBody.val.bank
            callee.checked.sourceFunction callee.checked.targetFunction plan allChecks memoryKey worldKey
          let originalBody ← tokenCalleeArguments callee.checked call.val.2.2.original.arguments
          let substitutedBody ← tokenCalleeArguments callee.checked call.val.2.2.substituted.arguments
          let binding ← resumeBindings types sourceBody.val.scope targetBody.val.bank
            call.val.1.results call.val.2.1 callee.type
          let calleeBody := token_demand_callee callee plan demand call.val.2.2 call.val.1 call.val.2.1 types
            sf sb (spc+count) sourceBody.val.scope tf targetBody.val.block targetBody.val.pc targetBody.val.bank
            binding originalBody.down substitutedBody.down
          let suffix ← tokenDemandCallerSuffix source target sf tf type table callees plan demand depth
            (guardsForReturn allChecks) (types ++ [IR.Ty.memory, IR.Ty.world]) types.length (types.length+1)
            sb (spc+count+1) binding.scope targetBody.val.block (targetBody.val.pc+1) binding.bank
          pure ⟨token_suffix_demand_call source target sf tf type table sb (spc+count) sourceBody.val.scope
            targetBody.val.block targetBody.val.pc targetBody.val.bank types memoryKey worldKey allChecks
            callee plan call.val.1 call.val.2.1 call.val.2.2 call.property calleeBody binding suffix.down⟩
    return ⟨token_suffix_prefix source target sf tf type table sb spc count scope tb tpc bank types
      memoryKey worldKey checks sourceBody.val targetBody.val sourceBody.property targetBody.property finish.down⟩

def tokenDemandCallerBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan)
    (point : TokenCutpoint sf tf) :
    Option (PLift (TokenBlockDiagram source target sf tf type table point)) := do
  let body ← tokenDemandCallerSuffix source target sf tf type table callees plan demand
    (point.sourceBlock.instructions.length+1) [] [.memory, .world] 0 1
    point.sourceBlock 0 point.scope point.targetBlock 0 point.bank
  return ⟨by
    intro σ memory owner nextFrame incoming sourceStack targetStack observationType
    exact body.down σ [.memory memory, .world] rfl memory owner nextFrame incoming sourceStack targetStack
      observationType rfl rfl rfl⟩

private def checkDemandCallerBlocks (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    (remaining : List (TokenCutpoint sf tf)) →
    Option (PLift (∀ point ∈ remaining, TokenBlockDiagram source target sf tf type table point))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let first ← tokenDemandCallerBlock source target sf tf type table callees plan demand point
    let tail ← checkDemandCallerBlocks source target sf tf type table callees plan demand rest
    return ⟨by
      intro member included
      rcases List.mem_cons.mp included with same | later
      · subst member; exact first.down
      · exact tail.down member later⟩

def tokenDemandCallerCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Option (CheckedTokenCFG source target sf tf type table) := do
  let blocks ← checkDemandCallerBlocks source target sf tf type table callees plan demand table
  return ⟨by
    intro owner sourceStack targetStack observationType sourceState targetState related
    apply Transition.OpenCheckpoint.closeWithin (entry := related)
    rintro _ _ ⟨point, member, σ, nextFrame, incoming, memory, rfl, rfl⟩
    exact blocks.down point member σ memory owner nextFrame incoming sourceStack targetStack observationType⟩

def tokenDemandCallerFunctionChecked (source : IR.Program) (target : EvoIR.Machine.Program)
    (name : String) (type : Source.IntType) (proposal : List (String × Nat))
    (callees : List (TokenCallee source target)) (plan : EvoIR.Machine.Demand.Plan)
    (demand : EvoIR.Machine.Demand.Checked target plan) :
    Option (CheckedTokenFunction source target name type) :=
  tokenFunctionUsing source target name type proposal fun sf tf table =>
    tokenDemandCallerCFG source target sf tf type table callees plan demand

def tokenDemandCallerFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (name : String) (type : Source.IntType) (proposal : List (String × Nat))
    (callees : List (TokenCallee source target)) (plan : EvoIR.Machine.Demand.Plan) :
    Option (CheckedTokenFunction source target name type) := do
  let demand ← EvoIR.Machine.Demand.checkDemand target plan
  tokenDemandCallerFunctionChecked source target name type proposal callees plan demand

end EvoIR.Core.Symbolic.MachineSelection
