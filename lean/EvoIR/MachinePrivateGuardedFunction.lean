import EvoIR.MachinePrivateGuarded
import EvoIR.MachinePrivateProposal

/-! The guarded private path uses the same actual cutpoint table, contextual
entry and open return as ordinary private CFGs. Discovery is untrusted; every
body, guard, complete edge and return is independently checked. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def privateGuardedFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) :
    Option (CheckedPrivateFunction source target entry type) := do
  let function ← source.functions.find? (·.name == entry)
  let proposal ← PrivateProposal.propose function
  privateFunctionUsing source target entry type
    (fun sf sb tf tb bank => privateCFGBodyUsing source target sf sb tf tb bank type proposal
      (fun table => privateGuardedBlock source target sf tf type table))

def privateGuardedFunctionPair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← privateGuardedFunction source target entry type
  privateFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
