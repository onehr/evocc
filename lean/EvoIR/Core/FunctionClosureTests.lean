import EvoIR.Core.CallingLoopPair
import EvoIR.Core.CallLoopFixtures

/-! Actual multi-level source exports, caller-state mutations and matching
nonterminating bodies. All contexts use the same function-closure builder. -/
namespace EvoIR.Core.Symbolic.FunctionClosureTests

private def plain := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[12],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"loop_calls_loop",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"],["parameter",11,["uint",false],"none"]],["compound",[["while",["ident","uint",10],["compound",[["expr",["assign","uint","set",9,["uint",false],["ident","uint",9],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",11]]]]]],["expr",["update","uint","pre_dec",10,["uint",false],["ident","uint",10]]]]]],["return",["uint",false],["ident","uint",9]]]]],["function",12,"call_nested_loop",["uint",false],false,[["parameter",13,["uint",false],"none"],["parameter",14,["uint",false],"none"],["parameter",15,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["call","uint",8,[["argument",["uint",false],["ident","uint",13]],["argument",["uint",false],["ident","uint",14]],["argument",["uint",false],["ident","uint",15]]]],["lit","uint","00000001"]]]]]]]]
"#

private def saved := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[16],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"loop_calls_loop",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"],["parameter",11,["uint",false],"none"]],["compound",[["while",["ident","uint",10],["compound",[["expr",["assign","uint","set",9,["uint",false],["ident","uint",9],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",11]]]]]],["expr",["update","uint","pre_dec",10,["uint",false],["ident","uint",10]]]]]],["return",["uint",false],["ident","uint",9]]]]],["function",12,"call_nested_loop",["uint",false],false,[["parameter",13,["uint",false],"none"],["parameter",14,["uint",false],"none"],["parameter",15,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["call","uint",8,[["argument",["uint",false],["ident","uint",13]],["argument",["uint",false],["ident","uint",14]],["argument",["uint",false],["ident","uint",15]]]],["lit","uint","00000001"]]]]]],["function",16,"deep_saved",["uint",false],false,[["parameter",17,["uint",false],"none"],["parameter",18,["uint",false],"none"],["parameter",19,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["ident","uint",17],["call","uint",12,[["argument",["uint",false],["ident","uint",17]],["argument",["uint",false],["ident","uint",18]],["argument",["uint",false],["ident","uint",19]]]]]]]]]]]
"#

private def short := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[20],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"loop_calls_loop",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"],["parameter",11,["uint",false],"none"]],["compound",[["while",["ident","uint",10],["compound",[["expr",["assign","uint","set",9,["uint",false],["ident","uint",9],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",11]]]]]],["expr",["update","uint","pre_dec",10,["uint",false],["ident","uint",10]]]]]],["return",["uint",false],["ident","uint",9]]]]],["function",12,"call_nested_loop",["uint",false],false,[["parameter",13,["uint",false],"none"],["parameter",14,["uint",false],"none"],["parameter",15,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["call","uint",8,[["argument",["uint",false],["ident","uint",13]],["argument",["uint",false],["ident","uint",14]],["argument",["uint",false],["ident","uint",15]]]],["lit","uint","00000001"]]]]]],["function",20,"deep_short",["int",false],false,[["parameter",21,["uint",false],"none"],["parameter",22,["uint",false],"none"],["parameter",23,["uint",false],"none"]],["compound",[["return",["int",false],["binary","int","and",["ident","uint",21],["call","uint",12,[["argument",["uint",false],["ident","uint",21]],["argument",["uint",false],["ident","uint",22]],["argument",["uint",false],["ident","uint",23]]]]]]]]]]]
"#

private def outer := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[24],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"loop_calls_loop",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"],["parameter",11,["uint",false],"none"]],["compound",[["while",["ident","uint",10],["compound",[["expr",["assign","uint","set",9,["uint",false],["ident","uint",9],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",11]]]]]],["expr",["update","uint","pre_dec",10,["uint",false],["ident","uint",10]]]]]],["return",["uint",false],["ident","uint",9]]]]],["function",12,"call_nested_loop",["uint",false],false,[["parameter",13,["uint",false],"none"],["parameter",14,["uint",false],"none"],["parameter",15,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["call","uint",8,[["argument",["uint",false],["ident","uint",13]],["argument",["uint",false],["ident","uint",14]],["argument",["uint",false],["ident","uint",15]]]],["lit","uint","00000001"]]]]]],["function",24,"deep_outer_loop",["uint",false],false,[["parameter",25,["uint",false],"none"],["parameter",26,["uint",false],"none"],["parameter",27,["uint",false],"none"],["parameter",28,["uint",false],"none"]],["compound",[["while",["ident","uint",28],["compound",[["expr",["assign","uint","set",25,["uint",false],["ident","uint",25],["call","uint",12,[["argument",["uint",false],["ident","uint",25]],["argument",["uint",false],["ident","uint",26]],["argument",["uint",false],["ident","uint",27]]]]]],["expr",["update","uint","pre_dec",28,["uint",false],["ident","uint",28]]]]]],["return",["uint",false],["ident","uint",25]]]]]]]
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

