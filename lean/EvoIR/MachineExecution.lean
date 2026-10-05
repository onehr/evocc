import EvoIR.Machine
import EvoIR.Core.Correspondence

/-!
Executable small-step semantics of the actual virtual EvoMIR constructors.
MIR is not decoded back into EvoIR. Integer widths, masked machine shift
counts, total machine division, NZCV and the mutable postSSA register bank
are defined here independently. Only private byte-object access/lifetime
primitives are shared with the EvoIR machine.

The formal machine has no fuel bound. Calls push explicit activation frames
and retain the entire logical argument vector, including positions past the
eight AArch64 argument registers. Physical stack argument offsets belong to
allocation/frame verification, not this virtual-machine semantics.
-/
namespace EvoIR.Machine

open Core.Source (Fault)
abbrev Memory := Core.IR.Memory
abbrev Object := Core.IR.Object

inductive Value where
  | int (bits : BitVec 64)
  | pointer (object : Nat) (offset : Int)
  deriving DecidableEq, Repr

inductive Flags where
  | integer (lhs rhs : BitVec 64) (width : Width)
  | arithmetic (negative zero carry overflow : Bool)
  | identity (equal : Bool)
  deriving DecidableEq, Repr

inductive Slot where
  | value (value : Value)
  | flags (flags : Flags)
  deriving DecidableEq, Repr

/-- Lookup uses the virtual identity, not the class spelling. A latest write
shadows earlier entries, exactly as updating a map at the same identity. -/
abbrev RegisterFile := List (Nat × Slot)

def readSlot (regs : RegisterFile) (reg : VReg) : Option Slot := regs.lookup reg.id

def writeSlot (regs : RegisterFile) (reg : VReg) (value : Slot) : RegisterFile :=
  (reg.id, value) :: regs

@[simp] theorem readSlot_writeSlot (regs : RegisterFile) (reg : VReg) (value : Slot) :
    readSlot (writeSlot regs reg value) reg = some value := by
  simp [readSlot, writeSlot]

@[simp] theorem readSlot_writeSlot_other (regs : RegisterFile) (dst src : VReg) (value : Slot)
    (different : src.id ≠ dst.id) :
    readSlot (writeSlot regs dst value) src = readSlot regs src := by
  have same : (src.id == dst.id) = false := by
    cases equality : src.id == dst.id
    · rfl
    · exact False.elim (different (beq_iff_eq.mp equality))
  simp [readSlot, writeSlot, List.lookup_cons, same]

def normalize (regClass : RegClass) : Value → Value
  | .int bits => .int (if regClass == .gpr32 then (bits.setWidth 32).setWidth 64 else bits)
  | .pointer object offset => .pointer object offset

def Value.Conforms (regClass : RegClass) (value : Value) : Prop :=
  match regClass, value with
  | .gpr32, .int bits => (bits.setWidth 32).setWidth 64 = bits
  | .gpr64, _ => True
  | _, _ => False

theorem normalize_eq_of_conforms (regClass : RegClass) (value : Value)
    (typed : value.Conforms regClass) : normalize regClass value = value := by
  cases regClass <;> cases value <;> simp_all [Value.Conforms, normalize]

def Slot.Conforms (regClass : RegClass) (slot : Slot) : Prop :=
  match regClass, slot with
  | .flags, .flags _ => True
  | .flags, .value _ => False
  | _, .value v => v.Conforms regClass
  | _, .flags _ => False

def write (regs : RegisterFile) (reg : VReg) (value : Value) : RegisterFile :=
  writeSlot regs reg (.value (normalize reg.class value))

def read (regs : RegisterFile) (reg : VReg) : Except Fault Value :=
  match readSlot regs reg with
  | some (.value value) => .ok value
  | some (.flags _) => .error (.malformed "MIR flags read as value")
  | none => .error (.malformed "MIR register read before write")

def readBits (regs : RegisterFile) (reg : VReg) : Except Fault (BitVec 64) := do
  let .int bits ← read regs reg | throw (.malformed "MIR pointer read as integer")
  return bits

def readFlags (regs : RegisterFile) (reg : VReg) : Except Fault Flags :=
  match readSlot regs reg with
  | some (.flags flags) => .ok flags
  | _ => .error (.malformed "MIR value read as flags")

@[simp] theorem read_write (regs : RegisterFile) (reg : VReg) (value : Value) :
    read (write regs reg value) reg = .ok (normalize reg.class value) := by
  simp [read, write]

@[simp] theorem read_write_other (regs : RegisterFile) (dst src : VReg) (value : Value)
    (different : src.id ≠ dst.id) :
    read (write regs dst value) src = read regs src := by
  simp [read, write, different]

def normalizeAt (width : Width) (bits : BitVec 64) : BitVec 64 :=
  (bits.setWidth width.bits).setWidth 64

