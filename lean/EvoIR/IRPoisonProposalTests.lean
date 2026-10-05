import EvoIR.IRPoisonProposal

/-! The original caller carries a dead uninitialized value through a complete
edge vector. Do not remove that vector element, change its type, or admit an
actual operand read merely because another result is unused. -/
namespace EvoIR.Core.IR.Poison.ProposalTests

private def effects : Effects := ⟨false, false, false, false, false, false, false, false⟩

private def function (read := false) (returnDead := false) : Function :=
  ⟨"carry", [.bits 32], [.bits 32], false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩],
      [⟨[⟨1, .bits 32⟩], .uninitialized (.bits 32), []⟩],
      .branch ⟨"next", [0, 1]⟩⟩,
    ⟨"next", [⟨2, .bits 32⟩, ⟨3, .bits 32⟩],
      (if read then [⟨[⟨4, .bits 32⟩], .scalar (.bin .add 32), [2, 3]⟩] else []),
      .return [if returnDead then 3 else 2]⟩]⟩

private def normalized (source : Program) : Option Program :=
  (propose source).map CheckedProposal.target

private def changed (source : Program) : Bool :=
  match normalized source with
  | some target => decide (source ≠ target)
  | none => false

example : changed ⟨[function]⟩ = true := by decide
example : changed ⟨[function true]⟩ = false := by decide
example : changed ⟨[function false true]⟩ = false := by decide

-- Edge parameters and arguments remain byte-for-byte the original syntax.
example : (normalized ⟨[function]⟩).map (fun target =>
    target.functions.flatMap fun f => f.blocks.map (fun b => (b.parameters, b.terminator))) =
    some ((function).blocks.map fun b => (b.parameters, b.terminator)) := by decide

private def malformedEdge : Program := ⟨[
  {function with blocks := [
    ⟨"entry", [⟨0, .bits 32⟩], [], .branch ⟨"next", [0]⟩⟩,
    ⟨"next", [⟨2, .bits 32⟩, ⟨3, .bits 32⟩], [], .return [2]⟩]}]⟩

example : (propose malformedEdge).isNone = true := by decide

private def missingSuccessor : Program := ⟨[
  {function with blocks := [⟨"entry", [⟨0, .bits 32⟩], [], .branch ⟨"missing", [0]⟩⟩]}]⟩

example : (propose missingSuccessor).isNone = true := by decide

-- A forged empty demand set does not permit removal of an observed value.
example : checkPair ⟨[function false true]⟩
    (rewriteProgram [("carry", [])] ⟨[function false true]⟩) [("carry", [])] = false := by decide

end EvoIR.Core.IR.Poison.ProposalTests
