import EvoIR.Core.ReturnCorrespondence
import EvoIR.Core.CallingLoopPair
import EvoIR.Core.PrivateFunctionClosure
import EvoIR.Core.PrivateCyclic
import EvoIR.Core.PrivateCallingClosure

/-! One proof-valued result for the existing source/IR entry routes. Route tags
retain CLI reporting only; every accepted result is the same all-argument
`CallFrontierPair`. An explicit proposal never falls back to another request. -/
namespace EvoIR.Core.Symbolic

def ReturnPair.toCallFrontierPair {source : Source.Program} {target : IR.Program}
    (pair : ReturnPair source target) : CallFrontierPair source target where
  sourceFunction := pair.sourceFunction
  targetFunction := pair.targetFunction
  tokens := false
  checked := by
    intro args typed
    simpa [loopInputValues] using pair.refines args typed

def CallPair.toCallFrontierPair {source : Source.Program} {target : IR.Program}
    (pair : CallPair source target) : CallFrontierPair source target where
  sourceFunction := pair.sourceFunction
  targetFunction := pair.targetFunction
  tokens := true
  checked := by
    intro args typed
    simpa [loopInputValues] using pair.refines args typed

def LoopPair.toCallFrontierPair {source : Source.Program} {target : IR.Program}
    (pair : LoopPair source target) : CallFrontierPair source target where
  sourceFunction := pair.sourceFunction
  targetFunction := pair.targetFunction
  tokens := pair.diagram.tokens
  checked := pair.refines

inductive SourceTargetRoute where
  | leaf
  | call
  | loop
  | loopCall
  deriving DecidableEq, BEq

inductive SourceTargetRequest where
  | finite
  | cutpoints (layouts : List CutpointLayout) (budget : Nat)
  | closure (proposals : CalleeProposals) (budget : Nat)

def checkSourceTargetPair (source : Source.Program) (target : IR.Program) :
    SourceTargetRequest → Option (SourceTargetRoute × CallFrontierPair source target)
  | .finite =>
    match returnPair source target with
    | some pair => some (.leaf, pair.toCallFrontierPair)
    | none =>
      match callPair source target with
      | some pair => some (.call, pair.toCallFrontierPair)
      | none =>
        match privateFinitePair source target 512 with
        | some pair => some (.call, pair)
        | none => (privateOpenCallPair source target (fun _ => none) 512).map fun pair => (.call, pair)
  | .cutpoints layouts budget =>
    match loopPair source target layouts budget with
    | some pair => some (.loop, pair.toCallFrontierPair)
    | none => (privateLoopPair source target layouts budget).map fun pair => (.loop, pair)
  | .closure proposals budget =>
    match callingLoopPair source target proposals budget with
    | some pair => some (.loopCall, pair)
    | none => (privateOpenCallPair source target proposals budget).map fun pair => (.loopCall, pair)

theorem check_source_target_refines {source : Source.Program} {target : IR.Program}
    (request : SourceTargetRequest) (route : SourceTargetRoute)
    (pair : CallFrontierPair source target)
    (_accepted : checkSourceTargetPair source target request = some (route, pair))
    (args : List Source.Value)
    (typed : args.map Source.Value.type = pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name (loopInputValues pair.tokens args) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry :=
  pair.refines args typed

end EvoIR.Core.Symbolic
