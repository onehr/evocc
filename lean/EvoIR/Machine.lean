import EvoIR.Core.IRExecution

/-!
The independent, pre-allocation EvoMIR syntax for the checked core-v1 path.
The constructors retain the operands and widths of the actual Rust MInst and
MTerm. Unsupported machine forms are refused by the reader, not represented
as opaque, supposedly preserving operations. Physical allocation, spill slots,
frame offsets and assembly emission are deliberately not part of this syntax.
-/
namespace EvoIR.Machine

inductive RegClass where
  | gpr32 | gpr64 | flags
  deriving DecidableEq, Repr

structure VReg where
  id : Nat
  «class» : RegClass
  deriving DecidableEq, Repr

inductive Width where
  | w | x
  deriving DecidableEq, Repr

def Width.bits : Width → Nat
  | .w => 32
  | .x => 64

def Width.class : Width → RegClass
  | .w => .gpr32
  | .x => .gpr64

def RegClass.width : RegClass → Option Width
  | .gpr32 => some .w
  | .gpr64 => some .x
  | .flags => none

inductive BinOp where
  | add | sub | mul | sdiv | udiv | and | orr | eor | lsl | lsr | asr
  deriving DecidableEq, Repr

inductive FlagOp where
  | adds | subs
  deriving DecidableEq, Repr

inductive WideMul where
  | smull | smulh | umulh
  deriving DecidableEq, Repr

inductive ExtOp where
  | sxtb | sxth | sxtw | uxtb | uxth
  deriving DecidableEq, Repr

def ExtOp.bits : ExtOp → Nat
  | .sxtb | .uxtb => 8
  | .sxth | .uxth => 16
  | .sxtw => 32

def ExtOp.signed : ExtOp → Bool
  | .sxtb | .sxth | .sxtw => true
  | .uxtb | .uxth => false

inductive Cond where
  | eq | ne | lt | le | gt | ge | lo | ls | hi | hs | vs | vc | cs | cc
  deriving DecidableEq, Repr

inductive LoadKind where
  | w | x
  deriving DecidableEq, Repr

def LoadKind.bits : LoadKind → Nat
  | .w => 32
  | .x => 64

inductive StoreKind where
  | w | x
  deriving DecidableEq, Repr

def StoreKind.bits : StoreKind → Nat
  | .w => 32
  | .x => 64

inductive Instruction where
  | mov (dst src : VReg)
  | movImm (dst : VReg) (imm : Nat)
  | binary (op : BinOp) (dst lhs rhs : VReg) (width : Width)
  | binaryImm (op : BinOp) (dst lhs : VReg) (imm : Nat) (width : Width)
  | flagBinary (op : FlagOp) (dst flags lhs rhs : VReg) (width : Width)
  | wideMultiply (op : WideMul) (dst lhs rhs : VReg)
  | msub (dst n m a : VReg) (width : Width)
  | madd (dst n m a : VReg) (width : Width)
  | extend (op : ExtOp) (dst src : VReg)
  | bfx (signed : Bool) (dst src : VReg) (lsb bits : Nat) (width : Width)
  | cmp (dst lhs rhs : VReg) (width : Width)
  | cmpImm (dst lhs : VReg) (imm : Nat) (width : Width)
  | tstImm (dst lhs : VReg) (imm : Nat) (width : Width)
  | cset (dst flags : VReg) (cond : Cond)
  | csel (dst flags lhs rhs : VReg) (cond : Cond) (width : Width)
  | load (align : Nat) (dst base : VReg) (offset : Int) (kind : LoadKind) (volatile : Bool)
  | store (align : Nat) (src base : VReg) (offset : Int) (kind : StoreKind) (volatile : Bool)
  | frameSlot (dst : VReg) (origin size align : Nat)
  | stackRelease (address : VReg)
  | bl (symbol : String) (args results : List VReg)
  deriving DecidableEq, Repr

