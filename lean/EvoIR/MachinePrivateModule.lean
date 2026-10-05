import EvoIR.MachinePrivateCaller
import EvoIR.MachineSelectionDemandModule

/-!
Bottom-up closure retains only completed private-function proofs. Each
callee proof covers arbitrary typed arguments and related incoming memories;
no pending function or sampled run can satisfy a call obligation. The pass
limit bounds certificate search, not loop iterations or execution steps.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

private def checkPrivateFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (function : IR.Function) (callees : List (PrivateCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Option (PrivateCallee source target) := do
  let type ← tokenResultType function
  let checked ← privateCallingFunction source target function.name type callees plan demand
  return ⟨function.name, type, checked⟩

private def privateModulePass (source : IR.Program) (target : EvoIR.Machine.Program)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    List IR.Function → List (PrivateCallee source target) → List (PrivateCallee source target)
  | [], callees => callees
  | function :: rest, callees =>
    let next := if callees.any (fun previous => previous.name == function.name) then callees else
      match checkPrivateFunction source target function callees plan demand with
      | none => callees
      | some checked => checked :: callees
    privateModulePass source target plan demand rest next

private def privateModulePasses (source : IR.Program) (target : EvoIR.Machine.Program)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Nat → List (PrivateCallee source target) → List (PrivateCallee source target)
  | 0, callees => callees
  | passes+1, callees => privateModulePasses source target plan demand passes
      (privateModulePass source target plan demand source.functions callees)

structure CheckedPrivateModule (source : IR.Program) (target : EvoIR.Machine.Program) where
  plan : EvoIR.Machine.Demand.Plan
  demand : EvoIR.Machine.Demand.Checked target plan
  callees : List (PrivateCallee source target)
  refused : List String

def privateModule (source : IR.Program) (target : EvoIR.Machine.Program) :
    Option (CheckedPrivateModule source target) := do
  let plan ← tokenDemandProposal target
  let demand ← EvoIR.Machine.Demand.checkDemand target plan
  let callees := privateModulePasses source target plan demand (source.functions.length+1) []
  let refused := (source.functions.filter fun function =>
    !(callees.any fun callee => callee.name == function.name)).map IR.Function.name
  return ⟨plan, demand, callees, refused⟩

def privateModuleFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) :
    Option (CheckedPrivateFunction source target entry type) := do
  let module ← privateModule source target
  privateCallingFunction source target entry type module.callees module.plan module.demand

def privateModulePair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← privateModuleFunction source target entry type
  privateFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
