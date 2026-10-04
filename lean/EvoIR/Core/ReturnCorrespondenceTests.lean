import EvoIR.Core.ReturnCorrespondence
import EvoIR.Core.AbsCorrespondence
import EvoIR.Core.CorrespondenceTests
import EvoIR.Core.SourceExpressionTests

namespace EvoIR.Core.Symbolic.ReturnTests

-- The original four-block function goes through structural checking, not
-- AbsCorrespondence.checkPilotPair's exact-input specialization.
example : (returnPair AbsCorrespondence.source AbsCorrespondence.target).isSome = true := by decide

example (x : BitVec 32) :
    ∃ se te,
      Source.initial AbsCorrespondence.source 0 [⟨.int, x⟩] = .ok se ∧
      IR.initial AbsCorrespondence.target "abs" [.bits 32 x] = .ok te ∧
      LoweringRefines AbsCorrespondence.source AbsCorrespondence.target .int se te := by
  let accepted := (returnPair AbsCorrespondence.source AbsCorrespondence.target).get (by decide)
  exact accepted.refines [⟨.int, x⟩] (by rfl)

private def renameSource (name : String) (s : Source.Program) : Source.Program :=
  {s with functions := s.functions.map fun f => {f with name}}
private def renameTarget (name : String) (t : IR.Program) : IR.Program :=
  {t with functions := t.functions.map fun f => {f with name}}

example : (returnPair (renameSource "renamed" AbsCorrespondence.source)
    (renameTarget "renamed" AbsCorrespondence.target)).isSome = true := by decide

private def mutate (change : IR.Block → IR.Block) : IR.Program :=
  {AbsCorrespondence.target with functions := AbsCorrespondence.target.functions.map fun f =>
    {f with blocks := f.blocks.map change}}

example : (returnPair AbsCorrespondence.source (mutate fun b =>
    match b.terminator with
    | .conditional id yes no => {b with terminator := .conditional id no yes}
    | _ => b)).isNone = true := by decide

example : (returnPair AbsCorrespondence.source (mutate fun b =>
    {b with instructions := b.instructions.filter fun i =>
      match i.op with | .assume _ => false | _ => true})).isNone = true := by decide

example : (returnPair AbsCorrespondence.source (mutate fun b =>
    {b with instructions := b.instructions.map fun i =>
      match i.op with
      | .assume _ => {i with op := .assume .invalidShiftCount}
      | _ => i})).isNone = true := by decide

example : (returnPair AbsCorrespondence.source (mutate fun b =>
    match b.terminator with
    | .branch edge => {b with terminator := .branch {edge with arguments := edge.arguments.reverse}}
    | _ => b)).isNone = true := by decide

example : (returnPair AbsCorrespondence.source (mutate fun b =>
    match b.terminator with
    | .conditional id yes no => {b with terminator := .conditional id {yes with block := "entry"} no}
    | _ => b)).isNone = true := by decide

private def accepts (source target : String) : Bool :=
  match Source.readProgram source, IR.Reader.readProgram target with
  | .ok s, .ok t => (returnPair s t).isSome
  | _, _ => false

def returnSelfTest : IO UInt32 := do
  let (source, target) := originalAbsTexts
  let (mulSource, mulTarget) := ExpressionTests.originalFixedMulTexts
  let mut failures := 0
  for (s, t) in [(source, target), (mulSource, mulTarget)] do
    if !accepts s t then
      IO.eprintln "generic return checker refused an original whole-function input"
      failures := failures+1
  let mutations := [
    ("swapped branches", "^b1(%v0), ^b2(%v0)", "^b2(%v0), ^b1(%v0)"),
    ("signedness", "icmp.slt", "icmp.ult"),
    ("comparison operands", "icmp.slt %v1, %v0", "icmp.slt %v0, %v1"),
    ("missing obligation", "ub.assume.signed_overflow %v6\n", ""),
    ("wrong diagnostic", "ub.assume.signed_overflow", "ub.assume.division_by_zero"),
    ("wrong guard", "signed_sub_defined", "signed_add_defined"),
    ("wrong operation", "sub.bits<32>", "add.bits<32>"),
    ("wrong operand order", "sub.bits<32> %v5, %v4", "sub.bits<32> %v4, %v5"),
    ("wrong return", "ret %v9", "ret %v8"),
    ("join swap", "br ^b3(%v4, %v7)", "br ^b3(%v7, %v4)"),
    ("cycle", "br ^b3(%v3, %v3)", "br ^b1(%v3)"),
    ("wrong return conversion", "-> (bits<32>)", "-> (bits<64>)"),
    ("weakened effects", "may_trap=true", "may_trap=false"),
    ("extra effects", "io=false", "io=true"),
    ("poison condition", "%v1 = const.bits<32> 0", "%v1 = uninitialized.bits<32>"),
    ("constant branch", "cond_br %v2", "cond_br %v0")]
  for (name, before, after) in mutations do
    let changed := target.replace before after
    if changed == target || accepts source changed then
      IO.eprintln s!"generic return mutation not refused: {name}"
      failures := failures+1
  let sourceMutations := [
    ("condition", "\"gt\"", "\"lt\""),
    ("negation", "\"neg\"", "\"plus\""),
    ("constant", "00000000", "00000001"),
    ("type", "\"int\"", "\"uint\"")]
  for (name, before, after) in sourceMutations do
    let changed := source.replace before after
    if changed == source || accepts changed target then
      IO.eprintln s!"generic return source mutation not refused: {name}"
      failures := failures+1
  -- These pairs preserve meaning but differ from the frozen pilot's trees.
  -- In particular, the unused join argument is not an observable result.
  let acceptedPairs := [
    (source.replace "abs" "renamed", target.replace "abs" "renamed"),
    (source.replace "00000000" "00000001",
      target.replace "%v1 = const.bits<32> 0" "%v1 = const.bits<32> 1"),
    (source, target.replace "br ^b3(%v4, %v7)" "br ^b3(%v7, %v7)"),
    (source.replace "\"gt\"" "\"ge\"", target.replace "icmp.slt" "icmp.sle")]
  for (s, t) in acceptedPairs do
    if !accepts s t then
      IO.eprintln "generic return checker refused a consistent changed pair"
      failures := failures+1
  if failures == 0 then
    IO.println s!"{2+mutations.length+sourceMutations.length+acceptedPairs.length} branching whole-return cases pass (two originals; loops/state/calls remain open)"
    return 0
  return 1

end EvoIR.Core.Symbolic.ReturnTests
