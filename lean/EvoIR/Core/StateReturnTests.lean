import EvoIR.Core.ReturnCorrespondence
import EvoIR.Core.StateExpressionTests

/-! Actual exports of tests/fixtures/verification/stateful-return.c. Mutations
change observable assignment, control or diagnostics, not private dead locals. -/
namespace EvoIR.Core.Symbolic.StateReturnTests

private def source := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[0],[["function",0,"stateful_return",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["int",false],"none"]],["compound",[["return",["int",false],["comma","int",["assign","int","set",1,["int",false],["ident","int",1],["ident","int",2]],["conditional","int",["ident","int",1],["binary","int","add",["ident","int",1],["lit","int","00000001"]],["binary","int","sub",["ident","int",2],["lit","int","00000001"]]]]]]]]]]"#

private def target := "func @stateful_return(%arg0: bits<32>, %arg1: bits<32>) -> (bits<32>) cc = target_default\n" ++
  "effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }\n{\n" ++
  "^entry(%v0: bits<32>, %v1: bits<32>):\n%v2 = const.bits<32> 0\n%v3 = icmp.ne %v1, %v2\n" ++
  "cond_br %v3, ^b1(%v1, %v1), ^b2(%v1, %v1)\n" ++
  "^b1(%v4: bits<32>, %v5: bits<32>):\n%v6 = const.bits<32> 1\n" ++
  "%v7 = signed_add_defined %v4, %v6\nub.assume.signed_overflow %v7\n" ++
  "%v8 = add.bits<32> %v4, %v6\nbr ^b3(%v4, %v5, %v8)\n" ++
  "^b2(%v9: bits<32>, %v10: bits<32>):\n%v11 = const.bits<32> 1\n" ++
  "%v12 = signed_sub_defined %v10, %v11\nub.assume.signed_overflow %v12\n" ++
  "%v13 = sub.bits<32> %v10, %v11\nbr ^b3(%v9, %v10, %v13)\n" ++
  "^b3(%v14: bits<32>, %v15: bits<32>, %v16: bits<32>):\nret %v16\n}\n"

private def accepts (s t : String) : Bool :=
  match Source.readProgram s, IR.Reader.readProgram t with
  | .ok s, .ok t => (returnPair s t).isSome
  | _, _ => false

def stateReturnSelfTest : IO UInt32 := do
  let mut failures := 0
  if !accepts source target then
    IO.eprintln "actual stateful whole-function pair refused"
    failures := failures+1
  let mutations := [
    ("lost assignment test", "icmp.ne %v1, %v2", "icmp.ne %v0, %v2"),
    ("lost assignment edge", "^b1(%v1, %v1)", "^b1(%v0, %v1)"),
    ("wrong arithmetic", "add.bits<32>", "sub.bits<32>"),
    ("wrong join result", "ret %v16", "ret %v14"),
    ("lost guard", "ub.assume.signed_overflow %v7\n", ""),
    ("wrong diagnostic", "ub.assume.signed_overflow %v7", "ub.assume.invalid_shift_count %v7"),
    ("wrong effect", "may_trap=true", "may_trap=false"),
    ("wrong branch", "^b1(%v1, %v1), ^b2(%v1, %v1)", "^b2(%v1, %v1), ^b1(%v1, %v1)")]
  for (name, before, after) in mutations do
    if accepts source (target.replace before after) then
      IO.eprintln s!"stateful whole-function mutation accepted: {name}"
      failures := failures+1
  let changes := [
    ("rhs binding", "[\"ident\",\"int\",1],[\"ident\",\"int\",2]", "[\"ident\",\"int\",1],[\"ident\",\"int\",1]"),
    ("const parameter", "[\"parameter\",1,[\"int\",false]", "[\"parameter\",1,[\"int\",true]"),
    ("source branch", "[\"binary\",\"int\",\"add\"", "[\"binary\",\"int\",\"sub\"")]
  for (name, before, after) in changes do
    let changed := source.replace before after
    if changed == source || accepts changed target then
      IO.eprintln s!"stateful source mutation not refused: {name}"
      failures := failures+1
  if failures == 0 then IO.println s!"{1+mutations.length+changes.length} stateful whole-return cases pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Symbolic.StateReturnTests
