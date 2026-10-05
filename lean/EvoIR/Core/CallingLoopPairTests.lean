import EvoIR.Core.CallingLoopPair
import EvoIR.Core.CallLoopFixtures

/-! The original cyclic caller/callee pair, independently malformed associations
and program mutations, and nonterminating variants. Execution samples supplement,
but never replace, the unbounded pair theorem produced by admission. -/
namespace EvoIR.Core.Symbolic.CallingLoopPairTests

private def source := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"loop_calls_loop",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"],["parameter",11,["uint",false],"none"]],["compound",[["while",["ident","uint",10],["compound",[["expr",["assign","uint","set",9,["uint",false],["ident","uint",9],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",11]]]]]],["expr",["update","uint","pre_dec",10,["uint",false],["ident","uint",10]]]]]],["return",["uint",false],["ident","uint",9]]]]]]]
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

func @loop_calls_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>, %v3: mem<all>, %v4: world):
    br ^b1(%v0, %v1, %v2, %v3, %v4)

^b1(%v5: bits<32>, %v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    %v10 = const.bits<32> 0
    %v11 = icmp.ne %v6, %v10
    cond_br %v11, ^b2(%v5, %v6, %v7, %v8, %v9), ^b3(%v5, %v8, %v9)

^b2(%v12: bits<32>, %v13: bits<32>, %v14: bits<32>, %v15: mem<all>, %v16: world):
    (%v17, %v18, %v19) = call @loop_call(%v12, %v14, %v15, %v16)
    %v20 = const.bits<32> 1
    %v21 = sub.bits<32> %v13, %v20
    br ^b1(%v17, %v21, %v14, %v18, %v19)

^b3(%v22: bits<32>, %v23: mem<all>, %v24: world):
    ret %v22, %v23, %v24
}
"#
private def plan := r#"["core-call-cutpoints-v1",256,[
  [2,["core-cutpoints-v2",256,[
    [0,"entry",[[3,"uint",false,true],[4,"uint",false,true]],[[1,4,"uint"],[0,3,"uint"]],[[3,"world"],[2,"mem"]]],
    [2,"b1",[[3,"uint",false,true],[4,"uint",false,true]],[[5,4,"uint"],[4,3,"uint"]],[[7,"world"],[6,"mem"]]]
  ]]],
  [8,["core-cutpoints-v2",256,[
    [0,"entry",[[9,"uint",false,true],[10,"uint",false,true],[11,"uint",false,true]],[[2,11,"uint"],[1,10,"uint"],[0,9,"uint"]],[[4,"world"],[3,"mem"]]],
    [2,"b1",[[9,"uint",false,true],[10,"uint",false,true],[11,"uint",false,true]],[[7,11,"uint"],[6,10,"uint"],[5,9,"uint"]],[[9,"world"],[8,"mem"]]]
  ]]]
]]
"#

private def accepts (s t : String) (proposal : String := plan) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let some (budget, rows) := readCallCutpoints source proposal | return false
  return (callingLoopPair source target (fun id => rows.lookup id) budget).isSome

private def mutateFunction (text name before after : String) : String := Id.run do
  let [leading, suffix] := text.splitOn s!"func @{name}(" | return text
  let body :: rest := suffix.splitOn "\nfunc @" | return text
  return leading ++ s!"func @{name}(" ++ body.replace before after ++
    String.join (rest.map ("\nfunc @" ++ ·))

