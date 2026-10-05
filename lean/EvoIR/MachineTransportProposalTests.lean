import EvoIR.MachineTransportProposal

/-! A copy-only output block need not have an entry copy prefix. This was
observed in the original abs/control modules: the copies transfer outgoing
edge values and must execute after the block's entry invariant. -/
namespace EvoIR.Machine.Transport.ProposalTests

private def r (id : Nat) : VReg := ⟨id, .gpr32⟩

private def before : Program := ⟨[⟨"join", [
  ⟨0, [r 0], [], .cb true (r 0) 1 [r 0] 2 [r 0]⟩,
  ⟨1, [r 1], [], .b 3 [r 1]⟩,
  ⟨2, [r 2], [.binaryImm .add (r 4) (r 2) 1 .w], .b 3 [r 4]⟩,
  ⟨3, [r 3], [], .ret [r 3]⟩]⟩]⟩

private def after (wrong := false) (effect := false) : Program := ⟨[⟨"join", [
  ⟨0, [r 0], [], .cb true (r 0) 1 [] 2 []⟩,
  ⟨1, [], (if effect then [.bl "hidden" [] []] else []) ++
    [.mov (r 3) (if wrong then r 4 else r 0)], .b 3 []⟩,
  ⟨2, [], [.binaryImm .add (r 4) (r 0) 1 .w, .mov (r 3) (r 4)], .b 3 []⟩,
  ⟨3, [], [], .ret [r 3]⟩]⟩]⟩

example : (proposeProgram before (after)).isSome = true := by decide
example : (proposeProgram before (after true)).isSome = false := by decide
example : (proposeProgram before (after false true)).isSome = false := by decide

example : (do
    let plan ← proposeProgram before (after)
    let function ← functionPlan plan "join"
    let block ← findPlan function 1
    return block.prefixLength) = some 0 := by decide

end EvoIR.Machine.Transport.ProposalTests