/-- The instruction spelling, not the operand's declared class, selects the
arithmetic width. Variable shifts mask their count to five or six low bits;
division by zero returns zero, and the overflowing signed quotient wraps. -/
def evalBinary (op : BinOp) (width : Width) (lhs rhs : BitVec 64) : BitVec 64 :=
  let x := lhs.setWidth width.bits
  let y := rhs.setWidth width.bits
  let count := y.toNat % width.bits
  let value : BitVec width.bits := match op with
    | .add => x + y
    | .sub => x - y
    | .mul => x * y
    | .and => x &&& y
    | .orr => x ||| y
    | .eor => x ^^^ y
    | .lsl => x <<< count
    | .lsr => x >>> count
    | .asr => x.sshiftRight count
    | .udiv => BitVec.ofNat width.bits (if y.toNat == 0 then 0 else x.toNat / y.toNat)
    | .sdiv => BitVec.ofInt width.bits (if y.toNat == 0 then 0 else x.toInt.tdiv y.toInt)
  value.setWidth 64

def arithmeticFlags (op : FlagOp) (width : Width) (lhs rhs : BitVec 64) :
    BitVec 64 × Flags :=
  let x := lhs.setWidth width.bits
  let y := rhs.setWidth width.bits
  let value := if op == .subs then x - y else x + y
  let exact := if op == .subs then x.toInt - y.toInt else x.toInt + y.toInt
  let carry := if op == .subs then decide (y.toNat ≤ x.toNat)
    else decide (2^width.bits ≤ x.toNat + y.toNat)
  let representable := decide (-(2^(width.bits-1) : Int) ≤ exact ∧
    exact ≤ (2^(width.bits-1) : Int)-1)
  (value.setWidth 64, .arithmetic (decide (value.toInt < 0)) (value == 0) carry (!representable))

def condition : Flags → Cond → Except Fault Bool
  | .identity equal, .eq => .ok equal
  | .identity equal, .ne => .ok (!equal)
  | .identity _, _ => .error (.malformed "MIR ordered pointer condition")
  | .integer lhs rhs width, cond =>
    let x := lhs.setWidth width.bits
    let y := rhs.setWidth width.bits
    let difference := x - y
    .ok (match cond with
      | .eq => x == y
      | .ne => x != y
      | .lt => decide (x.toInt < y.toInt)
      | .le => decide (x.toInt ≤ y.toInt)
      | .gt => decide (y.toInt < x.toInt)
      | .ge => decide (y.toInt ≤ x.toInt)
      | .lo | .cc => decide (x.toNat < y.toNat)
      | .ls => decide (x.toNat ≤ y.toNat)
      | .hi => decide (y.toNat < x.toNat)
      | .hs | .cs => decide (y.toNat ≤ x.toNat)
      | .vs => decide (x.toInt-y.toInt ≠ difference.toInt)
      | .vc => decide (x.toInt-y.toInt = difference.toInt))
  | .arithmetic negative zero carry overflow, cond =>
    .ok (match cond with
      | .eq => zero
      | .ne => !zero
      | .lt => negative != overflow
      | .ge => negative == overflow
      | .le => zero || (negative != overflow)
      | .gt => !zero && (negative == overflow)
      | .lo | .cc => !carry
      | .hs | .cs => carry
      | .ls => !carry || zero
      | .hi => carry && !zero
      | .vs => overflow
      | .vc => !overflow)

def identityEqual : Value → Value → Bool
  | .int x, .int y => x == y
  | .pointer id offset, .pointer id' offset' => id == id' && offset == offset'
  | .pointer id offset, .int bits | .int bits, .pointer id offset =>
      id == 0 && offset == 0 && bits == 0

theorem condition_integer_total (lhs rhs : BitVec 64) (width : Width) (cond : Cond) :
    ∃ value, condition (.integer lhs rhs width) cond = .ok value := ⟨_, rfl⟩

theorem condition_arithmetic_total (negative zero carry overflow : Bool) (cond : Cond) :
    ∃ value, condition (.arithmetic negative zero carry overflow) cond = .ok value := ⟨_, rfl⟩

def compare (lhs rhs : Value) (width : Width) : Flags :=
  match lhs, rhs with
  | .int x, .int y => .integer x y width
  | _, _ => .identity (identityEqual lhs rhs)

def evalBinaryValue (op : BinOp) (width : Width) (lhs rhs : Value) : Except Fault Value :=
  match op, lhs, rhs with
  | _, .int x, .int y => .ok (.int (evalBinary op width x y))
  | .add, .pointer id offset, .int delta => .ok (.pointer id (offset+delta.toInt))
  | .sub, .pointer id offset, .int delta => .ok (.pointer id (offset-delta.toInt))
  | .add, .int delta, .pointer id offset => .ok (.pointer id (offset+delta.toInt))
  | _, _, _ => .error (.malformed "MIR noninteger arithmetic operands")

