import EvoIR.MachineSelectionCallerCall

/-! Concrete caller-block checking. Construction recurses over the finite
number of call sites in one actual source block, never over callee execution
or loop iterations. Each callee is a previously checked context-parametric
function; the complete logical argument vector is retained at every BL. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenCallerSuffix (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target)) :
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
        let call ← tokenCallEntry source target sf sb (spc+count) sourceBody.val.scope
          tf targetBody.val.block targetBody.val.pc targetBody.val.bank
          callee.checked.sourceFunction callee.checked.targetFunction allChecks memoryKey worldKey
        let calleeBody ← tokenCalleeArguments callee.checked call.val.2.2.arguments
        let binding ← resumeBindings types sourceBody.val.scope targetBody.val.bank
          call.val.1.results call.val.2.1 callee.type
        let suffix ← tokenCallerSuffix source target sf tf type table callees depth
          (guardsForReturn allChecks) (types ++ [.memory, .world]) types.length (types.length+1)
          sb (spc+count+1) binding.scope targetBody.val.block (targetBody.val.pc+1) binding.bank
        pure ⟨token_suffix_call source target sf tf type table sb (spc+count) sourceBody.val.scope
          targetBody.val.block targetBody.val.pc targetBody.val.bank types memoryKey worldKey allChecks
          callee call.val.1 call.val.2.1 call.val.2.2 call.property calleeBody.down binding suffix.down⟩
    return ⟨token_suffix_prefix source target sf tf type table sb spc count scope tb tpc bank types
      memoryKey worldKey checks sourceBody.val targetBody.val sourceBody.property targetBody.property finish.down⟩

def tokenCallerBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target))
    (point : TokenCutpoint sf tf) :
    Option (PLift (TokenBlockDiagram source target sf tf type table point)) := do
  let body ← tokenCallerSuffix source target sf tf type table callees
    (point.sourceBlock.instructions.length+1) [] [.memory, .world] 0 1
    point.sourceBlock 0 point.scope point.targetBlock 0 point.bank
  return ⟨by
    intro σ memory owner nextFrame incoming sourceStack targetStack observationType
    exact body.down σ [.memory memory, .world] rfl memory owner nextFrame incoming sourceStack targetStack
      observationType rfl rfl rfl⟩

private def checkCallerBlocks (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target)) :
    (remaining : List (TokenCutpoint sf tf)) →
    Option (PLift (∀ point ∈ remaining, TokenBlockDiagram source target sf tf type table point))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let first ← tokenCallerBlock source target sf tf type table callees point
    let tail ← checkCallerBlocks source target sf tf type table callees rest
    return ⟨by
      intro member included
      rcases List.mem_cons.mp included with same | later
      · subst member; exact first.down
      · exact tail.down member later⟩

def tokenCallerCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (callees : List (TokenCallee source target)) :
    Option (CheckedTokenCFG source target sf tf type table) := do
  let blocks ← checkCallerBlocks source target sf tf type table callees table
  return ⟨by
    intro owner sourceStack targetStack observationType sourceState targetState related
    apply Transition.OpenCheckpoint.closeWithin (entry := related)
    rintro _ _ ⟨point, member, σ, nextFrame, incoming, memory, rfl, rfl⟩
    exact blocks.down point member σ memory owner nextFrame incoming sourceStack targetStack observationType⟩

def tokenCallerFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (name : String) (type : Source.IntType) (proposal : List (String × Nat))
    (callees : List (TokenCallee source target)) :
    Option (CheckedTokenFunction source target name type) :=
  tokenFunctionUsing source target name type proposal fun sf tf table =>
    tokenCallerCFG source target sf tf type table callees

end EvoIR.Core.Symbolic.MachineSelection
