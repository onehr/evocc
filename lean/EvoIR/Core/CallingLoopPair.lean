import EvoIR.Core.FunctionClosure

/-! The public cyclic-call entry uses the context-parametric closure builder.
There is no separate two-level admission path or trusted finite-callee summary. -/
namespace EvoIR.Core.Symbolic

def callingLoopPair (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Option (CallFrontierPair source target) :=
  openCallPair source target proposals budget

end EvoIR.Core.Symbolic