def evalWideMultiply (op : WideMul) (lhs rhs : BitVec 64) : BitVec 64 :=
  match op with
  | .smull => BitVec.ofInt 64 ((lhs.setWidth 32).toInt * (rhs.setWidth 32).toInt)
  | .smulh => BitVec.ofInt 64 ((lhs.toInt * rhs.toInt) >>> 64)
  | .umulh => BitVec.ofNat 64 ((lhs.toNat * rhs.toNat) / 2^64)

def evalExtend (op : ExtOp) (value : BitVec 64) : BitVec 64 :=
  let bits := value.setWidth op.bits
  if op.signed then bits.signExtend 64 else bits.setWidth 64

def evalBfx (signed : Bool) (width : Width) (value : BitVec 64) (lsb bits : Nat) : BitVec 64 :=
  let shifted := (normalizeAt width value) >>> lsb
  let low := shifted.setWidth bits
  normalizeAt width (if signed then low.signExtend 64 else low.setWidth 64)

def asPointer : Value → Except Fault (Nat × Int)
  | .pointer id offset => .ok (id, offset)
  | .int bits => .error (.diagnostic (if bits == 0 then 3009 else 3013))

def accessObject (memory : Memory) (address : Value) (size align : Nat) :
    Except Fault (Object × Nat) := do
  let (id, offset) ← asPointer address
  Core.IR.accessObject memory (.pointer id offset) size align

def offsetPointer (address : Value) (delta : Int) : Except Fault Value := do
  let (id, offset) ← asPointer address
  return .pointer id (offset+delta)

def loadPrivate (memory : Memory) (address : Value) (width align : Nat) :
    Except Fault (BitVec 64) := do
  let (object, offset) ← accessObject memory address (width/8) align
  let mut bits := 0
  for i in List.range (width/8) do
    let some (some byte) := object.bytes[offset+i]? | throw (.diagnostic 3001)
    bits := bits + byte.toNat * 2^(8*i)
  return BitVec.ofNat 64 bits

def storePrivate (memory : Memory) (address : Value) (width align : Nat) (bits : BitVec 64) :
    Except Fault Memory := do
  let (object, offset) ← accessObject memory address (width/8) align
  let bytes := (List.range (width/8)).map fun i => some (BitVec.ofNat 8 (bits.toNat / 2^(8*i)))
  let object := {object with bytes := (object.bytes.take offset ++ bytes ++
    object.bytes.drop (offset+width/8))}
  return memory.replace object

abbrev OperandReader := VReg → Except Fault Slot

def readOperand (regs : RegisterFile) (reg : VReg) : Except Fault Slot :=
  match readSlot regs reg with
  | some slot => .ok slot
  | none => .error (.malformed "MIR register read before write")

def operandValue (get : OperandReader) (reg : VReg) : Except Fault Value := do
  match ← get reg with
  | .value value => return value
  | .flags _ => throw (.malformed "MIR flags read as value")

def operandBits (get : OperandReader) (reg : VReg) : Except Fault (BitVec 64) := do
  let .int bits ← operandValue get reg | throw (.malformed "MIR pointer read as integer")
  return bits

def operandFlags (get : OperandReader) (reg : VReg) : Except Fault Flags := do
  match ← get reg with
  | .flags flags => return flags
  | .value _ => throw (.malformed "MIR value read as flags")

abbrev Writes := List (VReg × Slot)

def valueWrite (dst : VReg) (value : Value) : Writes :=
  [(dst, .value (normalize dst.class value))]

