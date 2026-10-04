import EvoIR.Core.ReturnCorrespondence
import EvoIR.Core.CutpointInput

/-! Div/rem admission covers all source integer ranks and the conversion back
to a compound-assignment target. Mutations exercise the actual two-input
checker, not a separate acceptance approximation. -/
namespace EvoIR.Core.Symbolic.DivisionTests

private def pair (remainder compound : Bool) (left right : Source.IntType) :
    Source.Program × IR.Program :=
  let dst := Source.usualType left right
  let result := if compound then left else dst
  let expr := if compound then
    Source.Expr.assign left (if remainder then .rem else .div) 1 ⟨left, false⟩
      (.ident left 1) (.ident right 2)
    else .binary dst (if remainder then .rem else .div) (.ident left 1) (.ident right 2)
  let source := Source.Program.mk [0] [
    ⟨0, "division", ⟨result, false⟩, false,
      [⟨1, ⟨left, false⟩, .none⟩, ⟨2, ⟨right, false⟩, .none⟩],
      .return ⟨result, false⟩ expr⟩]
  let convert := fun (src : Source.IntType) (input output : Nat) =>
    if src.width = dst.width then [] else
      [IR.Instruction.mk [⟨output, .bits dst.width⟩]
        (.scalar (.cast (if src.signed then .sext else .zext) src.width dst.width)) [input]]
  let a := if left.width = dst.width then 0 else 2
  let b := if right.width = dst.width then 1 else 3
  let reason := if remainder then IR.Reason.remainderByZero else IR.Reason.divisionByZero
  let code := convert left 0 2 ++ convert right 1 3 ++
    [⟨[⟨4, .bits 1⟩], .scalar (.nonzero dst.width), [b]⟩,
      ⟨[], .assume reason, [4]⟩] ++
    (if dst.signed then [⟨[⟨5, .bits 1⟩], .scalar (.guard .divRem dst.width), [a, b]⟩,
      ⟨[], .assume .signedOverflow, [5]⟩] else []) ++
    [⟨[⟨6, .bits dst.width⟩], .division dst.signed remainder dst.width, [a, b]⟩] ++
    (if result.width = dst.width then [] else
      [⟨[⟨7, .bits result.width⟩], .scalar (.cast .trunc dst.width result.width), [6]⟩])
  let target := IR.Program.mk [
    ⟨"division", [.bits left.width, .bits right.width], [.bits result.width], false,
      {IR.Reader.emptyEffects with mayTrap := true},
      [⟨"entry", [⟨0, .bits left.width⟩, ⟨1, .bits right.width⟩], code,
        .return [if result.width = dst.width then 6 else 7]⟩]⟩]
  (source, target)

private def mutate (target : IR.Program) (f : IR.Instruction → List IR.Instruction) : IR.Program :=
  {target with functions := target.functions.map fun fn =>
    {fn with blocks := fn.blocks.map fun b => {b with instructions := b.instructions.flatMap f}}}

private def σ : Subst (fun _ => 32) := fun _ => 0
private def word (n : Nat) : CWord (fun _ => 32) := ⟨.int, .const 32 (BitVec.ofNat 32 n)⟩
private def g (reason : IR.Reason) (n : Nat) : Guard (fun _ => 32) :=
  ⟨reason, .const 1 (BitVec.ofNat 1 n)⟩

-- Hand-derived truncation/remainder and exceptional inputs, independent of
-- the source/IR comparison. Naively using Euclidean Int division fails these.
example : (division false (word 0xfffffff9) (word 3)).2.denote σ =
    Source.Value.ofInt .int (-2) := by decide
example : (division true (word 0xfffffff9) (word 3)).2.denote σ =
    Source.Value.ofInt .int (-1) := by decide
example : guards σ (division false (word 7) (word 0)).1 =
    .error (.diagnostic 3002) := by decide
example : guards σ (division true (word 7) (word 0)).1 =
    .error (.diagnostic 3003) := by decide
example : guards σ (division false (word 0x80000000) (word 0xffffffff)).1 =
    .error (.diagnostic 3004) := by decide
example : guards σ (division true (word 0x80000000) (word 0xffffffff)).1 =
    .error (.diagnostic 3004) := by decide

-- Repeated checks never suppress a first failure or share facts between arms.
private def tree : ValueTree (fun _ => 32) :=
  .checked (g .divisionByZero 0) (.checked (g .signedOverflow 0) (.value (word 1)))
