import EvoIR.MachineSelectionCallerCall
import EvoIR.MachineSelectionDemandCallee

/-! The always-on private-argument zero substitution is composed as an
actual target-entry replacement, not an altered source signature. Both callee
progress premises come from prior all-argument admission; caller suffix
correctness is discharged separately after the full actual return transfer. -/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem token_suffix_demand_call (source : IR.Program) (target : EvoIR.Machine.Program)
    (sf : IR.Function) (tf : EvoIR.Machine.Function) (type : Source.IntType)
    (table : List (TokenCutpoint sf tf)) (sb : IR.Block) (spc : Nat) (scope : Scope inputWidths)
    (tb : EvoIR.Machine.Block) (tpc : Nat) (bank : Bank inputWidths) (types : List IR.Ty)
    (memoryKey worldKey : Nat) (checks : List (Guard inputWidths))
    (callee : TokenCallee source target) (plan : EvoIR.Machine.Demand.Plan)
    (inst : IR.Instruction) (results : List EvoIR.Machine.VReg)
    (entry : TokenDemandCallEntryResult inputWidths callee.checked.sourceFunction callee.checked.targetFunction plan)
    (called : TokenDemandCallEntryDiagram source target sf sb spc scope inst tf tb tpc bank results
      callee.checked.sourceFunction callee.checked.targetFunction plan entry checks memoryKey worldKey)
    (calleeBody : TokenDemandCalleeDiagram callee plan entry inst results types sf sb spc scope tf tb tpc bank)
    (binding : ResumeBindings types scope bank inst.results results callee.type)
    (suffix : TokenSuffixDiagram source target sf tf type table sb (spc+1) binding.scope
      tb (tpc+1) binding.bank (types ++ [.memory, .world]) types.length (types.length+1)
      (guardsForReturn checks)) :
    TokenSuffixDiagram source target sf tf type table sb spc scope tb tpc bank types memoryKey worldKey checks := by
  intro σ values typed memory owner nextFrame incoming sourceStack targetStack observationType passed current world
  have steps := called σ (privateToken values) memory owner nextFrame incoming sourceStack targetStack passed current world
  have opened := calleeBody σ values typed memory owner nextFrame incoming sourceStack targetStack observationType
  have continued := opened.bind (nextBoundary := TokenCallerBoundary type table owner sourceStack targetStack) (by
    intro a b returned
    right
    exact token_return_suffix source target sf tf type callee.type table sb (spc+1) scope tb (tpc+1) bank
      types checks inst.results results binding suffix σ values typed owner nextFrame incoming sourceStack targetStack
      observationType passed a b returned)
  exact continued.prepend (.next steps.1 (.refl _)) (.next steps.2 (.refl _))

end EvoIR.Core.Symbolic.MachineSelection
