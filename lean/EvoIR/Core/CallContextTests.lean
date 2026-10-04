import EvoIR.Core.CallFrontierPair
import EvoIR.Core.CallContextFixtures

/-! Original cyclic calls followed by conditional choice and assignment.
Admission constructs a proof for all entry bits; concrete executions are
additional regressions, not premises of that proof. -/
namespace EvoIR.Core.Symbolic.CallContextTests

private def accepts (s t : String) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let plan := "[\"core-call-cutpoints-v1\",256,[[2," ++ Source.CallLoopTests.proposal ++ "]]]"
  let some (budget, rows) := readCallCutpoints source plan | return false
  return (callFrontierPair source target (fun id => rows.lookup id) budget).isSome

def callContextSelfTest : IO UInt32 := do
  let cases := [(CallContextFixtures.conditionalCall, "conditional_call", 11),
    (CallContextFixtures.assignedCall, "assigned_call", 14),
    (CallContextFixtures.rightCall, "right_call", 8),
    (CallContextFixtures.nestedCall, "nested_call", 17)]
  let target := CallContextFixtures.target
  let mut failed := 0
  let mut executions := 0
  for (source, name, id) in cases do
    if !accepts source target then
      IO.eprintln s!"original {name} pair refused"
      failed := failed + 1
    if !accepts (source.replace name "unlisted_context") (target.replace name "unlisted_context") then
      IO.eprintln s!"renamed {name} pair refused"
      failed := failed + 1
    let .ok s := Source.readProgram source | return 1
    let .ok t := IR.Reader.readProgram target | return 1
    for (x, n, result) in [(0, 0, 0), (7, 0, 7), (1, 3, 40), (0xffffffff, 1, 0xfffffffe)] do
      let expected := if name == "conditional_call" then (if result == 0 then n else x)
        else if name == "right_call" then (x + result) % 2^32
        else if name == "nested_call" then (3 * result + 1) % 2^32 else result
      let args := [Source.Value.ofNat .uint x, Source.Value.ofNat .uint n]
      let .ok se := Source.initial s id args | return 1
      let .ok te := IR.initial t name (loopInputValues true args) | return 1
      if observeSource (Source.runSteps s 10000 se) != some (.returned .uint expected) ||
          observeTarget .uint (IR.runSteps t 10000 te) != some (.returned .uint expected) then
        IO.eprintln s!"{name} source/IR execution mismatch"
        failed := failed + 1
      executions := executions + 1
  let mutations := [
    (CallContextFixtures.nestedCall, "call @loop_call(%v4, %v7, %v5, %v6)",
      "call @loop_call(%v0, %v7, %v5, %v6)"),
    (CallContextFixtures.nestedCall, "call @loop_call(%v4, %v7, %v5, %v6)",
      "call @loop_call(%v4, %v1, %v5, %v6)"),
    (CallContextFixtures.nestedCall, "ret %v8, %v9, %v10", "ret %v4, %v9, %v10"),
    (CallContextFixtures.nestedCall, "call @loop_call(%v4, %v7, %v5, %v6)",
      "call @loop_call(%v4, %v7, %v2, %v3)"),
    (CallContextFixtures.rightCall, "add.bits<32> %v0, %v4", "add.bits<32> %v1, %v4"),
    (CallContextFixtures.rightCall, "add.bits<32> %v0, %v4", "add.bits<32> %v4, %v4"),
    (CallContextFixtures.rightCall, "add.bits<32> %v0, %v4", "sub.bits<32> %v0, %v4"),
    (CallContextFixtures.conditionalCall, "icmp.ne %v4, %v7", "icmp.eq %v4, %v7"),
    (CallContextFixtures.conditionalCall, "^b3(%v9, %v10, %v11, %v12, %v9)",
      "^b3(%v9, %v10, %v11, %v12, %v10)"),
    (CallContextFixtures.conditionalCall, "^b3(%v13, %v14, %v15, %v16, %v14)",
      "^b3(%v13, %v14, %v15, %v16, %v13)"),
    (CallContextFixtures.conditionalCall, "ret %v21, %v19, %v20", "ret %v17, %v19, %v20"),
    (CallContextFixtures.assignedCall, "ret %v4, %v5, %v6", "ret %v0, %v5, %v6"),
    (CallContextFixtures.assignedCall, "ret %v4, %v5, %v6", "ret %v1, %v5, %v6"),
    (CallContextFixtures.assignedCall, "call @loop_call(%v0, %v1, %v2, %v3)",
      "call @loop_call(%v1, %v0, %v2, %v3)"),
    (CallContextFixtures.assignedCall, "ret %v4, %v5, %v6", "ret %v4, %v2, %v3")]
  for (source, before, after) in mutations do
    let changed := target.replace before after
    if changed == target || accepts source changed then
      IO.eprintln s!"call continuation target mutation admitted: {before}"
      failed := failed + 1
  let sourceMutations := [
    ("[\"assign\",\"uint\",\"set\",15,", "[\"assign\",\"uint\",\"set\",16,"),
    ("[\"assign\",\"uint\",\"set\",15,", "[\"assign\",\"uint\",\"add\",15,"),
    ("[\"return\",[\"uint\",false],[\"ident\",\"uint\",15]]",
      "[\"return\",[\"uint\",false],[\"ident\",\"uint\",16]]")]
  for (before, after) in sourceMutations do
    let changed := CallContextFixtures.assignedCall.replace before after
    if changed == CallContextFixtures.assignedCall || accepts changed target then
      IO.eprintln s!"call continuation source mutation admitted: {before}"
      failed := failed + 1
  if failed == 0 then
    IO.println s!"{cases.length * 2} original/renamed call-context pairs, {mutations.length + sourceMutations.length} mutations and {executions} executions pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.CallContextTests
