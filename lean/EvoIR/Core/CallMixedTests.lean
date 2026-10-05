import EvoIR.Core.CallFrontierPair
import EvoIR.Core.CallLoopFixtures

/-! Byte-exact exports for finite/cyclic call composition. Entry-only callee
suggestions are checked, never trusted summaries or a way to skip a body. -/
namespace EvoIR.Core.Symbolic.CallMixedTests

private def beforeLoop := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"finite_before_loop",["uint",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["call","uint",0,[["argument",["uint",false],["ident","uint",9]]]],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",10]]]]]]]]]]]
"#

private def argument := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[11],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",11,"finite_loop_argument",["uint",false],false,[["parameter",12,["uint",false],"none"],["parameter",13,["uint",false],"none"]],["compound",[["return",["uint",false],["call","uint",2,[["argument",["uint",false],["call","uint",0,[["argument",["uint",false],["ident","uint",12]]]]],["argument",["uint",false],["ident","uint",13]]]]]]]]]]
"#

private def afterLoop := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[14],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",14,"loop_then_finite",["uint",false],false,[["parameter",15,["uint",false],"none"],["parameter",16,["uint",false],"none"]],["compound",[["return",["uint",false],["call","uint",0,[["argument",["uint",false],["call","uint",2,[["argument",["uint",false],["ident","uint",15]],["argument",["uint",false],["ident","uint",16]]]]]]]]]]]]]
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

func @finite_before_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @call_tick(%v0, %v2, %v3)
    (%v7, %v8, %v9) = call @loop_call(%v0, %v1, %v5, %v6)
    %v10 = add.bits<32> %v4, %v7
    ret %v10, %v8, %v9
}

func @finite_loop_argument(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @call_tick(%v0, %v2, %v3)
    (%v7, %v8, %v9) = call @loop_call(%v4, %v1, %v5, %v6)
    ret %v7, %v8, %v9
}

func @loop_then_finite(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    (%v7, %v8, %v9) = call @call_tick(%v4, %v5, %v6)
    ret %v7, %v8, %v9
}
"#

private def plan : String :=
  "[\"core-call-cutpoints-v1\",256,[[2," ++ Source.CallLoopTests.proposal ++ "]]]"

private def accepts (s t : String) (proposal : String := plan) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let some (budget, rows) := readCallCutpoints source proposal | return false
  return (callFrontierPair source target (fun id => rows.lookup id) budget).isSome

private def mutateFunction (text name before after : String) : String := Id.run do
  let marker := s!"func @{name}("
  let [leading, suffix] := text.splitOn marker | return text
  let body :: rest := suffix.splitOn "\nfunc @" | return text
  return leading ++ marker ++ body.replace before after ++
    String.join (rest.map ("\nfunc @" ++ ·))

def callMixedSelfTest : IO UInt32 := do
  let pairs := [(beforeLoop, "finite_before_loop"), (argument, "finite_loop_argument"),
    (afterLoop, "loop_then_finite")]
  let mut failed := 0
  let mut executions := 0
  let mut negatives := 0
  for (source, name) in pairs do
    if !accepts source target ||
        !accepts (source.replace name "unlisted_mixed") (target.replace name "unlisted_mixed") then
      IO.eprintln s!"original or renamed mixed-call pair refused: {name}"
      failed := failed+1
    let .ok s := Source.readProgram source | return 1
    let .ok t := IR.Reader.readProgram target | return 1
    let some sf := s.functions.find? (·.name == name) | return 1
    let tick (n : Nat) := (n*3+1) % 2^32
    for x in [0, 7, 0xffffffff] do
      for n in [0, 1, 3] do
        let mut result := if name == "finite_loop_argument" then tick x else x
        for _ in [:n] do result := tick result
        if name == "finite_before_loop" then result := (tick x + result) % 2^32
        if name == "loop_then_finite" then result := tick result
        let args := [Source.Value.ofNat .uint x, Source.Value.ofNat .uint n]
        let .ok se := Source.initial s sf.symbol args | return 1
        let .ok te := IR.initial t name (loopInputValues true args) | return 1
        let tf := IR.runSteps t 10000 te
        if observeSource (Source.runSteps s 10000 se) != some (.returned .uint result) ||
            observeTarget .uint tf != some (.returned .uint result) || tf.nextFrame != n+4 then
          IO.eprintln s!"mixed-call execution/frame mismatch: {name} {x} {n}"
          failed := failed+1
        executions := executions+1
    -- A missing cyclic invariant cannot become a finite summary, and an
    -- explicit invalid finite-callee plan cannot be silently replaced.
    let wrongFinite := "[\"core-call-cutpoints-v1\",256,[[2," ++ Source.CallLoopTests.proposal ++
      "],[0,[\"core-cutpoints-v2\",256,[[0,\"entry\",[[1,\"uint\",false,true]],[[0,2,\"uint\"]],[[2,\"world\"],[1,\"mem\"]]]]]]]]"
    for invalid in ["[\"core-call-cutpoints-v1\",256,[]]", wrongFinite] do
      if (readCallCutpoints s invalid).isNone then
        IO.eprintln s!"mixed-call negative did not reach invariant checking: {name}"
        failed := failed+1
      if accepts source target invalid then
        IO.eprintln s!"invalid mixed-call proposal admitted: {name}"
        failed := failed+1
      negatives := negatives+1
    let common := [
      ("call_tick", "mul.bits<32> %v0, %v3", "sub.bits<32> %v0, %v3"),
      ("call_tick", "ret %v6, %v1, %v2", "ret %v0, %v1, %v2"),
      ("loop_call", "sub.bits<32> %v11, %v17", "add.bits<32> %v11, %v17")]
    for (changedFunction, before, after) in common do
      let changed := mutateFunction target changedFunction before after
      if changed == target || accepts source changed then
        IO.eprintln s!"mixed callee mutation admitted: {name}: {before}"
        failed := failed+1
      negatives := negatives+1
  let mutations := [
    (beforeLoop, "finite_before_loop", "add.bits<32> %v4, %v7", "sub.bits<32> %v4, %v7"),
    (beforeLoop, "finite_before_loop", "call @loop_call(%v0, %v1, %v5, %v6)", "call @loop_call(%v0, %v1, %v2, %v3)"),
    (beforeLoop, "finite_before_loop", "call @call_tick(%v0,", "call @call_tick(%v1,"),
    (argument, "finite_loop_argument", "call @loop_call(%v4, %v1,", "call @loop_call(%v0, %v1,"),
    (argument, "finite_loop_argument", "ret %v7, %v8, %v9", "ret %v4, %v8, %v9"),
    (afterLoop, "loop_then_finite", "call @call_tick(%v4,", "call @call_tick(%v0,"),
    (afterLoop, "loop_then_finite", "call @call_tick(%v4, %v5, %v6)", "call @call_tick(%v4, %v2, %v3)"),
    (afterLoop, "loop_then_finite", "ret %v7, %v8, %v9", "ret %v4, %v8, %v9")]
  for (source, name, before, after) in mutations do
    let changed := mutateFunction target name before after
    if changed == target || accepts source changed then
      IO.eprintln s!"mixed caller mutation admitted: {name}: {before}"
      failed := failed+1
    negatives := negatives+1
  if failed == 0 then
    IO.println s!"{pairs.length*2} original/renamed mixed-call pairs, {negatives} refusals and {executions} execution/frame controls pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.CallMixedTests
