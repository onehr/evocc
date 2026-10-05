import EvoIR.MachinePrivateCFG
import EvoIR.MachinePrivateProposal

/-! Untrusted private cutpoint discovery followed by the independently
checked actual-pair CFG. Discovery bounds certificate construction only. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateCFGProposedFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) :
    Option (CheckedPrivateFunction source target entry type) := do
  let function ← source.functions.find? (·.name == entry)
  let proposal ← PrivateProposal.propose function
  privateCFGFunction source target entry type proposal

def privateCFGProposedFunctionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← privateCFGProposedFunction source target entry type
  privateFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
