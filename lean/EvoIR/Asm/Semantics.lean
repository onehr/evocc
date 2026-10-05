import EvoIR.Asm.Syntax
import EvoIR.Core.Correspondence

/-!
Small-step semantics of the restricted AArch64 assembly AST.

This file is the written specification of the target for the frame and
emission proof. Each rule follows the AArch64 pseudo-code of its instruction
and does not use the virtual machine evaluator. The state has the registers
`x0` to `x30` with their 32-bit views, the stack pointer, the NZCV flags and
byte memory. Memory accesses outside the stack region `[low, high)` are
faults. A memory access with base `sp` requires a 16-byte aligned `sp`.

Code addresses are an abstract injective encoding of a function index and a
line index. The actual placement of code is the assembler and linker
boundary. `ret` to an address outside the file returns to the environment;
the observation is then the value of `x0`. `brk #imm` stops with diagnostic
`imm`. A label line is a step without effect. Observations and diagnostics
have the same form as in `EvoIR.Machine`.
-/
namespace EvoIR.Asm

open Core.Source (Fault)

structure Flags where
  n : Bool
  z : Bool
  c : Bool
  v : Bool
  deriving DecidableEq, Repr

/-- ConditionHolds of the AArch64 pseudo-code. -/
def Flags.holds (f : Flags) : Machine.Cond → Bool
  | .eq => f.z
  | .ne => !f.z
  | .hs | .cs => f.c
  | .lo | .cc => !f.c
  | .hi => f.c && !f.z
  | .ls => !f.c || f.z
  | .ge => f.n == f.v
  | .lt => f.n != f.v
  | .gt => !f.z && f.n == f.v
  | .le => f.z || f.n != f.v
  | .vs => f.v
  | .vc => !f.v

/-- The first code address. Code addresses are never small integers. -/
def codeBase : Nat := 2^62

/-- The number of line positions of one function in the address encoding. -/
def functionSpan : Nat := 2^24

def codeAddress (function pc : Nat) : BitVec 64 :=
  BitVec.ofNat 64 (codeBase + 4 * (function * functionSpan + pc))

def decodeAddress (bits : BitVec 64) : Option (Nat × Nat) :=
  let n := bits.toNat
  if n < codeBase ∨ n % 4 ≠ 0 then none
  else
    let k := (n - codeBase) / 4
    some (k / functionSpan, k % functionSpan)

inductive Control where
  | running (function pc : Nat)
  | halted (result : Except Fault (BitVec 64))
  deriving DecidableEq, Repr

/-- The machine state. `low` and `high` are the bounds of the stack region;
no instruction changes them. -/
structure State where
  control : Control
  regs : Nat → BitVec 64
  sp : BitVec 64
  flags : Flags
  memory : Nat → BitVec 8
  low : Nat
  high : Nat

def update (f : Nat → α) (n : Nat) (value : α) : Nat → α :=
  fun m => if m = n then value else f m

@[simp] theorem update_same (f : Nat → α) (n : Nat) (value : α) :
    update f n value n = value := by simp [update]

@[simp] theorem update_other (f : Nat → α) (n m : Nat) (value : α) (h : m ≠ n) :
    update f n value m = f m := by simp [update, h]

/-- The bits of a register in one view. A 32-bit view reads the low half. -/
def view : Machine.Width → BitVec 64 → BitVec 64
  | .w, bits => (bits.setWidth 32).setWidth 64
  | .x, bits => bits

def State.reg (s : State) : GReg → BitVec 64
  | .r n => s.regs n
  | .zr => 0

def State.get (s : State) (width : Machine.Width) (r : GReg) : BitVec 64 :=
  view width (s.reg r)

/-- A 32-bit write sets the upper half to zero. A write to the zero register
has no effect. -/
def State.set (s : State) (width : Machine.Width) (r : GReg) (value : BitVec 64) : State :=
  match r with
  | .zr => s
  | .r n => {s with regs := update s.regs n (view width value)}