func @call_nested_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>, %v3: mem<all>, %v4: world):
    (%v5, %v6, %v7) = call @loop_calls_loop(%v0, %v1, %v2, %v3, %v4)
    %v8 = const.bits<32> 1
    %v9 = add.bits<32> %v5, %v8
    ret %v9, %v6, %v7
}

func @deep_saved(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>, %v3: mem<all>, %v4: world):
    (%v5, %v6, %v7) = call @call_nested_loop(%v0, %v1, %v2, %v3, %v4)
    %v8 = add.bits<32> %v0, %v5
    ret %v8, %v6, %v7
}

func @deep_short(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>, %v3: mem<all>, %v4: world):
    %v5 = const.bits<32> 0
    %v6 = icmp.ne %v0, %v5
    cond_br %v6, ^b1(%v0, %v1, %v2, %v3, %v4), ^b2(%v0, %v1, %v2, %v3, %v4)

^b1(%v7: bits<32>, %v8: bits<32>, %v9: bits<32>, %v10: mem<all>, %v11: world):
    (%v12, %v13, %v14) = call @call_nested_loop(%v7, %v8, %v9, %v10, %v11)
    %v15 = const.bits<32> 0
    %v16 = icmp.ne %v12, %v15
    br ^b3(%v7, %v8, %v9, %v13, %v14, %v16)

^b2(%v17: bits<32>, %v18: bits<32>, %v19: bits<32>, %v20: mem<all>, %v21: world):
    %v22 = const.bits<1> 0
    br ^b3(%v17, %v18, %v19, %v20, %v21, %v22)

^b3(%v23: bits<32>, %v24: bits<32>, %v25: bits<32>, %v26: mem<all>, %v27: world, %v28: bits<1>):
    %v29 = zext.bits<32> %v28
    ret %v29, %v26, %v27
}

func @deep_outer_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>, %arg3: bits<32>, %arg4: mem<all>, %arg5: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>, %v3: bits<32>, %v4: mem<all>, %v5: world):
    br ^b1(%v0, %v1, %v2, %v3, %v4, %v5)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: bits<32>, %v9: bits<32>, %v10: mem<all>, %v11: world):
    %v12 = const.bits<32> 0
    %v13 = icmp.ne %v9, %v12
    cond_br %v13, ^b2(%v6, %v7, %v8, %v9, %v10, %v11), ^b3(%v6, %v10, %v11)

^b2(%v14: bits<32>, %v15: bits<32>, %v16: bits<32>, %v17: bits<32>, %v18: mem<all>, %v19: world):
    (%v20, %v21, %v22) = call @call_nested_loop(%v14, %v15, %v16, %v18, %v19)
    %v23 = const.bits<32> 1
    %v24 = sub.bits<32> %v17, %v23
    br ^b1(%v20, %v15, %v16, %v24, %v21, %v22)

^b3(%v25: bits<32>, %v26: mem<all>, %v27: world):
    ret %v25, %v26, %v27
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

