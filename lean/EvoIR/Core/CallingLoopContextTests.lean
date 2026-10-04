import EvoIR.Core.CallingLoopContext
import EvoIR.Core.CallLoopFixtures

/-! Regression controls for the actual suspended-caller boundary. Operational
samples exercise restoration and diagnostics; the context theorem, not these
bounded runs, supplies unbounded composition. Caller admission is still open. -/
namespace EvoIR.Core.Source.CallingLoopContextTests

open Symbolic CallLoopTests

private def sourceUntilBoundary (p : Program) : Nat → State → State
  | 0, s => s
  | n+1, s =>
    if s.functionId == 901 then s
    else match step p s with
    | none => s
    | some next => sourceUntilBoundary p n next

private def targetUntilBoundary (p : IR.Program) : Nat → IR.State → IR.State
  | 0, s => s
  | n+1, s => match s.control with
    | .halted _ => s
    | .running a => if a.function == "suspended-caller" then s
        else match IR.step p s with
        | none => s
        | some next => targetUntilBoundary p n next

private def contextCase (source ir plan : String) (numbers : List Nat)
    (expected : Result) (expectedFrame : Nat) : Bool := Id.run do
  let .ok s := readProgram source | return false
  let .ok t := IR.Reader.readProgram ir | return false
  let some f := s.functions.find? (s.entries.contains ·.symbol) | return false
  let some g := t.functions.find? (·.name == f.name) | return false
  let some (budget, layouts) := readCutpoints f plan | return false
  let some d := loopDiagram s t f g layouts budget | return false
  if numbers.length != f.parameters.length then return false
  let args := (f.parameters.zip numbers).map fun (p, n) => Value.ofNat p.type.type n
  let σ := inputSubst args
  let saved : Locals := [⟨701, ⟨.uint, false⟩, some (.ofNat .uint 99)⟩]
  let continuation := [Frame.discard, Frame.function none .uint]
  let caller : SourceReturnContext := ⟨901, saved, continuation⟩
  let begin := (SourceCursor.mk (.statement f.body) d.locals
    [.function none f.returnType.type]).denote σ f.symbol (some caller)
  let results := [IR.Binding.mk 80 (.bits f.returnType.type.width),
    IR.Binding.mk 81 .memory, IR.Binding.mk 82 .world]
  let savedTarget : IR.Bindings := [(70, .bits 32 99)]
  let frame : IR.ReturnFrame := ⟨⟨"suspended-caller", "resume", 7, savedTarget, 11⟩, results⟩
  let outer : IR.ReturnFrame := ⟨⟨"outer-caller", "outer", 9, [], 3⟩, []⟩
  let entered := cutTargetState σ g d.block.name 0 17 37 d.scope [frame, outer]
  let sf := sourceUntilBoundary s 20000 begin
  let tf := targetUntilBoundary t 20000 entered
  if tf.nextFrame != expectedFrame then return false
  match expected with
  | .error fault =>
    let .halted (.error actual) := sf.control | return false
    let .halted (.error actualIR) := tf.control | return false
    return actual == fault && actualIR == fault && sf.functionId != caller.functionId &&
      tf.stack.length == 3
  | .ok value =>
    let .value actual := sf.control | return false
    let .running actualIR := tf.control | return false
    let .ok bound := IR.appendResults savedTarget results (loopInputValues true [value])
      | return false
    let [rest] := tf.stack | return false
    return actual == value && sf.locals == saved && sf.continuation == continuation &&
      sf.functionId == caller.functionId && actualIR.function == "suspended-caller" &&
      actualIR.block == "resume" && actualIR.pc == 7 && actualIR.owner == 11 &&
      actualIR.values == bound && rest.caller.function == "outer-caller" && rest.caller.owner == 3

def callingLoopContextSelfTest : IO UInt32 := do
  let cases : List (String × String × String × List Nat × Result × Nat) := [
    (loopCall, target, proposal, [1, 0], .ok (.ofNat .uint 1), 37),
    (loopCall, target, proposal, [1, 3], .ok (.ofNat .uint 40), 40),
    (loopCall, target, proposal, [0xffffffff, 1], .ok (.ofNat .uint 0xfffffffe), 38),
    (loopBranch, controlTarget, proposal, [0, 20], .ok (.ofNat .uint 0), 57),
    (loopChecked, controlTarget, checkedProposal, [0x7ffffffe, 1], .ok (.ofNat .int 0x7fffffff), 38),
    (loopChecked, controlTarget, checkedProposal, [0x7ffffffe, 2], .error (.diagnostic 3004), 39)]
  let mut failures := 0
  for (source, ir, plan, args, expected, frames) in cases do
    if !contextCase source ir plan args expected frames then
      IO.eprintln s!"cyclic callee context restoration failed: {args}"
      failures := failures+1
  -- A return classification must not discard an unconsumed callee frame.
  let word : CWord inputWidths := ⟨.uint, .const 32 0⟩
  let invalid : SourceCursor inputWidths := ⟨.returned word, [], [.discard]⟩
  let dummy : IR.Function := ⟨"test", [], [], false, IR.Reader.emptyEffects, []⟩
  if (checkExit [] 0 dummy false .uint invalid (.returned word)).isSome then
    IO.eprintln "return with residual callee continuation admitted"
    failures := failures+1
  if failures == 0 then
    IO.println s!"{cases.length} cyclic callee suspended-context controls and residual-return refusal pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Source.CallingLoopContextTests
