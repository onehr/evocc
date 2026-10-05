import EvoIR.Asm.Check
import EvoIR.Asm.Arith
import EvoIR.Asm.Memory
import EvoIR.Core.CheckpointSimulation

/-!
The simulation relation between allocated EvoMIR states and assembly states.

Each activation has an assembly frame: the function index, the stack pointer
of the body and the frame hint. From the stack pointer upward the frame has
the outgoing area, the spill slots, the save slots, then the frame record
(saved `x29` and `x30`) and the entry stack pointer. The incoming stack
arguments of a frame are the outgoing area of its caller, from the entry
stack pointer upward.

`Running` relates a running allocated state to an assembly state at the
template of the current instruction:

* The general registers, the flags and the outgoing slots of the current
  activation are the assembly registers, the NZCV flags and the outgoing area.
* The spill slots and incoming slots of each activation are in its frame.
* For each suspended caller, the frame record of its callee holds the frame
  pointer of the caller and the code address after its call. The
  callee-saved registers of the caller are the values that the epilogues of
  the inner frames restore (`restoreView`).
* The stack region contains the frames and the remaining stack is
  sufficient for the deepest call chain of the current function.

An unspecified allocated place (`none`) has no constraint. The relation
mentions no private frame object: the checker refuses those forms.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open EvoIR.Asm.Check

def load64 (mem : Nat → BitVec 8) (a : Nat) : BitVec 64 := BitVec.ofNat 64 (loadBytes mem a 8)

def load32 (mem : Nat → BitVec 8) (a : Nat) : BitVec 64 := BitVec.ofNat 64 (loadBytes mem a 4)

/-- A stack slot holds an integer cell: its defined bytes are the bits. -/
def SlotRel (mem : Nat → BitVec 8) (a : Nat) : Cell → Prop
  | ⟨.value (.int b), .x⟩ => load64 mem a = b
  | ⟨.value (.int b), .w⟩ => load32 mem a = view .w b
  | ⟨.value (.pointer _ _), _⟩ => False
  | ⟨.flags _, _⟩ => True

/-- The static inputs of the checker. -/
structure Ctx where
  p : Alloc.Program
  asm : File
  hints : List FrameHint

structure Frame where
  index : Nat
  sp : Nat
  hint : FrameHint

def Frame.fp (fr : Frame) : Nat := fr.sp + fr.hint.size

def Frame.entry (fr : Frame) : Nat := fr.sp + fr.hint.size + 16

def FunAt (ctx : Ctx) (i : Nat) (f : Alloc.Function) (h : FrameHint) (g : Asm.Function) : Prop :=
  ctx.p.functions[i]? = some f ∧ ctx.hints[i]? = some h ∧ ctx.asm.functions[i]? = some g

/-- The lines from instruction `i` of block `b` to the end of its terminator
template. -/
def restLines (h : FrameHint) (next : Option Nat) (b : Alloc.Block) (i : Nat) :
    Option (List Line) := do
  let insts ← (b.insts.drop i).mapM (instTemplate h)
  let term ← termTemplate h next b.term
  return (insts.flatten ++ term).map .inst

/-- Line `pos` of `body` starts the code of instruction `i` of block `b`.
The lines of the blocks after `b` follow the code of `b`. -/
def At (h : FrameHint) (f : Alloc.Function) (body : List Line) (b : Alloc.Block) (i pos : Nat) :
    Prop :=
  ∃ pre post code tail, f.blocks = pre ++ b :: post ∧
    restLines h (post.head?.map (·.id)) b i = some code ∧
    blocksLines h post = some tail ∧ body.drop pos = code ++ tail

/-- The activation runs function `fr.index` at the code of its block and
program counter, which starts at line `pos`. -/
def Placed (ctx : Ctx) (act : Activation) (fr : Frame) (pos : Nat) : Prop :=
  ∃ f g b, FunAt ctx fr.index f fr.hint g ∧ f.name = act.function ∧
    f.blocks.find? (·.id == act.block) = some b ∧ act.pc ≤ b.insts.length ∧
    At fr.hint f g.body b act.pc pos

/-- Every value of the storage is an integer, and general registers outside
the allocatable set are unspecified. -/
def Shape (st : Storage) : Prop :=
  (∀ place v d, st place = some ⟨.value v, d⟩ → ∃ b, v = .int b) ∧
  (∀ n, allocatable n = false → st (.gpr n) = none)

