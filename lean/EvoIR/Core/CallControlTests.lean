import EvoIR.Core.CallCorrespondence

/-! Original O0 call-control.c and call-branch-return.c exports. Whole-pair
acceptance is universal over signature bits; executions below are regressions,
not substitutes for the checked operational diagrams. -/
namespace EvoIR.Core.Source.CallControlTests

private def call_choice := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[2],[["function",0,"call_step",["int",false],false,[["parameter",1,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["ident","int",1],["lit","int","00000001"]]]]]],["function",2,"call_choice",["int",false],false,[["parameter",3,["int",false],"none"],["parameter",4,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["ident","int",3],["call","int",0,[["argument",["int",false],["ident","int",4]]]],["unary","int","neg",["call","int",0,[["argument",["int",false],["ident","int",4]]]]]]]]]]]]"#

private def call_and := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[5],[["function",0,"call_step",["int",false],false,[["parameter",1,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["ident","int",1],["lit","int","00000001"]]]]]],["function",5,"call_and",["int",false],false,[["parameter",6,["int",false],"none"],["parameter",7,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","and",["ident","int",6],["call","int",0,[["argument",["int",false],["ident","int",7]]]]]]]]]]]"#

private def call_or := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[8],[["function",0,"call_step",["int",false],false,[["parameter",1,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["ident","int",1],["lit","int","00000001"]]]]]],["function",8,"call_or",["int",false],false,[["parameter",9,["int",false],"none"],["parameter",10,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","or",["ident","int",9],["call","int",0,[["argument",["int",false],["ident","int",10]]]]]]]]]]]"#

private def call_assignment := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[11],[["function",0,"call_step",["int",false],false,[["parameter",1,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["ident","int",1],["lit","int","00000001"]]]]]],["function",11,"call_assignment",["int",false],false,[["parameter",12,["int",false],"none"]],["compound",[["object",13,"y",["int",false],"none",["store_scalar",["int",false],["lit","int","00000003"]]],["expr",["assign","int","add",13,["int",false],["ident","int",13],["call","int",0,[["argument",["int",false],["ident","int",12]]]]]],["return",["int",false],["ident","int",13]]]]]]]"#

private def call_comma := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[14],[["function",0,"call_step",["int",false],false,[["parameter",1,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["ident","int",1],["lit","int","00000001"]]]]]],["function",14,"call_comma",["int",false],false,[["parameter",15,["int",false],"none"]],["compound",[["return",["int",false],["comma","int",["call","int",0,[["argument",["int",false],["ident","int",15]]]],["ident","int",15]]]]]]]]"#

private def branch_result := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[3],[["function",0,"branch_value",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["ident","int",1],["binary","int","add",["ident","int",2],["lit","int","00000001"]],["binary","int","sub",["ident","int",2],["lit","int","00000001"]]]]]]],["function",3,"branch_result",["int",false],false,[["parameter",4,["int",false],"none"],["parameter",5,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["call","int",0,[["argument",["int",false],["ident","int",4]],["argument",["int",false],["ident","int",5]]]],["lit","int","00000002"]]]]]]]]"#

private def branch_argument := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[6],[["function",0,"branch_value",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["ident","int",1],["binary","int","add",["ident","int",2],["lit","int","00000001"]],["binary","int","sub",["ident","int",2],["lit","int","00000001"]]]]]]],["function",6,"branch_argument",["int",false],false,[["parameter",7,["int",false],"none"],["parameter",8,["int",false],"none"]],["compound",[["return",["int",false],["call","int",0,[["argument",["int",false],["call","int",0,[["argument",["int",false],["ident","int",7]],["argument",["int",false],["ident","int",8]]]]],["argument",["int",false],["ident","int",8]]]]]]]]]]"#

