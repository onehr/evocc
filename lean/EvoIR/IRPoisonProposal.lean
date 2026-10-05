import EvoIR.IRPoisonCheck

/-!
Finite proposal discovery for dead uninitialized values. Every actual operand
is demanded, even when its instruction result is unused. Only edge arguments
whose destination parameter is independently dead may remain undemanded.
The syntactic iteration bound limits discovery, not program execution.
-/
namespace EvoIR.Core.IR.Poison

private def seedDemand (function : Function) : List Nat :=
  (function.blocks.flatMap fun block =>
    block.instructions.flatMap Instruction.operands ++ testUses block.terminator).eraseDups

private def closeEdges (function : Function) (needed : List Nat) : List Nat :=
  (needed ++ function.blocks.flatMap fun block =>
    (edges block.terminator).flatMap fun edge =>
      match function.blocks.find? (·.name == edge.block) with
      | none => []
      | some destination =>
        (destination.parameters.zip edge.arguments).filterMap fun (parameter, argument) =>
          if needed.contains parameter.id then some argument else none).eraseDups

private def closure (function : Function) : Nat → List Nat → List Nat
  | 0, needed => needed
  | fuel + 1, needed =>
    let next := closeEdges function needed
    if next = needed then needed else closure function fuel next

def proposeNeeded (function : Function) : List Nat :=
  let ids := (function.blocks.flatMap fun block =>
    block.parameters.map Binding.id ++
    block.instructions.flatMap (fun inst => inst.results.map Binding.id ++ inst.operands) ++
    testUses block.terminator ++ (edges block.terminator).flatMap Edge.arguments).eraseDups
  closure function (ids.length + 1) (seedDemand function)

structure CheckedProposal (source : Program) where
  plan : DemandPlan
  target : Program
  checked : checkPair source target plan = true

/-- Missing successors, wrong edge arities and insufficient demand closure are
rejected by the independent checker. Discovery never repairs the input. -/
def propose (source : Program) : Option (CheckedProposal source) :=
  let plan := source.functions.map fun function => (function.name, proposeNeeded function)
  let target := rewriteProgram plan source
  if accepted : checkPair source target plan = true then
    some ⟨plan, target, accepted⟩
  else none

end EvoIR.Core.IR.Poison
