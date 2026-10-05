import EvoIR.MachinePrivateBodySpan
import EvoIR.MachinePrivateFunction

/-!
Close actual straight-line private bodies with their real return boundary.
The checker must consume both complete instruction streams and prove entry
storage for arbitrary caller snapshots. No all-input guard premise is exposed
to the caller: this route accepts only a computed empty check list. Branches,
calls, and nonempty guard traces require their separate checked continuations.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateInitialBank {Γ : Var → Nat} (bank : PrivateBank Γ) :
    Option (PLift (∀ σ sourceMemory targetMemory owner nextFrame,
      sourceMemory.BeforeFrame owner → owner < nextFrame →
      EvoIR.Machine.PrivateMemory.LiveRelated sourceMemory targetMemory →
      bank.Models σ [.memory sourceMemory, .world] sourceMemory owner nextFrame
        targetMemory (bankDenote σ bank.words))) := do
  if sourceShape : bank.source = privateEntryConfig bank.source.scope then
    if emptyPointers : bank.pointers = [] then
      if bounded : bank.source.scope.inBank 2 = true then
        return ⟨by
          intro σ sourceMemory targetMemory owner nextFrame fresh future related
          have realized := private_entry_realizes σ bank.source.scope sourceMemory owner nextFrame
            bounded related.sourceAllocated fresh future
          refine ⟨?_, bank_models_self σ bank.words, ?_, sourceMemory, ?_, related⟩
          · rw [sourceShape]
            exact realized
          · intro pointer member
            rw [emptyPointers] at member
            cases member
          · rw [sourceShape]
            rfl⟩
      else none
    else none
  else none

def privateFiniteBody (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (sb : IR.Block) (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block)
    (bank : PrivateBank inputWidths) (type : Source.IntType) :
    Option (PLift (PrivateEntryBodyDiagram source target sf sb tf tb bank type)) := do
  let entry ← privateInitialBank bank
  let body ← privateBodySpan source target sf sb 0 tf tb 0 bank
  if noChecks : body.val.checks = [] then
    if sourceEnd : body.val.sourceCount = sb.instructions.length then
      if targetEnd : body.val.targetCount = tb.insts.length then
        let finish ← privateReturn source target sf sb tf tb body.val.next type
        return ⟨by
          intro σ sourceMemory targetMemory owner nextFrame logicalInputs sourceStack targetStack
            observationType fresh future related
          have initialized := entry.down σ sourceMemory targetMemory owner nextFrame fresh future related
          obtain ⟨values, memory, regs, sourcePath, targetPath, models⟩ :=
            body.property σ [.memory sourceMemory, .world] sourceMemory owner nextFrame targetMemory
              (bankDenote σ bank.words) logicalInputs sourceStack targetStack initialized
              (by simp only [noChecks, guards])
          have returned := finish.down σ values sourceMemory owner nextFrame memory regs logicalInputs
            sourceStack targetStack observationType models (by rfl)
          have prefixSource := sourcePath.reaches
          have prefixTarget := targetPath.reaches
          simp only [Nat.zero_add, sourceEnd] at prefixSource
          simp only [Nat.zero_add, targetEnd] at prefixTarget
          exact returned.prepend prefixSource prefixTarget⟩
      else none
    else none
  else none

def privateFiniteFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) :
    Option (CheckedPrivateFunction source target entry type) :=
  privateFunctionUsing source target entry type
    (fun sf sb tf tb bank => privateFiniteBody source target sf sb tf tb bank type)

def privateFiniteFunctionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← privateFiniteFunction source target entry type
  privateFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