private def branch_short_left := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[9],[["function",0,"branch_value",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["ident","int",1],["binary","int","add",["ident","int",2],["lit","int","00000001"]],["binary","int","sub",["ident","int",2],["lit","int","00000001"]]]]]]],["function",9,"branch_short_left",["int",false],false,[["parameter",10,["int",false],"none"],["parameter",11,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","and",["call","int",0,[["argument",["int",false],["ident","int",10]],["argument",["int",false],["ident","int",11]]]],["call","int",0,[["argument",["int",false],["unary","int","not",["ident","int",10]]],["argument",["int",false],["ident","int",11]]]]]]]]]]]"#

private def branch_local := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[12],[["function",0,"branch_value",["int",false],false,[["parameter",1,["int",false],"none"],["parameter",2,["int",false],"none"]],["compound",[["return",["int",false],["conditional","int",["ident","int",1],["binary","int","add",["ident","int",2],["lit","int","00000001"]],["binary","int","sub",["ident","int",2],["lit","int","00000001"]]]]]]],["function",12,"branch_local",["int",false],false,[["parameter",13,["int",false],"none"],["parameter",14,["int",false],"none"]],["compound",[["object",15,"y",["int",false],"none",["store_scalar",["int",false],["call","int",0,[["argument",["int",false],["ident","int",13]],["argument",["int",false],["ident","int",14]]]]]],["return",["int",false],["conditional","int",["ident","int",15],["comma","int",["assign","int","add",15,["int",false],["ident","int",15],["call","int",0,[["argument",["int",false],["lit","int","00000001"]],["argument",["int",false],["lit","int","00000002"]]]]],["ident","int",15]],["ident","int",15]]]]]]]]"#

private def target := r#"func @call_step(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = signed_add_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v3
    ret %v5, %v1, %v2
}

func @call_choice(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b1(%v0, %v1, %v2, %v3), ^b2(%v0, %v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    (%v10, %v11, %v12) = call @call_step(%v7, %v8, %v9)
    br ^b3(%v6, %v7, %v11, %v12, %v10)

^b2(%v13: bits<32>, %v14: bits<32>, %v15: mem<all>, %v16: world):
    (%v17, %v18, %v19) = call @call_step(%v14, %v15, %v16)
    %v20 = const.bits<32> 0
    %v21 = signed_sub_defined %v20, %v17
    ub.assume.signed_overflow %v21
    %v22 = sub.bits<32> %v20, %v17
    br ^b3(%v13, %v14, %v18, %v19, %v22)

^b3(%v23: bits<32>, %v24: bits<32>, %v25: mem<all>, %v26: world, %v27: bits<32>):
    ret %v27, %v25, %v26
}

func @call_and(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b1(%v0, %v1, %v2, %v3), ^b2(%v0, %v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    (%v10, %v11, %v12) = call @call_step(%v7, %v8, %v9)
    %v13 = const.bits<32> 0
    %v14 = icmp.ne %v10, %v13
    br ^b3(%v6, %v7, %v11, %v12, %v14)

^b2(%v15: bits<32>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = const.bits<1> 0
    br ^b3(%v15, %v16, %v17, %v18, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<1>):
    %v25 = zext.bits<32> %v24
    ret %v25, %v22, %v23
}

func @call_or(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b2(%v0, %v1, %v2, %v3), ^b1(%v0, %v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    (%v10, %v11, %v12) = call @call_step(%v7, %v8, %v9)
    %v13 = const.bits<32> 0
    %v14 = icmp.ne %v10, %v13
    br ^b3(%v6, %v7, %v11, %v12, %v14)

^b2(%v15: bits<32>, %v16: bits<32>, %v17: mem<all>, %v18: world):
    %v19 = const.bits<1> 1
    br ^b3(%v15, %v16, %v17, %v18, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<1>):
    %v25 = zext.bits<32> %v24
    ret %v25, %v22, %v23
}

func @call_assignment(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 3
    (%v4, %v5, %v6) = call @call_step(%v0, %v1, %v2)
    %v7 = signed_add_defined %v3, %v4
    ub.assume.signed_overflow %v7
    %v8 = add.bits<32> %v3, %v4
    ret %v8, %v5, %v6
}

func @call_comma(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @call_step(%v0, %v1, %v2)
    ret %v0, %v4, %v5
}"#

private def branchTarget := r#"func @branch_value(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<32> 0
    %v5 = icmp.ne %v0, %v4
    cond_br %v5, ^b1(%v0, %v1, %v2, %v3), ^b2(%v0, %v1, %v2, %v3)

^b1(%v6: bits<32>, %v7: bits<32>, %v8: mem<all>, %v9: world):
    %v10 = const.bits<32> 1
    %v11 = signed_add_defined %v7, %v10
    ub.assume.signed_overflow %v11
    %v12 = add.bits<32> %v7, %v10
    br ^b3(%v6, %v7, %v8, %v9, %v12)

^b2(%v13: bits<32>, %v14: bits<32>, %v15: mem<all>, %v16: world):
    %v17 = const.bits<32> 1
    %v18 = signed_sub_defined %v14, %v17
    ub.assume.signed_overflow %v18
    %v19 = sub.bits<32> %v14, %v17
    br ^b3(%v13, %v14, %v15, %v16, %v19)

^b3(%v20: bits<32>, %v21: bits<32>, %v22: mem<all>, %v23: world, %v24: bits<32>):
    ret %v24, %v22, %v23
}

func @branch_result(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @branch_value(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 2
    %v8 = signed_add_defined %v4, %v7
    ub.assume.signed_overflow %v8
    %v9 = add.bits<32> %v4, %v7
    ret %v9, %v5, %v6
}

func @branch_argument(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @branch_value(%v0, %v1, %v2, %v3)
    (%v7, %v8, %v9) = call @branch_value(%v4, %v1, %v5, %v6)
    ret %v7, %v8, %v9
}

func @branch_short_left(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @branch_value(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 0
    %v8 = icmp.ne %v4, %v7
    cond_br %v8, ^b1(%v0, %v1, %v5, %v6), ^b2(%v0, %v1, %v5, %v6)

^b1(%v9: bits<32>, %v10: bits<32>, %v11: mem<all>, %v12: world):
    %v13 = const.bits<32> 0
    %v14 = icmp.eq %v9, %v13
    %v15 = zext.bits<32> %v14
    (%v16, %v17, %v18) = call @branch_value(%v15, %v10, %v11, %v12)
    %v19 = const.bits<32> 0
    %v20 = icmp.ne %v16, %v19
    br ^b3(%v9, %v10, %v17, %v18, %v20)

^b2(%v21: bits<32>, %v22: bits<32>, %v23: mem<all>, %v24: world):
    %v25 = const.bits<1> 0
    br ^b3(%v21, %v22, %v23, %v24, %v25)

^b3(%v26: bits<32>, %v27: bits<32>, %v28: mem<all>, %v29: world, %v30: bits<1>):
    %v31 = zext.bits<32> %v30
    ret %v31, %v28, %v29
}

func @branch_local(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = call @branch_value(%v0, %v1, %v2, %v3)
    %v7 = const.bits<32> 0
    %v8 = icmp.ne %v4, %v7
    cond_br %v8, ^b1(%v0, %v1, %v4, %v5, %v6), ^b2(%v0, %v1, %v4, %v5, %v6)

^b1(%v9: bits<32>, %v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    %v14 = const.bits<32> 1
    %v15 = const.bits<32> 2
    (%v16, %v17, %v18) = call @branch_value(%v14, %v15, %v12, %v13)
    %v19 = signed_add_defined %v11, %v16
    ub.assume.signed_overflow %v19
    %v20 = add.bits<32> %v11, %v16
    br ^b3(%v9, %v10, %v20, %v17, %v18, %v20)

^b2(%v21: bits<32>, %v22: bits<32>, %v23: bits<32>, %v24: mem<all>, %v25: world):
    br ^b3(%v21, %v22, %v23, %v24, %v25, %v23)

^b3(%v26: bits<32>, %v27: bits<32>, %v28: bits<32>, %v29: mem<all>, %v30: world, %v31: bits<32>):
    ret %v31, %v29, %v30
}"#

private def accepted (source ir : String) : Bool :=
  match readProgram source, IR.Reader.readProgram ir with
  | .ok s, .ok t => (Symbolic.callPair s t).isSome
  | _, _ => false

private def executes (source ir : String) (numbers : List Nat) (expected : Result)
    (owner : String) : Bool := Id.run do
  let .ok s := readProgram source | return false
  let .ok t := IR.Reader.readProgram ir | return false
  let some f := s.functions.find? (s.entries.contains ·.symbol) | return false
  let some expectedOwner := s.functions.find? (·.name == owner) | return false
  if numbers.length != f.parameters.length then return false
  let args := (f.parameters.zip numbers).map fun (p, n) => Value.ofNat p.type.type n
  let .ok start := initial s f.symbol args | return false
  let last := runSteps s 2000 start
  let .halted actual := last.control | return false
  if actual != expected || last.functionId != expectedOwner.symbol then return false
  let some diagram := Symbolic.callFunction s 512 f (Symbolic.parameterWords f.parameters)
    | return false
  if diagram.val.denote (Symbolic.inputSubst args) != expected then return false
  let .ok begin := IR.initial t f.name
    (args.map Symbolic.sourceValueIR ++ [.memory ⟨1, []⟩, .world]) | return false
  let expectedIR := expected.map fun value =>
    [Symbolic.sourceValueIR value, IR.Value.memory ⟨1, []⟩, IR.Value.world]
  let .halted actualIR := (IR.runSteps t 2000 begin).control | return false
  return actualIR == expectedIR

-- The normalization cannot change signed extension, zero-width casts, or
-- nonzero comparison constants into boolean identity.
private def widths : EvoIR.Var → Nat := fun _ => 1
example : Symbolic.Term.normalizeCasts (Γ := widths)
    (.cast .zext 32 (.const 1 1)) = .const 32 1 := by decide
example : Symbolic.Term.normalizeCasts (Γ := widths)
    (.cast .sext 32 (.const 1 1)) = .const 32 0xffffffff := by decide
example : Symbolic.branchCondition (Γ := widths)
    (.cmp .ne (.cast .zext 0 (.var "x" 1 rfl)) (.const 0 0)) =
    .cmp .ne (.cast .zext 0 (.var "x" 1 rfl)) (.const 0 0) := by decide
example : Symbolic.branchCondition (Γ := widths)
    (.cmp .ne (.cast .zext 32 (.var "x" 1 rfl)) (.const 32 1)) =
    .cmp .ne (.cast .zext 32 (.var "x" 1 rfl)) (.const 32 1) := by decide

def callControlSelfTest : IO UInt32 := do
  let pairs := [(call_choice, target), (call_and, target), (call_or, target),
    (call_assignment, target), (call_comma, target), (branch_result, branchTarget),
    (branch_argument, branchTarget), (branch_short_left, branchTarget), (branch_local, branchTarget)]
  let mut failures := 0
  for (source, ir) in pairs do
    if !accepted source ir then
      IO.eprintln "original calling-control whole pair refused"
      failures := failures + 1
  let value (n : Int) : Result := .ok (.ofInt .int n)
  let overflow : Result := .error (.diagnostic 3004)
  let cases : List (String × String × List Nat × Result × String) := [
    (call_choice, target, [1, 5], value 6, "call_choice"),
    (call_choice, target, [0, 5], value (-6), "call_choice"),
    (call_choice, target, [0, 0xffffffff], value 0, "call_choice"),
    (call_choice, target, [0, 0x7fffffff], overflow, "call_step"),
    (call_and, target, [0, 0x7fffffff], value 0, "call_and"),
    (call_and, target, [1, 0x7fffffff], overflow, "call_step"),
    (call_and, target, [1, 0xffffffff], value 0, "call_and"),
    (call_and, target, [1, 2], value 1, "call_and"),
    (call_or, target, [1, 0x7fffffff], value 1, "call_or"),
    (call_or, target, [0, 0x7fffffff], overflow, "call_step"),
    (call_or, target, [0, 0xffffffff], value 0, "call_or"),
    (call_or, target, [0, 2], value 1, "call_or"),
    (call_assignment, target, [5], value 9, "call_assignment"),
    (call_assignment, target, [0x7ffffffc], overflow, "call_assignment"),
    (call_assignment, target, [0x7fffffff], overflow, "call_step"),
    (call_comma, target, [5], value 5, "call_comma"),
    (call_comma, target, [0x7fffffff], overflow, "call_step"),
    (branch_result, branchTarget, [0, 5], value 6, "branch_result"),
    (branch_result, branchTarget, [1, 5], value 8, "branch_result"),
    (branch_result, branchTarget, [0, 0x80000000], overflow, "branch_value"),
    (branch_result, branchTarget, [1, 0x7ffffffe], overflow, "branch_result"),
    (branch_argument, branchTarget, [0, 1], value 0, "branch_argument"),
    (branch_argument, branchTarget, [1, 0xffffffff], value (-2), "branch_argument"),
    (branch_short_left, branchTarget, [0, 1], value 0, "branch_short_left"),
    (branch_short_left, branchTarget, [1, 1], value 0, "branch_short_left"),
    (branch_short_left, branchTarget, [0, 2], value 1, "branch_short_left"),
    (branch_short_left, branchTarget, [1, 0xffffffff], value 0, "branch_short_left"),
    (branch_short_left, branchTarget, [0, 0x7fffffff], overflow, "branch_value"),
    (branch_local, branchTarget, [0, 1], value 0, "branch_local"),
    (branch_local, branchTarget, [1, 1], value 5, "branch_local"),
    (branch_local, branchTarget, [1, 0x7ffffffc], overflow, "branch_local")]
  for (source, ir, inputs, result, owner) in cases do
    if !executes source ir inputs result owner then
      IO.eprintln s!"calling-control execution mismatch: {owner} {inputs}"
      failures := failures + 1
  let mutations := [
    (call_and, target, "const.bits<1> 0", "const.bits<1> 1"),
    (call_or, target, "const.bits<1> 1", "const.bits<1> 0"),
    (call_and, target, "zext.bits<32> %v24", "sext.bits<32> %v24"),
    (call_and, target, "icmp.ne %v10, %v13", "icmp.eq %v10, %v13"),
    (call_choice, target, "sub.bits<32> %v20, %v17", "sub.bits<32> %v17, %v20"),
    (call_choice, target, "    ub.assume.signed_overflow %v21\n", ""),
    (call_choice, target, "br ^b3(%v6, %v7, %v11, %v12, %v10)",
      "br ^b3(%v6, %v7, %v8, %v9, %v10)"),
    (call_and, target, "^b1(%v0, %v1, %v2, %v3), ^b2(%v0, %v1, %v2, %v3)",
      "^b2(%v0, %v1, %v2, %v3), ^b1(%v0, %v1, %v2, %v3)"),
    (call_and, target, "    ub.assume.signed_overflow %v4\n", ""),
    (call_comma, target, "    (%v3, %v4, %v5) = call @call_step(%v0, %v1, %v2)\n    ret %v0, %v4, %v5",
      "    ret %v0, %v1, %v2"),
    (call_assignment, target, "    ub.assume.signed_overflow %v7\n", ""),
    (branch_result, branchTarget, "add.bits<32> %v4, %v7", "sub.bits<32> %v4, %v7"),
    (branch_result, branchTarget, "    ub.assume.signed_overflow %v18\n", ""),
    (branch_result, branchTarget, "    ub.assume.signed_overflow %v11\n", ""),
    (branch_result, branchTarget, "ub.assume.signed_overflow %v18",
      "ub.assume.division_by_zero %v18"),
    (branch_argument, branchTarget, "call @branch_value(%v4, %v1, %v5, %v6)",
      "call @branch_value(%v0, %v1, %v5, %v6)"),
    (branch_argument, branchTarget, "call @branch_value(%v4, %v1, %v5, %v6)",
      "call @branch_value(%v4, %v1, %v2, %v3)"),
    (branch_short_left, branchTarget, "icmp.eq %v9, %v13", "icmp.ne %v9, %v13"),
    (branch_short_left, branchTarget, "ret %v31, %v28, %v29", "ret %v27, %v28, %v29"),
    (branch_local, branchTarget, "br ^b3(%v9, %v10, %v20, %v17, %v18, %v20)",
      "br ^b3(%v9, %v10, %v20, %v17, %v18, %v11)"),
    (branch_local, branchTarget, "may_trap=true", "may_trap=false")]
  for (source, ir, before, after) in mutations do
    let changed := ir.replace before after
    if changed == ir || accepted source changed then
      IO.eprintln s!"calling-control mutation failed to refuse: {before}"
      failures := failures + 1
  if failures == 0 then
    IO.println s!"{pairs.length} original calling-control pairs, {cases.length} source/IR executions and {mutations.length} target mutations pass"
  return if failures == 0 then 0 else 1

end EvoIR.Core.Source.CallControlTests
