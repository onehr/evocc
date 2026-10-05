import EvoIR.MachineSelectionCallee
import EvoIR.MachineSelectionDemandCallEntry
import EvoIR.MachineSelectionCallResume
import EvoIR.MachineDemandContextPrefixes

/-! Demand replacement obtains BOTH actual standalone progress premises
from the checked all-argument callee. The retained source entry is unchanged,
and only the target actual full vector differs at undemanded positions. -/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem token_demand_entry_related (sc : IR.Function) (tc : EvoIR.Machine.Function)
    (plan : EvoIR.Machine.Demand.Plan) (entry : TokenDemandCallEntryResult inputWidths sc tc plan)
    (σ : Subst inputWidths) (memory : IR.Memory) (owner nextFrame : Nat) :
    EvoIR.Machine.Demand.StateRelated plan
      ⟨.running ⟨tc.name, entry.original.targetBlock.id, 0, bankDenote σ entry.original.targetBank,
        owner, machineInputs σ entry.original.arguments⟩, [], nextFrame, memory⟩
      ⟨.running ⟨tc.name, entry.substituted.targetBlock.id, 0, bankDenote σ entry.substituted.targetBank,
        owner, machineInputs σ entry.substituted.arguments⟩, [], nextFrame, memory⟩ := by
  have activations := EvoIR.Machine.Demand.enter_success_related plan tc entry.entryBlock entry.entryFound owner
    (machineInputs σ entry.original.arguments) (machineInputs σ entry.substituted.arguments) (entry.agreement σ)
    _ _ (entry.original.targetEntry σ owner) (entry.substituted.targetEntry σ owner)
  exact ⟨.running activations, .nil, rfl, rfl⟩

def TokenDemandCalleeDiagram (callee : TokenCallee source target)
    (plan : EvoIR.Machine.Demand.Plan)
    (entry : TokenDemandCallEntryResult inputWidths callee.checked.sourceFunction callee.checked.targetFunction plan)
    (inst : IR.Instruction) (results : List EvoIR.Machine.VReg) (types : List IR.Ty)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (scope : Scope inputWidths)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths) : Prop :=
  ∀ σ (values : List IR.Value), values.map IR.Value.type = types →
    ∀ memory owner nextFrame incoming sourceStack targetStack observationType,
    Transition.OpenCheckpoint (IR.step source) (EvoIR.Machine.step .ssa target)
      (observeTarget observationType) (EvoIR.Machine.observe observationType)
      (TokenReturnBoundary callee.type nextFrame
        (⟨⟨sf.name, sb.name, spc+1, scope.denote σ (privateToken values), owner⟩, inst.results⟩ :: sourceStack)
        (⟨⟨tf.name, tb.id, tpc+1, bankDenote σ bank, owner, incoming⟩, results⟩ :: targetStack))
      ⟨.running ⟨callee.checked.sourceFunction.name, entry.original.sourceBlock.name, 0,
        entry.original.sourceScope.denote σ (tokenEnvironment memory), nextFrame⟩,
        ⟨⟨sf.name, sb.name, spc+1, scope.denote σ (privateToken values), owner⟩, inst.results⟩ :: sourceStack,
        nextFrame+1⟩
      ⟨.running ⟨callee.checked.targetFunction.name, entry.substituted.targetBlock.id, 0,
        bankDenote σ entry.substituted.targetBank, nextFrame, machineInputs σ entry.substituted.arguments⟩,
        ⟨⟨tf.name, tb.id, tpc+1, bankDenote σ bank, owner, incoming⟩, results⟩ :: targetStack,
        nextFrame+1, memory⟩

theorem token_demand_callee (callee : TokenCallee source target)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan)
    (entry : TokenDemandCallEntryResult inputWidths callee.checked.sourceFunction callee.checked.targetFunction plan)
    (inst : IR.Instruction) (results : List EvoIR.Machine.VReg) (types : List IR.Ty)
    (sf : IR.Function) (sb : IR.Block) (spc : Nat) (scope : Scope inputWidths)
    (tf : EvoIR.Machine.Function) (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths)
    (binding : ResumeBindings types scope bank inst.results results callee.type)
    (originalBody : TokenCalleeArgumentsDiagram callee.checked entry.original.arguments)
    (substitutedBody : TokenCalleeArgumentsDiagram callee.checked entry.substituted.arguments) :
    TokenDemandCalleeDiagram callee plan entry inst results types sf sb spc scope tf tb tpc bank := by
  intro σ values typed memory owner nextFrame incoming sourceStack targetStack observationType
  let original : EvoIR.Machine.State :=
    ⟨.running ⟨callee.checked.targetFunction.name, entry.original.targetBlock.id, 0,
      bankDenote σ entry.original.targetBank, nextFrame, machineInputs σ entry.original.arguments⟩,
      [], nextFrame+1, memory⟩
  let substituted : EvoIR.Machine.State :=
    ⟨.running ⟨callee.checked.targetFunction.name, entry.substituted.targetBlock.id, 0,
      bankDenote σ entry.substituted.targetBank, nextFrame, machineInputs σ entry.substituted.arguments⟩,
      [], nextFrame+1, memory⟩
  let savedSource : List IR.ReturnFrame :=
    ⟨⟨sf.name, sb.name, spc+1, scope.denote σ (privateToken values), owner⟩, inst.results⟩ :: sourceStack
  let savedTarget : List EvoIR.Machine.ReturnFrame :=
    ⟨⟨tf.name, tb.id, tpc+1, bankDenote σ bank, owner, incoming⟩, results⟩ :: targetStack
  have originalProgress := token_callee_progress callee.checked entry.original.arguments originalBody σ memory nextFrame
    (nextFrame+1) _ _ (entry.original.sourceEntry σ memory nextFrame) (entry.original.targetEntry σ nextFrame)
  have substitutedProgress := token_callee_progress callee.checked entry.substituted.arguments substitutedBody σ memory nextFrame
    (nextFrame+1) _ _ (entry.substituted.sourceEntry σ memory nextFrame) (entry.substituted.targetEntry σ nextFrame)
  have related := token_demand_entry_related _ _ plan entry σ memory nextFrame (nextFrame+1)
  have single : EvoIR.Machine.Demand.SingleCaller savedTarget := by
    intro frame rest same
    have head := (List.cons.inj same).1
    cases head
    exact binding.targetArity
  have old := originalBody σ memory nextFrame (nextFrame+1) savedSource savedTarget observationType
    _ _ (entry.original.sourceEntry σ memory nextFrame) (entry.original.targetEntry σ nextFrame)
  apply EvoIR.Machine.Demand.replace_context_target (IR.step source) (observeTarget observationType)
    observationType target plan demand.accepted callee.type original substituted originalProgress substitutedProgress
    related savedTarget single (TokenReturnBoundary callee.type nextFrame savedSource savedTarget) _ old
  rintro _ _ ⟨calleeSubst, word, afterMemory, afterFrame, width, rfl, rfl⟩
  have resumed := call_resume_exact types scope bank inst.results results callee.type binding
    sf sb (spc+1) tf tb (tpc+1) σ values typed owner nextFrame afterFrame incoming sourceStack targetStack
    calleeSubst word width afterMemory
  dsimp only at resumed
  rw [resumed.2]
  simp only [machineState, savedTarget, List.length_cons]
  omega

end EvoIR.Core.Symbolic.MachineSelection