example : tree.normalizeTrace.denote σ = .error (.diagnostic 3002) := by decide
example : (ValueTree.branch (.const 1 0)
    (.checked (g .divisionByZero 0) (.value (word 1)))
    (.checked (g .remainderByZero 0) (.value (word 2)))).normalizeTrace.denote σ =
      .error (.diagnostic 3003) := by decide


-- Exact compiler exports of division-control.c; proposal is untrusted input.
private def loopSource := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[0],[["function",0,"division_loop",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["int",false],"none"],["parameter",3,["int",false],"none"]],["compound",[["while",["ident","int",2],["compound",[["expr",["assign","int","div",1,["int",false],["ident","int",1],["ident","int",3]]],["expr",["update","int","pre_dec",2,["int",false],["ident","int",2]]]]]],["return",["int",false],["ident","int",1]]]]]]]"#
private def orderSource := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[4],[["function",4,"division_order",["int",false],false,[["parameter",5,["int",false],"none"],["parameter",6,["int",false],"none"],["parameter",7,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","rem",["binary","int","div",["ident","int",5],["ident","int",6]],["ident","int",7]]]]]]]]"#
private def choiceSource := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",8,"division_choice",["int",false],false,[["parameter",9,["int",false],"none"],["parameter",10,["int",false],"none"],["parameter",11,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["ident","int",9],["binary","int","div",["ident","int",10],["ident","int",11]],["binary","int","rem",["ident","int",10],["ident","int",11]]]]]]]]]"#
private def controlTarget := r#"func @division_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>):
    br ^b1(%v0, %v1, %v2)

^b1(%v3: bits<32>, %v4: bits<32>, %v5: bits<32>):
    %v6 = const.bits<32> 0
    %v7 = icmp.ne %v4, %v6
    cond_br %v7, ^b2(%v3, %v4, %v5), ^b3(%v3)

^b2(%v8: bits<32>, %v9: bits<32>, %v10: bits<32>):
    %v11 = divisor_nonzero %v10
    ub.assume.division_by_zero %v11
    %v12 = signed_div_rem_no_overflow %v8, %v10
    ub.assume.signed_overflow %v12
    %v13 = sdiv.bits<32> %v8, %v10
    %v14 = const.bits<32> 1
    %v15 = signed_sub_defined %v9, %v14
    ub.assume.signed_overflow %v15
    %v16 = sub.bits<32> %v9, %v14
    br ^b1(%v13, %v16, %v10)

^b3(%v17: bits<32>):
    ret %v17
}

func @division_order(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>):
    %v3 = divisor_nonzero %v1
    ub.assume.division_by_zero %v3
    %v4 = signed_div_rem_no_overflow %v0, %v1
    ub.assume.signed_overflow %v4
    %v5 = sdiv.bits<32> %v0, %v1
    %v6 = divisor_nonzero %v2
    ub.assume.remainder_by_zero %v6
    %v7 = signed_div_rem_no_overflow %v5, %v2
    ub.assume.signed_overflow %v7
    %v8 = srem.bits<32> %v5, %v2
    ret %v8
}

func @division_choice(%arg0: bits<32>, %arg1: bits<32>, %arg2: bits<32>) -> (bits<32>) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: bits<32>):
    %v3 = const.bits<32> 0
    %v4 = icmp.ne %v0, %v3
    cond_br %v4, ^b1(%v0, %v1, %v2), ^b2(%v0, %v1, %v2)

^b1(%v5: bits<32>, %v6: bits<32>, %v7: bits<32>):
    %v8 = divisor_nonzero %v7
    ub.assume.division_by_zero %v8
    %v9 = signed_div_rem_no_overflow %v6, %v7
    ub.assume.signed_overflow %v9
    %v10 = sdiv.bits<32> %v6, %v7
    br ^b3(%v5, %v6, %v7, %v10)

^b2(%v11: bits<32>, %v12: bits<32>, %v13: bits<32>):
    %v14 = divisor_nonzero %v13
    ub.assume.remainder_by_zero %v14
    %v15 = signed_div_rem_no_overflow %v12, %v13
    ub.assume.signed_overflow %v15
    %v16 = srem.bits<32> %v12, %v13
    br ^b3(%v11, %v12, %v13, %v16)

^b3(%v17: bits<32>, %v18: bits<32>, %v19: bits<32>, %v20: bits<32>):
    ret %v20
}"#
private def loopProposal := r#"["core-cutpoints-v1",256,[
  [0,"entry",[[1,"int",false,true],[2,"int",false,true],[3,"int",false,true]],[[2,3,"int"],[1,2,"int"],[0,1,"int"]]],
  [2,"b1",[[1,"int",false,true],[2,"int",false,true],[3,"int",false,true]],[[5,3,"int"],[4,2,"int"],[3,1,"int"]]]
]]"#

