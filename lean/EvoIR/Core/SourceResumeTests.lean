import EvoIR.Core.SourceSpan
import EvoIR.Core.CallLoopFixtures
import EvoIR.Core.ReturnContinuation

/-! Post-call continuation regressions. The checker constructs a symbolic
diagram before any sample valuation is chosen. Execution samples additionally
check the actual source machine, including right-operand effects and faults. -/
namespace EvoIR.Core.Symbolic.SourceResumeTests

private def checks (p : Source.Program) (type : Source.IntType)
    (frames : List Source.Frame) (locals : LocalStore inputWidths)
    (samples : List (Nat × Source.Result)) : Bool := Id.run do
  let cursor : SourceCursor inputWidths :=
    ⟨.value (inputWord type 0), locals,
      frames ++ [.return ⟨type, false⟩, .function none type]⟩
  let some built := sourceCutDiagram p [] 64 true cursor | return false
  for (number, expected) in samples do
    let σ := inputSubst [Source.Value.ofNat type number]
    let symbolic := match built.val.eval σ with
      | .error fault => some (.error fault)
      | .ok exit => match exit.control with
        | .returned word => some (.ok (word.denote σ))
        | _ => none
    if symbolic != some expected then return false
    let actual := Source.runSteps p 1000 (cursor.denote σ 901)
    let .halted answer := actual.control | return false
    if answer != expected then return false
  return true

private def untilCall (p : Source.Program) : Nat → Source.State → Option Source.State
  | 0, _ => none
  | n+1, state => match state.control with
    | .call .. => some state
    | _ => (Source.step p state).bind (untilCall p n)

/-- The unchanged caller's suspended frames are read from its actual source
execution. Only this regression driver samples entry arguments; the admitted
suffix diagram quantifies over independent caller inputs and return values. -/
private def originalSuffix (source ir : String) (pc : Nat := 1) : Bool := Id.run do
  let .ok s := Source.readProgram source | return false
  let .ok t := IR.Reader.readProgram ir | return false
  let some f := s.functions.find? (s.entries.contains ·.symbol) | return false
  let some g := t.functions.find? (·.name == f.name) | return false
  let some block := g.blocks.head? | return false
  let some call := block.instructions.head? | return false
  let .call name := call.op | return false
  let args := f.parameters.map fun arg => Source.Value.ofNat arg.type.type 0
  let .ok entry := Source.initial s f.symbol args | return false
  let some started := Source.step s entry | return false
  let some suspended := untilCall s 100 started | return false
  let .call id type _ := suspended.control | return false
  let some callee := s.functions.find? (·.symbol == id) | return false
  if callee.name != name then return false
  let words := parameterWords f.parameters
  let some locals := parameterStore f.parameters words | return false
  let some before := bindSlots callTokenTypes block.parameters (callInputSlots words) | return false
  return (checkedReturn s t f.symbol g true f.returnType.type type locals suspended.continuation
    block.name pc before call.results true 64).isSome

