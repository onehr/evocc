import EvoIR.MachineTransportCopies

/-! Exact two-input structural regressions. The all-input unbounded theorem
and its entry-level composition are checked separately in
MachineTransportSoundTests; these finite controls are not execution oracles. -/
namespace EvoIR.Machine.Transport.Tests

private def r (id : Nat) : VReg := ⟨id, .gpr64⟩
private def f (id : Nat) : VReg := ⟨id, .flags⟩

private def joined : Function := ⟨"joined", [
  ⟨0, [r 0], [], .cb true (r 0) 1 [r 0] 2 [r 0]⟩,
  ⟨1, [r 1], [], .ret [r 1]⟩,
  ⟨2, [r 2], [], .ret [r 2]⟩]⟩

private def joinedAfter : Function := ⟨"joined", [
  ⟨0, [r 0], [], .cb true (r 0) 1 [] 2 []⟩,
  ⟨1, [], [.mov (r 1) (r 0)], .ret [r 1]⟩,
  ⟨2, [], [.mov (r 2) (r 0)], .ret [r 2]⟩]⟩

private def joinedPlan : FunctionPlan := ⟨[0, 1, 2], [
  ⟨0, 0, [(0, r 0)], []⟩,
  ⟨1, 1, [(1, r 1)], []⟩,
  ⟨2, 1, [(2, r 2)], []⟩]⟩

example : checkStructure joined joinedAfter joinedPlan = true := by decide

private def swappedEdge : Function := {joinedAfter with blocks := [
  ⟨0, [r 0], [], .cb true (r 0) 2 [] 1 []⟩,
  ⟨1, [], [.mov (r 1) (r 0)], .ret [r 1]⟩,
  ⟨2, [], [.mov (r 2) (r 0)], .ret [r 2]⟩]}

example : checkStructure joined swappedEdge joinedPlan = false := by decide

private def missingCopy : Function := {joinedAfter with blocks := [
  ⟨0, [r 0], [], .cb true (r 0) 1 [] 2 []⟩,
  ⟨1, [], [], .ret [r 1]⟩,
  ⟨2, [], [.mov (r 2) (r 0)], .ret [r 2]⟩]}

example : checkStructure joined missingCopy joinedPlan = false := by decide

private def critical : Function := ⟨"critical", [
  ⟨0, [r 0], [], .cb true (r 0) 2 [r 0] 1 [r 0]⟩,
  ⟨1, [r 1], [], .b 2 [r 1]⟩,
  ⟨2, [r 2], [], .ret [r 2]⟩]⟩

private def criticalAfter : Function := ⟨"critical", [
  ⟨0, [r 0], [], .cb true (r 0) 3 [] 1 []⟩,
  ⟨1, [], [.mov (r 1) (r 0), .mov (r 2) (r 1)], .b 2 []⟩,
  ⟨2, [], [], .ret [r 2]⟩,
  ⟨3, [], [.mov (r 2) (r 0)], .b 2 []⟩]⟩

private def criticalPlan : FunctionPlan := ⟨[0, 1, 2], [
  ⟨0, 0, [(0, r 0)], []⟩,
  ⟨1, 1, [(1, r 1)], []⟩,
  ⟨2, 0, [(2, r 2)], []⟩]⟩

example : checkStructure critical criticalAfter criticalPlan = true := by decide

private def cyclic : Function := ⟨"cyclic", [
  ⟨0, [r 0, r 1], [], .b 1 [r 0, r 1]⟩,
  ⟨1, [r 2, r 3], [], .cb true (r 2) 1 [r 3, r 2] 2 [r 2]⟩,
  ⟨2, [r 4], [], .ret [r 4]⟩]⟩

private def cyclicAfter : Function := ⟨"cyclic", [
  ⟨0, [r 0, r 1], [.mov (r 2) (r 0), .mov (r 3) (r 1)], .b 1 []⟩,
  ⟨1, [], [], .cb true (r 2) 3 [] 2 []⟩,
  ⟨2, [], [.mov (r 4) (r 2)], .ret [r 4]⟩,
  ⟨3, [], [.mov (r 5) (r 3), .mov (r 3) (r 2), .mov (r 2) (r 5)], .b 1 []⟩]⟩

private def cyclicPlan : FunctionPlan := ⟨[0, 1, 2, 3, 4], [
  ⟨0, 0, [(0, r 0), (1, r 1)], []⟩,
  ⟨1, 0, [(2, r 2), (3, r 3)], []⟩,
  ⟨2, 1, [(4, r 4)], []⟩]⟩

example : checkStructure cyclic cyclicAfter cyclicPlan = true := by decide

private def sequentialCycle : Function := {cyclicAfter with blocks := [
  ⟨0, [r 0, r 1], [.mov (r 2) (r 0), .mov (r 3) (r 1)], .b 1 []⟩,
  ⟨1, [], [], .cb true (r 2) 3 [] 2 []⟩,
  ⟨2, [], [.mov (r 4) (r 2)], .ret [r 4]⟩,
  ⟨3, [], [.mov (r 2) (r 3), .mov (r 3) (r 2)], .b 1 []⟩]}

example : checkStructure cyclic sequentialCycle cyclicPlan = false := by decide

private def constantBefore : Function := ⟨"constant", [
  ⟨0, [], [.movImm (r 1) 15], .brk 3002⟩]⟩
private def constantAfter : Function := ⟨"constant", [⟨0, [], [], .brk 3002⟩]⟩
private def constantPlan : FunctionPlan := ⟨[], [⟨0, 0, [], [none]⟩]⟩

example : checkStructure constantBefore constantAfter constantPlan = true := by decide
example : checkStructure constantBefore
    ⟨"constant", [⟨0, [], [], .brk 3003⟩]⟩ constantPlan = false := by decide