/-- The spill and incoming slots of a frame. -/
def OwnSlots (high : Nat) (mem : Nat → BitVec 8) (fr : Frame) (st : Storage) : Prop :=
  (∀ k cell, st (.spill k) = some cell →
    ∃ off, lookupOffset fr.hint.spills k = some off ∧ SlotRel mem (fr.sp + off) cell) ∧
  (∀ k cell, st (.incoming k) = some cell →
    fr.entry + 8 * k + 8 ≤ high ∧ SlotRel mem (fr.entry + 8 * k) cell)

def RegsRel (regs : Nat → BitVec 64) (st : Storage) : Prop :=
  ∀ n b d, st (.gpr n) = some ⟨.value (.int b), d⟩ → regs n = b

def FlagsRel (flags : Flags) (st : Storage) : Prop :=
  ∀ fl d c b, st .flags = some ⟨.flags fl, d⟩ → Machine.condition fl c = .ok b →
    flags.holds c = b

def OutgoingRel (mem : Nat → BitVec 8) (fr : Frame) (st : Storage) : Prop :=
  ∀ k cell, st (.outgoing k) = some cell →
    8 * k + 8 ≤ fr.hint.outgoing ∧ SlotRel mem (fr.sp + 8 * k) cell

/-- The callee-saved registers after the epilogue of `fr`: a saved register
comes from its save slot, any other register is unchanged. -/
def restoreView (fr : Frame) (mem : Nat → BitVec 8) (regs : Nat → BitVec 64) : Nat → BitVec 64 :=
  fun r => match lookupOffset fr.hint.saved r with
    | some off => load64 mem (fr.sp + off)
    | none => regs r

def SavedRel (vis : Nat → BitVec 64) (st : Storage) : Prop :=
  ∀ r b d, isCalleeSaved r = true → st (.gpr r) = some ⟨.value (.int b), d⟩ → vis r = b

/-- The suspended callers. `inner` is the frame of the activation that the
first caller called, and `vis` gives the callee-saved registers as `inner`
sees them. -/
def Chain (ctx : Ctx) (low high : Nat) (mem : Nat → BitVec 8) :
    Frame → (Nat → BitVec 64) → List ReturnFrame → List Frame → Prop
  | inner, _, [], [] =>
    inner.entry ≤ high ∧ decodeAddress (load64 mem (inner.fp + 8)) = none
  | inner, vis, rf :: rest, fr :: frs =>
    fr.sp = inner.entry ∧ fr.sp % 16 = 0 ∧
    low + needOf ctx.p ctx.hints rf.caller.function ≤ fr.entry ∧
    load64 mem inner.fp = BitVec.ofNat 64 fr.fp ∧
    (∃ pos, Placed ctx rf.caller fr pos ∧ pos < functionSpan ∧
      load64 mem (inner.fp + 8) = codeAddress fr.index pos) ∧
    Shape rf.caller.storage ∧ OwnSlots high mem fr rf.caller.storage ∧
    SavedRel (restoreView inner mem vis) rf.caller.storage ∧
    Chain ctx low high mem fr (restoreView inner mem vis) rest frs
  | _, _, _, _ => False

/-- The function of the outermost activation. -/
def outermost (a : Activation) (stack : List ReturnFrame) : String :=
  (stack.getLast?.map (·.caller.function)).getD a.function

/-- A running allocated state and the assembly state at the start of the
code of its current instruction. `entry` is the outermost function. -/
def Running (ctx : Ctx) (entry : String) (s : Alloc.State) (t : Asm.State) : Prop :=
  ∃ a cur frs pos,
    s.control = .running a ∧ t.control = .running cur.index pos ∧
    outermost a s.stack = entry ∧
    Placed ctx a cur pos ∧
    t.sp = BitVec.ofNat 64 cur.sp ∧ cur.sp % 16 = 0 ∧
    t.regs 29 = BitVec.ofNat 64 cur.fp ∧
    t.low + needOf ctx.p ctx.hints a.function ≤ cur.entry ∧ t.high ≤ 2^64 ∧
    Shape a.storage ∧ RegsRel t.regs a.storage ∧ FlagsRel t.flags a.storage ∧
    OutgoingRel t.memory cur a.storage ∧ OwnSlots t.high t.memory cur a.storage ∧
    Chain ctx t.low t.high t.memory cur t.regs s.stack frs

end EvoIR.Asm.Proof
