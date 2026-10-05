import EvoIR.MachineStep

/-! Actual terminator dispatcher lifting for selection diagrams. All lookups,
the program counter and the terminator constructor are read from the real MIR.
The jump/flag premises are independent evaluations, not lowering assertions.
-/
namespace EvoIR.Machine.Selection

theorem step_branch (mode : Mode) (p : Program) (s : State) (a : Activation)
    (f : Function) (b : Block) (target : Nat) (args : List VReg) (entered : Activation)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.id == a.block) = some b)
    (atEnd : a.pc = b.insts.length) (term : b.term = .b target args)
    (jumped : jump mode f a target args = .ok entered) :
    step mode p s = some {s with control := .running entered} := by
  have missing : b.insts[a.pc]? = none := by
    apply List.getElem?_eq_none
    omega
  simp [step, running, execute, function, block, missing, atEnd, term, jumped,
    Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem step_branch_flags (mode : Mode) (p : Program) (s : State) (a : Activation)
    (f : Function) (b : Block) (flags : VReg) (cond : Cond)
    (taken : Nat) (takenArgs : List VReg) (untaken : Nat) (untakenArgs : List VReg)
    (flagValue : Flags) (yes : Bool) (entered : Activation)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.id == a.block) = some b)
    (atEnd : a.pc = b.insts.length)
    (term : b.term = .bCond flags cond taken takenArgs untaken untakenArgs)
    (read : readFlags a.regs flags = .ok flagValue)
    (tested : condition flagValue cond = .ok yes)
    (jumped : jump mode f a (if yes then taken else untaken)
      (if yes then takenArgs else untakenArgs) = .ok entered) :
    step mode p s = some {s with control := .running entered} := by
  have missing : b.insts[a.pc]? = none := by
    apply List.getElem?_eq_none
    omega
  simp [step, running, execute, function, block, missing, atEnd, term, read, tested,
    jumped, Bind.bind, Except.bind, Pure.pure, Except.pure]

theorem step_branch_zero (mode : Mode) (p : Program) (s : State) (a : Activation)
    (f : Function) (b : Block) (nonzero : Bool) (src : VReg)
    (taken : Nat) (takenArgs : List VReg) (untaken : Nat) (untakenArgs : List VReg)
    (width : Width) (raw : BitVec 64) (entered : Activation)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.id == a.block) = some b)
    (atEnd : a.pc = b.insts.length)
    (term : b.term = .cb nonzero src taken takenArgs untaken untakenArgs)
    (classWidth : src.class.width = some width) (read : readBits a.regs src = .ok raw)
    (jumped : jump mode f a
      (if (raw.setWidth width.bits != 0) == nonzero then taken else untaken)
      (if (raw.setWidth width.bits != 0) == nonzero then takenArgs else untakenArgs) = .ok entered) :
    step mode p s = some {s with control := .running entered} := by
  have missing : b.insts[a.pc]? = none := by
    apply List.getElem?_eq_none
    omega
  simp only [beq_iff_eq, BitVec.ofNat_eq_ofNat] at jumped
  simp [step, running, execute, function, block, missing, atEnd, term, classWidth, read,
    jumped, Bind.bind, Except.bind, Pure.pure, Except.pure, BitVec.ofNat_eq_ofNat]

theorem step_branch_bit (mode : Mode) (p : Program) (s : State) (a : Activation)
    (f : Function) (b : Block) (nonzero : Bool) (src : VReg) (bit : Nat)
    (taken : Nat) (takenArgs : List VReg) (untaken : Nat) (untakenArgs : List VReg)
    (raw : BitVec 64) (entered : Activation)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.id == a.block) = some b)
    (atEnd : a.pc = b.insts.length)
    (term : b.term = .tb nonzero src bit taken takenArgs untaken untakenArgs)
    (valid : bit < 64) (read : readBits a.regs src = .ok raw)
    (jumped : jump mode f a
      (if raw.getLsbD bit == nonzero then taken else untaken)
      (if raw.getLsbD bit == nonzero then takenArgs else untakenArgs) = .ok entered) :
    step mode p s = some {s with control := .running entered} := by
  have missing : b.insts[a.pc]? = none := by
    apply List.getElem?_eq_none
    omega
  simp only [beq_iff_eq, BitVec.getLsbD_eq_getElem valid] at jumped
  simp [step, running, execute, function, block, missing, atEnd, term, valid, read,
    jumped, Bind.bind, Except.bind, Pure.pure, Except.pure,
    show ¬64 ≤ bit by omega]

theorem step_trap (mode : Mode) (p : Program) (s : State) (a : Activation)
    (f : Function) (b : Block) (code : Nat)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.id == a.block) = some b)
    (atEnd : a.pc = b.insts.length) (term : b.term = .brk code) :
    step mode p s = some {s with control := .halted (.error (.diagnostic code))} := by
  have missing : b.insts[a.pc]? = none := by
    apply List.getElem?_eq_none
    omega
  simp [step, running, execute, function, block, missing, atEnd, term,
    Bind.bind, Except.bind, Pure.pure, Except.pure, throw, throwThe, MonadExceptOf.throw]

end EvoIR.Machine.Selection