/-- Explicit dispatch on every admitted MInst form. The operand reader is an
implementation factoring, not an instruction semantics supplied by a producer.
It permits independent value-transfer proofs without pretending that equal
uses/defs tables establish arithmetic or memory agreement. -/
def evalInstructionOperands (owner : Nat) (inst : Instruction) (get : OperandReader)
    (memory : Memory) : Except Fault (Writes × Memory) := do
  match inst with
  | .mov dst src => return (valueWrite dst (← operandValue get src), memory)
  | .movImm dst imm => return (valueWrite dst (.int (BitVec.ofNat 64 imm)), memory)
  | .binary op dst lhs rhs width =>
    let left ← operandValue get lhs
    let right ← operandValue get rhs
    return (valueWrite dst (← evalBinaryValue op width left right), memory)
  | .binaryImm op dst lhs imm width =>
    let left ← operandValue get lhs
    return (valueWrite dst (← evalBinaryValue op width left (.int (BitVec.ofNat 64 imm))), memory)
  | .flagBinary op dst flags lhs rhs width =>
    let left ← operandBits get lhs
    let right ← operandBits get rhs
    let (bits, set) := arithmeticFlags op width left right
    return (valueWrite dst (.int bits) ++ [(flags, .flags set)], memory)
  | .wideMultiply op dst lhs rhs =>
    let left ← operandBits get lhs
    let right ← operandBits get rhs
    return (valueWrite dst (.int (evalWideMultiply op left right)), memory)
  | .msub dst n m a width =>
    let x ← operandBits get n
    let y ← operandBits get m
    let z ← operandBits get a
    return (valueWrite dst (.int (evalBinary .sub width z (evalBinary .mul width x y))), memory)
  | .madd dst n m a width =>
    let x ← operandBits get n
    let y ← operandBits get m
    let z ← operandBits get a
    return (valueWrite dst (.int (evalBinary .add width z (evalBinary .mul width x y))), memory)
  | .extend op dst src =>
    return (valueWrite dst (.int (evalExtend op (← operandBits get src))), memory)
  | .bfx signed dst src lsb bits width =>
    return (valueWrite dst (.int (evalBfx signed width (← operandBits get src) lsb bits)), memory)
  | .cmp dst lhs rhs width =>
    let left ← operandValue get lhs
    let right ← operandValue get rhs
    return ([(dst, .flags (compare left right width))], memory)
  | .cmpImm dst lhs imm width =>
    let left ← operandValue get lhs
    return ([(dst, .flags (compare left (.int (BitVec.ofNat 64 imm)) width))], memory)
  | .tstImm dst lhs imm width =>
    let bits := (← operandBits get lhs).setWidth width.bits &&& BitVec.ofNat width.bits imm
    return ([(dst, .flags (.arithmetic (decide (bits.toInt < 0)) (bits == 0) false false))], memory)
  | .cset dst flags cond =>
    let yes ← condition (← operandFlags get flags) cond
    return (valueWrite dst (.int (BitVec.ofNat 64 (if yes then 1 else 0))), memory)
  | .csel dst flags lhs rhs cond width =>
    let yes ← condition (← operandFlags get flags) cond
    let value ← operandValue get (if yes then lhs else rhs)
    let value := match value with | .int bits => Value.int (normalizeAt width bits) | other => other
    return (valueWrite dst value, memory)
  | .load align dst base offset kind volatile =>
    if volatile then throw (.malformed "MIR volatile access outside core-v1")
    let address ← offsetPointer (← operandValue get base) offset
    let bits ← loadPrivate memory address kind.bits (if align == 0 then kind.bits/8 else align)
    return (valueWrite dst (.int bits), memory)
  | .store align src base offset kind volatile =>
    if volatile then throw (.malformed "MIR volatile access outside core-v1")
    let address ← offsetPointer (← operandValue get base) offset
    let bits ← operandBits get src
    let memory ← storePrivate memory address kind.bits (if align == 0 then kind.bits/8 else align) bits
    return ([], memory)
  | .frameSlot dst origin size align =>
    if size == 0 || align == 0 || memory.nextId == 0 then
      throw (.malformed "MIR allocation layout")
    let id := memory.nextId
    if memory.objects.any (·.id == id) then throw (.malformed "MIR reused object identity")
    let object : Object := ⟨id, origin, owner, align, true, List.replicate size none⟩
    return (valueWrite dst (.pointer id 0), ⟨id+1, object :: memory.objects⟩)
  | .stackRelease address =>
    let (id, offset) ← asPointer (← operandValue get address)
    let object ← Core.IR.liveObject memory id
    if offset != 0 || object.owner != owner then throw (.diagnostic 3018)
    return ([], memory.replace {object with live := false})
  | .bl _ _ _ => throw (.malformed "MIR call requires activation transition")

def applyWrites (regs : RegisterFile) (writes : Writes) : RegisterFile :=
  writes.foldl (fun regs (reg, slot) => writeSlot regs reg slot) regs

theorem applyWrites_readSlot_other (regs : RegisterFile) (writes : Writes) (reg : VReg)
    (fresh : ∀ entry ∈ writes, reg.id ≠ entry.1.id) :
    readSlot (applyWrites regs writes) reg = readSlot regs reg := by
  induction writes generalizing regs with
  | nil => rfl
  | cons entry rest ih =>
    change readSlot (applyWrites (writeSlot regs entry.1 entry.2) rest) reg = readSlot regs reg
    rw [ih (writeSlot regs entry.1 entry.2)
      (by intro next member; exact fresh next (List.mem_cons_of_mem _ member))]
    exact readSlot_writeSlot_other regs entry.1 reg entry.2 (fresh entry (by simp))

theorem applyWrites_lookup_congr (source target : RegisterFile) (writes : Writes) (reg : VReg)
    (same : readSlot source reg = readSlot target reg) :
    readSlot (applyWrites source writes) reg = readSlot (applyWrites target writes) reg := by
  induction writes generalizing source target with
  | nil => exact same
  | cons entry rest ih =>
    apply ih
    by_cases aliases : reg.id = entry.1.id
    · simp only [readSlot, writeSlot, List.lookup_cons, aliases, beq_self_eq_true]
    · simp only [readSlot_writeSlot_other _ _ _ _ aliases]
      exact same

def evalInstruction (owner : Nat) (inst : Instruction) (regs : RegisterFile)
    (memory : Memory) : Except Fault (RegisterFile × Memory) := do
  let (writes, memory) ← evalInstructionOperands owner inst (readOperand regs) memory
  return (applyWrites regs writes, memory)

