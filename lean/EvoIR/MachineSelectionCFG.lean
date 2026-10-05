import EvoIR.MachineSelectionControl
import EvoIR.MachineSelectionGuard
import EvoIR.Core.LeafCorrespondence

/-!
The scalar CFG checker closes a finite table of productive fragments. Its
proposal search is finite, but its result covers unbounded execution,
including nonterminating loops. Every fragment starts in the actual paired
blocks and consumes their checked instructions, guards and terminators.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def scalarControl {Γ : Var → Nat} (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) (sb : IR.Block) (scope : Scope Γ)
    (tb : EvoIR.Machine.Block) (bank : Bank Γ) (checks : List (Guard Γ)) :
    Option (PLift (ScalarControlDiagram source target sf tf type table sb scope tb bank checks)) :=
  match scalarReturnUnder source target sf sb scope tf tb bank type checks with
  | some returned => some ⟨by
      intro σ owner nextFrame incoming memory passed
      exact Transition.OpenCheckpoint.of_refines
        (returned.down σ (fun _ => .world) owner nextFrame incoming memory passed)⟩
  | none => match unconditionalControl source target sf tf type table sb scope tb bank checks with
    | some branch => some branch
    | none => conditionalControl source target sf tf type table sb scope tb bank checks

def ScalarBlockDiagram (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) (point : ScalarCutpoint sf tf) : Prop :=
  ∀ σ owner nextFrame incoming memory,
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget type) (EvoIR.Machine.observe type) (ScalarBoundary table)
      (sourceState σ (fun _ => .world) sf point.sourceBlock 0 point.scope owner nextFrame [])
      (machineState σ tf point.targetBlock 0 point.bank owner nextFrame incoming [] memory)

def scalarBlock (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) (point : ScalarCutpoint sf tf) :
    Option (PLift (ScalarBlockDiagram source target sf tf type table point)) := do
  let sourceBody ← sourceSpan source sf point.sourceBlock point.sourceBlock.instructions.length 0 point.scope
  let targetBody ← guardChain target tf point.targetBlock 0 point.bank sourceBody.val.checks
  if complete : targetBody.val.pc = targetBody.val.block.insts.length then
    let finish ← scalarControl source target sf tf type table point.sourceBlock sourceBody.val.scope
      targetBody.val.block targetBody.val.bank sourceBody.val.checks
    return ⟨by
      intro σ owner nextFrame incoming memory
      obtain ⟨sourceLast, sourcePath, sourceOutcome⟩ := sourceBody.property σ (fun _ => .world) owner nextFrame []
      obtain ⟨targetLast, targetPath, targetOutcome⟩ := targetBody.property σ owner nextFrame incoming [] memory
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
        have sourceEnd : sourceLast = sourceState σ (fun _ => .world) sf point.sourceBlock
            point.sourceBlock.instructions.length sourceBody.val.scope owner nextFrame [] := by
          simpa only [checked, Nat.zero_add] using sourceOutcome
        have targetEnd : targetLast = machineState σ tf targetBody.val.block
            targetBody.val.block.insts.length targetBody.val.bank owner nextFrame incoming [] memory := by
          simpa only [checked, complete] using targetOutcome
        rw [sourceEnd] at sourcePath
        rw [targetEnd] at targetPath
        exact (finish.down σ owner nextFrame incoming memory checked).prepend
          sourcePath.toReaches targetPath.toReaches⟩
  else none

private def checkScalarBlocks (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) : (remaining : List (ScalarCutpoint sf tf)) →
    Option (PLift (∀ point ∈ remaining, ScalarBlockDiagram source target sf tf type table point))
  | [] => some ⟨by intro point member; cases member⟩
  | point :: rest => do
    let first ← scalarBlock source target sf tf type table point
    let tail ← checkScalarBlocks source target sf tf type table rest
    return ⟨by
      intro member included
      rcases List.mem_cons.mp included with same | later
      · subst member; exact first.down
      · exact tail.down member later⟩

structure CheckedScalarCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) : Type where
  checked : ∀ sourceState targetState, ScalarBoundary table sourceState targetState →
    EvoIR.Machine.SelectionRefines source target type sourceState targetState

def scalarCFG (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (ScalarCutpoint sf tf)) :
    Option (CheckedScalarCFG source target sf tf type table) := do
  let blocks ← checkScalarBlocks source target sf tf type table table
  return ⟨by
    intro sourceState targetState related
    apply Transition.OpenCheckpoint.close (entry := related)
    rintro _ _ ⟨point, member, σ, owner, nextFrame, incoming, memory, rfl, rfl⟩
    exact blocks.down point member σ owner nextFrame incoming memory⟩

end EvoIR.Core.Symbolic.MachineSelection
