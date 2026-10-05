import EvoIR.Core.ValidatorStorage
import EvoIR.Core.CallCorrespondence
import EvoIR.Core.IRReader

/-! Storage proof boundary regressions and the original C initializer pair.
The numeric controls do not supply any premise to the universal theorems. -/
namespace EvoIR.Core.Source.StorageTests

private def q : QualifiedType := ⟨.int, false⟩
private def state (n : Int) : State :=
  ⟨.value (.ofInt .int n), [⟨1, q, some (.ofInt .int n)⟩], [.discard], 7⟩

private def observations (s : State) : Option Result × Result :=
  ((match s.control with | .value v => some (.ok v) | .halted r => some r | _ => none),
    readSlot s.locals 1 .int)

-- A failing compound write must preserve the old object and first diagnostic.
example : observations (resumeValue (state 9) (.assign .int .div 1 q) (.ofInt .int 0)) =
    (some (.error (.diagnostic 3002)), .ok (.ofInt .int 9)) := by decide
example : observations (resumeValue (state (-2147483648)) (.assign .int .rem 1 q) (.ofInt .int (-1))) =
    (some (.error (.diagnostic 3004)), .ok (.ofInt .int (-2147483648))) := by decide
example : observations (resumeValue (state 9) (.assign .int .shr 1 q) (.ofInt .long (-1))) =
    (some (.error (.diagnostic 3005)), .ok (.ofInt .int 9)) := by decide
example : observations (resumeValue (state 2147483647) (.assign .int .add 1 q) (.ofInt .int 1)) =
    (some (.error (.diagnostic 3004)), .ok (.ofInt .int 2147483647)) := by decide

example : observations (enterExpression (state 7) (.update .int .postInc 1 q (.ident .int 1))) =
    (some (.ok (.ofInt .int 7)), .ok (.ofInt .int 8)) := by decide
example : observations (enterExpression (state 7) (.update .int .preDec 1 q (.ident .int 1))) =
    (some (.ok (.ofInt .int 6)), .ok (.ofInt .int 6)) := by decide
example : observations (enterExpression (state 2147483647) (.update .int .postInc 1 q (.ident .int 1))) =
    (some (.error (.diagnostic 3004)), .ok (.ofInt .int 2147483647)) := by decide

-- Const initialization and ordinary writes do not share permissions.
example : writeSlot [⟨1, ⟨.int, true⟩, some (.ofInt .int 1)⟩]
    1 ⟨.int, true⟩ (.ofInt .int 2) true =
      .error (.malformed "duplicate initialization") := by rfl
example : writeSlot [⟨1, ⟨.int, true⟩, none⟩]
    1 ⟨.int, true⟩ (.ofInt .int 2) false =
      .error (.malformed "const assignment") := by rfl

-- The actual exported pair from initializer-self-assignment.c, without source
-- rewriting or input restriction. The target retains the inner write's unused
-- constant but returns the final initializer's value.
private def source := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[2],[["function",0,"initializer_self_assignment",["int",false],false,[],["compound",[["object",1,"x",["int",false],"none",["store_scalar",["int",false],["comma","int",["assign","int","set",1,["int",false],["ident","int",1],["lit","int","00000001"]],["lit","int","00000002"]]]],["return",["int",false],["ident","int",1]]]]],["function",2,"main",["int",false],false,[],["compound",[["return",["int",false],["binary","int","ne",["call","int",0,[]],["lit","int","00000002"]]]]]]]]"#

private def target := r#"func @initializer_self_assignment(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 1
    %v3 = const.bits<32> 2
    ret %v3, %v0, %v1
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = call @initializer_self_assignment(%v0, %v1)
    %v5 = const.bits<32> 2
    %v6 = icmp.ne %v2, %v5
    %v7 = zext.bits<32> %v6
    ret %v7, %v3, %v4
}
"#

private def accepted (s t : String) : Bool :=
  match readProgram s, IR.Reader.readProgram t with
  | .ok s, .ok t => (Symbolic.callPair s t).isSome
  | _, _ => false

def storageSelfTest : IO UInt32 := do
  let pairs := [
    ("original sequenced initializer", source, target, true),
    ("renamed closure", source.replace "initializer_self_assignment" "different_name",
      target.replace "initializer_self_assignment" "different_name", true),
    ("wrong final initializer", source, target.replace "ret %v3, %v0, %v1" "ret %v2, %v0, %v1", false),
    ("assignment into const", source.replace "[\"int\",false]" "[\"int\",true]", target, false),
    ("uninitialized self read", source.replace "[\"lit\",\"int\",\"00000001\"]" "[\"ident\",\"int\",1]", target, false)]
  let mut failures := 0
  for (name, s, t, expected) in pairs do
    if accepted s t != expected then
      IO.eprintln s!"storage proof regression: {name}"
      failures := failures + 1
  if failures != 0 then return 1
  IO.println "2 initializer closure pairs, 3 refusals and 9 kernel storage controls pass"
  return 0

end EvoIR.Core.Source.StorageTests