@[simp] theorem operandValue_readOperand (regs : RegisterFile) (reg : VReg) :
    operandValue (readOperand regs) reg = read regs reg := by
  simp only [operandValue, readOperand, read]
  cases readSlot regs reg with
  | none => rfl
  | some slot => cases slot <;> rfl

@[simp] theorem operandBits_readOperand (regs : RegisterFile) (reg : VReg) :
    operandBits (readOperand regs) reg = readBits regs reg := by
  simp [operandBits, readBits]

/-- All arithmetic instructions used by baseline DCE, as an exact syntax
predicate. Memory operations, calls and lifetime transitions are excluded. -/
def Instruction.pure : Instruction → Bool
  | .mov _ _ | .movImm _ _ | .binary _ _ _ _ _ | .binaryImm _ _ _ _ _
  | .flagBinary _ _ _ _ _ _ | .wideMultiply _ _ _ _ | .msub _ _ _ _ _
  | .madd _ _ _ _ _ | .extend _ _ _ | .bfx _ _ _ _ _ _
  | .cmp _ _ _ _ | .cmpImm _ _ _ _ | .tstImm _ _ _ _
  | .cset _ _ _ | .csel _ _ _ _ _ _ => true
  | _ => false

/-- A private pointer can be moved/compared/selected, but not assumed to be
integer arithmetic merely because a producer's uses/defs table says so. This
strong initialized-integer invariant is sufficient for every pure form. -/
def IntegerOperands (inst : Instruction) (get : OperandReader) : Prop :=
  ∀ reg, reg ∈ inst.uses →
    if reg.class = .flags then ∃ flags, get reg = .ok (.flags flags) ∧
      ∀ cond, ∃ value, condition flags cond = .ok value
    else ∃ bits, get reg = .ok (.value (.int bits))

theorem IntegerOperands.bits {inst : Instruction} {get : OperandReader}
    (operands : IntegerOperands inst get) (reg : VReg) (used : reg ∈ inst.uses)
    (integer : reg.class ≠ .flags) : ∃ bits, get reg = .ok (.value (.int bits)) := by
  simpa only [IntegerOperands, integer, if_false] using operands reg used

theorem IntegerOperands.flags {inst : Instruction} {get : OperandReader}
    (operands : IntegerOperands inst get) (reg : VReg) (used : reg ∈ inst.uses)
    (flags : reg.class = .flags) : ∃ value, get reg = .ok (.flags value) ∧
      ∀ cond, ∃ yes, condition value cond = .ok yes := by
  simpa only [IntegerOperands, flags, if_true] using operands reg used

def InstClassWellFormed : Instruction → Prop
  | .mov _ src | .extend _ _ src | .bfx _ _ src _ _ _ => src.class ≠ .flags
  | .movImm _ _ | .frameSlot _ _ _ _ => True
  | .binary _ _ lhs rhs _ | .flagBinary _ _ _ lhs rhs _
  | .wideMultiply _ _ lhs rhs | .cmp _ lhs rhs _ => lhs.class ≠ .flags ∧ rhs.class ≠ .flags
  | .binaryImm _ _ lhs _ _ | .cmpImm _ lhs _ _ | .tstImm _ lhs _ _ => lhs.class ≠ .flags
  | .msub _ n m a _ | .madd _ n m a _ =>
      n.class ≠ .flags ∧ m.class ≠ .flags ∧ a.class ≠ .flags
  | .cset _ flags _ => flags.class = .flags
  | .csel _ flags lhs rhs _ _ =>
      flags.class = .flags ∧ lhs.class ≠ .flags ∧ rhs.class ≠ .flags
  | .load _ _ base _ _ _ => base.class = .gpr64
  | .store _ src base _ _ _ => src.class ≠ .flags ∧ base.class = .gpr64
  | .stackRelease address => address.class = .gpr64
  | .bl _ _ _ => True

theorem evalInstructionOperands_mapUses (owner : Nat) (inst : Instruction)
    (f : VReg → VReg) (source target : OperandReader) (memory : Memory)
    (agree : ∀ reg, reg ∈ inst.uses → target (f reg) = source reg) :
    evalInstructionOperands owner (inst.mapUses f) target memory =
      evalInstructionOperands owner inst source memory := by
  cases inst <;>
    simp_all [Instruction.uses, Instruction.mapUses, evalInstructionOperands,
      operandValue, operandBits, operandFlags]
  case csel dst flags lhs rhs cond width =>
    have selected : ∀ yes : Bool, target (if yes then f lhs else f rhs) =
        source (if yes then lhs else rhs) := by
      intro yes
      cases yes <;> simp_all
    simp only [selected]

