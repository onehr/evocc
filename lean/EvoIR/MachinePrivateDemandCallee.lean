import EvoIR.MachinePrivateDemandCallEntry
import EvoIR.MachinePrivateProgress
import EvoIR.MachineSelectionDemandCallee

/-!
Replace only the target callee entry using independently checked demand.
Both original and substituted standalone progress follow from the preceding
all-argument private callee proof, at the real related incoming memories.
The suspended caller is not assumed to progress. At a successful return the
full machine resumed state is equal, including its independent memory.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem private_machine_return_short (caller : EvoIR.Machine.Activation)
    (results : List EvoIR.Machine.VReg) (arity : results.length = 1)
    (stack : List EvoIR.Machine.ReturnFrame) (value : Source.Value)
    (memory : IR.Memory) (owner frame : Nat) :
    (machineReturnState [EvoIR.Machine.sourceArgument value] memory owner frame
      (⟨caller, results⟩ :: stack)).stack.length < (⟨caller, results⟩ :: stack).length := by
  obtain ⟨reg, same⟩ := List.length_eq_one_iff.mp arity
  rw [same]
  simp only [machineReturnState]
  rw [EvoIR.Machine.bind_exact caller.regs [reg]
    [EvoIR.Machine.sourceArgument value] rfl (by rfl)]
  simp

def PrivateDemandCalleeDiagram (callee : PrivateCallee source target)
    (plan : EvoIR.Machine.Demand.Plan)
    (entry : TokenDemandCallEntryResult inputWidths callee.checked.sourceFunction callee.checked.targetFunction plan)
    (inst : IR.Instruction) (results : List EvoIR.Machine.VReg)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (before : PrivateBank inputWidths)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) : Prop :=
  ∀ σ values incoming owner frame targetMemory regs logicalInputs sourceStack targetStack
      observationType sourceMemory,
    before.Models σ values incoming owner frame targetMemory regs →
    before.source.memoryValue values = some sourceMemory →
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (PrivateReturnBoundary callee.type sourceMemory frame
        (⟨⟨sf.name, sb.name, spc+1, before.source.scope.denote σ (privateToken values), owner⟩,
          inst.results⟩ :: sourceStack)
        (⟨⟨tf.name, tb.id, tpc+1, regs, owner, logicalInputs⟩, results⟩ :: targetStack))
      ⟨.running ⟨callee.checked.sourceFunction.name, entry.original.sourceBlock.name, 0,
        entry.original.sourceScope.denote σ (tokenEnvironment sourceMemory), frame⟩,
        ⟨⟨sf.name, sb.name, spc+1, before.source.scope.denote σ (privateToken values), owner⟩,
          inst.results⟩ :: sourceStack, frame+1⟩
      ⟨.running ⟨callee.checked.targetFunction.name, entry.substituted.targetBlock.id, 0,
        bankDenote σ entry.substituted.targetBank, frame, machineInputs σ entry.substituted.arguments⟩,
        ⟨⟨tf.name, tb.id, tpc+1, regs, owner, logicalInputs⟩, results⟩ :: targetStack,
        frame+1, targetMemory⟩

theorem private_demand_callee (callee : PrivateCallee source target)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan)
    (entry : TokenDemandCallEntryResult inputWidths callee.checked.sourceFunction callee.checked.targetFunction plan)
    (inst : IR.Instruction) (results : List EvoIR.Machine.VReg)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (before : PrivateBank inputWidths)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat)
    (arity : results.length = 1)
    (originalBody : PrivateCalleeArgumentsDiagram callee.checked entry.original.arguments)
    (substitutedBody : PrivateCalleeArgumentsDiagram callee.checked entry.substituted.arguments) :
    PrivateDemandCalleeDiagram callee plan entry inst results sf sb spc before tf tb tpc := by
  intro σ values incoming owner frame targetMemory regs logicalInputs sourceStack targetStack
    observationType sourceMemory models current
  obtain ⟨actualMemory, actualCurrent, memoryRelated⟩ := models.memory
  have actualSame : actualMemory = sourceMemory :=
    Option.some.inj (actualCurrent.symm.trans current)
  have relatedMemory : EvoIR.Machine.PrivateMemory.LiveRelated sourceMemory targetMemory := by
    simpa only [actualSame] using memoryRelated
  obtain ⟨_, _, _, _, currentMemory, currentValue, _, bound, _, _⟩ := models.source
  have currentSame : currentMemory = sourceMemory := Option.some.inj (currentValue.symm.trans current)
  have fresh : sourceMemory.BeforeFrame frame := by simpa only [currentSame] using bound
  let original : EvoIR.Machine.State :=
    ⟨.running ⟨callee.checked.targetFunction.name, entry.original.targetBlock.id, 0,
      bankDenote σ entry.original.targetBank, frame, machineInputs σ entry.original.arguments⟩,
      [], frame+1, targetMemory⟩
  let substituted : EvoIR.Machine.State :=
    ⟨.running ⟨callee.checked.targetFunction.name, entry.substituted.targetBlock.id, 0,
      bankDenote σ entry.substituted.targetBank, frame, machineInputs σ entry.substituted.arguments⟩,
      [], frame+1, targetMemory⟩
  let savedSource : List IR.ReturnFrame :=
    ⟨⟨sf.name, sb.name, spc+1, before.source.scope.denote σ (privateToken values), owner⟩,
      inst.results⟩ :: sourceStack
  let savedTarget : List EvoIR.Machine.ReturnFrame :=
    ⟨⟨tf.name, tb.id, tpc+1, regs, owner, logicalInputs⟩, results⟩ :: targetStack
  have originalProgress := private_callee_progress callee.checked entry.original.arguments originalBody
    σ sourceMemory targetMemory frame (frame+1) _ _ fresh (by omega) relatedMemory
    (entry.original.sourceEntry σ sourceMemory frame) (entry.original.targetEntry σ frame)
  have substitutedProgress := private_callee_progress callee.checked entry.substituted.arguments substitutedBody
    σ sourceMemory targetMemory frame (frame+1) _ _ fresh (by omega) relatedMemory
    (entry.substituted.sourceEntry σ sourceMemory frame) (entry.substituted.targetEntry σ frame)
  have related := token_demand_entry_related _ _ plan entry σ targetMemory frame (frame+1)
  have single : EvoIR.Machine.Demand.SingleCaller savedTarget := by
    intro head tail same
    have equal := (List.cons.inj same).1
    cases equal
    exact arity
  have old := originalBody σ sourceMemory targetMemory frame (frame+1) savedSource savedTarget
    observationType _ _ fresh (by omega) relatedMemory
    (entry.original.sourceEntry σ sourceMemory frame) (entry.original.targetEntry σ frame)
  apply EvoIR.Machine.Demand.replace_context_target (IR.step source) (observeTarget observationType)
    observationType target plan demand.accepted callee.type original substituted originalProgress substitutedProgress
    related savedTarget single (PrivateReturnBoundary callee.type sourceMemory frame savedSource savedTarget) _ old
  rintro _ _ ⟨value, afterSource, afterTarget, afterFrame, typed, memory, closed, safe, bound,
    future, allocated, sourceAt, rfl⟩
  exact private_machine_return_short _ results arity targetStack value afterTarget frame afterFrame

end EvoIR.Core.Symbolic.MachineSelection
