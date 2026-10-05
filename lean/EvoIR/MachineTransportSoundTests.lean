import EvoIR.MachineTransportSound
import EvoIR.MachineSelectionCFGTests

/-! Public unbounded transport endpoint and its composition with the proved
actual selection progress. The checker controls below inspect both complete
AST inputs and are not finite execution oracles. -/
namespace EvoIR.Machine.Transport.SoundTests

private def r (id : Nat) : VReg := ⟨id, .gpr32⟩
private def flags : VReg := ⟨10, .flags⟩

private def before : Program := ⟨[⟨"countdown", [
  ⟨0, [r 0], [], .b 1 [r 0]⟩,
  ⟨1, [r 1], [.cmpImm flags (r 1) 0 .w], .bCond flags .ne 2 [r 1] 3 [r 1]⟩,
  ⟨2, [r 4], [.binaryImm .sub (r 6) (r 4) 1 .w], .b 1 [r 6]⟩,
  ⟨3, [r 7], [], .ret [r 7]⟩]⟩]⟩

private def after : Program := ⟨[⟨"countdown", [
  ⟨0, [r 0], [.mov (r 1) (r 0)], .b 1 []⟩,
  ⟨1, [], [.cmpImm flags (r 1) 0 .w], .bCond flags .ne 2 [] 3 []⟩,
  ⟨2, [], [.binaryImm .sub (r 6) (r 1) 1 .w, .mov (r 1) (r 6)], .b 1 []⟩,
  ⟨3, [], [], .ret [r 1]⟩]⟩]⟩

private def plan : ProgramPlan := ⟨[("countdown", ⟨[0, 1, 4, 6, 7, 10], [
  ⟨0, 0, [(0, r 0)], []⟩,
  ⟨1, 0, [(1, r 1)], [some 0]⟩,
  ⟨2, 0, [(4, r 1)], [some 0]⟩,
  ⟨3, 0, [(7, r 1)], []⟩]⟩)]⟩

example : checkProgram before after plan = true := by decide

/-- Every canonical uint input is covered, including arbitrarily many loop
iterations. The source progress is the exact independently checked selection
theorem for this same preallocation SSA input, not an assumed equivalence. -/
theorem countdown_all_inputs (values : List Core.Source.Value)
    (typed : values.map Core.Source.Value.type = [.uint]) :
    ∃ entry,
      initial before "countdown" (values.map sourceArgument) = .ok entry ∧
      initial after "countdown" (values.map sourceArgument) = .ok entry ∧
      TransportRefines before after .uint entry entry := by
  obtain ⟨_, entry, _, started, selection⟩ := Core.Symbolic.MachineSelection.Tests.countdown_all_inputs values
    typed
  have sourceStarted : initial before "countdown" (values.map sourceArgument) = .ok entry := by
    exact started
  have sourceProgress : ObservedProgress .ssa before .uint entry := selection.progress
  obtain ⟨targetStarted, refinement⟩ := sound before after plan (by decide) .uint "countdown"
    (values.map sourceArgument) ⟨1, []⟩ entry sourceStarted sourceProgress
  exact ⟨entry, sourceStarted, targetStarted, refinement⟩

private def manyArgs : List VReg := (List.range 32).map r
private def calleeArgs : List VReg := (List.range 32).map (fun id => r (id+100))

private def callsBefore : Program := ⟨[
  ⟨"caller", [⟨0, manyArgs, [.bl "callee" manyArgs [r 40]], .ret [r 40]⟩]⟩,
  ⟨"callee", [⟨0, calleeArgs, [.movImm (r 200) 4], .ret [r 100]⟩]⟩]⟩
private def callsAfter : Program := ⟨[
  ⟨"caller", [⟨0, manyArgs, [.bl "callee" manyArgs [r 40]], .ret [r 40]⟩]⟩,
  ⟨"callee", [⟨0, calleeArgs, [], .ret [r 100]⟩]⟩]⟩
private def callsPlan : ProgramPlan := ⟨[
  ("caller", ⟨List.range 32 ++ [40], [⟨0, 0, (List.range 32).map (fun id => (id, r id)), [some 0]⟩]⟩),
  ("callee", ⟨[100], [⟨0, 0, [(100, r 100)], [none]⟩]⟩)]⟩

example : checkProgram callsBefore callsAfter callsPlan = true := by decide

/-- Full logical argument vectors cannot be truncated to the physical ABI. -/
example : checkProgram callsBefore
    ⟨[⟨"caller", [⟨0, manyArgs, [.bl "callee" (manyArgs.take 8) [r 40]], .ret [r 40]⟩]⟩,
      ⟨"callee", [⟨0, calleeArgs, [], .ret [r 100]⟩]⟩]⟩ callsPlan = false := by decide

/-- An extra target function is not an ignored table suffix. -/
example : checkProgram before ⟨after.functions ++ [⟨"extra", [⟨0, [], [], .brk 3002⟩]⟩]⟩ plan = false := by decide

#print axioms checkpointSimulation
#print axioms sound
#print axioms countdown_all_inputs

end EvoIR.Machine.Transport.SoundTests
