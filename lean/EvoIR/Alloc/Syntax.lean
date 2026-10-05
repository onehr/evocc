import EvoIR.Machine

/-!
Allocated EvoMIR for the checked core-v1 path (work package V03).

The instruction forms are the forms of `EvoIR.Machine`. Each operand is a
location instead of a virtual register. A location is a physical general
register with a width view, the flags location, a spill slot, an incoming
stack argument slot or an outgoing stack argument slot. The allocator inserts
register moves (`mov`), spill stores, spill reloads, incoming argument reads
and outgoing argument writes. Frame layout, prologue, epilogue and stack
offsets are not part of this syntax. Work package V04 adds them.

Terminators have no edge arguments. After out-of-SSA, edge values are copies
in the predecessor block.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc

open EvoIR.Machine (Width RegClass)

/-- One storage place of an activation. The 32-bit view and the 64-bit view of
one general register are the same place. -/
inductive Place where
  | gpr (number : Nat)
  | flags
  | spill (slot : Nat)
  | incoming (slot : Nat)
  | outgoing (slot : Nat)
  deriving DecidableEq, Repr

/-- An operand location. A general register and a stack slot are accessed
through a width view. The flags location has no view. -/
inductive Loc where
  | gpr (number : Nat) (view : Width)
  | flags
  | spill (slot : Nat) (view : Width)
  | incoming (slot : Nat) (view : Width)
  | outgoing (slot : Nat) (view : Width)
  deriving DecidableEq, Repr

def Loc.place : Loc → Place
  | .gpr number _ => .gpr number
  | .flags => .flags
  | .spill slot _ => .spill slot
  | .incoming slot _ => .incoming slot
  | .outgoing slot _ => .outgoing slot

/-- The access width. The flags location reports the 64-bit width; no value
access uses it. -/
def Loc.width : Loc → Width
  | .gpr _ view | .spill _ view | .incoming _ view | .outgoing _ view => view
  | .flags => .x

/-- The virtual register class that this location can hold. -/
def Loc.class : Loc → RegClass
  | .flags => .flags
  | loc => loc.width.class

/-- The location that reads place `place` with the view of class `regClass`.
A general class cannot be in the flags place, and flags cannot be in a
general register or a stack slot. -/
def Place.at (place : Place) (regClass : RegClass) : Option Loc :=
  match place, regClass.width with
  | .flags, none => some .flags
  | .gpr number, some view => some (.gpr number view)
  | .spill slot, some view => some (.spill slot view)
  | .incoming slot, some view => some (.incoming slot view)
  | .outgoing slot, some view => some (.outgoing slot view)
  | _, _ => none

inductive Instruction where
  | mov (dst src : Loc)
  | movImm (dst : Loc) (imm : Nat)
  | binary (op : Machine.BinOp) (dst lhs rhs : Loc) (width : Width)
  | binaryImm (op : Machine.BinOp) (dst lhs : Loc) (imm : Nat) (width : Width)
  | flagBinary (op : Machine.FlagOp) (dst flags lhs rhs : Loc) (width : Width)
  | wideMultiply (op : Machine.WideMul) (dst lhs rhs : Loc)
  | msub (dst n m a : Loc) (width : Width)
  | madd (dst n m a : Loc) (width : Width)
  | extend (op : Machine.ExtOp) (dst src : Loc)
  | bfx (signed : Bool) (dst src : Loc) (lsb bits : Nat) (width : Width)
  | cmp (dst lhs rhs : Loc) (width : Width)
  | cmpImm (dst lhs : Loc) (imm : Nat) (width : Width)
  | tstImm (dst lhs : Loc) (imm : Nat) (width : Width)
  | cset (dst flags : Loc) (cond : Machine.Cond)
  | csel (dst flags lhs rhs : Loc) (cond : Machine.Cond) (width : Width)
  | load (align : Nat) (dst base : Loc) (offset : Int) (kind : Machine.LoadKind) (volatile : Bool)
  | store (align : Nat) (src base : Loc) (offset : Int) (kind : Machine.StoreKind) (volatile : Bool)
  | frameSlot (dst : Loc) (origin size align : Nat)
  | stackRelease (address : Loc)
  /-- `args` are the argument registers in ABI order. Stack arguments are
  written before the call by `outgoingArg`. `results` are the result registers. -/
  | bl (symbol : String) (args results : List Loc)
  /-- Write the value of `src` to spill slot `slot` through the view of `src`. -/
  | spillStore (slot : Nat) (src : Loc)
  /-- Read spill slot `slot` through the view of `dst` into `dst`. -/
  | spillReload (dst : Loc) (slot : Nat)
  /-- Read incoming stack argument slot `slot` through the view of `dst`. -/
  | incomingArg (dst : Loc) (slot : Nat)
  /-- Write `src` to outgoing stack argument slot `slot` through its view. -/
  | outgoingArg (slot : Nat) (src : Loc)
  deriving DecidableEq, Repr

inductive Terminator where
  | b (target : Nat)
  | bCond (flags : Loc) (cond : Machine.Cond) (taken untaken : Nat)
  | cb (nonzero : Bool) (src : Loc) (taken untaken : Nat)
  | tb (nonzero : Bool) (src : Loc) (bit : Nat) (taken untaken : Nat)
  /-- `values` are the result registers in ABI order. -/
  | ret (values : List Loc)
  | brk (imm : Nat)
  deriving DecidableEq, Repr

structure Block where
  id : Nat
  insts : List Instruction
  term : Terminator
  deriving DecidableEq, Repr

structure Function where
  name : String
  blocks : List Block
  deriving DecidableEq, Repr

structure Program where
  functions : List Function
  deriving DecidableEq, Repr

def Terminator.targets : Terminator → List Nat
  | .b target => [target]
  | .bCond _ _ taken untaken | .cb _ _ taken untaken | .tb _ _ _ taken untaken => [taken, untaken]
  | .ret _ | .brk _ => []

/-! ABI facts of `targets/aarch64/abi-linux` and `targets/aarch64/regfile`. -/

/-- `arguments gpr 8` and `results gpr 8`. -/
def abiRegisterCount : Nat := 8

/-- `set GPR_CALL_CLOBBERS 0 .. 17`. The flags and the outgoing stack argument
area are also unspecified after a call. -/
def Place.callClobbered : Place → Bool
  | .gpr number => decide (number < 18)
  | .flags => true
  | .outgoing _ => true
  | .spill _ | .incoming _ => false

/-- The locations are the general registers 0, 1, 2, ... in order, at most
`abiRegisterCount` of them. -/
def abiRegistersFrom (next : Nat) : List Loc → Bool
  | [] => true
  | loc :: rest => loc.place == .gpr next && abiRegistersFrom (next+1) rest

def abiRegisters (locs : List Loc) : Bool :=
  decide (locs.length ≤ abiRegisterCount) && abiRegistersFrom 0 locs

end EvoIR.Alloc
