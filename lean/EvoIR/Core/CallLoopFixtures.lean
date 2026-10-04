import EvoIR.Core.CutpointInput

/-! Byte-exact original O0 exports and untrusted loop-cutpoint proposals.
No input is normalized or rewritten before the whole-pair checker. -/
namespace EvoIR.Core.Source.CallLoopTests

def loopCall := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[2],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]]]]"#

def callLoop := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[5],[["function",0,"call_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["binary","uint","mul",["ident","uint",1],["lit","uint","00000003"]],["lit","uint","00000001"]]]]]],["function",2,"loop_call",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]],["function",5,"call_loop",["uint",false],false,[["parameter",6,["uint",false],"none"],["parameter",7,["uint",false],"none"]],["compound",[["return",["uint",false],["binary","uint","add",["call","uint",2,[["argument",["uint",false],["ident","uint",6]],["argument",["uint",false],["ident","uint",7]]]],["lit","uint","00000001"]]]]]]]]"#

def loopBranch := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[2],[["function",0,"loop_branch_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["conditional","uint",["ident","uint",1],["binary","uint","sub",["ident","uint",1],["lit","uint","00000001"]],["lit","uint","00000003"]]]]]],["function",2,"loop_branch",["uint",false],false,[["parameter",3,["uint",false],"none"],["parameter",4,["uint",false],"none"]],["compound",[["while",["ident","uint",4],["compound",[["expr",["assign","uint","set",3,["uint",false],["ident","uint",3],["call","uint",0,[["argument",["uint",false],["ident","uint",3]]]]]],["expr",["update","uint","pre_dec",4,["uint",false],["ident","uint",4]]]]]],["return",["uint",false],["ident","uint",3]]]]]]]"#

def loopChecked := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[7],[["function",5,"loop_checked_tick",["int",false],false,[["parameter",6,["int",false],"none"]],["compound",[["return",["int",false],["binary","int","add",["ident","int",6],["lit","int","00000001"]]]]]],["function",7,"loop_checked",["int",false],false,[["parameter",8,["int",false],"none"],["parameter",9,["uint",false],"none"]],["compound",[["while",["ident","uint",9],["compound",[["expr",["assign","int","set",8,["int",false],["ident","int",8],["call","int",5,[["argument",["int",false],["ident","int",8]]]]]],["expr",["update","uint","pre_dec",9,["uint",false],["ident","uint",9]]]]]],["return",["int",false],["ident","int",8]]]]]]]"#

def loopForever := r#"["core-v1","aarch64-unknown-linux-gnu","checked",[10],[["function",0,"loop_branch_tick",["uint",false],false,[["parameter",1,["uint",false],"none"]],["compound",[["return",["uint",false],["conditional","uint",["ident","uint",1],["binary","uint","sub",["ident","uint",1],["lit","uint","00000001"]],["lit","uint","00000003"]]]]]],["function",10,"loop_forever",["uint",false],false,[["parameter",11,["uint",false],"none"]],["compound",[["for",null,null,null,["compound",[["expr",["assign","uint","set",11,["uint",false],["ident","uint",11],["call","uint",0,[["argument",["uint",false],["ident","uint",11]]]]]]]]],["return",["uint",false],["ident","uint",11]]]]]]]"#

def target := r#"func @call_tick(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
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
}"#

def controlTarget := r#"func @loop_branch_tick(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 0
    %v4 = icmp.ne %v0, %v3
    cond_br %v4, ^b1(%v0, %v1, %v2), ^b2(%v0, %v1, %v2)

^b1(%v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 1
    %v9 = sub.bits<32> %v5, %v8
    br ^b3(%v5, %v6, %v7, %v9)

^b2(%v10: bits<32>, %v11: mem<all>, %v12: world):
    %v13 = const.bits<32> 3
    br ^b3(%v10, %v11, %v12, %v13)

^b3(%v14: bits<32>, %v15: mem<all>, %v16: world, %v17: bits<32>):
    ret %v17, %v15, %v16
}

func @loop_branch(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    br ^b1(%v0, %v1, %v2, %v3)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 0
    %v9 = icmp.ne %v5, %v8
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v4, %v6, %v7)

^b2(%v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    (%v14, %v15, %v16) = call @loop_branch_tick(%v10, %v12, %v13)
    %v17 = const.bits<32> 1
    %v18 = sub.bits<32> %v11, %v17
    br ^b1(%v14, %v18, %v15, %v16)

^b3(%v19: bits<32>, %v20: mem<all>, %v21: world):
    ret %v19, %v20, %v21
}

func @loop_checked_tick(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = signed_add_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v3
    ret %v5, %v1, %v2
}

func @loop_checked(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    br ^b1(%v0, %v1, %v2, %v3)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 0
    %v9 = icmp.ne %v5, %v8
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v4, %v6, %v7)

^b2(%v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    (%v14, %v15, %v16) = call @loop_checked_tick(%v10, %v12, %v13)
    %v17 = const.bits<32> 1
    %v18 = sub.bits<32> %v11, %v17
    br ^b1(%v14, %v18, %v15, %v16)

^b3(%v19: bits<32>, %v20: mem<all>, %v21: world):
    ret %v19, %v20, %v21
}

func @loop_forever(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    br ^b1(%v0, %v1, %v2)

^b1(%v3: bits<32>, %v4: mem<all>, %v5: world):
    br ^b3(%v3, %v4, %v5)

^b2(%v6: bits<32>, %v7: mem<all>, %v8: world):
    br ^b1(%v6, %v7, %v8)

^b3(%v9: bits<32>, %v10: mem<all>, %v11: world):
    (%v12, %v13, %v14) = call @loop_branch_tick(%v9, %v10, %v11)
    br ^b2(%v12, %v13, %v14)
}"#

def proposal := r#"["core-cutpoints-v2",256,[
  [0,"entry",[[3,"uint",false,true],[4,"uint",false,true]],[[1,4,"uint"],[0,3,"uint"]],[[3,"world"],[2,"mem"]]],
  [2,"b1",[[3,"uint",false,true],[4,"uint",false,true]],[[5,4,"uint"],[4,3,"uint"]],[[7,"world"],[6,"mem"]]]
]]"#

def checkedProposal := r#"["core-cutpoints-v2",256,[
  [0,"entry",[[8,"int",false,true],[9,"uint",false,true]],[[1,9,"uint"],[0,8,"int"]],[[3,"world"],[2,"mem"]]],
  [2,"b1",[[8,"int",false,true],[9,"uint",false,true]],[[5,9,"uint"],[4,8,"int"]],[[7,"world"],[6,"mem"]]]
]]"#

def foreverProposal := r#"["core-cutpoints-v2",256,[
  [0,"entry",[[11,"uint",false,true]],[[0,11,"uint"]],[[2,"world"],[1,"mem"]]],
  [2,"b1",[[11,"uint",false,true]],[[3,11,"uint"]],[[5,"world"],[4,"mem"]]]
]]"#

def callerProposal := r#"["core-cutpoints-v2",256,[
  [0,"entry",[[6,"uint",false,true],[7,"uint",false,true]],[[1,7,"uint"],[0,6,"uint"]],[[3,"world"],[2,"mem"]]]
]]"#

end EvoIR.Core.Source.CallLoopTests
