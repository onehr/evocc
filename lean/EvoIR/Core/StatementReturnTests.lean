import EvoIR.Core.ReturnCorrespondence

/-! Actual exports of statement-return.c and the unchanged short_circuit body
in control.c. Tests mutate the complete source/target inputs at observed
declaration, scope, branch, sequencing and diagnostic boundaries. -/
namespace EvoIR.Core.Symbolic.StatementReturnTests

private def source := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[0],[["function",0,"statement_return",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["uint",false],"none"]],["compound",[["object",3,"a",["uint",true],"none",["store_scalar",["uint",true],["binary","uint","add",["ident","uint",2],["lit","uint","00000001"]]]],["object",4,"z",["int",false],"none",["store_scalar",["int",false],["cast","int",["int",false],["ident","uint",3]]]],["compound",[["object",5,"t",["int",false],"none",["store_scalar",["int",false],["ident","int",1]]],["expr",["assign","int","add",4,["int",false],["ident","int",4],["ident","int",5]]]]],["if",["ident","int",1],["expr",["assign","int","add",4,["int",false],["ident","int",4],["lit","int","00000002"]]],["expr",["assign","int","sub",4,["int",false],["ident","int",4],["lit","int","00000003"]]]],["return",["int",false],["ident","int",4]]]]]]]"#
private def target := r#"func @statement_return(%arg0: bits<32>, %arg1: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>):
    %v2 = const.bits<32> 1
    %v3 = add.bits<32> %v1, %v2
    %v4 = signed_add_defined %v3, %v0
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v3, %v0
    %v6 = const.bits<32> 0
    %v7 = icmp.ne %v0, %v6
    cond_br %v7, ^b1(%v5), ^b2(%v5)

^b1(%v8: bits<32>):
    %v9 = const.bits<32> 2
    %v10 = signed_add_defined %v8, %v9
    ub.assume.signed_overflow %v10
    %v11 = add.bits<32> %v8, %v9
    br ^b3(%v11)

^b2(%v12: bits<32>):
    %v13 = const.bits<32> 3
    %v14 = signed_sub_defined %v12, %v13
    ub.assume.signed_overflow %v14
    %v15 = sub.bits<32> %v12, %v13
    br ^b3(%v15)

^b3(%v16: bits<32>):
    ret %v16
}"#
private def shortSource := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[5],[["function",5,"short_circuit",["int",false],false,[["parameter",6,["int",false],"none"]],["compound",[["object",7,"y",["int",false],"none",["store_scalar",["int",false],["lit","int","00000000"]]],["if",["binary","int","and",["ident","int",6],["assign","int","set",7,["int",false],["ident","int",7],["lit","int","00000003"]]],["expr",["assign","int","add",7,["int",false],["ident","int",7],["lit","int","00000001"]]],null],["if",["binary","int","or",["unary","int","not",["ident","int",6]],["assign","int","set",7,["int",false],["ident","int",7],["lit","int","00000009"]]],["expr",["assign","int","add",7,["int",false],["ident","int",7],["lit","int","00000002"]]],null],["return",["int",false],["ident","int",7]]]]]]]"#
private def shortTarget := r#"func @short_circuit(%arg0: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>):
    %v1 = const.bits<32> 0
    %v2 = const.bits<32> 0
    %v3 = icmp.ne %v0, %v2
    cond_br %v3, ^b1(%v0, %v1), ^b2(%v0, %v1)

^b1(%v4: bits<32>, %v5: bits<32>):
    %v6 = const.bits<32> 3
    %v7 = const.bits<32> 0
    %v8 = icmp.ne %v6, %v7
    br ^b3(%v4, %v6, %v8)

^b2(%v9: bits<32>, %v10: bits<32>):
    %v11 = const.bits<1> 0
    br ^b3(%v9, %v10, %v11)

^b3(%v12: bits<32>, %v13: bits<32>, %v14: bits<1>):
    cond_br %v14, ^b4(%v12, %v13), ^b5(%v12, %v13)

^b4(%v15: bits<32>, %v16: bits<32>):
    %v17 = const.bits<32> 1
    %v18 = signed_add_defined %v16, %v17
    ub.assume.signed_overflow %v18
    %v19 = add.bits<32> %v16, %v17
    br ^b6(%v15, %v19)

^b5(%v20: bits<32>, %v21: bits<32>):
    br ^b6(%v20, %v21)

^b6(%v22: bits<32>, %v23: bits<32>):
    %v24 = const.bits<32> 0
    %v25 = icmp.eq %v22, %v24
    cond_br %v25, ^b8(%v22, %v23), ^b7(%v22, %v23)

^b7(%v26: bits<32>, %v27: bits<32>):
    %v28 = const.bits<32> 9
    %v29 = const.bits<32> 0
    %v30 = icmp.ne %v28, %v29
    br ^b9(%v26, %v28, %v30)

^b8(%v31: bits<32>, %v32: bits<32>):
    %v33 = const.bits<1> 1
    br ^b9(%v31, %v32, %v33)

^b9(%v34: bits<32>, %v35: bits<32>, %v36: bits<1>):
    cond_br %v36, ^b10(%v35), ^b11(%v35)

^b10(%v37: bits<32>):
    %v38 = const.bits<32> 2
    %v39 = signed_add_defined %v37, %v38
    ub.assume.signed_overflow %v39
    %v40 = add.bits<32> %v37, %v38
    br ^b12(%v40)

