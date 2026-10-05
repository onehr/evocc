import EvoIR.MachineReader

namespace EvoIR.Machine.Reader

private def readInstruction (s : String) : Option Instruction :=
  (Lean.Json.parse s).toOption.bind instruction

private def readTerminator (s : String) : Option Terminator :=
  (Lean.Json.parse s).toOption.bind terminator

private def cases : List (String × Bool) := [
  ("explicit branch flags",
    readTerminator "[\"b_cond\",[7,\"flags\"],\"ne\",2,[],3,[]]" ==
      some (.bCond ⟨7, .flags⟩ .ne 2 [] 3 [])),
  ("missing branch flags refused",
    (readTerminator "[\"b_cond\",\"ne\",2,[],3,[]]").isNone),
  ("extra branch fields refused",
    (readTerminator "[\"b_cond\",[7,\"flags\"],\"ne\",2,[],3,[],0]").isNone),
  ("register class retained",
    readInstruction "[\"binary\",\"add\",[2,\"gpr64\"],[0,\"gpr32\"],[1,\"gpr64\"],\"w\"]" ==
      some (.binary .add ⟨2, .gpr64⟩ ⟨0, .gpr32⟩ ⟨1, .gpr64⟩ .w)),
  ("full immediate retained",
    readInstruction "[\"mov_imm\",[1,\"gpr64\"],18446744073709551615]" ==
      some (.movImm ⟨1, .gpr64⟩ (2^64-1))),
  ("immediate overflow refused",
    (readInstruction "[\"mov_imm\",[1,\"gpr64\"],18446744073709551616]").isNone),
  ("register identity overflow refused",
    (readInstruction "[\"mov_imm\",[4294967296,\"gpr64\"],0]").isNone),
  ("negative immediate refused",
    (readInstruction "[\"mov_imm\",[1,\"gpr64\"],-1]").isNone),
  ("memory attributes retained",
    readInstruction "[\"load\",1,[3,\"gpr32\"],[4,\"gpr64\"],-9223372036854775808,\"w\",true]" ==
      some (.load 1 ⟨3, .gpr32⟩ ⟨4, .gpr64⟩ (-(2^63)) .w true)),
  ("offset overflow refused",
    (readInstruction "[\"load\",1,[3,\"gpr32\"],[4,\"gpr64\"],9223372036854775808,\"w\",false]").isNone),
  ("unknown opcode refused", (readInstruction "[\"invented\"]").isNone),
  ("post-allocation stack form refused",
    (readInstruction "[\"incoming_arg\",[1,\"gpr64\"],8]").isNone),
  ("unknown call mode refused", (readInstruction "[\"bl\",\"callee\",[],[],\"strict\"]").isNone),
  ("call fields retained", readInstruction "[\"bl\",\"callee\",[[4,\"gpr32\"]],[[5,\"gpr64\"]],null]" ==
    some (.bl "callee" [⟨4, .gpr32⟩] [⟨5, .gpr64⟩])),
  ("checked profile decoded", (readProgram "[\"core-mir-v1\",\"aarch64-unknown-linux-gnu\",\"checked\",\"ssa\",[]]").toOption ==
    some (.ssa, ⟨[]⟩)),
  ("unchecked profile refused", (readProgram "[\"core-mir-v1\",\"aarch64-unknown-linux-gnu\",\"unchecked\",\"ssa\",[]]").toOption.isNone),
  ("wrong target refused", (readProgram "[\"core-mir-v1\",\"aarch64-apple-darwin\",\"checked\",\"ssa\",[]]").toOption.isNone),
  ("unknown mode refused", (readProgram "[\"core-mir-v1\",\"aarch64-unknown-linux-gnu\",\"checked\",\"allocated\",[]]").toOption.isNone)
]

def selfTest : IO Unit := do
  for (name, passed) in cases do
    unless passed do throw (IO.userError s!"machine reader failed: {name}")
  IO.println s!"machine reader: {cases.length} passed"

#eval selfTest

end EvoIR.Machine.Reader
