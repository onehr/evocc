import EvoIR.Core.PrivateContextCall

/-! One private body builder for the admitted nonrecursive closure. Depth is
finite call-graph inspection, not execution fuel; each cyclic callee closes
its own productive checkpoint table over arbitrary caller memory. -/
namespace EvoIR.Core.Symbolic

def privateCallingFunctionClosure (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Nat → PrivateFunctionBuilder source target
  | 0, _, _ => none
  | depth+1, sf, tf => do
    let (fuel, layouts) ← match proposals sf.symbol with
      | some plan => some plan
      | none => (entryCutpoint sf tf).map (fun layout => (budget, [layout]))
    privateFunctionWith source target sf tf layouts (fun points tokens =>
      privateContextCallerSuffix source target tf points sf.symbol tokens sf.returnType.type
        (privateCallingFunctionClosure source target proposals budget depth) fuel)

def privateOpenCallPair (source : Source.Program) (target : IR.Program)
    (proposals : CalleeProposals) (budget : Nat) : Option (CallFrontierPair source target) :=
  privatePairWith source target
    (privateCallingFunctionClosure source target proposals budget (source.functions.length+1))

end EvoIR.Core.Symbolic
