import EvoIR.Alloc.Syntax
import EvoIR.MachineExecution

/-!
Small-step semantics of allocated EvoMIR.

Each activation has its own storage: general registers, the flags, spill
slots, incoming stack argument slots and outgoing stack argument slots. A
place that has no cell holds an unspecified value. A read of an unspecified
value is a malformed execution, not a program observation. Thus a refinement
proof shows that an accepted program never reads such a value, and any
concrete value at that place gives the same behavior.

A general register holds 64 bits. A read through the 32-bit view gives the
low 32 bits. A write through the 32-bit view sets the high 32 bits to zero.
A stack slot write through the 32-bit view defines only the low 32 bits of
the slot. A 64-bit read of such a slot is malformed.

The operation of each original instruction form is the operation of
`Machine.evalInstructionOperands`. The operands are read and written through
their locations. Inserted moves, spill stores, spill reloads and stack
argument accesses are register-to-location copies of the same evaluator.

At a call, the callee receives the cells of the argument registers and the
outgoing stack argument area of the caller as its incoming area. All other
callee places are unspecified. At a return, the caller receives the cells of
the result registers. The other call-clobbered places of the caller
(`x0`-`x17`, the flags and the outgoing area) are unspecified. All other
places of the caller, which include the callee-saved registers and the spill
slots, keep their values. Work package V04 must show that the frame code
implements this preservation.

Private memory objects, observations and diagnostics are those of
`MachineExecution`.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Alloc

open EvoIR.Machine (Width RegClass Value Flags Slot Memory Object normalize condition)
open Core.Source (Fault)

/-- The content of one place. `defined` is the width of the defined low part
of a value. A register write always defines all 64 bits. -/
structure Cell where
  slot : Slot
  defined : Width
  deriving DecidableEq, Repr

/-- The storage of one activation. `none` is an unspecified value. -/
abbrev Storage := Place → Option Cell

def unspecified : Storage := fun _ => none

/-- Read a location. Flags are returned unchanged. A value is read through
the view of the location. -/
def readLoc (storage : Storage) (loc : Loc) : Except Fault Slot :=
  match storage loc.place with
  | none => .error (.malformed "allocated read of an unspecified location")
  | some ⟨.flags nzcv, _⟩ => .ok (.flags nzcv)
  | some ⟨.value value, defined⟩ =>
    match loc with
    | .flags => .ok (.value value)
    | _ =>
      if loc.width = .x ∧ defined = .w then
        .error (.malformed "allocated read above the defined width")
      else .ok (.value (normalize loc.width.class value))

/-- The defined width after a value write. A register write zero-extends.
A stack slot write defines only the bytes of its view. -/
def Loc.written : Loc → Width
  | .gpr _ _ | .flags => .x
  | .spill _ view | .incoming _ view | .outgoing _ view => view

def Loc.cell (loc : Loc) : Slot → Cell
  | .flags nzcv => ⟨.flags nzcv, .x⟩
  | .value value =>
    match loc with
    | .flags => ⟨.value value, .x⟩
    | _ => ⟨.value (normalize loc.width.class value), loc.written⟩

def writeLoc (storage : Storage) (loc : Loc) (slot : Slot) : Storage :=
  fun place => if place = loc.place then some (loc.cell slot) else storage place

def readValue (storage : Storage) (loc : Loc) : Except Fault Value := do
  match ← readLoc storage loc with
  | .value value => return value
  | .flags _ => throw (.malformed "allocated flags read as value")

def readBits (storage : Storage) (loc : Loc) : Except Fault (BitVec 64) := do
  let .int bits ← readValue storage loc | throw (.malformed "allocated pointer read as integer")
  return bits

def readFlags (storage : Storage) (loc : Loc) : Except Fault Flags := do
  match ← readLoc storage loc with
  | .flags flags => return flags
  | .value _ => throw (.malformed "allocated value read as flags")

/-! The operation of an instruction is the operation of the virtual machine
instruction whose virtual registers name the locations. The code below is a
bijection between locations and register identities. -/

def Loc.code : Loc → Nat
  | .gpr number .w => 10 * number
  | .gpr number .x => 10 * number + 1
  | .flags => 2
  | .spill slot .w => 10 * slot + 3
  | .spill slot .x => 10 * slot + 4
  | .incoming slot .w => 10 * slot + 5
  | .incoming slot .x => 10 * slot + 6
  | .outgoing slot .w => 10 * slot + 7
  | .outgoing slot .x => 10 * slot + 8

