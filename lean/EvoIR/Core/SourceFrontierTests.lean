import EvoIR.Core.SourceFrontier
import EvoIR.Core.CallLoopFixtures

/-! Call-frontier controls start at the original statement/expression, not at
a sampled suspended caller. The checked diagram must stop before entering the
callee and retain argument effects, conversion and the first diagnostic. -/
namespace EvoIR.Core.Symbolic.SourceFrontierTests

private def untilLoopCall (p : Source.Program) : Nat → Source.State → Option Source.State
  | 0, _ => none
  | fuel+1, state => match state.control with
    | .call 2 .. => some state
    | _ => (Source.step p state).bind (untilLoopCall p fuel)

private def stops (p : Source.Program) (cursor : SourceCursor inputWidths)
    (args : List Source.Value) (expected : Except Source.Fault (List Source.Value)) : Bool := Id.run do
  let some built := sourceCallFrontier p 64 cursor | return false
  let σ := inputSubst args
  match built.val.eval σ, expected with
  | .error actual, .error fault => return actual == fault
  | .ok exit, .ok values =>
    let represented := exit.denote σ 5
    let .call id type actual := represented.control | return false
    let some reached := untilLoopCall p 1000 (cursor.denote σ 5) | return false
    let .call realId realType realArgs := reached.control | return false
    return id == 2 && type == .uint && actual == values && realId == id && realType == type &&
      realArgs == actual && reached.locals == represented.locals &&
      reached.continuation == represented.continuation && reached.functionId == represented.functionId
  | _, _ => return false

def sourceFrontierSelfTest : IO UInt32 := do
  let .ok p := Source.readProgram Source.CallLoopTests.callLoop | return 1
  let some f := p.functions.find? (·.symbol == 5) | return 1
  let some locals := parameterStore f.parameters (parameterWords f.parameters) | return 1
  let entry : SourceCursor inputWidths :=
    ⟨.statement f.body, locals, [.function none .uint]⟩
  let u (n : Nat) := Source.Value.ofNat .uint n
  let expression (xs : List (Source.QualifiedType × Source.Expr)) : SourceCursor inputWidths :=
    ⟨.expression (.call .uint 2 xs), locals, [.return ⟨.uint, false⟩, .function none .uint]⟩
  let write : Source.Expr := .assign .uint .set 6 ⟨.uint, false⟩
    (.ident .uint 6) (.literal .uint 9)
  let assigned : SourceCursor inputWidths := ⟨.expression (.assign .uint .set 6
    ⟨.uint, false⟩ (.ident .uint 6) (.call .uint 2 [
      (⟨.uint, false⟩, .ident .uint 6), (⟨.uint, false⟩, .ident .uint 7)])),
    locals, [.return ⟨.uint, false⟩, .function none .uint]⟩
  let tests := [
    ("original caller entry", stops p entry [u 7, u 3] (.ok [u 7, u 3])),
    ("boundary caller arguments", stops p entry [u 0xffffffff, u 0] (.ok [u 0xffffffff, u 0])),
    ("assignment suspends before write", stops p assigned [u 7, u 3] (.ok [u 7, u 3])),
    ("second argument call retains first argument", stops p
      (expression [(⟨.uint, false⟩, .ident .uint 6),
        (⟨.uint, false⟩, .call .uint 2 [(⟨.uint, false⟩, .literal .uint 0),
          (⟨.uint, false⟩, .ident .uint 7)])]) [u 7, u 3] (.ok [u 0, u 3])),
    ("argument order retains writes", stops p
      (expression [(⟨.uint, false⟩, write), (⟨.uint, false⟩, .ident .uint 6)])
      [u 7, u 3] (.ok [u 9, u 9])),
    ("argument conversion", stops p
      (expression [(⟨.uint, false⟩, .literal .ulong 0x100000001), (⟨.uint, false⟩, .literal .uint 0)])
      [u 7, u 3] (.ok [u 1, u 0])),
    ("finite call in argument", stops p
      (expression [(⟨.uint, false⟩, .call .uint 0 [(⟨.uint, false⟩, .ident .uint 6)]),
        (⟨.uint, false⟩, .ident .uint 7)]) [u 7, u 3] (.ok [u 22, u 3])),
    ("first argument diagnostic", stops p
      (expression [(⟨.uint, false⟩, .binary .int .add
        (.cast .int ⟨.int, false⟩ (.ident .uint 6)) (.literal .int 1)),
        (⟨.uint, false⟩, .binary .uint .div (.literal .uint 1) (.literal .uint 0))])
      [u 0x7fffffff, u 3] (.error (.diagnostic 3004)))]
  let mut failed := 0
  for (label, passed) in tests do
    if !passed then
      IO.eprintln s!"source frontier failure: {label}"
      failed := failed+1
  if failed == 0 then IO.println s!"{tests.length} source call-frontier controls pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.SourceFrontierTests
