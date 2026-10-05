import EvoIR.Core.CallFrontierPair
import EvoIR.Core.CallLoopFixtures

/-! Byte-exact original exports for call/statement composition regressions. -/
namespace EvoIR.Core.Symbolic.CallClosureTests

private def initialized := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"initialized_loop_call",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"]],["compound",[["object",11,"y",["uint",false],"none",["store_scalar",["uint",false],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",10]]]]]],["return",["uint",false],["ident","uint",11]]]]]]]
"#

private def andCall := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[12],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",12,"and_loop_call",["int",false],false,[["parameter",13,["uint",false],"none"],["parameter",14,["uint",false],"none"]],["compound",[["return",["int",false],["binary","int","and",["call","uint",2,[["argument",["uint",false],["ident","uint",13]],["argument",["uint",false],["ident","uint",14]]]],["ident","uint",13]]]]]]]]
"#

private def orCall := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[15],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",15,"or_loop_call",["int",false],false,[["parameter",16,["uint",false],"none"],["parameter",17,["uint",false],"none"]],["compound",[["return",["int",false],["binary","int","or",["call","uint",2,[["argument",["uint",false],["ident","uint",16]],["argument",["uint",false],["ident","uint",17]]]],["ident","uint",17]]]]]]]]
"#

private def branchCall := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[18],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",18,"branch_loop_call",["uint",false],false,[["parameter",19,["uint",false],"none"],["parameter",20,["uint",false],"none"]],["compound",[["if",["ident","uint",19],["return",["uint",false],["call","uint",2,[["argument",["uint",false],["ident","uint",19]],["argument",["uint",false],["ident","uint",20]]]]],null],["return",["uint",false],["ident","uint",20]]]]]]]
"#

private def target := r#"func @call_tick(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 3
    %v4 = mul.bits<32> %v0, %v3
    %v5 = const.bits<32> 1
    %v6 = add.bits<32> %v4, %v5
    ret %v6, %v1, %v2
}

func @loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    br ^b1(%v0, %v1, %v2, %v3)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 0
    %v9 = icmp.ne %v5, %v8
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v4, %v6, %v7)

^b2(%v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    (%v14, %v15, %v16) = call @call_tick(%v10, %v12, %v13)
    %v17 = const.bits<32> 1
    %v18 = sub.bits<32> %v11, %v17
    br ^b1(%v14, %v18, %v15, %v16)

^b3(%v19: bits<32>, %v20: mem<all>, %v21: world):
    ret %v19, %v20, %v21
}

func @call_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 1
    %v8 = add.bits<32> %v4, %v7
    ret %v8, %v5, %v6
}

func @initialized_loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    ret %v4, %v5, %v6
}

func @and_loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 0
    %v8 = icmp.ne %v4, %v7
    cond_br %v8, ^b1(%v0, %v1, %v5, %v6), ^b2(%v0, %v1, %v5, %v6)

^b1(%v9: bits<32>, %v10: bits<32>, %v11: mem<all>, %v12: world):
    %v13 = const.bits<32> 0
    %v14 = icmp.ne %v9, %v13
    br ^b3(%v9, %v10, %v11, %v12, %v14)

^b2(%v15: bits<32>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = const.bits<1> 0
    br ^b3(%v15, %v16, %v17, %v18, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<1>):
    %v25 = zext.bits<32> %v24
    ret %v25, %v22, %v23
}

func @or_loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 0
    %v8 = icmp.ne %v4, %v7
    cond_br %v8, ^b2(%v0, %v1, %v5, %v6), ^b1(%v0, %v1, %v5, %v6)

^b1(%v9: bits<32>, %v10: bits<32>, %v11: mem<all>, %v12: world):
    %v13 = const.bits<32> 0
    %v14 = icmp.ne %v10, %v13
    br ^b3(%v9, %v10, %v11, %v12, %v14)

^b2(%v15: bits<32>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = const.bits<1> 1
    br ^b3(%v15, %v16, %v17, %v18, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<1>):
    %v25 = zext.bits<32> %v24
    ret %v25, %v22, %v23
}

func @branch_loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b1(%v0, %v1, %v2, %v3), ^b2(%v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    (%v10, %v11, %v12) = call @loop_call(%v6, %v7, %v8, %v9)
    ret %v10, %v11, %v12

^b2(%v13: bits<32>, %v14: mem<all>, %v15: world):
    ret %v13, %v14, %v15
}
"#

private def accepts (s t : String) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let plan := "[\"core-call-cutpoints-v1\",256,[[2," ++ Source.CallLoopTests.proposal ++ "]]]"
  let some (budget, rows) := readCallCutpoints source plan | return false
  return (callFrontierPair source target (fun id => rows.lookup id) budget).isSome

