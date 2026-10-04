import EvoIR.Core.AbsCorrespondence
import EvoIR.Core.IRWellFormed

/-! Complete-input mutations of the first original-function proof case. The
wire inputs below are the existing exporter's actual -O0 -fno-builtin output,
not a source-expression summary or normalized target program. -/
namespace EvoIR.Core

private def sourceText := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[0],[["function",0,"abs",["int",false],false,[["parameter",1,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["binary","int","gt",["ident","int",1],["lit","int","00000000"]],["ident","int",1],["unary","int","neg",["ident","int",1]]]]]]]]]"#

private def targetText := "func @abs(%arg0: bits<32>) -> (bits<32>) cc = target_default\n" ++
  "effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }\n{\n" ++
  "^entry(%v0: bits<32>):\n%v1 = const.bits<32> 0\n%v2 = icmp.slt %v1, %v0\n" ++
  "cond_br %v2, ^b1(%v0), ^b2(%v0)\n" ++
  "^b1(%v3: bits<32>):\nbr ^b3(%v3, %v3)\n" ++
  "^b2(%v4: bits<32>):\n%v5 = const.bits<32> 0\n" ++
  "%v6 = signed_sub_defined %v5, %v4\nub.assume.signed_overflow %v6\n" ++
  "%v7 = sub.bits<32> %v5, %v4\nbr ^b3(%v4, %v7)\n" ++
  "^b3(%v8: bits<32>, %v9: bits<32>):\nret %v9\n}\n"

private def accepts (s t : String) : Bool :=
  match Source.readProgram s, IR.Reader.readProgram t with
  | .ok source, .ok target => AbsCorrespondence.checkPilotPair source target
  | _, _ => false

def correspondenceSelfTest : IO UInt32 := do
  let mut failures := 0
  if !accepts sourceText targetText then
    IO.eprintln "original full source/IR pilot pair refused"
    failures := failures+1
  let sourceChanges := [
    ("condition", "\"gt\"", "\"lt\""),
    ("negation", "\"neg\"", "\"plus\""),
    ("literal", "00000000", "00000001"),
    ("identifier", "\"ident\",\"int\",1", "\"ident\",\"int\",2"),
    ("rank", "\"int\"", "\"long\""),
    ("parameter", "\"parameter\",1", "\"parameter\",2"),
    ("parameter storage", "\"none\"", "\"register\""),
    ("name", "\"abs\"", "\"other\""),
    ("entry", "\"checked\",[0]", "\"checked\",[1]"),
    ("symbol", "\"function\",0", "\"function\",1"),
    ("linkage", "[\"int\",false],false,", "[\"int\",false],true,"),
    ("qualifier", "[\"int\",false]", "[\"int\",true]"),
    ("missing parameter", "[[\"parameter\",1,[\"int\",false],\"none\"]]", "[]")]
  for (name, before, after) in sourceChanges do
    let changed := sourceText.replace before after
    if changed == sourceText || accepts changed targetText then
      IO.eprintln s!"source-pair mutation not refused: {name}"
      failures := failures+1
  let targetChanges := [
    ("predicate", "signed_sub_defined", "signed_add_defined"),
    ("guard operands", "signed_sub_defined %v5, %v4", "signed_sub_defined %v4, %v5"),
    ("removed obligation", "ub.assume.signed_overflow %v6\n", ""),
    ("wrong diagnostic", "ub.assume.signed_overflow", "ub.assume.division_by_zero"),
    ("guard bypass", "ub.assume.signed_overflow %v6", "ub.assume.signed_overflow %v2"),
    ("weakened effect", "may_trap=true", "may_trap=false"),
    ("added effect", "io=false", "io=true"),
    ("compare", "icmp.slt", "icmp.ult"),
    ("arithmetic", "sub.bits<32>", "add.bits<32>"),
    ("arithmetic operands", "sub.bits<32> %v5, %v4", "sub.bits<32> %v4, %v5"),
    ("return", "ret %v9", "ret %v8"),
    ("branch", "^b1(%v0), ^b2(%v0)", "^b2(%v0), ^b1(%v0)"),
    ("unused edge argument", "br ^b3(%v4, %v7)", "br ^b3(%v7, %v7)"),
    ("new cycle", "br ^b3(%v3, %v3)", "br ^b1(%v3)"),
    ("literal", "const.bits<32> 0", "const.bits<32> 1"),
    ("signature", "bits<32>", "bits<64>"),
    ("name", "@abs", "@other"),
    ("extra definition", "}\n", "}\n" ++ targetText)]
  for (name, before, after) in targetChanges do
    let changed := targetText.replace before after
    if changed == targetText || accepts sourceText changed then
      IO.eprintln s!"target-pair mutation not refused: {name}"
      failures := failures+1
  -- A new/dead source statement must not be omitted by structural matching.
  let original := AbsCorrespondence.source
  let changed := {original with functions := original.functions.map fun f =>
    {f with body := .compound [.statement f.body, .statement .null]}}
  if AbsCorrespondence.checkPilotPair changed AbsCorrespondence.target then
    IO.eprintln "extra complete-source statement accepted"
    failures := failures+1
  -- These diagnostics are outside core-v1 and cannot stand in for termination.
  for code in [3001, 3006, 0] do
    if (checkedDiagnostic code).isSome then
      IO.eprintln s!"non-profile diagnostic observable: {code}"
      failures := failures+1
  if failures == 0 then
    IO.println s!"{sourceChanges.length + targetChanges.length + 5} complete-input correspondence cases pass (one source/IR pilot proof; backend unproved)"
    return 0
  return 1

end EvoIR.Core
