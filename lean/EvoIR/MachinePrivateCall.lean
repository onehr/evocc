import EvoIR.MachinePrivateCallResume
import EvoIR.MachinePrivateDemandCallee

/-! A private call leaf composes actual call dispatch, a previously checked
contextual callee, and a separately checked caller suffix. Demand substitution
changes only the real machine entry and is proved by the demand checker. The
saved source and target banks and their independent memories remain actual
snapshots throughout call and return. -/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem private_return_suffix (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type returnType : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (sb : IR.Block) (spc : Nat)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (before : PrivateBank inputWidths)
    (prior : List (Guard inputWidths)) (sourceResults : List IR.Binding)
    (targetResults : List EvoIR.Machine.VReg)
    (binding : PrivateResumeBindings before sourceResults targetResults returnType)
    (suffix : PrivateSuffixDiagram source target sf tf type table
      sb spc tb tpc binding.next (guardsForReturn prior))
    (σ : Subst inputWidths) (values : List IR.Value) (incoming : IR.Memory)
    (owner calleeOwner : Nat) (targetBefore : IR.Memory) (regs : EvoIR.Machine.RegisterFile)
    (logicalInputs : List EvoIR.Machine.Value) (sourceStack : List IR.ReturnFrame)
    (targetStack : List EvoIR.Machine.ReturnFrame) (observationType : Source.IntType)
    (sourceBefore : IR.Memory)
    (models : before.Models σ values incoming owner calleeOwner targetBefore regs)
    (current : before.source.memoryValue values = some sourceBefore)
    (passed : guards σ prior = .ok ()) (s : IR.State) (t : EvoIR.Machine.State)
    (boundary : PrivateReturnBoundary returnType sourceBefore calleeOwner
      (⟨⟨sf.name, sb.name, spc, before.source.scope.denote σ (privateToken values), owner⟩,
        sourceResults⟩ :: sourceStack)
      (⟨⟨tf.name, tb.id, tpc, regs, owner, logicalInputs⟩, targetResults⟩ :: targetStack) s t) :
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (fun s t => PrivateBoundary table incoming owner sourceStack targetStack s t ∨
        PrivateReturnBoundary type incoming owner sourceStack targetStack s t) s t := by
  obtain ⟨value, sourceMemory, targetMemory, afterFrame, typed, related,
    closed, safe, frame, future, newOwners, rfl, rfl⟩ := boundary
  obtain ⟨nextRegs, sourceBound, targetBound, nextModels⟩ := binding.sound
    σ values incoming owner calleeOwner targetBefore regs models sourceBefore current
    value typed (sourceMemory.endFrame calleeOwner) (targetMemory.endFrame calleeOwner) afterFrame
    closed safe frame future newOwners (related.endFrame calleeOwner)
  have fullSourceBound : IR.appendResults
      (before.source.scope.denote σ (privateToken values)) sourceResults
      [sourceValueIR value, .memory (sourceMemory.endFrame calleeOwner), .world] =
      .ok (binding.next.source.scope.denote (returnSubst σ value)
        (privateToken (values ++ privateCallValues true value (sourceMemory.endFrame calleeOwner)))) := by
    simpa only [privateCallValues, ↓reduceIte, List.cons_append, List.nil_append] using sourceBound
  have sourceAt : cutResume [sourceValueIR value, .memory (sourceMemory.endFrame calleeOwner), .world]
      afterFrame
      (⟨⟨sf.name, sb.name, spc, before.source.scope.denote σ (privateToken values), owner⟩,
        sourceResults⟩ :: sourceStack) =
      privateState (returnSubst σ value)
        (values ++ privateCallValues true value (sourceMemory.endFrame calleeOwner))
        sf sb.name spc owner afterFrame binding.next.source sourceStack := by
    simp only [cutResume, fullSourceBound, privateState]
  have targetAt : machineReturnState [EvoIR.Machine.sourceArgument value]
      targetMemory calleeOwner afterFrame
      (⟨⟨tf.name, tb.id, tpc, regs, owner, logicalInputs⟩, targetResults⟩ :: targetStack) =
      privateMachineState tf tb tpc nextRegs owner afterFrame logicalInputs targetStack
        (targetMemory.endFrame calleeOwner) := by
    simp only [machineReturnState, targetBound, privateMachineState]
  rw [sourceAt, targetAt]
  exact suffix (returnSubst σ value)
    (values ++ privateCallValues true value (sourceMemory.endFrame calleeOwner))
    incoming owner afterFrame (targetMemory.endFrame calleeOwner) nextRegs logicalInputs
    sourceStack targetStack observationType nextModels
    (by simpa only [guards_for_return] using passed)

theorem private_suffix_call (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (sb : IR.Block) (spc : Nat)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (before : PrivateBank inputWidths)
    (prior : List (Guard inputWidths)) (callee : PrivateCallee source target)
    (plan : EvoIR.Machine.Demand.Plan) (inst : IR.Instruction)
    (results : List EvoIR.Machine.VReg)
    (entry : TokenDemandCallEntryResult inputWidths
      callee.checked.sourceFunction callee.checked.targetFunction plan)
    (called : PrivateDemandCallEntryDiagram source target sf sb spc before inst tf tb tpc
      results callee.checked.sourceFunction callee.checked.targetFunction plan entry prior)
    (opened : PrivateDemandCalleeDiagram callee plan entry inst results sf sb spc before tf tb tpc)
    (binding : PrivateResumeBindings before inst.results results callee.type)
    (suffix : PrivateSuffixDiagram source target sf tf type table
      sb (spc+1) tb (tpc+1) binding.next (guardsForReturn prior)) :
    PrivateSuffixDiagram source target sf tf type table sb spc tb tpc before prior := by
  intro σ values incoming owner frame memory regs logicalInputs sourceStack targetStack
    observationType models passed
  obtain ⟨sourceMemory, current, related⟩ := models.memory
  have steps := called σ values incoming owner frame memory regs logicalInputs
    sourceStack targetStack sourceMemory models passed current
  have calleeOpened := opened σ values incoming owner frame memory regs logicalInputs
    sourceStack targetStack observationType sourceMemory models current
  have continued := calleeOpened.bind
    (nextBoundary := fun s t => PrivateBoundary table incoming owner sourceStack targetStack s t ∨
      PrivateReturnBoundary type incoming owner sourceStack targetStack s t) (by
    intro s t boundary
    right
    exact private_return_suffix source target sf tf type callee.type table sb (spc+1)
      tb (tpc+1) before prior inst.results results binding suffix σ values incoming owner frame
      memory regs logicalInputs sourceStack targetStack observationType sourceMemory models current
      passed s t boundary)
  exact continued.prepend (.next steps.1 (.refl _)) (.next steps.2 (.refl _))

structure PrivateCallLink (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (sb : IR.Block) (spc : Nat)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (before : PrivateBank inputWidths)
    (prior : List (Guard inputWidths)) where
  next : PrivateBank inputWidths
  finish : PrivateSuffixDiagram source target sf tf type table
      sb (spc+1) tb (tpc+1) next (guardsForReturn prior) →
    PrivateSuffixDiagram source target sf tf type table sb spc tb tpc before prior

def privateCall (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (PrivateCutpoint sf tf)) (sb : IR.Block) (spc : Nat)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (before : PrivateBank inputWidths)
    (prior : List (Guard inputWidths)) (callee : PrivateCallee source target)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Option (PrivateCallLink source target sf tf type table sb spc tb tpc before prior) := do
  let entered ← privateDemandCallEntry source target sf sb spc before tf tb tpc
    callee.checked.sourceFunction callee.checked.targetFunction plan prior
  let binding ← privateResumeBindings before entered.val.1.results entered.val.2.1 callee.type
  let originalBody ← privateCalleeArguments callee.checked entered.val.2.2.original.arguments
  let substitutedBody ← privateCalleeArguments callee.checked entered.val.2.2.substituted.arguments
  let opened := private_demand_callee callee plan demand entered.val.2.2 entered.val.1 entered.val.2.1
    sf sb spc before tf tb tpc binding.targetArity originalBody.down substitutedBody.down
  return {
    next := binding.next,
    finish := fun suffix => private_suffix_call source target sf tf type table sb spc tb tpc
      before prior callee plan entered.val.1 entered.val.2.1 entered.val.2.2 entered.property
      opened binding suffix }

end EvoIR.Core.Symbolic.MachineSelection
