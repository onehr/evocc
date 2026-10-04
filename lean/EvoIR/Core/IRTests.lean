import EvoIR.Core.IRReader
import EvoIR.Core.IRExecution
import EvoIR.Core.IRWellFormed

namespace EvoIR.Core.IR

private def effects : String :=
  "effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }"

private def functionText (body : String) : String :=
  "func @f(%arg0: bits<32>) -> (bits<32>) cc = target_default\n" ++
  effects ++ "\n{\n^entry(%v0: bits<32>):\n" ++ body ++ "\n}\n"

private def increment := functionText
  "%v1 = const.bits<32> 1\n%v2 = signed_add_defined %v0, %v1\nub.assume.signed_overflow %v2\n%v3 = add.bits<32> %v0, %v1\nret %v3"

private def loop := functionText
  "br ^loop(%v0)\n^loop(%v1: bits<32>):\n%v2 = const.bits<32> 0\n%v3 = icmp.ne %v1, %v2\ncond_br %v3, ^next(%v1), ^exit(%v1)\n^next(%v4: bits<32>):\n%v5 = const.bits<32> 1\n%v6 = sub.bits<32> %v4, %v5\nbr ^loop(%v6)\n^exit(%v7: bits<32>):\nret %v7"

private def runText (text : String) (args : List Value) (fuel : Nat := 1000) :
    Except String (Option (Except Source.Fault (List Value))) := do
  let p ← Reader.readProgram text
  let s ← (initial p "f" args).mapError reprStr
  match (runSteps p fuel s).control with
  | .halted r => return some r
  | _ => return none

