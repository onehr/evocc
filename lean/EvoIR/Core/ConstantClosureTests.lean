import EvoIR.Core.FunctionClosure
import EvoIR.Core.CutpointInput

/-! Original constant-tail closure and constant-only diagram normalization.
False guards and first diagnostics are preserved; variables never acquire
constant facts. These tests do not turn numeric samples into proof premises. -/
namespace EvoIR.Core.Symbolic.ConstantClosureTests

private def source := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",8,"call_constant_tail",["int",false],false,[["parameter",9,["uint",false],"none"],["parameter",10,["uint",false],"none"]],["compound",[["return",["int",false],["binary","int","or",["binary","int","or",["binary","int","or",["binary","int","or",["binary","int","or",["binary","int","ne",["call","uint",2,[["argument",["uint",false],["ident","uint",9]],["argument",["uint",false],["ident","uint",10]]]],["lit","int","00000000"]],["binary","int","ne",["cast","ulong",["ulong",false],["unary","int","neg",["lit","int","00000001"]]],["lit","ulong","ffffffffffffffff"]]],["binary","int","ne",["cast","long",["long",false],["lit","uint","ffffffff"]],["lit","long","00000000ffffffff"]]],["binary","int","ne",["binary","int","div",["unary","int","neg",["lit","int","00000007"]],["lit","int","00000003"]],["unary","int","neg",["lit","int","00000002"]]]],["binary","int","ne",["binary","int","rem",["unary","int","neg",["lit","int","00000007"]],["lit","int","00000003"]],["unary","int","neg",["lit","int","00000001"]]]],["binary","int","ne",["cast","uint",["uint",false],["binary","int","add",["lit","int","7fffffff"],["lit","int","00000001"]]],["lit","uint","80000000"]]]]]]]]]
"#
private def target := r#"func @call_tick(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 3
    %v4 = mul.bits<32> %v0, %v3
    %v5 = const.bits<32> 1
    %v6 = add.bits<32> %v4, %v5
    ret %v6, %v1, %v2
}

func @loop_call(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    br ^b1(%v0, %v1, %v2, %v3)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 0
    %v9 = icmp.ne %v5, %v8
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v4, %v6, %v7)

^b2(%v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    (%v14, %v15, %v16) = call @call_tick(%v10, %v12, %v13)
    %v17 = const.bits<32> 1
    %v18 = sub.bits<32> %v11, %v17
    br ^b1(%v14, %v18, %v15, %v16)

^b3(%v19: bits<32>, %v20: mem<all>, %v21: world):
    ret %v19, %v20, %v21
}

func @call_loop(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 1
    %v8 = add.bits<32> %v4, %v7
    ret %v8, %v5, %v6
}

