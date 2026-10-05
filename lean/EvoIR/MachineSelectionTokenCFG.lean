import EvoIR.MachineSelectionTokenControl
import EvoIR.MachineSelectionGuard
import EvoIR.Core.LeafCorrespondence

/-! Productive token-preserving function bodies close their own backedges
while leaving actual suspended-caller returns open. These first fragments
cover pure integer instructions and checked traps; calls require the separate
callee/continuation construction and are refused by this builder. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def TokenBlockDiagram (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (point : TokenCutpoint sf tf) : Prop :=
  ∀ σ memory owner nextFrame incoming sourceStack targetStack observationType,
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (fun s t => TokenBoundary table owner sourceStack targetStack s t ∨
        TokenReturnBoundary type owner sourceStack targetStack s t)
      (sourceState σ (tokenEnvironment memory) sf point.sourceBlock 0 point.scope owner nextFrame sourceStack)
      (machineState σ tf point.targetBlock 0 point.bank owner nextFrame incoming targetStack memory)

def tokenBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (point : TokenCutpoint sf tf) :
    Option (PLift (TokenBlockDiagram source target sf tf type table point)) := do
  let sourceBody ← sourceSpan source sf point.sourceBlock point.sourceBlock.instructions.length 0 point.scope
  let targetBody ← guardChain target tf point.targetBlock 0 point.bank sourceBody.val.checks
  if complete : targetBody.val.pc = targetBody.val.block.insts.length then
    let finish ← tokenControl source target sf tf type table point.sourceBlock sourceBody.val.scope
      targetBody.val.block targetBody.val.bank sourceBody.val.checks
    return ⟨by
      intro σ memory owner nextFrame incoming sourceStack targetStack observationType
      obtain ⟨sourceLast, sourcePath, sourceOutcome⟩ := sourceBody.property σ
        (tokenEnvironment memory) owner nextFrame sourceStack
      obtain ⟨targetLast, targetPath, targetOutcome⟩ := targetBody.property σ
        owner nextFrame incoming targetStack memory
      cases checked : guards σ sourceBody.val.checks with
      | error fault =>
        have sourceHalt : sourceLast.control = .halted (.error fault) := by
          simpa only [checked] using sourceOutcome
        have targetHalt : targetLast.control = .halted (.error fault) := by
          simpa only [checked] using targetOutcome
        obtain ⟨code, faultCode, observed⟩ := guards_fault_observation σ sourceBody.val.checks fault checked
        subst fault
        apply Transition.OpenCheckpoint.of_refines
        apply Transition.refines_of_terminals sourcePath.toReaches targetPath.toReaches
          (by simp [IR.step, sourceHalt]) (by simp [EvoIR.Machine.step, targetHalt])
          (o := Observation.diagnostic code)
        · simpa only [observeTarget, sourceHalt] using observed
        · simpa only [EvoIR.Machine.observe, targetHalt] using observed
      | ok value =>
        cases value
        have sourceEnd : sourceLast = sourceState σ (tokenEnvironment memory) sf point.sourceBlock
            point.sourceBlock.instructions.length sourceBody.val.scope owner nextFrame sourceStack := by
          simpa only [checked, Nat.zero_add] using sourceOutcome
        have targetEnd : targetLast = machineState σ tf targetBody.val.block
            targetBody.val.block.insts.length targetBody.val.bank owner nextFrame incoming targetStack memory := by
          simpa only [checked, complete] using targetOutcome
        rw [sourceEnd] at sourcePath
        rw [targetEnd] at targetPath
        exact (finish.down σ (tokenEnvironment memory) memory owner nextFrame incoming sourceStack targetStack
          observationType checked rfl rfl).prepend sourcePath.toReaches targetPath.toReaches⟩
  else none

private def checkTokenBlocks (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) : (remaining : List (TokenCutpoint sf tf)) →
    Option (PLift (∀ point ∈ remaining, TokenBlockDiagram source target sf tf type table point))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let first ← tokenBlock source target sf tf type table point
    let tail ← checkTokenBlocks source target sf tf type table rest
    return ⟨by
      intro member included
      rcases List.mem_cons.mp included with same | later
      · subst member; exact first.down
      · exact tail.down member later⟩

structure CheckedTokenCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) : Type where
  openBody : ∀ owner sourceStack targetStack observationType sourceState targetState,
    TokenBoundary table owner sourceStack targetStack sourceState targetState →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (TokenReturnBoundary type owner sourceStack targetStack) sourceState targetState

def tokenCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) :
    Option (CheckedTokenCFG source target sf tf type table) := do
  let blocks ← checkTokenBlocks source target sf tf type table table
  return ⟨by
    intro owner sourceStack targetStack observationType sourceState targetState related
    apply Transition.OpenCheckpoint.closeWithin (entry := related)
    rintro _ _ ⟨point, member, σ, nextFrame, incoming, memory, rfl, rfl⟩
    exact blocks.down point member σ memory owner nextFrame incoming sourceStack targetStack observationType⟩

end EvoIR.Core.Symbolic.MachineSelection