private def outerPlan := r#"["core-call-cutpoints-v1",256,[[2,["core-cutpoints-v2",256,[[0,"entry",[[3,"uint",false,true],[4,"uint",false,true]],[[1,4,"uint"],[0,3,"uint"]],[[3,"world"],[2,"mem"]]],[2,"b1",[[3,"uint",false,true],[4,"uint",false,true]],[[5,4,"uint"],[4,3,"uint"]],[[7,"world"],[6,"mem"]]]]]],[8,["core-cutpoints-v2",256,[[0,"entry",[[9,"uint",false,true],[10,"uint",false,true],[11,"uint",false,true]],[[2,11,"uint"],[1,10,"uint"],[0,9,"uint"]],[[4,"world"],[3,"mem"]]],[2,"b1",[[9,"uint",false,true],[10,"uint",false,true],[11,"uint",false,true]],[[7,11,"uint"],[6,10,"uint"],[5,9,"uint"]],[[9,"world"],[8,"mem"]]]]]],[24,["core-cutpoints-v2",256,[[0,"entry",[[25,"uint",false,true],[26,"uint",false,true],[27,"uint",false,true],[28,"uint",false,true]],[[3,28,"uint"],[2,27,"uint"],[1,26,"uint"],[0,25,"uint"]],[[5,"world"],[4,"mem"]]],[2,"b1",[[25,"uint",false,true],[26,"uint",false,true],[27,"uint",false,true],[28,"uint",false,true]],[[9,28,"uint"],[8,27,"uint"],[7,26,"uint"],[6,25,"uint"]],[[11,"world"],[10,"mem"]]]]]]]]
"#


private def accepts (s t p : String) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let some (budget, rows) := readCallCutpoints source p | return false
  return (callingLoopPair source target (fun id => rows.lookup id) budget).isSome

private def mutateFunction (text name before after : String) : String := Id.run do
  let [leading, suffix] := text.splitOn s!"func @{name}(" | return text
  let body :: rest := suffix.splitOn "\nfunc @" | return text
  return leading ++ s!"func @{name}(" ++ body.replace before after ++
    String.join (rest.map ("\nfunc @" ++ ·))