def callingLoopPairSelfTest : IO UInt32 := do
  let mut failed := 0
  let mut negatives := 0
  let mut executions := 0
  if !accepts source target ||
      !accepts (source.replace "loop_calls_loop" "renamed_caller") (target.replace "loop_calls_loop" "renamed_caller") then
    IO.eprintln "original/renamed cyclic caller/callee refused"
    failed := failed+1
  let changes := [
    ("loop_calls_loop", "sub.bits<32> %v13, %v20", "add.bits<32> %v13, %v20"),
    ("loop_calls_loop", "br ^b1(%v17, %v21, %v14, %v18, %v19)", "br ^b1(%v12, %v21, %v14, %v18, %v19)"),
    ("loop_calls_loop", "br ^b1(%v17, %v21, %v14, %v18, %v19)", "br ^b1(%v17, %v13, %v14, %v18, %v19)"),
    ("loop_calls_loop", "br ^b1(%v17, %v21, %v14, %v18, %v19)", "br ^b1(%v17, %v21, %v13, %v18, %v19)"),
    ("loop_calls_loop", "br ^b1(%v17, %v21, %v14, %v18, %v19)", "br ^b1(%v17, %v21, %v14, %v15, %v16)"),
    ("loop_calls_loop", "call @loop_call(%v12, %v14,", "call @loop_call(%v12, %v13,"),
    ("loop_calls_loop", "call @loop_call(%v12, %v14,", "call @loop_call(%v14, %v12,"),
    ("loop_calls_loop", "icmp.ne %v6, %v10", "icmp.eq %v6, %v10"),
    ("loop_calls_loop", "^b3(%v5, %v8, %v9)", "^b3(%v6, %v8, %v9)"),
    ("loop_calls_loop", "const.bits<32> 1", "const.bits<32> 2"),
    ("loop_calls_loop", "io=false", "io=true"),
    ("loop_calls_loop", "br ^b1(%v0, %v1, %v2,", "br ^b1(%v0, %v2, %v1,"),
    ("loop_call", "sub.bits<32> %v11, %v17", "add.bits<32> %v11, %v17"),
    ("loop_call", "ret %v19, %v20, %v21", "ret %v19, %v21, %v20"),
    ("call_tick", "mul.bits<32> %v0, %v3", "sub.bits<32> %v0, %v3")]
  for (name, before, after) in changes do
    let changed := mutateFunction target name before after
    if changed == target || accepts source changed then
      IO.eprintln s!"cyclic caller mutation admitted: {name} {before}"
      failed := failed+1
    negatives := negatives+1
  let wrongPlans := [
    plan.replace "[7,11,\"uint\"]" "[7,10,\"uint\"]",
    plan.replace "[6,10,\"uint\"]" "[6,11,\"uint\"]",
    plan.replace "[5,9,\"uint\"]" "[5,10,\"uint\"]",
    plan.replace "[9,\"world\"]" "[9,\"mem\"]",
    plan.replace "[11,\"uint\",false,true]" "[11,\"uint\",false,false]",
    plan.replace "[2,\"b1\",[[9," "[0,\"b1\",[[9,",
    plan.replace "[2,\"b1\",[[3," "[0,\"b1\",[[3,",
    plan.replace "\"core-cutpoints-v2\",256" "\"core-cutpoints-v2\",0",
    "[\"core-call-cutpoints-v1\",256,[]]"]
  for changed in wrongPlans do
    if changed == plan || accepts source target changed then
      IO.eprintln "forged cyclic caller/callee plan admitted"
      failed := failed+1
    negatives := negatives+1
  let .ok s := Source.readProgram source | return 1
  let .ok t := IR.Reader.readProgram target | return 1
  for x in [0, 7, 0xffffffff] do
    for n in [0, 1, 3] do
      for m in [0, 1, 3] do
        let mut result := x
        for _ in [:n*m] do result := (result*3+1) % 2^32
        let args := [Source.Value.ofNat .uint x, Source.Value.ofNat .uint n, Source.Value.ofNat .uint m]
        let .ok se := Source.initial s 8 args | return 1
        let .ok te := IR.initial t "loop_calls_loop" (loopInputValues true args) | return 1
        let tf := IR.runSteps t 20000 te
        if observeSource (Source.runSteps s 20000 se) != some (.returned .uint result) ||
            observeTarget .uint tf != some (.returned .uint result) || tf.nextFrame != 2+n*(m+1) then
          IO.eprintln s!"cyclic caller execution/frame mismatch: {x} {n} {m}"
          failed := failed+1
        executions := executions+1
  -- A matched nonterminating inner or outer loop is valid correspondence.
  -- In contrast, the one-sided counter changes above must always refuse.
  for (id, name, removed, oldCounter, newCounter) in [
      (4, "loop_call", "    %v18 = sub.bits<32> %v11, %v17\n", "%v18", "%v11"),
      (10, "loop_calls_loop", "    %v21 = sub.bits<32> %v13, %v20\n", "%v21", "%v13")] do
    let before := s!"[\"update\",\"uint\",\"pre_dec\",{id},[\"uint\",false],[\"ident\",\"uint\",{id}]]"
    let ss := source.replace before "[\"lit\",\"uint\",\"00000000\"]"
    let tt := mutateFunction (mutateFunction target name removed "") name oldCounter newCounter
    if ss == source || tt == target || !accepts ss tt then
      IO.eprintln s!"nonterminating cyclic pair refused: {name}"
      failed := failed+1
  if failed == 0 then
    IO.println s!"2 original/renamed cyclic caller pairs, {negatives} refusals, {executions} execution/frame controls and 2 unbounded divergent variants pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.CallingLoopPairTests
