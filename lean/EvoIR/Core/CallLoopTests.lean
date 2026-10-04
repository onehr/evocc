import EvoIR.Core.CallLoopFixtures

/-! Calls inside cyclic spans: complete pairs, exact diagnostic order,
frame-counter regression controls and independent input/proposal mutations. -/
namespace EvoIR.Core.Source.CallLoopTests

private def accepted (source ir plan : String) : Bool := Id.run do
  let .ok s := readProgram source | return false
  let .ok t := IR.Reader.readProgram ir | return false
  let some f := s.functions.find? (s.entries.contains ·.symbol) | return false
  let some (budget, layouts) := Symbolic.readCutpoints f plan | return false
  return (Symbolic.loopPair s t layouts budget).isSome

private def executes (source ir : String) (numbers : List Nat) (expected : Result)
    (owner : String) (frames : Nat) : Bool := Id.run do
  let .ok s := readProgram source | return false
  let .ok t := IR.Reader.readProgram ir | return false
  let some f := s.functions.find? (s.entries.contains ·.symbol) | return false
  let some expectedOwner := s.functions.find? (·.name == owner) | return false
  if numbers.length != f.parameters.length then return false
  let args := (f.parameters.zip numbers).map fun (p, n) => Value.ofNat p.type.type n
  let .ok start := initial s f.symbol args | return false
  let last := runSteps s 20000 start
  let .halted actual := last.control | return false
  if actual != expected || last.functionId != expectedOwner.symbol then return false
  let .ok begin := IR.initial t f.name (Symbolic.loopInputValues true args) | return false
  let expectedIR := expected.map fun value =>
    [Symbolic.sourceValueIR value, IR.Value.memory ⟨1, []⟩, IR.Value.world]
  let finished := IR.runSteps t 20000 begin
  let .halted actualIR := finished.control | return false
  return actualIR == expectedIR && finished.nextFrame == frames