def irSelfTest : IO UInt32 := do
  let mut failures := 0
  let positive := [increment, loop, functionText "ret %v0",
    functionText "%v1 = uninitialized.bits<32>\nbr ^exit(%v0, %v1)\n^exit(%v2: bits<32>, %v3: bits<32>):\nret %v2"]
  for text in positive do
    if (Reader.readProgram text).toOption.isNone then
      IO.eprintln s!"core IR positive refused: {reprStr (Reader.readProgram text)}"
      failures := failures+1
  let negative := [
    increment.replace "signed_add_defined" "signed_shl_defined",
    increment.replace "fenv=false, " "",
    increment.replace "reads={}" "reads={unknown}",
    increment.replace "cc = target_default" "cc = unknown",
    increment.replace "%v3 = add.bits<32>" "%v0 = add.bits<32>",
    increment.replace "add.bits<32>" "add.bits<64>",
    increment.replace "ret %v3" "ret %v99",
    increment.replace "const.bits<32> 1" "const.bits<32> 4294967296",
    increment.replace "const.bits<32> 1" "const.bits<32> 01",
    increment.replace "signed_add_defined %v0, %v1" "signed_add_defined %v0",
    increment.replace "ret %v3" "ret %v2",
    loop.replace "br ^loop(%v6)" "br ^loop()",
    loop.replace "^next(%v1)" "^absent(%v1)",
    loop.replace "cond_br %v3" "cond_br %v1",
    loop.replace "ret %v7" "ret %v0",
    loop.replace "^exit(%v7: bits<32>)" "^exit(%v0: bits<32>)",
    increment.replace "may_unwind=false }" "may_unwind=false, extra=false }",
    increment ++ "garbage\n",
    functionText "%v1 = sext.bits<32> %v0\nret %v1",
    functionText "(%v1) = call @external(%v0)\nret %v1"]
  for text in negative do
    if (Reader.readProgram text).toOption.isSome then
      IO.eprintln s!"core IR malformed input accepted: {text}"
      failures := failures+1
  let memoryText := "func @f(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default\n" ++
    "effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }\n{\n" ++
    "^entry(%v0: mem<all>, %v1: world):\n" ++
    "(%v2, %v3, %v4) = alloca %v0, %v1 [origin=0, size=4, align=4]\n" ++
    "%v5 = const.bits<32> 29\n%v6 = access_in_bounds %v3, %v2 [size=4, align=4]\n" ++
    "(%v7, %v8) = store %v6, %v4, %v2, %v5\n" ++
    "%v9 = access_in_bounds %v7, %v2 [size=4, align=4]\n" ++
    "(%v10, %v11, %v12) = load %v9, %v8, %v2 : bits<32>\nret %v10, %v11, %v12\n}\n"
  let checkText := fun text => Reader.readProgram text >>= checkProgram
  for text in [increment, memoryText, loop.replace "may_trap=true" "may_trap=false"] do
    if (checkText text).toOption.isNone then
      IO.eprintln s!"core IR input check refused: {reprStr (checkText text)}"
      failures := failures+1
  let malformedEffects := [
    increment.replace "may_trap=true" "may_trap=false",
    increment.replace "fenv=false" "fenv=true",
    memoryText.replace "store %v6" "store %v3",
    memoryText.replace "load %v9, %v8" "load %v9, %v4",
    memoryText.replace "ret %v10, %v11" "ret %v10, %v9",
    memoryText.replace "store %v6, %v4, %v2" "store %v6, %v4, %v99",
    memoryText.replace "load %v9" "load %v7",
    memoryText.replace "reads={all}" "reads={}",
    memoryText.replace "size=4, align=4]\n(%v10" "size=1, align=1]\n(%v10"]
  for text in malformedEffects do
    if (checkText text).toOption.isSome then
      IO.eprintln "core IR token/access/effect mutation accepted"
      failures := failures+1
  let bits (n : Nat) : Value := .bits 32 (BitVec.ofNat 32 n)
  let runs := [
    (increment, [bits 9], some (.ok [bits 10])),
    (increment, [bits 0x7fffffff], some (.error (.diagnostic 3004))),
    (loop, [bits 8], some (.ok [bits 0])),
    (functionText "br ^entry(%v0)", [bits 1], none),
    (positive[3]!, [bits 7], some (.ok [bits 7])),
    (functionText "%v1 = uninitialized.bits<32>\nret %v1", [bits 1],
      some (.error (.diagnostic 3001))),
    (functionText "%v1 = const.bits<32> 0\n%v2 = udiv.bits<32> %v0, %v1\nret %v2",
      [bits 1], some (.error (.diagnostic 3002))),
    (functionText "%v1 = const.bits<32> 0\n%v2 = urem.bits<32> %v0, %v1\nret %v2",
      [bits 1], some (.error (.diagnostic 3003)))]
  for (text, args, expected) in runs do
    if runText text args != .ok expected then
      IO.eprintln s!"core IR execution mismatch: {reprStr (runText text args)}"
      failures := failures+1
  -- Simultaneous backedge copies capture both old values before rebinding.
  let params : List Binding := [⟨0, .bits 32⟩, ⟨1, .bits 32⟩]
  let f : Function := ⟨"swap", [.bits 32, .bits 32], [.bits 32], false,
    Reader.emptyEffects, [⟨"loop", params, [], .return [0]⟩]⟩
  let a : Activation := ⟨"swap", "loop", 0, [(0, bits 3), (1, bits 7)], 1⟩
  let swapped := (jump f a ⟨"loop", [1, 0]⟩).map (·.values.lookup 0)
  if swapped != .ok (some (bits 7)) then failures := failures+1
  -- Identity/initializedness/lifetime checks use byte snapshots, not SSA names.
  let m : Memory := ⟨2, [⟨1, 7, 1, 4, true, List.replicate 4 none⟩]⟩
  let p := Value.pointer 1 0
  let loadEmpty := evalOperation 1 (.load 32) [.memory m, .world, p]
  if loadEmpty != .error (.diagnostic 3001) then failures := failures+1
  let loaded := do
    let [.memory stored, .world] ← evalOperation 1 .store [.memory m, .world, p, bits 0x12345678]
      | throw (Source.Fault.malformed "bad store result")
    evalOperation 1 (.load 32) [.memory stored, .world, p]
  if loaded.toOption.bind List.head? != some (bits 0x12345678) then failures := failures+1
  match runText memoryText [.memory ⟨1, []⟩, .world] with
  | .ok (some (.ok [.bits 32 v, .memory final, .world])) =>
    if v.toNat != 29 || final.objects.any (·.live) then failures := failures+1
  | other => IO.eprintln s!"private memory program failed: {reprStr other}"; failures := failures+1
  if evalOperation 2 .endLifetime [.memory m, .world, p] != .error (.diagnostic 3018) then
    failures := failures+1
  if evalOperation 1 (.load 32) [.memory (m.endFrame 1), .world, p] !=
      .error (.diagnostic 3010) then failures := failures+1
  if evalOperation 1 .store [.memory m, .world, .pointer 0 0, .poison (.bits 32)] !=
      .error (.diagnostic 3009) then failures := failures+1
  if failures == 0 then
    IO.println s!"{positive.length + negative.length + runs.length + 10 + malformedEffects.length} core IR reader/execution cases pass (no compilation claim)"
    return 0
  IO.eprintln s!"{failures} core IR cases failed"
  return 1

end EvoIR.Core.IR