/-- The definitions table is proved against actual instruction execution.
This includes flag writes, selected operands and private-memory instructions;
it is not a producer-supplied footprint axiom. A call is stepped separately. -/
theorem evalInstructionOperands_resultRegs (owner : Nat) (inst : Instruction)
    (get : OperandReader) (memory : Memory) (writes : Writes) (nextMemory : Memory)
    (evaluated : evalInstructionOperands owner inst get memory = .ok (writes, nextMemory)) :
    writes.map Prod.fst = inst.defs := by
  cases inst <;>
    simp only [evalInstructionOperands, bind, Except.bind, Pure.pure, Except.pure] at evaluated
  all_goals repeat' split at evaluated
  all_goals simp_all [Instruction.defs, valueWrite]
  all_goals rcases evaluated with ⟨writesEq, _⟩
  all_goals rw [← writesEq]
  all_goals simp

/-- This is the progress fact DCE actually needs: a pure opcode together with
checked initialized operand kinds yields a successful evaluation and exactly
the same private memory. The typed-operand premise is not inferred from a
producer's instruction tables or from a successful bounded test. -/
theorem evalInstructionOperands_pure_total (owner : Nat) (inst : Instruction)
    (get : OperandReader) (memory : Memory) (pure : inst.pure = true)
    (typed : InstClassWellFormed inst) (operands : IntegerOperands inst get) :
    ∃ writes, evalInstructionOperands owner inst get memory = .ok (writes, memory) := by
  cases inst with
  | mov dst src =>
    obtain ⟨x, hx⟩ := operands.bits src (by simp [Instruction.uses]) typed
    exact ⟨_, by simp [evalInstructionOperands, operandValue, hx]; rfl⟩
  | movImm dst imm => exact ⟨_, rfl⟩
  | binary op dst lhs rhs width =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed.1
    obtain ⟨y, hy⟩ := operands.bits rhs (by simp [Instruction.uses]) typed.2
    exact ⟨_, by simp [evalInstructionOperands, operandValue, hx, hy, evalBinaryValue]; rfl⟩
  | binaryImm op dst lhs imm width =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed
    exact ⟨_, by simp [evalInstructionOperands, operandValue, hx, evalBinaryValue]; rfl⟩
  | flagBinary op dst flags lhs rhs width =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed.1
    obtain ⟨y, hy⟩ := operands.bits rhs (by simp [Instruction.uses]) typed.2
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx, hy]; rfl⟩
  | wideMultiply op dst lhs rhs =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed.1
    obtain ⟨y, hy⟩ := operands.bits rhs (by simp [Instruction.uses]) typed.2
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx, hy]; rfl⟩
  | msub dst n m a width =>
    obtain ⟨x, hx⟩ := operands.bits n (by simp [Instruction.uses]) typed.1
    obtain ⟨y, hy⟩ := operands.bits m (by simp [Instruction.uses]) typed.2.1
    obtain ⟨z, hz⟩ := operands.bits a (by simp [Instruction.uses]) typed.2.2
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx, hy, hz]; rfl⟩
  | madd dst n m a width =>
    obtain ⟨x, hx⟩ := operands.bits n (by simp [Instruction.uses]) typed.1
    obtain ⟨y, hy⟩ := operands.bits m (by simp [Instruction.uses]) typed.2.1
    obtain ⟨z, hz⟩ := operands.bits a (by simp [Instruction.uses]) typed.2.2
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx, hy, hz]; rfl⟩
  | extend op dst src =>
    obtain ⟨x, hx⟩ := operands.bits src (by simp [Instruction.uses]) typed
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx]; rfl⟩
  | bfx signed dst src lsb bits width =>
    obtain ⟨x, hx⟩ := operands.bits src (by simp [Instruction.uses]) typed
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx]; rfl⟩
  | cmp dst lhs rhs width =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed.1
    obtain ⟨y, hy⟩ := operands.bits rhs (by simp [Instruction.uses]) typed.2
    exact ⟨_, by simp [evalInstructionOperands, operandValue, hx, hy]; rfl⟩
  | cmpImm dst lhs imm width =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed
    exact ⟨_, by simp [evalInstructionOperands, operandValue, hx]; rfl⟩
  | tstImm dst lhs imm width =>
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed
    exact ⟨_, by simp [evalInstructionOperands, operandBits, operandValue, hx]; rfl⟩
  | cset dst flags cond =>
    obtain ⟨value, hv, total⟩ := operands.flags flags (by simp [Instruction.uses]) typed
    obtain ⟨yes, hy⟩ := total cond
    refine ⟨valueWrite dst (.int (BitVec.ofNat 64 (if yes then 1 else 0))), ?_⟩
    simp only [evalInstructionOperands, operandFlags, hv]
    change (fun yes => (valueWrite dst (.int (BitVec.ofNat 64 (if yes then 1 else 0))), memory)) <$>
      condition value cond = .ok (valueWrite dst (.int (BitVec.ofNat 64 (if yes then 1 else 0))), memory)
    rw [hy]
    rfl
  | csel dst flags lhs rhs cond width =>
    obtain ⟨value, hv, total⟩ := operands.flags flags (by simp [Instruction.uses]) typed.1
    obtain ⟨x, hx⟩ := operands.bits lhs (by simp [Instruction.uses]) typed.2.1
    obtain ⟨y, hr⟩ := operands.bits rhs (by simp [Instruction.uses]) typed.2.2
    obtain ⟨yes, hcond⟩ := total cond
    refine ⟨valueWrite dst (.int (normalizeAt width (if yes then x else y))), ?_⟩
    simp only [evalInstructionOperands, operandFlags, operandValue, hv,
      bind, Except.bind, Pure.pure, Except.pure]
    rw [hcond]
    cases yes <;> simp only [Bool.false_eq_true, ↓reduceIte, hr, hx] <;> rfl
  | load | store | frameSlot | stackRelease | bl => simp [Instruction.pure] at pure