private def acceptsControl (source target : String) (cyclic : Bool) : Bool :=
  match Source.readProgram source, IR.Reader.readProgram target with
  | .ok s, .ok t =>
    if cyclic then
      match s.functions with
      | [sf] => match readCutpoints sf loopProposal with
        | some (budget, layouts) => (loopPair s t layouts budget).isSome
        | none => false
      | _ => false
    else (returnPair s t).isSome
  | _, _ => false


def divisionSelfTest : IO UInt32 := do
  let types : List Source.IntType := [.int, .uint, .long, .ulong, .longLong, .ulongLong]
  let mut failures := 0
  let mut admitted := 0
  let mut refused := 0
  for remainder in [false, true] do
    for compound in [false, true] do
      for left in types do
        for right in types do
          let (s, t) := pair remainder compound left right
          if (returnPair s t).isNone then
            IO.eprintln s!"division pair refused: rem={remainder} compound={compound} {repr left}/{repr right}"
            failures := failures+1
          else admitted := admitted+1
          let mutations : List IR.Program := [
            mutate t (fun i => [match i.op with
              | .division signed rem n => {i with op := .division (!signed) rem n}
              | _ => i]),
            mutate t (fun i => [match i.op with
              | .division signed rem n => {i with op := .division signed (!rem) n}
              | _ => i]),
            mutate t (fun i => [match i.op with
              | .division .. => {i with operands := i.operands.reverse}
              | _ => i]),
            mutate t (fun i => match i.op with
              | .assume .divisionByZero | .assume .remainderByZero => []
              | _ => [i]),
            mutate t (fun i => [match i.op with
              | .assume .divisionByZero | .assume .remainderByZero =>
                {i with op := .assume .invalidShiftCount}
              | _ => i]),
            mutate t (fun i => [match i.op with
              | .scalar (.nonzero _) => {i with operands := [0]}
              | _ => i]),
            {t with functions := t.functions.map fun f =>
              {f with effects := {f.effects with mayTrap := false}}}]
          let mutations := mutations ++ (if (Source.usualType left right).signed then
            [mutate t (fun i => match i.op with
              | .assume .signedOverflow => [] | _ => [i])] else [])
          for wrong in mutations do
            if wrong == t || (returnPair s wrong).isSome then
              IO.eprintln s!"division mutation not refused: {repr left}/{repr right}"
              failures := failures+1
            else refused := refused+1
  let mut controlCount := 0
  for (source, cyclic) in [(loopSource, true), (orderSource, false), (choiceSource, false)] do
    if !acceptsControl source controlTarget cyclic then
      IO.eprintln "original division control pair refused"
      failures := failures+1
    controlCount := controlCount+1
  let controls := [
    (loopSource, true, "sdiv.bits<32> %v8, %v10", "sdiv.bits<32> %v10, %v8"),
    (loopSource, true, "br ^b1(%v13, %v16, %v10)", "br ^b1(%v8, %v16, %v10)"),
    (loopSource, true, "ub.assume.division_by_zero %v11\n", ""),
    (loopSource, true, "ub.assume.signed_overflow %v12\n", ""),
    (orderSource, false, "ub.assume.division_by_zero %v3", "ub.assume.remainder_by_zero %v3"),
    (orderSource, false, "signed_div_rem_no_overflow %v5, %v2", "signed_div_rem_no_overflow %v0, %v2"),
    (choiceSource, false, "^b1(%v0, %v1, %v2), ^b2(%v0, %v1, %v2)",
      "^b2(%v0, %v1, %v2), ^b1(%v0, %v1, %v2)"),
    (choiceSource, false, "ret %v20", "ret %v19")]
  for (source, cyclic, before, after) in controls do
    let wrong := controlTarget.replace before after
    if wrong == controlTarget || acceptsControl source wrong cyclic then
      IO.eprintln s!"division control mutation not refused: {before}"
      failures := failures+1
    controlCount := controlCount+1
  if failures == 0 then
    IO.println s!"{admitted} mixed-type div/rem and compound pairs pass; {refused} mutations refuse"
    IO.println s!"{controlCount} original division loop/order/choice cases pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Symbolic.DivisionTests