/-! Byte memory, little-endian. -/

def loadBytes (memory : Nat → BitVec 8) (address : Nat) : Nat → Nat
  | 0 => 0
  | n+1 => (memory address).toNat + 256 * loadBytes memory (address+1) n

def storeBytes (memory : Nat → BitVec 8) (address : Nat) : Nat → Nat → (Nat → BitVec 8)
  | 0, _ => memory
  | n+1, value => update (storeBytes memory (address+1) n (value / 256)) address
      (BitVec.ofNat 8 value)

def widthBytes (width : Machine.Width) : Nat := width.bits / 8

def State.inStack (s : State) (address size : Nat) : Bool :=
  s.low ≤ address && address + size ≤ s.high

def State.baseValue (s : State) : Base → Except Fault (BitVec 64)
  | .sp => if s.sp.toNat % 16 = 0 then .ok s.sp
      else .error (.malformed "stack pointer misaligned for a memory access")
  | .r n => .ok (s.regs n)

def State.load (s : State) (address : BitVec 64) (size : Nat) : Except Fault (BitVec 64) :=
  if s.inStack address.toNat size then .ok (BitVec.ofNat 64 (loadBytes s.memory address.toNat size))
  else .error (.malformed "load outside the stack region")

def State.store (s : State) (address : BitVec 64) (size : Nat) (value : BitVec 64) :
    Except Fault State :=
  if s.inStack address.toNat size then
    .ok {s with memory := storeBytes s.memory address.toNat size value.toNat}
  else .error (.malformed "store outside the stack region")

/-! Data processing, following the pseudo-code with `datasize` from the view. -/

/-- The result of a two-register data-processing instruction. Variable
shifts use the shift amount modulo `datasize`. Division by zero gives zero. -/
def alu (op : Machine.BinOp) (width : Machine.Width) (lhs rhs : BitVec 64) : BitVec 64 :=
  let n := width.bits
  let a := lhs.setWidth n
  let b := rhs.setWidth n
  let result : BitVec n := match op with
    | .add => a + b
    | .sub => a - b
    | .mul => a * b
    | .and => a &&& b
    | .orr => a ||| b
    | .eor => a ^^^ b
    | .lsl => a <<< (b.toNat % n)
    | .lsr => a >>> (b.toNat % n)
    | .asr => a.sshiftRight (b.toNat % n)
    | .udiv => BitVec.ofNat n (if b.toNat = 0 then 0 else a.toNat / b.toNat)
    | .sdiv => BitVec.ofInt n (if b.toNat = 0 then 0 else a.toInt.tdiv b.toInt)
  result.setWidth 64

/-- AddWithCarry of the pseudo-code, at `datasize`. -/
def addWithCarry (width : Machine.Width) (x y : BitVec 64) (carry : Bool) : BitVec 64 × Flags :=
  let n := width.bits
  let a := x.setWidth n
  let b := y.setWidth n
  let unsignedSum := a.toNat + b.toNat + carry.toNat
  let signedSum := a.toInt + b.toInt + (carry.toNat : Int)
  let result := BitVec.ofNat n unsignedSum
  (result.setWidth 64, ⟨result.msb, result == 0, decide (result.toNat ≠ unsignedSum),
    decide (result.toInt ≠ signedSum)⟩)

def flagAlu (op : Machine.FlagOp) (width : Machine.Width) (x y : BitVec 64) : BitVec 64 × Flags :=
  match op with
  | .adds => addWithCarry width x y false
  | .subs => addWithCarry width x (~~~y) true

/-- The flags of ANDS at `datasize`. -/
def logicalFlags (width : Machine.Width) (x y : BitVec 64) : Flags :=
  let result := x.setWidth width.bits &&& y.setWidth width.bits
  ⟨result.msb, result == 0, false, false⟩

