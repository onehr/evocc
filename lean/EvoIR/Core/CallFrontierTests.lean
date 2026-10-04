import EvoIR.Core.CallFrontierPair
import EvoIR.Core.CallLoopFixtures

/-! Whole-entry cyclic-callee admission and independent source, call-site,
callee and continuation mutations. The original source and IR are unchanged. -/
namespace EvoIR.Core.Symbolic.CallFrontierTests

private def closurePlan (plan : String := Source.CallLoopTests.proposal) : String :=
  "[\"core-call-cutpoints-v1\",256,[[2," ++ plan ++ "]]]"

private def accepts (s t plan : String) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let some (budget, rows) := readCallCutpoints source plan | return false
  return (callFrontierPair source target (fun id => rows.lookup id) budget).isSome

def callFrontierSelfTest : IO UInt32 := do
  let source := Source.CallLoopTests.callLoop
  let target := Source.CallLoopTests.target
  let plan := closurePlan
  let mut failed := 0
  if !accepts source target plan then
    IO.eprintln "original call_loop whole entry refused"
    failed := failed+1
  let renamedSource := source.replace "call_loop" "outer_unlisted" |>.replace "loop_call" "inner_unlisted"
  let renamedTarget := target.replace "call_loop" "outer_unlisted" |>.replace "loop_call" "inner_unlisted"
  if !accepts renamedSource renamedTarget plan then
    IO.eprintln "renamed whole caller pair refused"
    failed := failed+1
  let beforeCall := "    (%v4, %v5, %v6) = call @loop_call"
  let withPrefix := target.replace beforeCall
    ("    %v90 = const.bits<32> 123\n" ++ beforeCall)
  if !accepts source withPrefix plan then
    IO.eprintln "nonempty target prefix refused"
    failed := failed+1
  let targetMutations := [
    ("call @loop_call(%v0, %v1, %v2, %v3)", "call @loop_call(%v1, %v0, %v2, %v3)"),
    ("call @loop_call(%v0, %v1, %v2, %v3)", "call @loop_call(%v0, %v0, %v2, %v3)"),
    ("call @loop_call(%v0, %v1, %v2, %v3)", "call @loop_call(%v0, %v1, %v3, %v2)"),
    ("call @loop_call", "call @missing"),
    ("%v7 = const.bits<32> 1", "%v7 = const.bits<32> 2"),
    ("add.bits<32> %v4, %v7", "add.bits<32> %v0, %v7"),
    ("ret %v8, %v5, %v6", "ret %v4, %v5, %v6"),
    ("mul.bits<32> %v0, %v3", "add.bits<32> %v0, %v3"),
    ("icmp.ne %v5, %v8", "icmp.eq %v5, %v8"),
    ("sub.bits<32> %v11, %v17", "add.bits<32> %v11, %v17"),
    ("br ^b1(%v14, %v18, %v15, %v16)", "br ^b1(%v14, %v11, %v15, %v16)")]
  for (before, after) in targetMutations do
    let changed := target.replace before after
    if changed == target || accepts source changed plan then
      IO.eprintln s!"whole caller target mutation admitted: {before}"
      failed := failed+1
  let sourceMutations := [
    ("[\"call\",\"uint\",2,", "[\"call\",\"uint\",0,"),
    ("[\"argument\",[\"uint\",false],[\"ident\",\"uint\",7]]", "[\"argument\",[\"uint\",false],[\"ident\",\"uint\",6]]"),
    ("[\"call\",\"uint\",2,", "[\"call\",\"int\",2,")]
  for (before, after) in sourceMutations do
    let changed := source.replace before after
    if changed == source || accepts changed target plan then
      IO.eprintln s!"whole caller source mutation admitted: {before}"
      failed := failed+1
  let plans := [
    "[\"core-call-cutpoints-v1\",256,[]]",
    plan.replace "[[2," "[[5,",
    plan.replace "[[2," "[[99,",
    plan.replace "[2,\"b1\"" "[0,\"b1\"",
    plan.replace "[5,4,\"uint\"]" "[5,3,\"uint\"]",
    "[\"core-call-cutpoints-v1\",256,[[2," ++ Source.CallLoopTests.proposal ++
      "],[2," ++ Source.CallLoopTests.proposal ++ "]]]",
    closurePlan Source.CallLoopTests.callerProposal]
  for changed in plans do
    if changed == plan || accepts source target changed then
      IO.eprintln "missing, duplicate or forged callee proposal admitted"
      failed := failed+1
  let .ok s := Source.readProgram source | return 1
  let .ok t := IR.Reader.readProgram target | return 1
  for (x, n, expected) in [(1, 0, 2), (1, 3, 41), (0xffffffff, 1, 0xffffffff)] do
    let args := [Source.Value.ofNat .uint x, Source.Value.ofNat .uint n]
    let .ok se := Source.initial s 5 args | return 1
    let .ok te := IR.initial t "call_loop" (loopInputValues true args) | return 1
    let sf := Source.runSteps s 10000 se
    let tf := IR.runSteps t 10000 te
    if observeSource sf != some (.returned .uint expected) ||
        observeTarget .uint tf != some (.returned .uint expected) || tf.nextFrame != n+3 then
      IO.eprintln "whole caller execution/frame mismatch"
      failed := failed+1
  if failed == 0 then
    IO.println s!"3 whole caller-frontier pairs, {targetMutations.length+sourceMutations.length+plans.length} mutations and 3 execution/frame controls pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.CallFrontierTests