theorem evalInstruction_pure_total (owner : Nat) (inst : Instruction)
    (regs : RegisterFile) (memory : Memory) (pure : inst.pure = true)
    (typed : InstClassWellFormed inst) (operands : IntegerOperands inst (readOperand regs)) :
    ∃ next, evalInstruction owner inst regs memory = .ok (next, memory) := by
  obtain ⟨writes, evaluated⟩ := evalInstructionOperands_pure_total owner inst
    (readOperand regs) memory pure typed operands
  exact ⟨applyWrites regs writes, by simp [evalInstruction, evaluated]; rfl⟩

/-- The shared demand-preservation fact for baseline DCE and zero-substituted
unused call arguments. It proves only the local deletion obligation; an
enclosing CFG/call checker must establish this initialized demand invariant
at entry and on every reachable edge, including loop backedges. -/
theorem evalInstruction_drop_unobserved (owner : Nat) (inst : Instruction)
    (regs : RegisterFile) (memory : Memory) (live : List VReg)
    (pure : inst.pure = true) (typed : InstClassWellFormed inst)
    (operands : IntegerOperands inst (readOperand regs))
    (unobserved : ∀ reg ∈ live, ∀ dst ∈ inst.defs, reg.id ≠ dst.id) :
    ∃ next, evalInstruction owner inst regs memory = .ok (next, memory) ∧
      ∀ reg ∈ live, readSlot next reg = readSlot regs reg := by
  obtain ⟨writes, evaluated⟩ := evalInstructionOperands_pure_total owner inst
    (readOperand regs) memory pure typed operands
  have destinations := evalInstructionOperands_resultRegs owner inst
    (readOperand regs) memory writes memory evaluated
  refine ⟨applyWrites regs writes, ?_, ?_⟩
  · simp [evalInstruction, evaluated]
    rfl
  · intro reg liveReg
    apply applyWrites_readSlot_other
    intro entry member
    apply unobserved reg liveReg entry.1
    rw [← destinations]
    exact List.mem_map.mpr ⟨entry, member, rfl⟩

structure Activation where
  function : String
  block : Nat
  pc : Nat
  regs : RegisterFile
  owner : Nat
  incoming : List Value
  deriving DecidableEq, Repr

structure ReturnFrame where
  caller : Activation
  results : List VReg
  deriving DecidableEq, Repr

inductive Control where
  | running (activation : Activation)
  | halted (result : Except Fault (List Value))
  deriving DecidableEq, Repr

structure State where
  control : Control
  stack : List ReturnFrame
  nextFrame : Nat
  memory : Memory
  deriving DecidableEq, Repr

def bind (regs : RegisterFile) (parameters : List VReg) (args : List Value) :
    Except Fault RegisterFile := do
  if parameters.length != args.length then throw (.malformed "MIR binding arity")
  if (parameters.map VReg.id).eraseDups.length != parameters.length then
    throw (.malformed "MIR duplicate binding")
  return (parameters.zip args).foldl (fun regs (reg, value) => write regs reg value) regs

def enter (f : Function) (args : List Value) (owner : Nat) : Except Fault Activation := do
  let some block := f.blocks.find? (·.id == 0) | throw (.malformed "MIR function has no entry block 0")
  return ⟨f.name, block.id, 0, ← bind [] block.args args, owner, args⟩

def jump (mode : Mode) (f : Function) (a : Activation) (target : Nat) (args : List VReg) :
    Except Fault Activation := do
  let some block := f.blocks.find? (·.id == target) | throw (.malformed "MIR missing successor")
  match mode with
  | .ssa =>
    -- Every source is captured before any destination write, including cycles.
    let values ← args.mapM (read a.regs)
    let regs ← bind [] block.args values
    return {a with block := target, pc := 0, regs}
  | .postSSA =>
    -- Block zero retains its call signature, not an internal edge binding.
    -- Actual out-of-SSA copies have already updated the persistent bank.
    if !args.isEmpty then throw (.malformed "MIR post-SSA edge arguments")
    return {a with block := target, pc := 0}