def callLoopSelfTest : IO UInt32 := do
  let pairs := [(loopCall, target, proposal), (loopBranch, controlTarget, proposal),
    (loopChecked, controlTarget, checkedProposal), (loopForever, controlTarget, foreverProposal)]
  let mut failures := 0
  for (source, ir, plan) in pairs do
    if !accepted source ir plan then
      IO.eprintln "original call-in-loop pair refused"
      failures := failures+1
  let cases : List (String × String × List Nat × Result × String × Nat) := [
    (loopCall, target, [1, 0], .ok (.ofNat .uint 1), "loop_call", 2),
    (loopCall, target, [1, 3], .ok (.ofNat .uint 40), "loop_call", 5),
    (loopCall, target, [0xffffffff, 1], .ok (.ofNat .uint 0xfffffffe), "loop_call", 3),
    (loopBranch, controlTarget, [0, 1], .ok (.ofNat .uint 3), "loop_branch", 3),
    (loopBranch, controlTarget, [2, 3], .ok (.ofNat .uint 3), "loop_branch", 5),
    (loopBranch, controlTarget, [0, 20], .ok (.ofNat .uint 0), "loop_branch", 22),
    (loopChecked, controlTarget, [0x7fffffff, 0], .ok (.ofNat .int 0x7fffffff), "loop_checked", 2),
    (loopChecked, controlTarget, [0x7fffffff, 1], .error (.diagnostic 3004), "loop_checked_tick", 3),
    (loopChecked, controlTarget, [0x7ffffffe, 2], .error (.diagnostic 3004), "loop_checked_tick", 4),
    (loopChecked, controlTarget, [0x80000000, 3], .ok (.ofNat .int 0x80000003), "loop_checked", 5)]
  for (source, ir, args, expected, owner, frames) in cases do
    if !executes source ir args expected owner frames then
      IO.eprintln s!"call-in-loop execution/frame mismatch: {owner} {args}"
      failures := failures+1
  let mutations := [
    (loopCall, target, proposal, "mul.bits<32> %v0, %v3", "add.bits<32> %v0, %v3"),
    (loopCall, target, proposal, "call @call_tick(%v10, %v12, %v13)", "call @call_tick(%v11, %v12, %v13)"),
    (loopCall, target, proposal, "call @call_tick", "call @missing"),
    (loopCall, target, proposal, "br ^b1(%v14, %v18, %v15, %v16)", "br ^b1(%v18, %v14, %v15, %v16)"),
    (loopCall, target, proposal, "br ^b1(%v14, %v18, %v15, %v16)", "br ^b1(%v14, %v11, %v15, %v16)"),
    (loopCall, target, proposal, "br ^b1(%v14, %v18, %v15, %v16)", "br ^b1(%v14, %v18, %v12, %v13)"),
    (loopCall, target, proposal, "ret %v19, %v20, %v21", "ret %v19, %v21, %v20"),
    (loopCall, target, proposal, "ret %v6, %v1, %v2", "ret %v0, %v1, %v2"),
    (loopCall, target, proposal, "icmp.ne %v5, %v8", "icmp.eq %v5, %v8"),
    (loopCall, target, proposal, "%v14, %v15, %v16", "%v10, %v15, %v16"),
    (loopChecked, controlTarget, checkedProposal, "    ub.assume.signed_overflow %v4\n", ""),
    (loopChecked, controlTarget, checkedProposal, "ub.assume.signed_overflow %v4", "ub.assume.division_by_zero %v4"),
    (loopChecked, controlTarget, checkedProposal, "may_trap=true", "may_trap=false"),
    (loopBranch, controlTarget, proposal, "const.bits<32> 3", "const.bits<32> 4"),
    (loopForever, controlTarget, foreverProposal, "call @loop_branch_tick(%v9, %v10, %v11)",
      "call @loop_branch_tick(%v10, %v9, %v11)")]
  for (source, ir, plan, before, after) in mutations do
    let changed := ir.replace before after
    if changed == ir || accepted source changed plan then
      IO.eprintln s!"call-in-loop target mutation escaped: {before}"
      failures := failures+1
  let plans := [
    ("[7,\"world\"]", "[7,\"mem\"]"),
    ("[6,\"mem\"]", "[6,\"world\"]"),
    ("[[7,\"world\"],[6,\"mem\"]]", "[]"),
    ("[5,4,\"uint\"]", "[5,3,\"uint\"]"),
    ("[4,3,\"uint\"]", "[4,3,\"int\"]"),
    ("[2,\"b1\"", "[0,\"b1\""),
    ("[4,\"uint\",false,true]", "[4,\"uint\",false,false]"),
    ("[7,\"world\"]", "[7,\"poison\"]"),
    ("core-cutpoints-v2", "core-cutpoints-v1"),
    (",256,", ",1,"), (",256,", ",0,")]
  for (before, after) in plans do
    let changed := proposal.replace before after
    if changed == proposal || accepted loopCall target changed then
      IO.eprintln s!"call-in-loop proposal mutation escaped: {before}"
      failures := failures+1
  let sources := [
    ("\"mul\"", "\"add\""),
    ("\"pre_dec\"", "\"pre_inc\""),
    ("\"call\",\"uint\",0", "\"call\",\"uint\",999")]
  for (before, after) in sources do
    let changed := loopCall.replace before after
    if changed == loopCall || accepted changed target proposal then
      IO.eprintln s!"call-in-loop source mutation escaped: {before}"
      failures := failures+1
  -- A finite callee diagram cannot certify a cyclic callee by spending fuel.
  if accepted callLoop target callerProposal then
    IO.eprintln "call-to-loop boundary admitted without a continuation proof"
    failures := failures+1
  if failures == 0 then
    IO.println s!"{pairs.length} call-in-loop pairs, {cases.length} executions, {mutations.length} target mutations, {plans.length} proposal mutations and {sources.length} source mutations pass; call-to-loop remains refused"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Source.CallLoopTests
