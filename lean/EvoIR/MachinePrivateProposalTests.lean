import EvoIR.MachinePrivateProposal

/-! Proposal controls for the actual pointer-carrying loop fixture. These
checks are discovery tests, not proof admission: the paired body checker must
still establish every proposed incoming cell and its initialized bytes. -/
namespace EvoIR.Core.Symbolic.MachineSelection.PrivateProposalTests

private def effects : IR.Effects := ⟨true, true, false, false, false, false, true, false⟩

private def function (stored := true) (width := 32) : IR.Function :=
  ⟨"cell", [.bits 32, .memory, .world], [.bits 32, .memory, .world], false, effects, [
    ⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩],
      [⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 0 4 4, [1, 2]⟩] ++
        (if stored then [⟨[⟨6, .memory⟩, ⟨7, .world⟩], .store, [4, 5, 3, 0]⟩] else []),
      .branch ⟨"loop", [3, if stored then 6 else 4, if stored then 7 else 5]⟩⟩,
    -- Reuse value IDs in another block. They are not aliases by spelling.
    ⟨"loop", [⟨0, .pointer⟩, ⟨1, .memory⟩, ⟨2, .world⟩],
      [⟨[⟨3, .bits width⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .load width, [1, 2, 0]⟩],
      .branch ⟨"loop", [0, 4, 5]⟩⟩]⟩

private def loopShape (input : IR.Function) : Option (List LocalShape) := do
  let points ← PrivateProposal.propose input
  let point ← points.find? (·.layout.label == "loop")
  return point.layout.shape

example : loopShape (function) = some [⟨0, ⟨.uint, false⟩, true⟩] := by decide

-- A backedge cannot manufacture initialized bytes absent on the entry edge.
example : loopShape (function false) = some [⟨0, ⟨.uint, false⟩, false⟩] := by decide

-- Conflicting allocation and load widths remain a refusal, not a chosen fit.
example : (PrivateProposal.propose (function true 64)).isNone = true := by decide

private def missingArgument : IR.Function :=
  {function with blocks := (function).blocks.map fun block =>
    if block.name == "entry" then {block with terminator := .branch ⟨"loop", [3, 6]⟩}
    else block}

example : (PrivateProposal.propose missingArgument).isNone = true := by decide

private def missingBlock : IR.Function :=
  {function with blocks := (function).blocks.map fun block =>
    if block.name == "entry" then {block with terminator := .branch ⟨"missing", [3, 6, 7]⟩}
    else block}

example : (PrivateProposal.propose missingBlock).isNone = true := by decide

private def oneBitParameter : IR.Function :=
  ⟨"predicate", [.bits 1, .memory, .world], [.bits 1, .memory, .world], false, effects,
    [⟨"entry", [⟨0, .bits 1⟩, ⟨1, .memory⟩, ⟨2, .world⟩], [], .return [0, 1, 2]⟩]⟩

-- The private cell layout currently uses only 32/64-bit word carriers.
-- Unsupported widths are not silently widened to match that representation.
example : (PrivateProposal.propose oneBitParameter).isNone = true := by decide

end EvoIR.Core.Symbolic.MachineSelection.PrivateProposalTests