/-- Mutate only the selected function; unrelated helpers remain byte-identical. -/
private def mutateFunction (text name before after : String) : String := Id.run do
  let marker := s!"func @{name}("
  let [leading, suffix] := text.splitOn marker | return text
  let body :: rest := suffix.splitOn "\nfunc @" | return text
  return leading ++ marker ++ body.replace before after ++
    String.join (rest.map ("\nfunc @" ++ ·))

def callClosureSelfTest : IO UInt32 := do
  let pairs := [(initialized, "initialized_loop_call"), (andCall, "and_loop_call"),
    (orCall, "or_loop_call"), (branchCall, "branch_loop_call")]
  let mut failed := 0
  let mut executions := 0
  for (source, name) in pairs do
    if !accepts source target then
      IO.eprintln s!"original call closure refused: {name}"
      failed := failed+1
    if !accepts (source.replace name "unlisted_closure") (target.replace name "unlisted_closure") then
      IO.eprintln s!"renamed call closure refused: {name}"
      failed := failed+1
    let .ok s := Source.readProgram source | return 1
    let .ok t := IR.Reader.readProgram target | return 1
    let some sf := s.functions.find? (·.name == name) | return 1
    for (x, n, result) in [(0, 0, 0), (0, 3, 13), (7, 0, 7), (1, 3, 40),
        (0xffffffff, 1, 0xfffffffe)] do
      let expected := if name == "and_loop_call" then
          (if result != 0 && x != 0 then 1 else 0)
        else if name == "or_loop_call" then (if result != 0 || n != 0 then 1 else 0)
        else if name == "branch_loop_call" then (if x == 0 then n else result)
        else result
      let args := [Source.Value.ofNat .uint x, Source.Value.ofNat .uint n]
      let .ok se := Source.initial s sf.symbol args | return 1
      let .ok te := IR.initial t name (loopInputValues true args) | return 1
      if observeSource (Source.runSteps s 10000 se) != some (.returned sf.returnType.type expected) ||
          observeTarget sf.returnType.type (IR.runSteps t 10000 te) !=
            some (.returned sf.returnType.type expected) then
        IO.eprintln s!"call closure execution mismatch: {name}"
        failed := failed+1
      executions := executions+1
  let mutations := [
    (initialized, "initialized_loop_call", "ret %v4, %v5, %v6", "ret %v0, %v5, %v6"),
    (initialized, "initialized_loop_call", "ret %v4, %v5, %v6", "ret %v4, %v2, %v3"),
    (initialized, "initialized_loop_call", "call @loop_call(%v0, %v1,", "call @loop_call(%v1, %v0,"),
    (andCall, "and_loop_call", "icmp.ne %v4, %v7", "icmp.eq %v4, %v7"),
    (andCall, "and_loop_call", "const.bits<1> 0", "const.bits<1> 1"),
    (andCall, "and_loop_call", "icmp.ne %v9, %v13", "icmp.ne %v10, %v13"),
    (andCall, "and_loop_call", "zext.bits<32> %v24", "sext.bits<32> %v24"),
    (orCall, "or_loop_call", "icmp.ne %v4, %v7", "icmp.eq %v4, %v7"),
    (orCall, "or_loop_call", "const.bits<1> 1", "const.bits<1> 0"),
    (orCall, "or_loop_call", "icmp.ne %v10, %v13", "icmp.ne %v9, %v13"),
    (branchCall, "branch_loop_call", "icmp.ne %v0, %v4", "icmp.eq %v0, %v4"),
    (branchCall, "branch_loop_call", "^b2(%v1, %v2, %v3)", "^b2(%v0, %v2, %v3)"),
    (branchCall, "branch_loop_call", "call @loop_call(%v6, %v7,", "call @loop_call(%v7, %v6,"),
    (branchCall, "branch_loop_call", "ret %v10, %v11, %v12", "ret %v6, %v11, %v12")]
  for (source, name, before, after) in mutations do
    let changed := mutateFunction target name before after
    if changed == target || accepts source changed then
      IO.eprintln s!"call closure mutation admitted: {name}: {before}"
      failed := failed+1
  let sourceMutations := [
    ("[\"object\",11,\"y\"", "[\"object\",9,\"y\""),
    ("[\"return\",[\"uint\",false],[\"ident\",\"uint\",11]]",
      "[\"return\",[\"uint\",false],[\"ident\",\"uint\",9]]"),
    ("[\"store_scalar\",[\"uint\",false]", "[\"store_scalar\",[\"int\",false]")]
  for (before, after) in sourceMutations do
    let changed := initialized.replace before after
    if changed == initialized || accepts changed target then
      IO.eprintln s!"initializer source mutation admitted: {before}"
      failed := failed+1
  if failed == 0 then
    IO.println s!"{pairs.length * 2} original/renamed call-closure pairs, {mutations.length + sourceMutations.length} mutations and {executions} executions pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.CallClosureTests