def wideMul (op : Machine.WideMul) (x y : BitVec 64) : BitVec 64 :=
  match op with
  | .smull => BitVec.ofInt 64 ((x.setWidth 32).toInt * (y.setWidth 32).toInt)
  | .smulh => BitVec.ofInt 64 ((x.toInt * y.toInt) >>> 64)
  | .umulh => BitVec.ofNat 64 (x.toNat * y.toNat / 2^64)

def extend (op : Machine.ExtOp) (value : BitVec 64) : BitVec 64 :=
  let bits := value.setWidth op.bits
  if op.signed then bits.signExtend 64 else bits.setWidth 64

def bitfield (signed : Bool) (width : Machine.Width) (value : BitVec 64) (lsb bits : Nat) :
    BitVec 64 :=
  let field := (view width value >>> lsb).setWidth bits
  if signed then field.signExtend 64 else field.setWidth 64

/-- MOVK: `result<shift+15:shift> = imm16`; the other bits keep their value.
The bits above, the new sixteen bits and the bits below are added. -/
def insertHalf (old : BitVec 64) (imm shift : Nat) : BitVec 64 :=
  BitVec.ofNat 64 (old.toNat / 2^(shift+16) * 2^(shift+16) + imm % 2^16 * 2^shift +
    old.toNat % 2^shift)

/-! Control. -/

def labelIndex (body : List Line) (block : Nat) : Option Nat :=
  body.findIdx? (· == .label block)

def functionIndex (file : File) (symbol : String) : Option Nat :=
  file.functions.findIdx? (·.name == symbol)

/-- The control after `ret`: a return address inside the file continues
there; any other address returns to the environment with `x0`. -/
def returnControl (file : File) (s : State) : Control :=
  match decodeAddress (s.regs 30) with
  | some (function, pc) =>
    if function < file.functions.length then .running function pc else .halted (.ok (s.regs 0))
  | none => .halted (.ok (s.regs 0))

def State.next (s : State) (function pc : Nat) : State :=
  {s with control := .running function (pc+1)}

def jump (file : File) (s : State) (function block : Nat) : Except Fault State := do
  let some f := file.functions[function]? | throw (.malformed "assembly missing function")
  let some target := labelIndex f.body block | throw (.malformed "assembly missing label")
  return {s with control := .running function target}