func @call_constant_tail(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @loop_call(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 0
    %v8 = icmp.ne %v4, %v7
    cond_br %v8, ^b2(%v0, %v1, %v5, %v6), ^b1(%v0, %v1, %v5, %v6)

^b1(%v9: bits<32>, %v10: bits<32>, %v11: mem<all>, %v12: world):
    %v13 = const.bits<32> 1
    %v14 = const.bits<32> 0
    %v15 = signed_sub_defined %v14, %v13
    ub.assume.signed_overflow %v15
    %v16 = sub.bits<32> %v14, %v13
    %v17 = sext.bits<64> %v16
    %v18 = const.bits<64> 18446744073709551615
    %v19 = icmp.ne %v17, %v18
    br ^b3(%v9, %v10, %v11, %v12, %v19)

^b2(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world):
    %v24 = const.bits<1> 1
    br ^b3(%v20, %v21, %v22, %v23, %v24)

^b3(%v25: bits<32>, %v26: bits<32>, %v27: mem<all>, %v28: world, %v29: bits<1>):
    cond_br %v29, ^b5(%v25, %v26, %v27, %v28), ^b4(%v25, %v26, %v27, %v28)

^b4(%v30: bits<32>, %v31: bits<32>, %v32: mem<all>, %v33: world):
    %v34 = const.bits<64> 4294967295
    %v35 = const.bits<64> 4294967295
    %v36 = icmp.ne %v34, %v35
    br ^b6(%v30, %v31, %v32, %v33, %v36)

^b5(%v37: bits<32>, %v38: bits<32>, %v39: mem<all>, %v40: world):
    %v41 = const.bits<1> 1
    br ^b6(%v37, %v38, %v39, %v40, %v41)

^b6(%v42: bits<32>, %v43: bits<32>, %v44: mem<all>, %v45: world, %v46: bits<1>):
    cond_br %v46, ^b8(%v42, %v43, %v44, %v45), ^b7(%v42, %v43, %v44, %v45)

^b7(%v47: bits<32>, %v48: bits<32>, %v49: mem<all>, %v50: world):
    %v51 = const.bits<32> 7
    %v52 = const.bits<32> 0
    %v53 = signed_sub_defined %v52, %v51
    ub.assume.signed_overflow %v53
    %v54 = sub.bits<32> %v52, %v51
    %v55 = const.bits<32> 3
    %v56 = divisor_nonzero %v55
    ub.assume.division_by_zero %v56
    %v57 = signed_div_rem_no_overflow %v54, %v55
    ub.assume.signed_overflow %v57
    %v58 = sdiv.bits<32> %v54, %v55
    %v59 = const.bits<32> 2
    %v60 = const.bits<32> 0
    %v61 = signed_sub_defined %v60, %v59
    ub.assume.signed_overflow %v61
    %v62 = sub.bits<32> %v60, %v59
    %v63 = icmp.ne %v58, %v62
    br ^b9(%v47, %v48, %v49, %v50, %v63)

^b8(%v64: bits<32>, %v65: bits<32>, %v66: mem<all>, %v67: world):
    %v68 = const.bits<1> 1
    br ^b9(%v64, %v65, %v66, %v67, %v68)

^b9(%v69: bits<32>, %v70: bits<32>, %v71: mem<all>, %v72: world, %v73: bits<1>):
    cond_br %v73, ^b11(%v69, %v70, %v71, %v72), ^b10(%v69, %v70, %v71, %v72)

^b10(%v74: bits<32>, %v75: bits<32>, %v76: mem<all>, %v77: world):
    %v78 = const.bits<32> 7
    %v79 = const.bits<32> 0
    %v80 = signed_sub_defined %v79, %v78
    ub.assume.signed_overflow %v80
    %v81 = sub.bits<32> %v79, %v78
    %v82 = const.bits<32> 3
    %v83 = divisor_nonzero %v82
    ub.assume.remainder_by_zero %v83
    %v84 = signed_div_rem_no_overflow %v81, %v82
    ub.assume.signed_overflow %v84
    %v85 = srem.bits<32> %v81, %v82
    %v86 = const.bits<32> 1
    %v87 = const.bits<32> 0
    %v88 = signed_sub_defined %v87, %v86
    ub.assume.signed_overflow %v88
    %v89 = sub.bits<32> %v87, %v86
    %v90 = icmp.ne %v85, %v89
    br ^b12(%v74, %v75, %v76, %v77, %v90)

^b11(%v91: bits<32>, %v92: bits<32>, %v93: mem<all>, %v94: world):
    %v95 = const.bits<1> 1
    br ^b12(%v91, %v92, %v93, %v94, %v95)

^b12(%v96: bits<32>, %v97: bits<32>, %v98: mem<all>, %v99: world, %v100: bits<1>):
    cond_br %v100, ^b14(%v96, %v97, %v98, %v99), ^b13(%v96, %v97, %v98, %v99)

^b13(%v101: bits<32>, %v102: bits<32>, %v103: mem<all>, %v104: world):
    %v105 = const.bits<32> 2147483648
    %v106 = const.bits<32> 2147483648
    %v107 = icmp.ne %v105, %v106
    br ^b15(%v101, %v102, %v103, %v104, %v107)

^b14(%v108: bits<32>, %v109: bits<32>, %v110: mem<all>, %v111: world):
    %v112 = const.bits<1> 1
    br ^b15(%v108, %v109, %v110, %v111, %v112)

^b15(%v113: bits<32>, %v114: bits<32>, %v115: mem<all>, %v116: world, %v117: bits<1>):
    %v118 = zext.bits<32> %v117
    ret %v118, %v115, %v116
}
"#
private def plan := r#"["core-call-cutpoints-v1",256,[[2,["core-cutpoints-v2",256,[
  [0,"entry",[[3,"uint",false,true],[4,"uint",false,true]],[[1,4,"uint"],[0,3,"uint"]],[[3,"world"],[2,"mem"]]],
  [2,"b1",[[3,"uint",false,true],[4,"uint",false,true]],[[5,4,"uint"],[4,3,"uint"]],[[7,"world"],[6,"mem"]]]
]]]]]
"#