private def callBefore : Function := ⟨"call", [
  ⟨0, [r 0], [.bl "callee" [r 0] [r 1]], .ret [r 1]⟩]⟩
private def callAfter : Function := ⟨"call", [⟨0, [r 0], [], .ret [r 0]⟩]⟩
private def callPlan : FunctionPlan := ⟨[0, 1], [⟨0, 0, [(0, r 0)], [none]⟩]⟩

example : checkStructure callBefore callAfter callPlan = false := by decide

private def flagsBefore : Function := ⟨"flags", [
  ⟨0, [r 0], [.cmpImm (f 1) (r 0) 0 .x, .cset (r 2) (f 1) .eq,
    .cmpImm (f 3) (r 0) 1 .x, .cset (r 4) (f 3) .eq], .ret [r 4]⟩]⟩
private def staleFlags : Function := ⟨"flags", [
  ⟨0, [r 0], [.cmpImm (f 1) (r 0) 0 .x, .cset (r 2) (f 1) .eq,
    .cmpImm (f 3) (r 0) 1 .x, .cset (r 4) (f 1) .eq], .ret [r 4]⟩]⟩
private def flagsPlan : FunctionPlan := ⟨[0, 1, 2, 3, 4], [
  ⟨0, 0, [(0, r 0)], [some 0, some 1, some 2, some 3]⟩]⟩

example : checkStructure flagsBefore flagsBefore flagsPlan = true := by decide
example : checkStructure flagsBefore staleFlags flagsPlan = false := by decide

private def widthBefore : Function := ⟨"width", [
  ⟨0, [r 0], [.binary .add (r 1) (r 0) (r 0) .w], .ret [r 1]⟩]⟩
private def wrongWidth : Function := ⟨"width", [
  ⟨0, [r 0], [.binary .add (r 1) (r 0) (r 0) .x], .ret [r 1]⟩]⟩
private def widthPlan : FunctionPlan := ⟨[0, 1], [⟨0, 0, [(0, r 0)], [some 0]⟩]⟩

example : checkStructure widthBefore wrongWidth widthPlan = false := by decide

/-- A candidate cannot smuggle an unmatched effect into an inserted block. -/
private def extraEffect : Function := {criticalAfter with blocks := [
  ⟨0, [r 0], [], .cb true (r 0) 3 [] 1 []⟩,
  ⟨1, [], [.mov (r 1) (r 0), .mov (r 2) (r 1)], .b 2 []⟩,
  ⟨2, [], [], .ret [r 2]⟩,
  ⟨3, [], [.bl "hidden" [] [], .mov (r 2) (r 0)], .b 2 []⟩]}

example : checkStructure critical extraEffect criticalPlan = false := by decide

/-- Forwarded parameters are proposed correspondences, checked on both paths. -/
private def forwarded : Function := ⟨"joined", [
  ⟨0, [r 0], [], .cb true (r 0) 1 [] 2 []⟩,
  ⟨1, [], [], .ret [r 0]⟩,
  ⟨2, [], [], .ret [r 0]⟩]⟩
private def forwardedPlan : FunctionPlan := ⟨[0, 1, 2], [
  ⟨0, 0, [(0, r 0)], []⟩,
  ⟨1, 0, [(1, r 0)], []⟩,
  ⟨2, 0, [(2, r 0)], []⟩]⟩

example : checkStructure joined forwarded forwardedPlan = true := by decide

/-- Entry arguments are initial-call bindings only. A postSSA backedge must
execute its actual copies while retaining the mutable register bank. -/
private def entryBackedge : Function := ⟨"entry_loop", [
  ⟨0, [r 0], [.movImm (r 1) 0], .b 0 [r 1]⟩]⟩
private def entryBackedgeAfter : Function := ⟨"entry_loop", [
  ⟨0, [r 0], [.movImm (r 1) 0, .mov (r 0) (r 1)], .b 0 []⟩]⟩
private def entryBackedgePlan : FunctionPlan := ⟨[0, 1], [
  ⟨0, 0, [(0, r 0)], [some 0]⟩]⟩

example : checkStructure entryBackedge entryBackedgeAfter entryBackedgePlan = true := by decide
example : checkStructure entryBackedge
    ⟨"entry_loop", [⟨0, [r 0], [.movImm (r 1) 0], .b 0 []⟩]⟩
    entryBackedgePlan = false := by decide

/-- General arithmetic DCE has a structural demand row. Its concrete totality
must be discharged from typed reads or actual source progress, not this test. -/
example : checkStructure
    ⟨"dead_arithmetic", [⟨0, [r 0], [.binary .mul (r 1) (r 0) (r 0) .x], .brk 3002⟩]⟩
    ⟨"dead_arithmetic", [⟨0, [r 0], [], .brk 3002⟩]⟩
    ⟨[], [⟨0, 0, [], [none]⟩]⟩ = true := by decide

/-- An unchecked dead parameter correspondence is not accepted as an entry
equality and cannot justify an otherwise unrelated target operand read. -/
example : checkStructure
    ⟨"dead_binding", [⟨0, [r 0], [], .b 1 [r 0]⟩,
      ⟨1, [r 1], [], .brk 3002⟩]⟩
    ⟨"dead_binding", [⟨0, [r 0], [], .b 1 []⟩,
      ⟨1, [], [], .brk 3002⟩]⟩
    ⟨[], [⟨0, 0, [], []⟩, ⟨1, 0, [(1, r 9)], []⟩]⟩ = false := by decide

#print axioms matchOrigins_eval
#print axioms copyOrigins_sound
#print axioms copyOrigins_sound_normalized
#print axioms checkDeleted_sound

end EvoIR.Machine.Transport.Tests