/-- One instruction. `function` and `pc` locate the instruction. -/
def execute (file : File) (s : State) (function pc : Nat) : Inst → Except Fault State
  | .stpPre first second off => do
    let base ← s.baseValue .sp
    let address := base + BitVec.ofInt 64 off
    let s ← s.store address 8 (s.regs first)
    let s ← s.store (address + 8) 8 (s.regs second)
    return {s with sp := address}.next function pc
  | .ldpPost first second off => do
    let base ← s.baseValue .sp
    let one ← s.load base 8
    let two ← s.load (base + 8) 8
    let s := (s.set .x (.r first) one).set .x (.r second) two
    return {s with sp := base + BitVec.ofInt 64 off}.next function pc
  | .movFromSp d => return (s.set .x (.r d) s.sp).next function pc
  | .subSp n => return {s with sp := s.sp - BitVec.ofNat 64 n}.next function pc
  | .addSp n => return {s with sp := s.sp + BitVec.ofNat 64 n}.next function pc
  | .addFromSp d n => return (s.set .x d (s.sp + BitVec.ofNat 64 n)).next function pc
  | .ldr width t base off => do
    let address := (← s.baseValue base) + BitVec.ofInt 64 off
    let value ← s.load address (widthBytes width)
    return (s.set width t value).next function pc
  | .str width t base off => do
    let address := (← s.baseValue base) + BitVec.ofInt 64 off
    let s' ← s.store address (widthBytes width) (s.get width t)
    return s'.next function pc
  | .mov width d src => return (s.set width d (s.get width src)).next function pc
  | .movz width d n => return (s.set width d (BitVec.ofNat 64 n)).next function pc
  | .movk width d n shift =>
    return (s.set width d (insertHalf (s.reg d) n shift)).next function pc
  | .alu op width d lhs rhs =>
    return (s.set width d (alu op width (s.reg lhs) (s.reg rhs))).next function pc
  | .aluImm op width d lhs n =>
    return (s.set width d (alu op width (s.reg lhs) (BitVec.ofNat 64 n))).next function pc
  | .flagAlu op width d lhs rhs =>
    let (value, flags) := flagAlu op width (s.reg lhs) (s.reg rhs)
    return {s.set width d value with flags}.next function pc
  | .wideMul op d lhs rhs =>
    return (s.set .x d (wideMul op (s.reg lhs) (s.reg rhs))).next function pc
  | .msub width d n m a =>
    return (s.set width d (alu .sub width (s.reg a) (alu .mul width (s.reg n) (s.reg m)))).next
      function pc
  | .madd width d n m a =>
    return (s.set width d (alu .add width (s.reg a) (alu .mul width (s.reg n) (s.reg m)))).next
      function pc
  | .extend op width d src => return (s.set width d (extend op (s.reg src))).next function pc
  | .bfx signed width d src lsb bits =>
    return (s.set width d (bitfield signed width (s.reg src) lsb bits)).next function pc
  | .cmp width lhs rhs =>
    return {s with flags := (flagAlu .subs width (s.reg lhs) (s.reg rhs)).2}.next function pc
  | .cmpImm width lhs n =>
    return {s with flags := (flagAlu .subs width (s.reg lhs) (BitVec.ofNat 64 n)).2}.next
      function pc
  | .tstImm width lhs n =>
    return {s with flags := logicalFlags width (s.reg lhs) (BitVec.ofNat 64 n)}.next function pc
  | .cset width d cond =>
    return (s.set width d (if s.flags.holds cond then 1 else 0)).next function pc
  | .csel width d lhs rhs cond =>
    return (s.set width d (if s.flags.holds cond then s.reg lhs else s.reg rhs)).next function pc
  | .bl symbol => do
    let some callee := functionIndex file symbol | throw (.malformed "assembly external call")
    let s := s.set .x (.r 30) (codeAddress function (pc+1))
    return {s with control := .running callee 0}
  | .b block => jump file s function block
  | .bCond cond block =>
    if s.flags.holds cond then jump file s function block else return s.next function pc
  | .cb nonzero width src block =>
    if (s.get width src != 0) == nonzero then jump file s function block
    else return s.next function pc
  | .tb nonzero width src bit block =>
    if (s.get width src).getLsbD bit == nonzero then jump file s function block
    else return s.next function pc
  | .ret => return {s with control := returnControl file s}
  | .brk n => throw (.diagnostic n)

/-- Fetch and execute the line at `pc` of `function`. -/
def run (file : File) (s : State) (function pc : Nat) : Except Fault State := do
  let some f := file.functions[function]? | throw (.malformed "assembly missing function")
  match f.body[pc]? with
  | some (.label _) => return s.next function pc
  | some (.inst i) => execute file s function pc i
  | none => throw (.malformed "assembly execution left the function body")

def step (file : File) (s : State) : Option State :=
  match s.control with
  | .halted _ => none
  | .running function pc => some (match run file s function pc with
    | .ok next => next
    | .error fault => {s with control := .halted (.error fault)})

/-- The same observations as `EvoIR.Machine.observe`: returned bits of the
result type, or the first admitted diagnostic. -/
def observe (type : Core.Source.IntType) (s : State) : Option Core.Observation :=
  match s.control with
  | .halted (.ok bits) =>
    if type.width == 32 || type.width == 64 then
      some (.returned type (bits.setWidth type.width).toNat) else none
  | .halted (.error (.diagnostic code)) => Core.checkedDiagnostic code
  | _ => none

theorem running_progress (file : File) (s : State) (function pc : Nat)
    (running : s.control = .running function pc) : ∃ next, step file s = some next := by
  simp [step, running]

theorem halted_stuck (file : File) (s : State) (result : Except Fault (BitVec 64))
    (halted : s.control = .halted result) : step file s = none := by
  simp [step, halted]

end EvoIR.Asm