def sourceResumeSelfTest : IO UInt32 := do
  let .ok p := Source.readProgram Source.CallLoopTests.callLoop | return 1
  let uint (n : Nat) : Source.Result := .ok (.ofNat .uint n)
  let int (n : Nat) : Source.Result := .ok (.ofNat .int n)
  let overflow : Source.Result := .error (.diagnostic 3004)
  let divideZero : Source.Result := .error (.diagnostic 3002)
  let one : Source.Expr := .literal .uint 1
  let update : Source.Expr := .assign .uint .set 71 ⟨.uint, false⟩
    (.ident .uint 71) (.literal .uint 7)
  let locals : LocalStore inputWidths := [⟨71, ⟨.uint, false⟩, some (.const 32 3)⟩]
  let tests := [
    ("unsigned post-call add", checks p .uint [.binaryLeft .uint .add one] []
      [(40, uint 41), (0xffffffff, uint 0)]),
    ("signed post-call overflow", checks p .int [.binaryLeft .int .add (.literal .int 1)] []
      [(40, int 41), (0x7fffffff, overflow)]),
    ("post-call division diagnostic", checks p .uint
      [.binaryLeft .uint .div (.literal .uint 0)] [] [(40, divideZero)]),
    ("right operand writes before next expression", checks p .uint
      [.binaryLeft .uint .add update, .binaryLeft .uint .add (.ident .uint 71)] locals
      [(40, uint 54)]),
    ("right operand calls actual function", checks p .uint
      [.binaryLeft .uint .add (.call .uint 0 [(⟨.uint, false⟩, one)])] [] [(40, uint 44)]),
    ("first fault skips later continuation", checks p .int
      [.binaryLeft .int .add (.literal .int 1),
       .binaryLeft .int .div (.literal .int 0)] [] [(0x7fffffff, overflow)]),
    ("post-call conditional", checks p .uint
      [.choice .uint (.literal .uint 17) (.literal .uint 29)] []
      [(0, uint 29), (1, uint 17), (0xffffffff, uint 17)]),
    ("conditional skips unselected fault", checks p .uint
      [.choice .uint (.literal .uint 7)
        (.binary .uint .div (.literal .uint 1) (.literal .uint 0))] []
      [(1, uint 7), (0, divideZero)]),
    ("post-call assignment updates later read", checks p .uint
      [.assign .uint .set 71 ⟨.uint, false⟩,
       .binaryLeft .uint .add (.ident .uint 71)] locals
      [(0, uint 0), (21, uint 42), (0xffffffff, uint 0xfffffffe)]),
    ("post-call compound assignment reads old local", checks p .uint
      [.assign .uint .add 71 ⟨.uint, false⟩,
       .binaryLeft .uint .add (.ident .uint 71)] locals
      [(4, uint 14), (0xffffffff, uint 4)]),
    ("compound assignment reports overflow before write", checks p .int
      [.assign .int .add 72 ⟨.int, false⟩,
       .binaryLeft .int .div (.literal .int 0)]
      [⟨72, ⟨.int, false⟩, some (.const 32 0x7fffffff)⟩]
      [(1, overflow)]),
    ("post-call unary signed overflow", checks p .int [.unary .int .neg] []
      [(1, int 0xffffffff), (0x80000000, overflow)]),
    ("post-call unary boolean", checks p .int [.unary .int .not] []
      [(0, int 1), (1, int 0), (0x80000000, int 0)])]
  let mut failed := 0
  for (label, passed) in tests do
    if !passed then
      IO.eprintln s!"source resume failure: {label}"
      failed := failed + 1
  if !originalSuffix Source.CallLoopTests.callLoop Source.CallLoopTests.target then
    IO.eprintln "original call_loop post-return source/IR pair refused"
    failed := failed+1
  let mutations := [
    ("%v7 = const.bits<32> 1", "%v7 = const.bits<32> 2"),
    ("add.bits<32> %v4, %v7", "sub.bits<32> %v4, %v7"),
    ("add.bits<32> %v4, %v7", "add.bits<32> %v0, %v7"),
    ("ret %v8, %v5, %v6", "ret %v4, %v5, %v6"),
    ("%v7 = const.bits<32> 1", "%v7 = const.bits<64> 1")]
  for (before, after) in mutations do
    let changed := Source.CallLoopTests.target.replace before after
    if changed == Source.CallLoopTests.target || originalSuffix Source.CallLoopTests.callLoop changed then
      IO.eprintln s!"post-return target mutation admitted: {before}"
      failed := failed+1
  for pc in [0, 2, 4] do
    if originalSuffix Source.CallLoopTests.callLoop Source.CallLoopTests.target pc then
      IO.eprintln s!"wrong return instruction position admitted: {pc}"
      failed := failed+1
  let types : List Source.IntType := [.int, .uint, .long, .ulong, .longLong, .ulongLong]
  let mut captures := 0
  for type in types do
    for number in [0, 2^(type.width-1), 2^type.width-1] do
      let value := Source.Value.ofNat type number
      let σ := inputSubst [Source.Value.ofNat type 7]
      let τ := returnSubst σ value
      let saved : LocalStore inputWidths := [
        ⟨70, ⟨type, false⟩, some (inputWord type 0).term⟩,
        ⟨71, ⟨type, false⟩, none⟩]
      let scope : Scope inputWidths := [(0, .word (inputWord type 0).word), (1, .opaque 0)]
      if (inputWord type 0).denote τ != value || saved.forReturn.denote τ != saved.denote σ ||
          scope.forReturn.denote τ callTokens != scope.denote σ callTokens then
        IO.eprintln s!"return value captured caller input: {repr type} {number}"
        failed := failed+1
      captures := captures+1
  if failed == 0 then IO.println s!"{tests.length} symbolic continuations, original call_loop suffix, {mutations.length+3} suffix mutations and {captures} fresh-return controls pass"
  return if failed == 0 then 0 else 1

end EvoIR.Core.Symbolic.SourceResumeTests
