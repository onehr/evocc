import EvoIR.Core.CallCorrespondence

/-! Operational regressions for the original direct-calls.c exports. These
tests do not claim that the whole source/IR call checker is implemented. -/
namespace EvoIR.Core.Source.CallTests

private def converted_caller := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[3],[["function",0,"converted_callee",["long",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["uint",false],"none"]],["compound",[["return",["long",false],["binary","long","add",["cast","long",["long",false],["ident","int",1]],["cast","long",["long",false],["ident","uint",2]]]]]]],["function",3,"converted_caller",["int",false],false,[["parameter",4,["long",false],"none"],["parameter",5,["ulong",false],"none"]],["compound",[["return",["int",false],["call","long",0,[["argument",["int",false],["ident","long",4]],["argument",["uint",false],["ident","ulong",5]]]]]]]]]]"#

private def checked_caller := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[9],[["function",6,"checked_callee",["int",false],false,[["parameter",7,["int",false],"none"],["parameter",8,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","div",["ident","int",7],["ident","int",8]]]]]],["function",9,"checked_caller",["int",false],false,[["parameter",10,["int",false],"none"],["parameter",11,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["call","int",6,[["argument",["int",false],["ident","int",10]],["argument",["int",false],["ident","int",11]]]],["lit","int","00000001"]]]]]]]]"#

private def nested_caller := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[12],[["function",6,"checked_callee",["int",false],false,[["parameter",7,["int",false],"none"],["parameter",8,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","div",["ident","int",7],["ident","int",8]]]]]],["function",9,"checked_caller",["int",false],false,[["parameter",10,["int",false],"none"],["parameter",11,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["call","int",6,[["argument",["int",false],["ident","int",10]],["argument",["int",false],["ident","int",11]]]],["lit","int","00000001"]]]]]],["function",12,"nested_caller",["int",false],false,[["parameter",13,["int",false],"none"],["parameter",14,["int",false],"none"]],["compound",[["return",["int",false],["call","int",9,[["argument",["int",false],["call","int",6,[["argument",["int",false],["ident","int",13]],["argument",["int",false],["lit","int","00000002"]]]]],["argument",["int",false],["ident","int",14]]]]]]]]]]"#

private def pressure := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[33],[["function",0,"wide",["ulong",false],false,[["parameter",1,["ulong",false],"none"],["parameter",2,["ulong",false],"none"],["parameter",3,["ulong",false],"none"],["parameter",4,["ulong",false],"none"],["parameter",5,["ulong",false],"none"],["parameter",6,["ulong",false],"none"],["parameter",7,["ulong",false],"none"],["parameter",8,["ulong",false],"none"],["parameter",9,["ulong",false],"none"],["parameter",10,["ulong",false],"none"],["parameter",11,["ulong",false],"none"],["parameter",12,["ulong",false],"none"],["parameter",13,["ulong",false],"none"],["parameter",14,["ulong",false],"none"],["parameter",15,["ulong",false],"none"],["parameter",16,["ulong",false],"none"],["parameter",17,["ulong",false],"none"],["parameter",18,["ulong",false],"none"],["parameter",19,["ulong",false],"none"],["parameter",20,["ulong",false],"none"],["parameter",21,["ulong",false],"none"],["parameter",22,["ulong",false],"none"],["parameter",23,["ulong",false],"none"],["parameter",24,["ulong",false],"none"],["parameter",25,["ulong",false],"none"],["parameter",26,["ulong",false],"none"],["parameter",27,["ulong",false],"none"],["parameter",28,["ulong",false],"none"],["parameter",29,["ulong",false],"none"],["parameter",30,["ulong",false],"none"],["parameter",31,["ulong",false],"none"],["parameter",32,["ulong",false],"none"]],["compound",[["return",["ulong",false],["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["binary","ulong","add",["ident","ulong",1],["ident","ulong",2]],["ident","ulong",3]],["ident","ulong",4]],["ident","ulong",5]],["ident","ulong",6]],["ident","ulong",7]],["ident","ulong",8]],["ident","ulong",9]],["ident","ulong",10]],["ident","ulong",11]],["ident","ulong",12]],["ident","ulong",13]],["ident","ulong",14]],["ident","ulong",15]],["ident","ulong",16]],["ident","ulong",17]],["ident","ulong",18]],["ident","ulong",19]],["ident","ulong",20]],["ident","ulong",21]],["ident","ulong",22]],["ident","ulong",23]],["ident","ulong",24]],["ident","ulong",25]],["ident","ulong",26]],["ident","ulong",27]],["ident","ulong",28]],["ident","ulong",29]],["ident","ulong",30]],["ident","ulong",31]],["ident","ulong",32]]]]]],["function",33,"pressure",["ulong",false],false,[["parameter",34,["ulong",false],"none"]],["compound",[["return",["ulong",false],["call","ulong",0,[["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000000"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000001"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000002"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000003"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000004"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000005"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000006"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000007"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000008"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000009"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000000a"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000000b"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000000c"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000000d"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000000e"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000000f"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000010"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000011"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000012"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000013"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000014"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000015"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000016"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000017"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000018"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","0000000000000019"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000001a"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000001b"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000001c"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000001d"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000001e"]]],["argument",["ulong",false],["binary","ulong","add",["ident","ulong",34],["lit","ulong","000000000000001f"]]]]]]]]]]]"#

private def directTarget := r#"func @converted_callee(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = sext.bits<64> %v0
    %v5 = zext.bits<64> %v1
    %v6 = signed_add_defined %v4, %v5
    ub.assume.signed_overflow %v6
    %v7 = add.bits<64> %v4, %v5
    ret %v7, %v2, %v3
}

func @converted_caller(%arg0: bits<64>, %arg1: bits<64>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: bits<64>, %v2: mem<all>, %v3: world):
    %v4 = trunc.bits<32> %v0
    %v5 = trunc.bits<32> %v1
    (%v6, %v7, %v8) = call @converted_callee(%v4, %v5, %v2, %v3)
    %v9 = trunc.bits<32> %v6
    ret %v9, %v7, %v8
}

func @checked_callee(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = divisor_nonzero %v1
    ub.assume.division_by_zero %v4
    %v5 = signed_div_rem_no_overflow %v0, %v1
    ub.assume.signed_overflow %v5
    %v6 = sdiv.bits<32> %v0, %v1
    ret %v6, %v2, %v3
}

func @checked_caller(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @checked_callee(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 1
    %v8 = signed_add_defined %v4, %v7
    ub.assume.signed_overflow %v8
    %v9 = add.bits<32> %v4, %v7
    ret %v9, %v5, %v6
}

func @nested_caller(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 2
    (%v5, %v6, %v7) = call @checked_callee(%v0, %v4, %v2, %v3)
    (%v8, %v9, %v10) = call @checked_caller(%v5, %v1, %v6, %v7)
    ret %v8, %v9, %v10
}"#

private def pressureTarget := r#"func @wide(%arg0: bits<64>, %arg1: bits<64>, %arg2: bits<64>, %arg3: bits<64>, %arg4: bits<64>, %arg5: bits<64>, %arg6: bits<64>, %arg7: bits<64>, %arg8: bits<64>, %arg9: bits<64>, %arg10: bits<64>, %arg11: bits<64>, %arg12: bits<64>, %arg13: bits<64>, %arg14: bits<64>, %arg15: bits<64>, %arg16: bits<64>, %arg17: bits<64>, %arg18: bits<64>, %arg19: bits<64>, %arg20: bits<64>, %arg21: bits<64>, %arg22: bits<64>, %arg23: bits<64>, %arg24: bits<64>, %arg25: bits<64>, %arg26: bits<64>, %arg27: bits<64>, %arg28: bits<64>, %arg29: bits<64>, %arg30: bits<64>, %arg31: bits<64>, %arg32: mem<all>, %arg33: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: bits<64>, %v2: bits<64>, %v3: bits<64>, %v4: bits<64>, %v5: bits<64>, %v6: bits<64>, %v7: bits<64>, %v8: bits<64>, %v9: bits<64>, %v10: bits<64>, %v11: bits<64>, %v12: bits<64>, %v13: bits<64>, %v14: bits<64>, %v15: bits<64>, %v16: bits<64>, %v17: bits<64>, %v18: bits<64>, %v19: bits<64>, %v20: bits<64>, %v21: bits<64>, %v22: bits<64>, %v23: bits<64>, %v24: bits<64>, %v25: bits<64>, %v26: bits<64>, %v27: bits<64>, %v28: bits<64>, %v29: bits<64>, %v30: bits<64>, %v31: bits<64>, %v32: mem<all>, %v33: world):
    %v34 = add.bits<64> %v0, %v1
    %v35 = add.bits<64> %v34, %v2
    %v36 = add.bits<64> %v35, %v3
    %v37 = add.bits<64> %v36, %v4
    %v38 = add.bits<64> %v37, %v5
    %v39 = add.bits<64> %v38, %v6
    %v40 = add.bits<64> %v39, %v7
    %v41 = add.bits<64> %v40, %v8
    %v42 = add.bits<64> %v41, %v9
    %v43 = add.bits<64> %v42, %v10
    %v44 = add.bits<64> %v43, %v11
    %v45 = add.bits<64> %v44, %v12
    %v46 = add.bits<64> %v45, %v13
    %v47 = add.bits<64> %v46, %v14
    %v48 = add.bits<64> %v47, %v15
    %v49 = add.bits<64> %v48, %v16
    %v50 = add.bits<64> %v49, %v17
    %v51 = add.bits<64> %v50, %v18
    %v52 = add.bits<64> %v51, %v19
    %v53 = add.bits<64> %v52, %v20
    %v54 = add.bits<64> %v53, %v21
    %v55 = add.bits<64> %v54, %v22
    %v56 = add.bits<64> %v55, %v23
    %v57 = add.bits<64> %v56, %v24
    %v58 = add.bits<64> %v57, %v25
    %v59 = add.bits<64> %v58, %v26
    %v60 = add.bits<64> %v59, %v27
    %v61 = add.bits<64> %v60, %v28
    %v62 = add.bits<64> %v61, %v29
    %v63 = add.bits<64> %v62, %v30
    %v64 = add.bits<64> %v63, %v31
    ret %v64, %v32, %v33
}

func @pressure(%arg0: bits<64>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<64> 0
    %v4 = add.bits<64> %v0, %v3
    %v5 = const.bits<64> 1
    %v6 = add.bits<64> %v0, %v5
    %v7 = const.bits<64> 2
    %v8 = add.bits<64> %v0, %v7
    %v9 = const.bits<64> 3
    %v10 = add.bits<64> %v0, %v9
    %v11 = const.bits<64> 4
    %v12 = add.bits<64> %v0, %v11
    %v13 = const.bits<64> 5
    %v14 = add.bits<64> %v0, %v13
    %v15 = const.bits<64> 6
    %v16 = add.bits<64> %v0, %v15
    %v17 = const.bits<64> 7
    %v18 = add.bits<64> %v0, %v17
    %v19 = const.bits<64> 8
    %v20 = add.bits<64> %v0, %v19
    %v21 = const.bits<64> 9
    %v22 = add.bits<64> %v0, %v21
    %v23 = const.bits<64> 10
    %v24 = add.bits<64> %v0, %v23
    %v25 = const.bits<64> 11
    %v26 = add.bits<64> %v0, %v25
    %v27 = const.bits<64> 12
    %v28 = add.bits<64> %v0, %v27
    %v29 = const.bits<64> 13
    %v30 = add.bits<64> %v0, %v29
    %v31 = const.bits<64> 14
    %v32 = add.bits<64> %v0, %v31
    %v33 = const.bits<64> 15
    %v34 = add.bits<64> %v0, %v33
    %v35 = const.bits<64> 16
    %v36 = add.bits<64> %v0, %v35
    %v37 = const.bits<64> 17
    %v38 = add.bits<64> %v0, %v37
    %v39 = const.bits<64> 18
    %v40 = add.bits<64> %v0, %v39
    %v41 = const.bits<64> 19
    %v42 = add.bits<64> %v0, %v41
    %v43 = const.bits<64> 20
    %v44 = add.bits<64> %v0, %v43
    %v45 = const.bits<64> 21
    %v46 = add.bits<64> %v0, %v45
    %v47 = const.bits<64> 22
    %v48 = add.bits<64> %v0, %v47
    %v49 = const.bits<64> 23
    %v50 = add.bits<64> %v0, %v49
    %v51 = const.bits<64> 24
    %v52 = add.bits<64> %v0, %v51
    %v53 = const.bits<64> 25
    %v54 = add.bits<64> %v0, %v53
    %v55 = const.bits<64> 26
    %v56 = add.bits<64> %v0, %v55
    %v57 = const.bits<64> 27
    %v58 = add.bits<64> %v0, %v57
    %v59 = const.bits<64> 28
    %v60 = add.bits<64> %v0, %v59
    %v61 = const.bits<64> 29
    %v62 = add.bits<64> %v0, %v61
    %v63 = const.bits<64> 30
    %v64 = add.bits<64> %v0, %v63
    %v65 = const.bits<64> 31
    %v66 = add.bits<64> %v0, %v65
    (%v67, %v68, %v69) = call @wide(%v4, %v6, %v8, %v10, %v12, %v14, %v16, %v18, %v20, %v22, %v24, %v26, %v28, %v30, %v32, %v34, %v36, %v38, %v40, %v42, %v44, %v46, %v48, %v50, %v52, %v54, %v56, %v58, %v60, %v62, %v64, %v66, %v1, %v2)
    ret %v67, %v68, %v69
}"#

private def outcome (text entry : String) (numbers : List Nat) :
    Option (Result × Nat) := do
  let p ← (readProgram text).toOption
  let f ← p.functions.find? (·.name == entry)
  if numbers.length != f.parameters.length then none else
  let args := (f.parameters.zip numbers).map fun (param, n) => Value.ofNat param.type.type n
  let state ← (initial p f.symbol args).toOption
  let final := runSteps p 1000 state
  match final.control with
  | .halted result => some (result, final.functionId)
  | _ => none

private def provedOutcome (text entry : String) (numbers : List Nat) : Option Result := do
  let p ← (readProgram text).toOption
  let f ← p.functions.find? (·.name == entry)
  if numbers.length != f.parameters.length then none else
  let args := (f.parameters.zip numbers).map fun (param, n) => Value.ofNat param.type.type n
  let tree ← Symbolic.callFunction p 256 f (Symbolic.parameterWords f.parameters)
  return tree.val.denote (Symbolic.inputSubst args)

private def refusesBadCalls : Bool := Id.run do
  let some p := (readProgram checked_caller).toOption | return false
  let some f := p.functions.find? (·.name == "checked_caller") | return false
  let words := Symbolic.parameterWords f.parameters
  let test (program : Program) (function : Function) (args : List (Symbolic.CWord Symbolic.inputWidths)) :=
    (Symbolic.callFunction program 40 function args).isNone
  let missing := {p with functions := [f]}
  let recursive := {f with body := .compound [.statement (.return f.returnType
    (.call f.returnType.type f.symbol []))]}
  let wrongResult := {f with body := .compound [.statement (.return f.returnType
    (.call .long 6 []))]}
  return test p f [] && test p f (words ++ words) &&
    test p f (words.map (·.cast .ulong)) && test missing f words &&
    test {p with functions := [recursive]} recursive words &&
    test {p with functions := [wrongResult]} wrongResult words

private def pairAccepted (source target : String) : Bool :=
  match readProgram source, IR.Reader.readProgram target with
  | .ok s, .ok t => (Symbolic.callPair s t).isSome
  | _, _ => false

def callSelfTest : IO UInt32 := do
  let cases : List (String × String × List Nat × Result × Nat) := [
    (converted_caller, "converted_caller", [0x1ffffffff, 0x100000005], .ok (.ofNat .int 4), 3),
    (checked_caller, "checked_caller", [13, 2], .ok (.ofNat .int 7), 9),
    (checked_caller, "checked_caller", [0xfffffff9, 3], .ok (.ofInt .int (-1)), 9),
    (checked_caller, "checked_caller", [13, 0], .error (.diagnostic 3002), 6),
    (checked_caller, "checked_caller", [0x80000000, 0xffffffff], .error (.diagnostic 3004), 6),
    (checked_caller, "checked_caller", [0x7fffffff, 1], .error (.diagnostic 3004), 9),
    (nested_caller, "nested_caller", [13, 3], .ok (.ofNat .int 3), 12),
    (nested_caller, "nested_caller", [13, 0], .error (.diagnostic 3002), 6),
    (pressure, "pressure", [17], .ok (.ofNat .ulong 1040), 33)]
  let mut failures := 0
  for (source, entry, inputs, result, fid) in cases do
    let actual := outcome source entry inputs
    if actual != some (result, fid) then
      IO.eprintln s!"source call boundary mismatch: {entry} {inputs}: {repr actual}"
      failures := failures+1
    if provedOutcome source entry inputs != some result then
      IO.eprintln s!"source call diagram mismatch: {entry} {inputs}"
      failures := failures+1
  if !refusesBadCalls then
    IO.eprintln "source call diagram accepted a malformed argument or callee boundary"
    failures := failures+1
  let pairs := [(converted_caller, directTarget), (checked_caller, directTarget),
    (nested_caller, directTarget), (pressure, pressureTarget)]
  for (source, target) in pairs do
    if !(pairAccepted source target) then
      IO.eprintln "original whole-call pair refused"
      failures := failures+1
  let mutations := [
    (checked_caller, "call @checked_callee(%v0, %v1, %v2, %v3)", "call @checked_callee(%v1, %v0, %v2, %v3)"),
    (nested_caller, "call @checked_caller(%v5, %v1, %v6, %v7)", "call @checked_callee(%v5, %v1, %v6, %v7)"),
    (checked_caller, "call @checked_callee(", "call @missing_callee("),
    (checked_caller, "call @checked_callee(%v0, %v1, %v2, %v3)", "call @checked_callee(%v0, %v1, %v3, %v2)"),
    (checked_caller, "call @checked_callee(%v0, %v1, %v2, %v3)", "call @checked_callee(%v0, %v1)"),
    (checked_caller, "(%v4, %v5, %v6) = call", "(%v4, %v6, %v5) = call"),
    (checked_caller, "sdiv.bits<32>", "srem.bits<32>"),
    (checked_caller, "sdiv.bits<32>", "udiv.bits<32>"),
    (checked_caller, "    ub.assume.division_by_zero %v4\n", ""),
    (checked_caller, "ub.assume.division_by_zero", "ub.assume.remainder_by_zero"),
    (checked_caller, "    ub.assume.signed_overflow %v5\n", ""),
    (checked_caller, "ret %v9, %v5, %v6", "ret %v4, %v5, %v6"),
    (checked_caller, "ret %v9, %v5, %v6", "ret %v9"),
    (checked_caller, "may_trap=true", "may_trap=false"),
    (converted_caller, "sext.bits<64> %v0", "zext.bits<64> %v0"),
    (converted_caller, "zext.bits<64> %v1", "sext.bits<64> %v1"),
    (converted_caller, "add.bits<64> %v4, %v5", "sub.bits<64> %v4, %v5"),
    (converted_caller, "ret %v9, %v7, %v8", "ret %v9, %v2, %v3")]
  for (source, before, after) in mutations do
    let changed := directTarget.replace before after
    if changed == directTarget || pairAccepted source changed then
      IO.eprintln s!"whole-call mutation failed to refuse: {before}"
      failures := failures+1
  if failures == 0 then
    IO.println s!"{cases.length} original call execution/diagram cases, 4 whole-call pairs, 6 source refusals and {mutations.length} target mutations pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Source.CallTests
