import EvoIR.Core.Scalar

/-!
The admitted EvoIR syntax. This is a distinct input from Core C-HIR, with
block-local SSA, explicit edge arguments, effects and memory/world operands.
No source expression is substituted for an instruction during decoding.

The source/target checker and canonical-text reader are still being built.
Constructing this AST or evaluating a pure instruction is not admission.
-/
namespace EvoIR.Core.IR

open Source (Fault)

inductive Ty where
  | bits (width : Nat)
  | pointer | memory | world
  deriving DecidableEq, Repr

structure Binding where
  id : Nat
  type : Ty
  deriving DecidableEq, Repr

structure Effects where
  readsMemory : Bool
  writesMemory : Bool
  volatile : Bool
  atomic : Bool
  io : Bool
  fenv : Bool
  mayTrap : Bool
  mayUnwind : Bool
  deriving DecidableEq, Repr

inductive Reason where
  | divisionByZero | remainderByZero | signedOverflow | invalidShiftCount
  deriving DecidableEq, Repr

def Reason.code : Reason → Nat
  | .divisionByZero => 3002
  | .remainderByZero => 3003
  | .signedOverflow => 3004
  | .invalidShiftCount => 3005

inductive Operation where
  | constant (width bits : Nat)
  | uninitialized (type : Ty)
  | scalar (op : Op)
  | division (signed remainder : Bool) (width : Nat)
  | assume (reason : Reason)
  | call (function : String)
  | allocate (origin size align : Nat)
  | endLifetime
  | access (size align : Nat)
  | load (width : Nat)
  | store
  deriving DecidableEq, Repr

structure Instruction where
  results : List Binding
  op : Operation
  operands : List Nat
  deriving DecidableEq, Repr

structure Edge where
  block : String
  arguments : List Nat
  deriving DecidableEq, Repr

inductive Terminator where
  | branch (edge : Edge)
  | conditional (condition : Nat) (yes no : Edge)
  | return (values : List Nat)
  deriving DecidableEq, Repr

structure Block where
  name : String
  parameters : List Binding
  instructions : List Instruction
  terminator : Terminator
  deriving DecidableEq, Repr

structure Function where
  name : String
  parameters : List Ty
  returns : List Ty
  internal : Bool
  effects : Effects
  blocks : List Block
  deriving DecidableEq, Repr

structure Program where
  functions : List Function
  deriving DecidableEq, Repr

/-- Snapshots preserve private object identity, ownership, lifetime and
per-byte initialization. Even a same-sized allocation gets a fresh ID. -/
structure Object where
  id : Nat
  origin : Nat
  owner : Nat
  align : Nat
  live : Bool
  bytes : List (Option (BitVec 8))
  deriving DecidableEq, Repr

structure Memory where
  nextId : Nat
  objects : List Object
  deriving DecidableEq, Repr

inductive Value where
  | bits (width : Nat) (value : BitVec width)
  | poison (type : Ty)
  | pointer (object : Nat) (offset : Int)
  | memory (snapshot : Memory)
  | world
  deriving DecidableEq, Repr

def Value.type : Value → Ty
  | .bits n _ => .bits n
  | .poison t => t
  | .pointer _ _ => .pointer
  | .memory _ => .memory
  | .world => .world

def Value.asBits (n : Nat) : Value → Except Fault (BitVec n)
  | .bits m value =>
    if m == n then .ok (value.setWidth n)
    else .error (.malformed "IR integer operand width mismatch")
  | .poison _ => .error (.diagnostic 3001)
  | _ => .error (.malformed "IR noninteger operand")

/-- The shared total bit-vector semantics, with independently checked operand
shapes. A guard consumes poisoned data, including the shifted operand even
when its count alone determines the predicate's initialized result. -/
def evalScalar : Op → List Value → Except Fault Value
  | .bin op n, [a, b] => do return .bits n (denoteOp (.bin op n) (← a.asBits n, ← b.asBits n))
  | .cmp op n, [a, b] => do return .bits 1 (denoteOp (.cmp op n) (← a.asBits n, ← b.asBits n))
  | .shift op n m, [a, b] => do return .bits n (denoteOp (.shift op n m) (← a.asBits n, ← b.asBits m))
  | .cast op n m, [a] => do return .bits m (denoteOp (.cast op n m) (← a.asBits n))
  | .guard op n, [a, b] => do return .bits 1 (denoteOp (.guard op n) (← a.asBits n, ← b.asBits n))
  | .nonzero n, [a] => do return .bits 1 (denoteOp (.nonzero n) (← a.asBits n))
  | .count signed n m, [a, b] => do
      return .bits 1 (denoteOp (.count signed n m) (← a.asBits n, ← b.asBits m))
  | _, _ => .error (.malformed "IR scalar operand count mismatch")

def evalDivision (signed remainder : Bool) (width : Nat) (args : List Value) : Except Fault Value := do
  let [a, b] := args | throw (.malformed "IR division operand count mismatch")
  let x ← a.asBits width
  let y ← b.asBits width
  if y.toNat == 0 then throw (.diagnostic (if remainder then 3003 else 3002))
  if signed then
    if x.toInt == signedMin width && y.toInt == -1 then throw (.diagnostic 3004)
    return .bits width (BitVec.ofInt width
      (if remainder then x.toInt.tmod y.toInt else x.toInt.tdiv y.toInt))
  else
    return .bits width (BitVec.ofNat width
      (if remainder then x.toNat % y.toNat else x.toNat / y.toNat))

-- A predicate's apparently irrelevant operand still carries poison.
example : evalScalar (.count false 32 64) [.poison (.bits 32), .bits 64 (0#64)] =
    .error (.diagnostic 3001) := by rfl
example : evalScalar (.count false 32 64) [.bits 32 (1#32), .bits 64 (0x100000001#64)] =
    .ok (.bits 1 (0#1)) := by rfl
example : evalDivision true true 32 [.bits 32 (0x80000000#32), .bits 32 (0xffffffff#32)] =
    .error (.diagnostic 3004) := by decide

end EvoIR.Core.IR