^b11(%v41: bits<32>):
    br ^b12(%v41)

^b12(%v42: bits<32>):
    ret %v42
}"#

private def accepts (s t : String) : Bool :=
  match Source.readProgram s, IR.Reader.readProgram t with
  | .ok s, .ok t => (returnPair s t).isSome
  | _, _ => false

private def widths : Var → Nat := fun _ => 32
private def yes : ValueTree widths := .value ⟨.int, .const 32 1⟩
private def no : ValueTree widths := .value ⟨.int, .const 32 0⟩
example : (ValueTree.branch (.const 1 1) yes no).normalizeBranches = yes := by decide
example : (ValueTree.branch (.cmp .ne (.const 32 3) (.const 32 0)) yes no).normalizeBranches = yes := by decide
example : (ValueTree.branch (.cmp .ne (.const 32 0) (.const 32 0)) yes no).normalizeBranches = no := by decide
example : (ValueTree.branch (.cmp .ne (.var "x" 32 rfl) (.const 32 0)) yes no).normalizeBranches =
    .branch (.cmp .ne (.var "x" 32 rfl) (.const 32 0)) yes no := by decide
example : (ValueTree.checked ⟨.signedOverflow, .const 1 0⟩
    (.branch (.const 1 1) yes no)).normalizeBranches =
    .checked ⟨.signedOverflow, .const 1 0⟩ yes := by decide

def statementReturnSelfTest : IO UInt32 := do
  let mut failures := 0
  let positives := [(source, target), (shortSource, shortTarget),
    (shortSource, target ++ "\n" ++ shortTarget),
    (source.replace "statement_return" "renamed", target.replace "statement_return" "renamed")]
  for (s, t) in positives do
    if !accepts s t then
      IO.eprintln "actual finite-statement pair refused"
      failures := failures+1
  let targetMutations := [
    ("changed initializer", "const.bits<32> 1", "const.bits<32> 2"),
    ("lost inner-scope write", "add.bits<32> %v3, %v0", "add.bits<32> %v3, %v3"),
    ("lost initialized operand", "signed_add_defined %v3, %v0", "signed_add_defined %v0, %v0"),
    ("lost first diagnostic", "    ub.assume.signed_overflow %v4\n", ""),
    ("changed first diagnostic", "ub.assume.signed_overflow %v4", "ub.assume.invalid_shift_count %v4"),
    ("swapped branch", "^b1(%v5), ^b2(%v5)", "^b2(%v5), ^b1(%v5)"),
    ("lost branch update", "br ^b3(%v11)", "br ^b3(%v8)"),
    ("lost false-arm update", "br ^b3(%v15)", "br ^b3(%v12)"),
    ("wrong effect", "may_trap=true", "may_trap=false")]
  for (name, before, after) in targetMutations do
    if !target.contains before || accepts source (target.replace before after) then
      IO.eprintln s!"statement target mutation escaped: {name}"
      failures := failures+1
  let sourceMutations := [
    ("read out-of-scope object", r#"["return",["int",false],["ident","int",4]]"#,
      r#"["return",["int",false],["ident","int",5]]"#),
    ("changed initializer", r#"["lit","uint","00000001"]"#, r#"["lit","uint","00000002"]"#),
    ("read uninitialized self", r#"["cast","int",["int",false],["ident","uint",3]]"#,
      r#"["ident","int",4]"#),
    ("wrong initialization destination", r#"["store_scalar",["uint",true]"#,
      r#"["store_scalar",["uint",false]"#),
    ("wrong inner-scope source", r#"["store_scalar",["int",false],["ident","int",1]]"#,
      r#"["store_scalar",["int",false],["ident","int",4]]"#),
    ("changed branch", r#"["if",["ident","int",1]"#, r#"["if",["ident","int",4]"#),
    ("wrong return local", r#"["return",["int",false],["ident","int",4]]"#,
      r#"["return",["int",false],["ident","int",1]]"#)]
  for (name, before, after) in sourceMutations do
    if !source.contains before || accepts (source.replace before after) target then
      IO.eprintln s!"statement source mutation escaped: {name}"
      failures := failures+1
  let shortMutations := [
    ("wrong short value", "const.bits<32> 3", "const.bits<32> 4"),
    ("skipped required write", "br ^b3(%v4, %v6, %v8)", "br ^b3(%v4, %v5, %v8)"),
    ("wrong skip decision", "const.bits<1> 0", "const.bits<1> 1"),
    ("wrong OR decision", "const.bits<1> 1", "const.bits<1> 0"),
    ("lost executed check", "    ub.assume.signed_overflow %v18\n", ""),
    ("wrong short branch", "^b8(%v22, %v23), ^b7(%v22, %v23)", "^b7(%v22, %v23), ^b8(%v22, %v23)"),
    ("lost OR write", "br ^b9(%v26, %v28, %v30)", "br ^b9(%v26, %v27, %v30)"),
    ("lost final update", "br ^b12(%v40)", "br ^b12(%v37)")]
  for (name, before, after) in shortMutations do
    if !shortTarget.contains before || accepts shortSource (shortTarget.replace before after) then
      IO.eprintln s!"short-circuit target mutation escaped: {name}"
      failures := failures+1
  if failures == 0 then
    IO.println s!"{positives.length+targetMutations.length+sourceMutations.length+shortMutations.length} finite-statement whole-function cases pass"
    return 0
  return 1

end EvoIR.Core.Symbolic.StatementReturnTests