def Loc.decode (code : Nat) : Option Loc :=
  match code % 10 with
  | 0 => some (.gpr (code / 10) .w)
  | 1 => some (.gpr (code / 10) .x)
  | 2 => some .flags
  | 3 => some (.spill (code / 10) .w)
  | 4 => some (.spill (code / 10) .x)
  | 5 => some (.incoming (code / 10) .w)
  | 6 => some (.incoming (code / 10) .x)
  | 7 => some (.outgoing (code / 10) .w)
  | 8 => some (.outgoing (code / 10) .x)
  | _ => none

/-- The virtual register that names a location. Its class is the class of
the view, so a value write through a 32-bit view zero-extends. -/
def Loc.reg (loc : Loc) : Machine.VReg := ⟨loc.code, loc.class⟩

/-- The virtual machine instruction with the same operation. Inserted
accesses are moves between a location and a stack slot location. -/
def Instruction.toMachine (f : Loc → Machine.VReg) : Instruction → Machine.Instruction
  | .mov dst src => .mov (f dst) (f src)
  | .movImm dst imm => .movImm (f dst) imm
  | .binary op dst lhs rhs width => .binary op (f dst) (f lhs) (f rhs) width
  | .binaryImm op dst lhs imm width => .binaryImm op (f dst) (f lhs) imm width
  | .flagBinary op dst flags lhs rhs width => .flagBinary op (f dst) (f flags) (f lhs) (f rhs) width
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
  | .spillStore slot src => .mov (f (.spill slot src.width)) (f src)
  | .spillReload dst slot => .mov (f dst) (f (.spill slot dst.width))
  | .incomingArg dst slot => .mov (f dst) (f (.incoming slot dst.width))
  | .outgoingArg slot src => .mov (f (.outgoing slot src.width)) (f src)

def operandReader (storage : Storage) : Machine.OperandReader := fun reg =>
  match Loc.decode reg.id with
  | some loc => readLoc storage loc
  | none => .error (.malformed "allocated operand code")

def applyWrites (storage : Storage) (writes : Machine.Writes) : Storage :=
  writes.foldl (fun storage (reg, slot) =>
    match Loc.decode reg.id with
    | some loc => writeLoc storage loc slot
    | none => storage) storage

def evalInstruction (owner : Nat) (inst : Instruction) (storage : Storage)
    (memory : Memory) : Except Fault (Storage × Memory) := do
  let (writes, memory) ←
    Machine.evalInstructionOperands owner (inst.toMachine Loc.reg) (operandReader storage) memory
  return (applyWrites storage writes, memory)

structure Activation where
  function : String
  block : Nat
  pc : Nat
  storage : Storage
  owner : Nat

structure ReturnFrame where
  /-- The caller, with its program counter after the call. -/
  caller : Activation
  results : List Loc

inductive Control where
  | running (activation : Activation)
  | halted (result : Except Fault (List Value))

structure State where
  control : Control
  stack : List ReturnFrame
  nextFrame : Nat
  memory : Memory

/-- The callee storage at a call with `count` argument registers. -/
def callStorage (caller : Storage) (count : Nat) : Storage := fun place =>
  match place with
  | .gpr number => if number < count then caller place else none
  | .incoming slot => caller (.outgoing slot)
  | _ => none

/-- The caller storage after a return with `count` result registers. -/
def returnStorage (caller callee : Storage) (count : Nat) : Storage := fun place =>
  match place with
  | .gpr number =>
    if number < count then callee place
    else if place.callClobbered then none else caller place
  | _ => if place.callClobbered then none else caller place

/-- The argument registers and the incoming stack argument slots at program
entry, in the order of `abi-linux`. -/
def entryStorage (args : List Value) : Storage := fun place =>
  match place with
  | .gpr number =>
    if number < abiRegisterCount then (args[number]?).map (fun value => ⟨.value value, .x⟩) else none
  | .incoming slot => (args[abiRegisterCount + slot]?).map (fun value => ⟨.value value, .x⟩)
  | _ => none

