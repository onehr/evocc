import EvoIR.Core.CallFrontierPair
import EvoIR.Core.CallLoopFixtures

/-! Original short-circuit RHS calls, branch-premise isolation and fresh-return
renaming. Sample executions supplement, but do not establish, the pair theorem. -/
namespace EvoIR.Core.Symbolic.CallShortRightTests

private def andCall := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"and_right_loop_call",["int",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"]],["compound",[["return",["int",false],["binary","int","and",["ident","uint",9],["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",10]]]]]]]]]]]
"#

private def orCall := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[11],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",11,"or_right_loop_call",["int",false],false,[["parameter",12,["uint",false],"none"],["parameter",13,["uint",false],"none"]],["compound",[["return",["int",false],["binary","int","or",["ident","uint",12],["call","uint",2,[["argument",["uint",false],["ident","uint",12]],["argument",["uint",false],["ident","uint",13]]]]]]]]]]]
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

func @and_right_loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b1(%v0, %v1, %v2, %v3), ^b2(%v0, %v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    (%v10, %v11, %v12) = call @loop_call(%v6, %v7, %v8, %v9)
    %v13 = const.bits<32> 0
    %v14 = icmp.ne %v10, %v13
    br ^b3(%v6, %v7, %v11, %v12, %v14)

^b2(%v15: bits<32>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = const.bits<1> 0
    br ^b3(%v15, %v16, %v17, %v18, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<1>):
    %v25 = zext.bits<32> %v24
    ret %v25, %v22, %v23
}

func @or_right_loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b2(%v0, %v1, %v2, %v3), ^b1(%v0, %v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    (%v10, %v11, %v12) = call @loop_call(%v6, %v7, %v8, %v9)
    %v13 = const.bits<32> 0
    %v14 = icmp.ne %v10, %v13
    br ^b3(%v6, %v7, %v11, %v12, %v14)

^b2(%v15: bits<32>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = const.bits<1> 1
    br ^b3(%v15, %v16, %v17, %v18, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<1>):
    %v25 = zext.bits<32> %v24
    ret %v25, %v22, %v23
}
"#

private def accepts (s t : String) : Bool := Id.run do
  let .ok source := Source.readProgram s | return false
  let .ok target := IR.Reader.readProgram t | return false
  let plan := "[\"core-call-cutpoints-v1\",256,[[2," ++ Source.CallLoopTests.proposal ++ "]]]"
  let some (budget, rows) := readCallCutpoints source plan | return false
  return (callFrontierPair source target (fun id => rows.lookup id) budget).isSome

private def mutateFunction (text name before after : String) : String := Id.run do
  let marker := s!"func @{name}("
  let [leading, suffix] := text.splitOn marker | return text
  let body :: rest := suffix.splitOn "\nfunc @" | return text
  return leading ++ marker ++ body.replace before after ++
    String.join (rest.map ("\nfunc @" ++ ·))

private def treesMatch (path : PathConditions inputWidths)
    (source target : CutTree inputWidths Nat) : Bool :=
  (cutMatchUnder (related := fun _ a b => a = b)
    (fun _ a b => if same : a = b then some ⟨by intros; exact same⟩ else none)
    64 path source target).isSome

private def premiseTests : List (String × Bool) := Id.run do
  let condition := (inputWord .uint 0).truthTerm
  let other := (inputWord .uint 1).truthTerm
  let one : CutTree inputWidths Nat := .leaf 1
  let two : CutTree inputWidths Nat := .leaf 2
  let path : PathConditions inputWidths := [(condition, true)]
  let saved := condition.renameInputs returnName
  let twice := saved.renameInputs returnName
  let g : Guard inputWidths := ⟨.signedOverflow, condition⟩
  let h : Guard inputWidths := ⟨.divisionByZero, other⟩
  return [
    ("selected source branch", treesMatch [] (.branch condition (.branch condition one two) two)
      (.branch condition one two)),
    ("selected target branch", treesMatch [] (.branch condition one two)
      (.branch condition (.branch condition one two) two)),
    ("false-arm premise", treesMatch [] (.branch condition one (.branch condition one two))
      (.branch condition one two)),
    ("no unconditional premise", !treesMatch [] (.branch condition one two) one),
    ("no sibling premise leakage", !treesMatch []
      (.branch condition one (.branch condition one two)) (.branch condition one one)),
    ("wrong selected leaf", !treesMatch path (.branch condition one two) two),
    ("wrong predicate", !treesMatch path (.branch other one two) one),
    ("guard cannot disappear", !treesMatch path (.checked g one) one),
    ("guard order retained", !treesMatch [] (.checked g (.checked h one)) (.checked h (.checked g one))),
    ("guard diagnostic retained", !treesMatch [] (.checked g one)
      (.checked ⟨.divisionByZero, condition⟩ one)),
    ("renamed premise", treesMatch path.forReturn (.branch saved one two) one),
    ("stale premise rejected", !treesMatch path (.branch saved one two) one),
    ("fresh result not caller input", !treesMatch path.forReturn (.branch condition one two) one),
    ("nested return renaming", treesMatch path.forReturn.forReturn (.branch twice one two) one),
    ("one rename insufficient after two calls", !treesMatch path.forReturn (.branch twice one two) one)]

def callShortRightSelfTest : IO UInt32 := do
  let pairs := [(andCall, "and_right_loop_call", false), (orCall, "or_right_loop_call", true)]
  let mut failed := 0
  let mut executions := 0
  let mut mutations := 0
  for (label, passed) in premiseTests do
    if !passed then
      IO.eprintln s!"path premise regression: {label}"
      failed := failed + 1
  for (source, name, isOr) in pairs do
    if !accepts source target ||
        !accepts (source.replace name "unlisted_rhs") (target.replace name "unlisted_rhs") then
      IO.eprintln s!"short-circuit right-call pair refused: {name}"
      failed := failed+1
    let .ok s := Source.readProgram source | return 1
    let .ok t := IR.Reader.readProgram target | return 1
    let some sf := s.functions.find? (·.name == name) | return 1
    let samples := [(0, 0, 0), (0, 3, 13), (7, 0, 7), (1, 3, 40),
      (0xffffffff, 1, 0xfffffffe), (0x55555555, 1, 0),
      (if isOr then 1 else 0, 0xffffffff, 0)]
    for (x, n, result) in samples do
      let expected := if isOr then (if x != 0 || result != 0 then 1 else 0)
        else (if x != 0 && result != 0 then 1 else 0)
      let args := [Source.Value.ofNat .uint x, Source.Value.ofNat .uint n]
      let .ok se := Source.initial s sf.symbol args | return 1
      let .ok te := IR.initial t name (loopInputValues true args) | return 1
      if observeSource (Source.runSteps s 10000 se) != some (.returned .int expected) ||
          observeTarget .int (IR.runSteps t 10000 te) != some (.returned .int expected) then
        IO.eprintln s!"short-circuit right-call execution mismatch: {name} {x} {n}"
        failed := failed+1
      executions := executions+1
    let skipBefore := if isOr then "const.bits<1> 1" else "const.bits<1> 0"
    let skipAfter := if isOr then "const.bits<1> 0" else "const.bits<1> 1"
    let changes := [
      ("icmp.ne %v0, %v4", "icmp.eq %v0, %v4"),
      ("icmp.ne %v0, %v4", "icmp.ne %v1, %v4"),
      (skipBefore, skipAfter),
      ("call @loop_call(%v6, %v7,", "call @loop_call(%v7, %v6,"),
      ("icmp.ne %v10, %v13", "icmp.ne %v6, %v13"),
      ("icmp.ne %v10, %v13", "icmp.eq %v10, %v13"),
      ("br ^b3(%v6, %v7, %v11, %v12,", "br ^b3(%v6, %v7, %v8, %v9,"),
      ("zext.bits<32> %v24", "sext.bits<32> %v24"),
      ("ret %v25, %v22, %v23", "ret %v20, %v22, %v23")]
    for (before, after) in changes do
      let changed := mutateFunction target name before after
      if changed == target || accepts source changed then
        IO.eprintln s!"short-circuit right-call mutation admitted: {name}: {before}"
        failed := failed+1
      mutations := mutations+1
  if failed == 0 then
    IO.println s!"{pairs.length * 2} original/renamed short-right pairs, {mutations} mutations, {executions} executions and {premiseTests.length} path-premise controls pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.CallShortRightTests