def execute (mode : Mode) (p : Program) (s : State) (a : Activation) : Except Fault State := do
  let some f := p.functions.find? (·.name == a.function) | throw (.malformed "MIR missing function")
  let some block := f.blocks.find? (·.id == a.block) | throw (.malformed "MIR missing block")
  if let some inst := block.insts[a.pc]? then
    if let .bl symbol args results := inst then
      let values ← args.mapM (read a.regs)
      let some callee := p.functions.find? (·.name == symbol) | throw (.malformed "MIR external call")
      let entered ← enter callee values s.nextFrame
      return {s with control := .running entered, stack := ⟨{a with pc := a.pc+1}, results⟩ :: s.stack, nextFrame := s.nextFrame+1}
    else
      let (regs, memory) ← evalInstruction a.owner inst a.regs s.memory
      return {s with control := .running {a with pc := a.pc+1, regs}, memory}
  if a.pc != block.insts.length then throw (.malformed "MIR invalid program counter")
  match block.term with
  | .b target args => return {s with control := .running (← jump mode f a target args)}
  | .bCond flags cond taken takenArgs untaken untakenArgs =>
    let yes ← condition (← readFlags a.regs flags) cond
    return {s with control := .running (← jump mode f a
      (if yes then taken else untaken) (if yes then takenArgs else untakenArgs))}
  | .cb nonzero src taken takenArgs untaken untakenArgs =>
    let some width := src.class.width | throw (.malformed "MIR flags tested as bits")
    let raw := (← readBits a.regs src).setWidth width.bits
    let yes := (raw != 0) == nonzero
    return {s with control := .running (← jump mode f a
      (if yes then taken else untaken) (if yes then takenArgs else untakenArgs))}
  | .tb nonzero src bit taken takenArgs untaken untakenArgs =>
    if bit ≥ 64 then throw (.malformed "MIR test bit outside register")
    let yes := (← readBits a.regs src).getLsbD bit == nonzero
    return {s with control := .running (← jump mode f a
      (if yes then taken else untaken) (if yes then takenArgs else untakenArgs))}
  | .brk imm => throw (.diagnostic imm)
  | .ret ids =>
    let values ← ids.mapM (read a.regs)
    let memory := s.memory.endFrame a.owner
    match s.stack with
    | [] => return {s with control := .halted (.ok values), memory}
    | frame :: rest =>
      let regs ← bind frame.caller.regs frame.results values
      return {s with control := .running {frame.caller with regs}, stack := rest, memory}

def step (mode : Mode) (p : Program) (s : State) : Option State :=
  match s.control with
  | .halted _ => none
  | .running a => some (match execute mode p s a with
    | .ok next => next
    | .error fault => {s with control := .halted (.error fault)})

def initial (p : Program) (entry : String) (args : List Value)
    (memory : Memory := ⟨1, []⟩) : Except Fault State := do
  let some f := p.functions.find? (·.name == entry) | throw (.malformed "MIR unknown entry")
  return ⟨.running (← enter f args 1), [], 2, memory⟩

def observe (type : Core.Source.IntType) (s : State) : Option Core.Observation :=
  match s.control with
  | .halted (.ok [.int bits]) =>
    if type.width == 32 || type.width == 64 then
      some (.returned type (bits.setWidth type.width).toNat) else none
  | .halted (.error (.diagnostic code)) => Core.checkedDiagnostic code
  | _ => none

inductive Step (mode : Mode) (p : Program) : State → State → Prop where
  | execute {s t} : step mode p s = some t → Step mode p s t

inductive Steps (mode : Mode) (p : Program) : State → State → Prop where
  | refl (s) : Steps mode p s s
  | next {s t u} : Step mode p s t → Steps mode p t u → Steps mode p s u

def Diverges (mode : Mode) (p : Program) (s : State) : Prop :=
  Core.Transition.Infinite (step mode p) s

def runSteps (mode : Mode) (p : Program) : Nat → State → State
  | 0, s => s
  | n+1, s => match step mode p s with | none => s | some next => runSteps mode p n next

theorem step_deterministic (mode : Mode) (p : Program) (s t u : State)
    (ht : Step mode p s t) (hu : Step mode p s u) : t = u := by
  cases ht with | execute ht =>
    cases hu with | execute hu => exact Option.some.inj (ht.symm.trans hu)

theorem Steps.toReaches {mode : Mode} {p : Program} {s t : State}
    (path : Steps mode p s t) : Core.Transition.Reaches (step mode p) s t := by
  induction path with
  | refl => exact .refl _
  | next edge _ ih => cases edge with | execute edge => exact .next edge ih

theorem runSteps_prefix (mode : Mode) (p : Program) (n : Nat) (s : State) :
    Steps mode p s (runSteps mode p n s) := by
  induction n generalizing s with
  | zero => exact .refl s
  | succ n ih =>
    simp only [runSteps]
    split
    next => exact .refl s
    next next h => exact .next (.execute h) (ih next)

theorem running_progress (mode : Mode) (p : Program) (s : State) (a : Activation)
    (running : s.control = .running a) : ∃ next, step mode p s = some next := by
  simp [step, running]

end EvoIR.Machine