inductive Terminator where
  | b (target : Nat) (args : List VReg)
  | bCond (flags : VReg) (cond : Cond) (taken : Nat) (takenArgs : List VReg)
      (untaken : Nat) (untakenArgs : List VReg)
  | cb (nonzero : Bool) (src : VReg) (taken : Nat) (takenArgs : List VReg)
      (untaken : Nat) (untakenArgs : List VReg)
  | tb (nonzero : Bool) (src : VReg) (bit : Nat) (taken : Nat) (takenArgs : List VReg)
      (untaken : Nat) (untakenArgs : List VReg)
  | ret (values : List VReg)
  | brk (imm : Nat)
  deriving DecidableEq, Repr

structure Block where
  id : Nat
  args : List VReg
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

/-- SSA block arguments are simultaneous fresh bindings. In postSSA the
actual edge copies operate on a persistent virtual-register bank instead. -/
inductive Mode where
  | ssa | postSSA
  deriving DecidableEq, Repr

/-- These finite operand lists are conveniences for checkers, not semantic
axioms. MachineExecution dispatches on the actual instruction constructors. -/
def Instruction.uses : Instruction → List VReg
  | .mov _ src | .extend _ _ src | .bfx _ _ src _ _ _ => [src]
  | .movImm _ _ | .frameSlot _ _ _ _ => []
  | .binary _ _ lhs rhs _ | .flagBinary _ _ _ lhs rhs _
  | .wideMultiply _ _ lhs rhs | .cmp _ lhs rhs _ => [lhs, rhs]
  | .binaryImm _ _ lhs _ _ | .cmpImm _ lhs _ _ | .tstImm _ lhs _ _ => [lhs]
  | .msub _ n m a _ | .madd _ n m a _ => [n, m, a]
  | .cset _ flags _ => [flags]
  | .csel _ flags lhs rhs _ _ => [flags, lhs, rhs]
  | .load _ _ base _ _ _ => [base]
  | .store _ src base _ _ _ => [src, base]
  | .stackRelease address => [address]
  | .bl _ args _ => args

def Instruction.defs : Instruction → List VReg
  | .mov dst _ | .movImm dst _ | .binary _ dst _ _ _
  | .binaryImm _ dst _ _ _ | .wideMultiply _ dst _ _
  | .msub dst _ _ _ _ | .madd dst _ _ _ _ | .extend _ dst _
  | .bfx _ dst _ _ _ _ | .cmp dst _ _ _ | .cmpImm dst _ _ _
  | .tstImm dst _ _ _ | .cset dst _ _ | .csel dst _ _ _ _ _
  | .load _ dst _ _ _ _ | .frameSlot dst _ _ _ => [dst]
  | .flagBinary _ dst flags _ _ _ => [dst, flags]
  | .store _ _ _ _ _ _ | .stackRelease _ => []
  | .bl _ _ results => results

def Instruction.mapRegs (f : VReg → VReg) : Instruction → Instruction
  | .mov dst src => .mov (f dst) (f src)
  | .movImm dst imm => .movImm (f dst) imm
  | .binary op dst lhs rhs width => .binary op (f dst) (f lhs) (f rhs) width
  | .binaryImm op dst lhs imm width => .binaryImm op (f dst) (f lhs) imm width
  | .flagBinary op dst flags lhs rhs width =>
      .flagBinary op (f dst) (f flags) (f lhs) (f rhs) width
  | .wideMultiply op dst lhs rhs => .wideMultiply op (f dst) (f lhs) (f rhs)
  | .msub dst n m a width => .msub (f dst) (f n) (f m) (f a) width
  | .madd dst n m a width => .madd (f dst) (f n) (f m) (f a) width
  | .extend op dst src => .extend op (f dst) (f src)
  | .bfx signed dst src lsb bits width => .bfx signed (f dst) (f src) lsb bits width
  | .cmp dst lhs rhs width => .cmp (f dst) (f lhs) (f rhs) width
  | .cmpImm dst lhs imm width => .cmpImm (f dst) (f lhs) imm width
  | .tstImm dst lhs imm width => .tstImm (f dst) (f lhs) imm width
  | .cset dst flags cond => .cset (f dst) (f flags) cond
  | .csel dst flags lhs rhs cond width => .csel (f dst) (f flags) (f lhs) (f rhs) cond width
  | .load align dst base offset kind volatile => .load align (f dst) (f base) offset kind volatile
  | .store align src base offset kind volatile => .store align (f src) (f base) offset kind volatile
  | .frameSlot dst origin size align => .frameSlot (f dst) origin size align
  | .stackRelease address => .stackRelease (f address)
  | .bl symbol args results => .bl symbol (args.map f) (results.map f)