def functionClosureSelfTest : IO UInt32 := do
  let pairs := [(plain, "call_nested_loop", plan), (saved, "deep_saved", plan),
    (short, "deep_short", plan), (outer, "deep_outer_loop", outerPlan)]
  let mut failed := 0
  let mut negatives := 0
  let mut executions := 0
  let mut divergent := 0
  for (text, name, proposal) in pairs do
    if !accepts text target proposal ||
        !accepts (text.replace name "renamed_deep") (target.replace name "renamed_deep") proposal then
      IO.eprintln s!"deep closure refused: {name}"
      failed := failed+1
    let .ok s := Source.readProgram text | return 1
    let .ok t := IR.Reader.readProgram target | return 1
    let some sf := s.functions.find? (·.name == name) | return 1
    let some tf := t.functions.find? (·.name == name) | return 1
    let some (budget, rows) := readCallCutpoints s proposal | return 1
    if (openFunctionClosure s t (fun id => rows.lookup id) budget 0 sf tf).isSome then
      IO.eprintln "exhausted call-graph construction accepted"
      failed := failed+1
    negatives := negatives+1
    -- Zero construction fuel is malformed at the reader boundary, whereas
    -- the following invariant mutations must reach the semantic checker.
    let zeroBudget := proposal.replace ",256," ",0,"
    if zeroBudget == proposal || (readCallCutpoints s zeroBudget).isSome ||
        accepts text target zeroBudget then
      IO.eprintln s!"zero-budget closure plan admitted: {name}"
      failed := failed+1
    negatives := negatives+1
    for changed in [proposal.replace "[5,4,\"uint\"]" "[5,3,\"uint\"]",
        "[\"core-call-cutpoints-v1\",256,[]]"] do
      if changed == proposal || (readCallCutpoints s changed).isNone || accepts text target changed then
        IO.eprintln s!"invalid deep closure plan: {name}"
        failed := failed+1
      negatives := negatives+1
    for (changedName, before, after) in [
        ("call_tick", "mul.bits<32> %v0, %v3", "sub.bits<32> %v0, %v3"),
        ("loop_call", "sub.bits<32> %v11, %v17", "add.bits<32> %v11, %v17"),
        ("loop_calls_loop", "br ^b1(%v17, %v21, %v14,", "br ^b1(%v12, %v21, %v14,"),
        ("call_nested_loop", "add.bits<32>", "sub.bits<32>"),
        ("call_nested_loop", "io=false", "io=true")] do
      let changed := mutateFunction target changedName before after
      if changed == target || accepts text changed proposal then
        IO.eprintln s!"deep callee mutation admitted: {name}, {changedName}"
        failed := failed+1
      negatives := negatives+1
    for x in [0, 7, 0xffffffff] do
      for n in [0, 1, 3] do
        for m in [0, 1, 3] do
          for k in (if name == "deep_outer_loop" then [0, 1, 2] else [1]) do
            let mut answer := x
            for _ in [:k] do
              for _ in [:n*m] do answer := (answer*3+1) % 2^32
              answer := (answer+1) % 2^32
            let (wanted, frames) :=
              if name == "deep_outer_loop" then (answer, 2+k*(2+n*(m+1)))
              else if name == "deep_saved" then ((x+answer)%2^32, 4+n*(m+1))
              else if name == "deep_short" then
                (if x != 0 && answer != 0 then 1 else 0, if x == 0 then 2 else 4+n*(m+1))
              else (answer, 3+n*(m+1))
            let args := ([x,n,m] ++ if name == "deep_outer_loop" then [k] else []).map (Source.Value.ofNat .uint)
            let .ok se := Source.initial s sf.symbol args | return 1
            let .ok te := IR.initial t name (loopInputValues true args) | return 1
            let after := IR.runSteps t 40000 te
            if observeSource (Source.runSteps s 40000 se) != some (.returned sf.returnType.type wanted) ||
                observeTarget sf.returnType.type after != some (.returned sf.returnType.type wanted) ||
                after.nextFrame != frames then
              IO.eprintln s!"deep execution/frame mismatch: {name} {x} {n} {m} {k}"
              failed := failed+1
            executions := executions+1
    for (id, callee, removed, oldCounter, newCounter) in [
        (4, "loop_call", "    %v18 = sub.bits<32> %v11, %v17\n", "%v18", "%v11"),
        (10, "loop_calls_loop", "    %v21 = sub.bits<32> %v13, %v20\n", "%v21", "%v13")] do
      let before := s!"[\"update\",\"uint\",\"pre_dec\",{id},[\"uint\",false],[\"ident\",\"uint\",{id}]]"
      let ss := text.replace before "[\"lit\",\"uint\",\"00000000\"]"
      let tt := mutateFunction (mutateFunction target callee removed "") callee oldCounter newCounter
      if ss == text || tt == target || !accepts ss tt proposal then
        IO.eprintln s!"matched divergent deep closure refused: {name}, {callee}"
        failed := failed+1
      divergent := divergent+1
  for (text, name, proposal, before, after) in [
      (saved, "deep_saved", plan, "add.bits<32> %v0,", "sub.bits<32> %v0,"),
      (short, "deep_short", plan, "icmp.ne %v0,", "icmp.eq %v0,"),
      (short, "deep_short", plan, "zext.bits<32>", "sext.bits<32>"),
      (outer, "deep_outer_loop", outerPlan, "sub.bits<32> %v17, %v23", "add.bits<32> %v17, %v23"),
      (outer, "deep_outer_loop", outerPlan, "br ^b1(%v20, %v15, %v16, %v24,", "br ^b1(%v20, %v16, %v15, %v24,"),
      (outer, "deep_outer_loop", outerPlan, "br ^b1(%v20, %v15, %v16, %v24, %v21, %v22)", "br ^b1(%v20, %v15, %v16, %v24, %v18, %v19)")] do
    let tt := mutateFunction target name before after
    if tt == target || accepts text tt proposal then
      IO.eprintln s!"deep caller mutation admitted: {name} {before}"
      failed := failed+1
    negatives := negatives+1
  if failed == 0 then
    IO.println s!"8 original/renamed deep closures, {negatives} refusals, {executions} execution/frame controls and {divergent} matched divergent variants pass"
  for width in [32,64] do
    let instruction := s!"    %v1000 = uninitialized.bits<{width}>\n"
    for location in ["    (%v5, %v6, %v7) = call @call_nested_loop", "    %v8 = add.bits<32> %v0, %v5"] do
      let carried := mutateFunction target "deep_saved" location (instruction ++ location)
      if carried == target || !accepts saved carried plan then
        IO.eprintln s!"unused poison transport refused: {width}, {location}"
        failed := failed+1
      for (before, after) in [
          ("add.bits<32> %v0, %v5", "add.bits<32> %v1000, %v5"),
          ("ret %v8, %v6, %v7", "ret %v1000, %v6, %v7"),
          ("call @call_nested_loop(%v0,", "call @call_nested_loop(%v1000,"),
          (s!"uninitialized.bits<{width}>", s!"uninitialized.bits<{width}> %v0")] do
        let changed := mutateFunction carried "deep_saved" before after
        if changed == carried || accepts saved changed plan then
          IO.eprintln s!"used/malformed poison admitted: {width}, {before}"
          failed := failed+1
  if failed == 0 then
    IO.println "4 unused-poison call-boundary transports and 16 used/malformed refusals pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.FunctionClosureTests