private def accepts (s t : String) : Bool := Id.run do
  let .ok s := Source.readProgram s | return false
  let .ok t := IR.Reader.readProgram t | return false
  let some (fuel, rows) := readCallCutpoints s plan | return false
  return (openCallPair s t (fun id => rows.lookup id) fuel).isSome

private def widths (_ : Var) := 32
private def variableTerm : Term widths 32 := .var "x" 32 rfl
private def zero : Term widths 32 := .const 32 0
private def one : Term widths 32 := .const 32 1

def constantClosureSelfTest : IO UInt32 := do
  let mut failures := 0
  for name in ["call_constant_tail", "renamed_constant_tail"] do
    if !accepts (source.replace "call_constant_tail" name) (target.replace "call_constant_tail" name) then
      IO.eprintln s!"constant-tail closure refused: {name}"
      failures := failures+1
  for (before, after) in [
      ("icmp.ne %v5, %v8", "icmp.eq %v5, %v8"),
      ("zext.bits<32>", "sext.bits<32>"),
      ("mul.bits<32> %v0, %v3", "sub.bits<32> %v0, %v3"),
      ("ret %v6, %v1, %v2", "ret %v0, %v1, %v2")] do
    let changed := target.replace before after
    if changed == target || accepts source changed then
      IO.eprintln s!"constant-tail mutation admitted or not applied: {before}"
      failures := failures+1
  let yes : Guard widths := ⟨.signedOverflow, .guard .add zero one⟩
  let no : Guard widths := ⟨.divisionByZero, .nonzero zero⟩
  let other : Guard widths := ⟨.remainderByZero, .nonzero zero⟩
  let unknown : Guard widths := ⟨.divisionByZero, .nonzero variableTerm⟩
  let leaf : CutTree widths Nat := .leaf 7
  let empty : Subst widths := fun _ => 0
  let positive : Subst widths := fun _ => 1
  let good := (CutTree.checked yes leaf).normalizeTrace
  let bad := (CutTree.checked yes (.checked no (.checked other leaf))).normalizeTrace
  let unproved := (CutTree.checked unknown leaf).normalizeTrace
  if good.eval empty != .ok 7 || bad.eval positive != .error (.diagnostic 3002) ||
      unproved.eval empty != .error (.diagnostic 3002) || unproved.eval positive != .ok 7 then
    IO.eprintln "constant normalization changed the first diagnostic"
    failures := failures+1
  if (Term.bin .mul zero variableTerm).constantValue.isSome ||
      (Term.cmp .eq variableTerm variableTerm).constantValue.isSome then
    IO.eprintln "closed-term evaluator used a nonconstant fact"
    failures := failures+1
  let mixed : Term widths 1 := .count true (.const 64 0) (.const 2 1)
  let negative : Term widths 1 := .count true (.const 64 0) (.const 2 3)
  let overflow : Term widths 1 := .guard .divRem (.const 32 0x80000000) (.const 32 0xffffffff)
  if mixed.constantValue != some 1 || negative.constantValue != some 0 ||
      overflow.constantValue != some 0 then
    IO.eprintln "constant normalization lost operand width or signedness"
    failures := failures+1
  if failures == 0 then
    IO.println "2 constant-tail closures, 4 mutations and 9 constant/diagnostic boundary controls pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Symbolic.ConstantClosureTests