/-- Rename semantic inputs only, retaining the actual destinations and all
opcode/width/effect fields. This matches operand forwarding, not selection. -/
def Instruction.mapUses (f : VReg → VReg) : Instruction → Instruction
  | .mov dst src => .mov dst (f src)
  | .movImm dst imm => .movImm dst imm
  | .binary op dst lhs rhs width => .binary op dst (f lhs) (f rhs) width
  | .binaryImm op dst lhs imm width => .binaryImm op dst (f lhs) imm width
  | .flagBinary op dst flags lhs rhs width => .flagBinary op dst flags (f lhs) (f rhs) width
  | .wideMultiply op dst lhs rhs => .wideMultiply op dst (f lhs) (f rhs)
  | .msub dst n m a width => .msub dst (f n) (f m) (f a) width
  | .madd dst n m a width => .madd dst (f n) (f m) (f a) width
  | .extend op dst src => .extend op dst (f src)
  | .bfx signed dst src lsb bits width => .bfx signed dst (f src) lsb bits width
  | .cmp dst lhs rhs width => .cmp dst (f lhs) (f rhs) width
  | .cmpImm dst lhs imm width => .cmpImm dst (f lhs) imm width
  | .tstImm dst lhs imm width => .tstImm dst (f lhs) imm width
  | .cset dst flags cond => .cset dst (f flags) cond
  | .csel dst flags lhs rhs cond width => .csel dst (f flags) (f lhs) (f rhs) cond width
  | .load align dst base offset kind volatile => .load align dst (f base) offset kind volatile
  | .store align src base offset kind volatile => .store align (f src) (f base) offset kind volatile
  | .frameSlot dst origin size align => .frameSlot dst origin size align
  | .stackRelease address => .stackRelease (f address)
  | .bl symbol args results => .bl symbol (args.map f) results

@[simp] theorem Instruction.mapRegs_id (i : Instruction) : i.mapRegs id = i := by
  cases i <;> simp [Instruction.mapRegs]

@[simp] theorem Instruction.uses_mapRegs (f : VReg → VReg) (i : Instruction) :
    (i.mapRegs f).uses = i.uses.map f := by
  cases i <;> simp [Instruction.mapRegs, Instruction.uses]

@[simp] theorem Instruction.defs_mapRegs (f : VReg → VReg) (i : Instruction) :
    (i.mapRegs f).defs = i.defs.map f := by
  cases i <;> simp [Instruction.mapRegs, Instruction.defs]

@[simp] theorem Instruction.uses_mapUses (f : VReg → VReg) (i : Instruction) :
    (i.mapUses f).uses = i.uses.map f := by
  cases i <;> simp [Instruction.mapUses, Instruction.uses]

@[simp] theorem Instruction.defs_mapUses (f : VReg → VReg) (i : Instruction) :
    (i.mapUses f).defs = i.defs := by
  cases i <;> simp [Instruction.mapUses, Instruction.defs]

def Terminator.edges : Terminator → List (Nat × List VReg)
  | .b target args => [(target, args)]
  | .bCond _ _ taken takenArgs untaken untakenArgs
  | .cb _ _ taken takenArgs untaken untakenArgs
  | .tb _ _ _ taken takenArgs untaken untakenArgs => [(taken, takenArgs), (untaken, untakenArgs)]
  | .ret _ | .brk _ => []

end EvoIR.Machine
