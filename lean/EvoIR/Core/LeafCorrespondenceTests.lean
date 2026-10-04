import EvoIR.Core.LeafCorrespondence
import EvoIR.Core.SourceExpressionTests

/-! Whole-leaf admission and refusal through the proved executable checker.
Original functions and changed-pair controls use the same structural path. -/
namespace EvoIR.Core.Symbolic.LeafTests

private def source : Source.Program := ⟨[0], [
  ⟨0, "sum", ⟨.uint, false⟩, false,
    [⟨1, ⟨.uint, false⟩, .none⟩, ⟨2, ⟨.uint, false⟩, .none⟩],
    .compound [.statement (.return ⟨.uint, false⟩
      (.binary .uint .add (.ident .uint 1) (.ident .uint 2))) ]⟩]⟩
private def target : IR.Program := ⟨[
  ⟨"sum", [.bits 32, .bits 32], [.bits 32], false, IR.Reader.emptyEffects,
    [⟨"entry", [⟨0, .bits 32⟩, ⟨1, .bits 32⟩],
      [⟨[⟨2, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩], .return [2]⟩]⟩]⟩

example : (leafPair source target).isSome = true := by decide

-- The theorem applies to all two-word inputs, not only a test vector. Its
-- source and IR entries are the actual initial-state constructors.
example (x y : BitVec 32) :
    ∃ se te,
      Source.initial source 0 [⟨.uint, x⟩, ⟨.uint, y⟩] = .ok se ∧
      IR.initial target "sum" [.bits 32 x, .bits 32 y] = .ok te ∧
      LoweringRefines source target .uint se te := by
  let accepted := (leafPair source target).get (by decide)
  exact accepted.refines [⟨.uint, x⟩, ⟨.uint, y⟩] (by rfl)

private def changedTarget (f : IR.Function → IR.Function) : IR.Program :=
  ⟨target.functions.map f⟩
example : (leafPair source (changedTarget fun f => {f with returns := [.bits 64]})).isNone := by decide
example : (leafPair source (changedTarget fun f => {f with internal := true})).isNone := by decide
example : (leafPair source (changedTarget fun f => {f with name := "other"})).isNone := by decide
example : (leafPair source (changedTarget fun f => {f with parameters := [.bits 64, .bits 32]})).isNone := by decide
example : (leafPair source (changedTarget fun f => {f with blocks := f.blocks ++ f.blocks})).isNone := by decide
example : (leafPair source (changedTarget fun f => {f with effects := {f.effects with io := true}})).isNone := by decide
example : (leafPair {source with entries := []} target).isNone := by decide
example : (leafPair {source with functions := source.functions ++ source.functions} target).isNone := by decide

private def p (id : Nat) (type : Source.IntType) : Source.Parameter := ⟨id, ⟨type, false⟩, .none⟩
example : (parameterEnvironment [p 1 .int, p 1 .int]
    [inputWord .int 0, inputWord .int 1]).isNone := by decide
example : (parameterEnvironment [p 1 .long] [inputWord .longLong 0]).isNone := by decide
example : (parameterEnvironment [p 1 .int] [inputWord .uint 0]).isNone := by decide
example : (parameterEnvironment [p 1 .int] ([] : List (CWord inputWidths))).isNone := by decide
example : (parameterWords [p 1 .int, p 2 .long]).map
    (CWord.denote (inputSubst [.ofNat .int 0x80000000, .ofNat .long 0xffffffffffffffff])) =
    [.ofNat .int 0x80000000, .ofNat .long 0xffffffffffffffff] := by decide

private def checkTexts (s t : String) : Bool :=
  match Source.readProgram s, IR.Reader.readProgram t with
  | .ok source, .ok target => (leafPair source target).isSome
  | _, _ => false

private def conversionPair (src dst : Source.IntType) : Source.Program × IR.Program :=
  let source := Source.Program.mk [0] [
    ⟨0, "convert", ⟨dst, false⟩, false, [p 1 src],
      .compound [.statement (.return ⟨dst, false⟩ (.ident src 1))]⟩]
  let code := if src.width = dst.width then [] else
    [IR.Instruction.mk [⟨1, .bits dst.width⟩]
      (.scalar (.cast (if src.width < dst.width then
        if src.signed then .sext else .zext else .trunc) src.width dst.width)) [0]]
  let target := IR.Program.mk [
    ⟨"convert", [.bits src.width], [.bits dst.width], false, IR.Reader.emptyEffects,
      [⟨"entry", [⟨0, .bits src.width⟩], code,
        .return [if src.width = dst.width then 0 else 1]⟩]⟩]
  (source, target)

private def bitwisePair (op : Bitwise) (left right : Source.IntType) : Source.Program × IR.Program :=
  let dst := Source.usualType left right
  let source := Source.Program.mk [0] [
    ⟨0, "bits", ⟨dst, false⟩, false, [p 1 left, p 2 right],
      .compound [.statement (.return ⟨dst, false⟩
        (.binary dst op.source (.ident left 1) (.ident right 2)))]⟩]
  let convert := fun (src : Source.IntType) (input output : Nat) =>
    if src.width = dst.width then [] else
      [IR.Instruction.mk [⟨output, .bits dst.width⟩]
        (.scalar (.cast (if src.signed then .sext else .zext) src.width dst.width)) [input]]
  let code := convert left 0 2 ++ convert right 1 3 ++
    [⟨[⟨4, .bits dst.width⟩], .scalar (.bin op.target dst.width),
      [if left.width = dst.width then 0 else 2, if right.width = dst.width then 1 else 3]⟩]
  let target := IR.Program.mk [
    ⟨"bits", [.bits left.width, .bits right.width], [.bits dst.width], false, IR.Reader.emptyEffects,
      [⟨"entry", [⟨0, .bits left.width⟩, ⟨1, .bits right.width⟩], code, .return [4]⟩]⟩]
  (source, target)

def bitwiseSelfTest : IO UInt32 := do
  let types : List Source.IntType := [.int, .uint, .long, .ulong, .longLong, .ulongLong]
  let ops : List Bitwise := [.and, .or, .xor]
  let mut failed := 0
  for op in ops do
    for left in types do
      for right in types do
        let (s, t) := bitwisePair op left right
        if (leafPair s t).isNone then
          IO.eprintln s!"whole-leaf bitwise pair refused: {repr left}, {repr right}"
          failed := failed+1
        let wrong := if op == .and then BinOp.or else .and
        let changed := {t with functions := t.functions.map fun f =>
          {f with blocks := f.blocks.map fun b => {b with instructions := b.instructions.map fun i =>
            match i.op with
            | .scalar (.bin _ n) => {i with op := .scalar (.bin wrong n)}
            | _ => i}}}
        if (leafPair s changed).isSome then
          IO.eprintln "whole-leaf bitwise operator mutation accepted"
          failed := failed+1
  if failed == 0 then
    IO.println "216 mixed-type whole-leaf bitwise cases pass (108 admitted, 108 mutations refused)"
    return 0
  return 1

def leafSelfTest : IO UInt32 := do
  let (sourceText, targetText) := ExpressionTests.originalFixedMulTexts
  let mut failures := 0
  if !checkTexts sourceText targetText then
    IO.eprintln "original complete FixedMul pair refused"
    failures := failures+1
  let targetMutations := [
    ("signed extension", "sext.bits<64>", "zext.bits<64>"),
    ("shift sign", "ashr.bits<64>", "lshr.bits<64>"),
    ("count sign", "shift_count_defined.signed", "shift_count_defined.unsigned"),
    ("count value", "const.bits<32> 16", "const.bits<32> 15"),
    ("missing overflow", "ub.assume.signed_overflow %v4\n", ""),
    ("missing count obligation", "ub.assume.invalid_shift_count %v7\n", ""),
    ("diagnostic", "ub.assume.invalid_shift_count", "ub.assume.signed_overflow"),
    ("wrong arithmetic", "mul.bits<64>", "add.bits<64>"),
    ("reused argument", "mul.bits<64> %v2, %v3", "mul.bits<64> %v2, %v2"),
    ("return wrong SSA", "ret %v9", "ret %v0"),
    ("extra return", "ret %v9", "ret %v9, %v0"),
    ("wrong return width", "-> (bits<32>)", "-> (bits<64>)"),
    ("entry parameter width", "%v1: bits<32>", "%v1: bits<64>"),
    ("duplicate parameter", "%v1: bits<32>", "%v0: bits<32>"),
    ("false effects", "may_trap=true", "may_trap=false"),
    ("function identity", "@FixedMul", "@other")]
  for (name, before, after) in targetMutations do
    let changed := targetText.replace before after
    if changed == targetText || checkTexts sourceText changed then
      IO.eprintln s!"whole-leaf target mutation not refused: {name}"
      failures := failures+1
  let sourceMutations := [
    ("source count", "00000010", "0000000f"),
    ("source operation", "\"mul\"", "\"add\""),
    ("source operand", "\"ident\",\"int\",3", "\"ident\",\"int\",4"),
    ("source shift", "\"shr\"", "\"shl\""),
    ("source signedness", "\"long\"", "\"ulong\""),
    ("duplicate parameter", "\"parameter\",4", "\"parameter\",3"),
    ("return conversion", "\"return\",[\"int\",false]", "\"return\",[\"long\",false]")]
  for (name, before, after) in sourceMutations do
    let changed := sourceText.replace before after
    if changed == sourceText || checkTexts changed targetText then
      IO.eprintln s!"whole-leaf source mutation not refused: {name}"
      failures := failures+1
  let changedPairs := [
    (sourceText.replace "00000010" "0000000f", targetText.replace "const.bits<32> 16" "const.bits<32> 15"),
    (sourceText.replace "FixedMul" "renamed", targetText.replace "FixedMul" "renamed"),
    (sourceText.replace "\"mul\"" "\"add\"", (targetText.replace "signed_mul_defined" "signed_add_defined").replace "mul.bits<64>" "add.bits<64>")]
  for (changedSource, changedTarget) in changedPairs do
    if !checkTexts changedSource changedTarget then
      IO.eprintln "structural leaf checker refused consistent changed pair"
      failures := failures+1
  let subSource := sourceText.replace "\"mul\"" "\"sub\""
  let subTarget := (targetText.replace "signed_mul_defined" "signed_sub_defined").replace
    "mul.bits<64>" "sub.bits<64>"
  if !checkTexts subSource subTarget ||
      checkTexts subSource (subTarget.replace "sub.bits<64> %v2, %v3" "sub.bits<64> %v3, %v2") then
    IO.eprintln "independent parameter order not enforced for subtraction"
    failures := failures+1
  let types : List Source.IntType := [.int, .uint, .long, .ulong, .longLong, .ulongLong]
  for src in types do
    for dst in types do
      let (s, t) := conversionPair src dst
      if (leafPair s t).isNone then
        IO.eprintln s!"whole-leaf return conversion refused: {repr src} -> {repr dst}"
        failures := failures+1
  if failures == 0 then
    IO.println s!"{targetMutations.length+sourceMutations.length+changedPairs.length+3+types.length*types.length} whole-leaf cases pass (source-to-EvoIR only; core-v1 incomplete)"
    return 0
  return 1

end EvoIR.Core.Symbolic.LeafTests
