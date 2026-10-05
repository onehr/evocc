import EvoIR.Core.SourceTargetCheck
import EvoIR.Core.AbsCorrespondence
import EvoIR.Core.CallLoopFixtures

namespace EvoIR.Core.Symbolic.SourceTargetCheckTests

example {source : Source.Program} {target : IR.Program} (pair : ReturnPair source target) :
    pair.toCallFrontierPair.tokens = false := rfl

example {source : Source.Program} {target : IR.Program} (pair : CallPair source target) :
    pair.toCallFrontierPair.tokens = true := rfl

example {source : Source.Program} {target : IR.Program} (pair : LoopPair source target) :
    pair.toCallFrontierPair.tokens = pair.diagram.tokens := rfl

example {source : Source.Program} {target : IR.Program} (pair : CallFrontierPair source target)
    (args : List Source.Value)
    (typed : args.map Source.Value.type = pair.sourceFunction.parameters.map (fun p => p.type.type)) :
    ∃ sourceEntry targetEntry,
      Source.initial source pair.sourceFunction.symbol args = .ok sourceEntry ∧
      IR.initial target pair.targetFunction.name (loopInputValues pair.tokens args) = .ok targetEntry ∧
      LoweringRefines source target pair.sourceFunction.returnType.type sourceEntry targetEntry :=
  pair.refines args typed

private def finite := checkSourceTargetPair AbsCorrespondence.source AbsCorrespondence.target .finite

example : finite.map Prod.fst = some .leaf := by decide

private def callRoute : Option SourceTargetRoute := do
  let source ← (Source.readProgram Source.CallLoopTests.loopCall).toOption
  let callee ← source.functions.head?
  let target ← (IR.Reader.readProgram Source.CallLoopTests.target).toOption
  let (route, _) ← checkSourceTargetPair
    {source with entries := [callee.symbol], functions := [callee]} target .finite
  return route

private def loopRoute : Option SourceTargetRoute := do
  let source ← (Source.readProgram Source.CallLoopTests.loopCall).toOption
  let target ← (IR.Reader.readProgram Source.CallLoopTests.target).toOption
  let [entry] := source.entries | none
  let selected ← source.functions.find? (·.symbol == entry)
  let (budget, layouts) ← readCutpoints selected Source.CallLoopTests.proposal
  let (route, _) ← checkSourceTargetPair source target (.cutpoints layouts budget)
  return route

private def closureRoute : Option SourceTargetRoute := do
  let source ← (Source.readProgram Source.CallLoopTests.callLoop).toOption
  let target ← (IR.Reader.readProgram Source.CallLoopTests.target).toOption
  let callee ← source.functions.find? (·.symbol == 2)
  let (budget, layouts) ← readCutpoints callee Source.CallLoopTests.proposal
  let (route, _) ← checkSourceTargetPair source target
    (.closure (fun id => if id == 2 then some (budget, layouts) else none) budget)
  return route

private def privateSource : Source.Program :=
  ⟨[0], [⟨0, "stored", ⟨.int, false⟩, false, [⟨1, ⟨.int, false⟩, .none⟩],
    .compound [
      .declaration ⟨2, "saved", ⟨.int, false⟩, .none,
        some (⟨.int, false⟩, .ident .int 1)⟩,
      .statement (.return ⟨.int, false⟩ (.ident .int 2))]⟩]⟩

private def privateCode : List IR.Instruction := [
  ⟨[⟨3, .pointer⟩, ⟨4, .memory⟩, ⟨5, .world⟩], .allocate 2 4 4, [1, 2]⟩,
  ⟨[⟨6, .memory⟩], .access 4 4, [4, 3]⟩,
  ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 0]⟩,
  ⟨[⟨9, .memory⟩], .access 4 4, [7, 3]⟩,
  ⟨[⟨10, .bits 32⟩, ⟨11, .memory⟩, ⟨12, .world⟩], .load 32, [9, 8, 3]⟩]

private def privateTarget (code : List IR.Instruction := privateCode) : IR.Program :=
  ⟨[⟨"stored", [.bits 32, .memory, .world], [.bits 32, .memory, .world],
    false, ⟨true, true, false, false, false, false, true, false⟩,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .memory⟩, ⟨2, .world⟩], code, .return [10, 11, 12]⟩]⟩]⟩

private def privatePair := checkSourceTargetPair privateSource (privateTarget) .finite

private def wrongValueCode : List IR.Instruction :=
  privateCode.take 2 ++ [⟨[⟨13, .bits 32⟩], .constant 32 29, []⟩,
    ⟨[⟨7, .memory⟩, ⟨8, .world⟩], .store, [6, 5, 3, 13]⟩] ++ privateCode.drop 3

-- The same finite pair is acceptable without a proposal; an explicit failed
-- proposal must nevertheless refuse instead of falling back to that route.
example : (checkSourceTargetPair AbsCorrespondence.source AbsCorrespondence.target
    (.cutpoints [] 0)).isNone = true := by decide

example : (checkSourceTargetPair AbsCorrespondence.source AbsCorrespondence.target
    (.closure (fun _ => none) 0)).isNone = true := by decide

def sourceTargetCheckSelfTest : IO UInt32 := do
  let controls := [
    ("finite retains leaf route", decide (finite.map Prod.fst = some .leaf)),
    ("finite retains tokenless entry", finite.any fun result => !result.2.tokens),
    ("finite retains call route", decide (callRoute = some .call)),
    ("memory-bearing all-argument pair uses common proof route",
      privatePair.any fun result => result.1 == .call && result.2.tokens),
    ("memory-bearing pair requires the new storage route",
      (returnPair privateSource (privateTarget)).isNone &&
        (callPair privateSource (privateTarget)).isNone),
    ("private pair with wrong symbolic stored value refuses",
      (checkSourceTargetPair privateSource (privateTarget wrongValueCode) .finite).isNone),
    ("private pair with explicit rejected cutpoints has no finite fallback",
      (checkSourceTargetPair privateSource (privateTarget) (.cutpoints [] 0)).isNone),
    ("explicit cutpoints retain loop route", decide (loopRoute = some .loop)),
    ("explicit callee plans retain loop-call route", decide (closureRoute = some .loopCall)),
    ("explicit cutpoint refusal has no finite fallback",
      (checkSourceTargetPair AbsCorrespondence.source AbsCorrespondence.target (.cutpoints [] 0)).isNone),
    ("explicit closure refusal has no finite fallback",
      (checkSourceTargetPair AbsCorrespondence.source AbsCorrespondence.target
        (.closure (fun _ => none) 0)).isNone),
    ("duplicate selected entry refuses",
      (checkSourceTargetPair {AbsCorrespondence.source with entries := [0, 0]}
        AbsCorrespondence.target .finite).isNone)]
  let mut failures : UInt32 := 0
  for (name, passed) in controls do
    if !passed then
      IO.eprintln s!"source/IR dispatcher control failed: {name}"
      failures := failures + 1
  if failures == 0 then IO.println s!"{controls.length} source/IR dispatcher controls pass"
  return failures

end EvoIR.Core.Symbolic.SourceTargetCheckTests