def enter (f : Function) (storage : Storage) (owner : Nat) : Except Fault Activation := do
  let some block := f.blocks.find? (·.id == 0)
    | throw (.malformed "allocated function has no entry block 0")
  return ⟨f.name, block.id, 0, storage, owner⟩

def jump (f : Function) (a : Activation) (target : Nat) : Except Fault Activation := do
  let some _ := f.blocks.find? (·.id == target) | throw (.malformed "allocated missing successor")
  return {a with block := target, pc := 0}

def execute (p : Program) (s : State) (a : Activation) : Except Fault State := do
  let some f := p.functions.find? (·.name == a.function)
    | throw (.malformed "allocated missing function")
  let some block := f.blocks.find? (·.id == a.block) | throw (.malformed "allocated missing block")
  if let some inst := block.insts[a.pc]? then
    if let .bl symbol args results := inst then
      if !abiRegisters args then throw (.malformed "allocated call arguments outside the ABI")
      let some callee := p.functions.find? (·.name == symbol)
        | throw (.malformed "allocated external call")
      let entered ← enter callee (callStorage a.storage args.length) s.nextFrame
      let frame : ReturnFrame := ⟨{a with pc := a.pc+1}, results⟩
      return {s with control := .running entered, stack := frame :: s.stack, nextFrame := s.nextFrame+1}
    else
      let (storage, memory) ← evalInstruction a.owner inst a.storage s.memory
      return {s with control := .running {a with pc := a.pc+1, storage}, memory}
  if a.pc != block.insts.length then throw (.malformed "allocated invalid program counter")
  match block.term with
  | .b target => return {s with control := .running (← jump f a target)}
  | .bCond flags cond taken untaken =>
    let yes ← condition (← readFlags a.storage flags) cond
    return {s with control := .running (← jump f a (if yes then taken else untaken))}
  | .cb nonzero src taken untaken =>
    let some width := src.class.width | throw (.malformed "allocated flags tested as bits")
    let raw := (← readBits a.storage src).setWidth width.bits
    let yes := (raw != 0) == nonzero
    return {s with control := .running (← jump f a (if yes then taken else untaken))}
  | .tb nonzero src bit taken untaken =>
    if bit ≥ 64 then throw (.malformed "allocated test bit outside register")
    let yes := (← readBits a.storage src).getLsbD bit == nonzero
    return {s with control := .running (← jump f a (if yes then taken else untaken))}
  | .brk imm => throw (.diagnostic imm)
  | .ret locs =>
    if !abiRegisters locs then throw (.malformed "allocated results outside the ABI")
    let values ← locs.mapM (readValue a.storage)
    let memory := s.memory.endFrame a.owner
    match s.stack with
    | [] => return {s with control := .halted (.ok values), memory}
    | frame :: rest =>
      if !abiRegisters frame.results || frame.results.length != locs.length then
        throw (.malformed "allocated result arity")
      let storage := returnStorage frame.caller.storage a.storage locs.length
      return {s with control := .running {frame.caller with storage}, stack := rest, memory}

def step (p : Program) (s : State) : Option State :=
  match s.control with
  | .halted _ => none
  | .running a => some (match execute p s a with
    | .ok next => next
    | .error fault => {s with control := .halted (.error fault)})

def initial (p : Program) (entry : String) (args : List Value)
    (memory : Memory := ⟨1, []⟩) : Except Fault State := do
  let some f := p.functions.find? (·.name == entry) | throw (.malformed "allocated unknown entry")
  return ⟨.running (← enter f (entryStorage args) 1), [], 2, memory⟩

/-- The observation of `Machine.observe`: returned bits of the result type or
the first checked diagnostic. -/
def observe (type : Core.Source.IntType) (s : State) : Option Core.Observation :=
  match s.control with
  | .halted (.ok [.int bits]) =>
    if type.width == 32 || type.width == 64 then
      some (.returned type (bits.setWidth type.width).toNat) else none
  | .halted (.error (.diagnostic code)) => Core.checkedDiagnostic code
  | _ => none

def Diverges (p : Program) (s : State) : Prop :=
  Core.Transition.Infinite (step p) s

def runSteps (p : Program) : Nat → State → State
  | 0, s => s
  | n+1, s => match step p s with | none => s | some next => runSteps p n next

end EvoIR.Alloc
