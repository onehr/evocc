import EvoIR.MachineSelectionModule
import EvoIR.MachineSelectionDemandCaller
import EvoIR.MachineTransportProposal

/-! Production demand-aware selection. The demand proposal is finite syntax
search on the actual SSA module and is independently checked once. Bottom-up
functions retain only actual proof-valued callee records; no unresolved call,
private effect, poison or proposal failure is silently accepted. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def tokenDemandProposal (target : EvoIR.Machine.Program) : Option EvoIR.Machine.Demand.Plan :=
  target.functions.mapM fun function => do
    let needed ← EvoIR.Machine.Transport.proposeDemand function
    return ⟨function.name, needed⟩

private def checkDemandTokenFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (function : IR.Function) (callees : List (TokenCallee source target))
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Option (TokenCallee source target) := do
  let type ← tokenResultType function
  let checked ← tokenDemandCallerFunctionChecked source target function.name type
    (tokenBlockProposal function) callees plan demand
  return ⟨function.name, type, checked⟩

private def demandTokenModulePass (source : IR.Program) (target : EvoIR.Machine.Program)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    List IR.Function → List (TokenCallee source target) → List (TokenCallee source target)
  | [], callees => callees
  | function :: rest, callees =>
    let next := if callees.any (fun previous => previous.name == function.name) then callees else
      match checkDemandTokenFunction source target function callees plan demand with
      | none => callees
      | some checked => checked :: callees
    demandTokenModulePass source target plan demand rest next

private def demandTokenModulePasses (source : IR.Program) (target : EvoIR.Machine.Program)
    (plan : EvoIR.Machine.Demand.Plan) (demand : EvoIR.Machine.Demand.Checked target plan) :
    Nat → List (TokenCallee source target) → List (TokenCallee source target)
  | 0, callees => callees
  | passes+1, callees => demandTokenModulePasses source target plan demand passes
      (demandTokenModulePass source target plan demand source.functions callees)

structure CheckedTokenDemandModule (source : IR.Program) (target : EvoIR.Machine.Program) where
  plan : EvoIR.Machine.Demand.Plan
  demand : EvoIR.Machine.Demand.Checked target plan
  functions : TokenModuleCheck source target

def tokenDemandModule (source : IR.Program) (target : EvoIR.Machine.Program) :
    Option (CheckedTokenDemandModule source target) := do
  let plan ← tokenDemandProposal target
  let demand ← EvoIR.Machine.Demand.checkDemand target plan
  let callees := demandTokenModulePasses source target plan demand (source.functions.length+1) []
  let refused := (source.functions.filter fun function =>
    !(callees.any fun callee => callee.name == function.name)).map IR.Function.name
  return ⟨plan, demand, ⟨callees, refused⟩⟩

def tokenDemandModuleFunction (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (type : Source.IntType) :
    Option (CheckedTokenFunction source target entry type) := do
  let function ← source.functions.find? (·.name == entry)
  let module ← tokenDemandModule source target
  tokenDemandCallerFunctionChecked source target entry type (tokenBlockProposal function)
    module.functions.callees module.plan module.demand

def tokenDemandModulePair (source : IR.Program) (target : EvoIR.Machine.Program)
    (entry : String) (parameters : List Source.Parameter) (type : Source.IntType) :
    Option (TokenSelectionPair source target entry parameters type) := do
  let function ← tokenDemandModuleFunction source target entry type
  tokenFunctionPairUsing source target entry parameters type function

end EvoIR.Core.Symbolic.MachineSelection
