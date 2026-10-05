import EvoIR.MachineExecution

/-! Dispatcher lifting facts for actual instruction snapshots. The premises
name the function/block/instruction actually found by Machine.step; they do
not assume a compiler transformation or a proposed semantic equivalence. -/
namespace EvoIR.Machine

theorem execute_evalInstruction (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (inst : Instruction)
    (nextRegs : RegisterFile) (nextMemory : Memory)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (foundInstruction : block.insts[a.pc]? = some inst)
    (notCall : ∀ symbol args results, inst ≠ .bl symbol args results)
    (evaluated : evalInstruction a.owner inst a.regs s.memory = .ok (nextRegs, nextMemory)) :
    execute mode p s a = .ok {s with
      control := .running {a with pc := a.pc+1, regs := nextRegs}, memory := nextMemory} := by
  cases inst <;> try exact False.elim (notCall _ _ _ rfl)
  all_goals simp only [execute, foundFunction, foundBlock,
    foundInstruction, evaluated]
  all_goals rfl

theorem step_evalInstruction (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (inst : Instruction)
    (nextRegs : RegisterFile) (nextMemory : Memory) (running : s.control = .running a)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (foundInstruction : block.insts[a.pc]? = some inst)
    (notCall : ∀ symbol args results, inst ≠ .bl symbol args results)
    (evaluated : evalInstruction a.owner inst a.regs s.memory = .ok (nextRegs, nextMemory)) :
    step mode p s = some {s with
      control := .running {a with pc := a.pc+1, regs := nextRegs}, memory := nextMemory} := by
  simp only [step, running,
    execute_evalInstruction mode p s a function block inst nextRegs nextMemory
      foundFunction foundBlock foundInstruction notCall evaluated]

theorem execute_evalInstruction_error (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (inst : Instruction) (fault : Core.Source.Fault)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (foundInstruction : block.insts[a.pc]? = some inst)
    (notCall : ∀ symbol args results, inst ≠ .bl symbol args results)
    (evaluated : evalInstruction a.owner inst a.regs s.memory = .error fault) :
    execute mode p s a = .error fault := by
  cases inst <;> try exact False.elim (notCall _ _ _ rfl)
  all_goals simp only [execute, foundFunction, foundBlock,
    foundInstruction, evaluated]
  all_goals rfl

theorem step_evalInstruction_error (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (inst : Instruction) (fault : Core.Source.Fault)
    (running : s.control = .running a)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (foundInstruction : block.insts[a.pc]? = some inst)
    (notCall : ∀ symbol args results, inst ≠ .bl symbol args results)
    (evaluated : evalInstruction a.owner inst a.regs s.memory = .error fault) :
    step mode p s = some {s with control := .halted (.error fault)} := by
  simp only [step, running,
    execute_evalInstruction_error mode p s a function block inst fault
      foundFunction foundBlock foundInstruction notCall evaluated]

theorem execute_bl (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (symbol : String) (args results : List VReg)
    (values : List Value) (callee : Function) (entered : Activation)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (foundInstruction : block.insts[a.pc]? = some (.bl symbol args results))
    (readArgs : args.mapM (read a.regs) = .ok values)
    (foundCallee : p.functions.find? (·.name == symbol) = some callee)
    (enteredCallee : enter callee values s.nextFrame = .ok entered) :
    execute mode p s a = .ok {s with control := .running entered, stack := ⟨{a with pc := a.pc+1}, results⟩ :: s.stack, nextFrame := s.nextFrame+1} := by
  simp only [execute, foundFunction, foundBlock, foundInstruction, readArgs,
    foundCallee, Bind.bind, Except.bind, enteredCallee]
  rfl

theorem step_bl (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (symbol : String) (args results : List VReg)
    (values : List Value) (callee : Function) (entered : Activation)
    (running : s.control = .running a)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (foundInstruction : block.insts[a.pc]? = some (.bl symbol args results))
    (readArgs : args.mapM (read a.regs) = .ok values)
    (foundCallee : p.functions.find? (·.name == symbol) = some callee)
    (enteredCallee : enter callee values s.nextFrame = .ok entered) :
    step mode p s = some {s with control := .running entered, stack := ⟨{a with pc := a.pc+1}, results⟩ :: s.stack, nextFrame := s.nextFrame+1} := by
  simp only [step, running, execute_bl mode p s a function block symbol args results values
    callee entered foundFunction foundBlock foundInstruction readArgs foundCallee enteredCallee]

theorem execute_ret_noCaller (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (ids : List VReg) (values : List Value)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (noInstruction : block.insts[a.pc]? = none) (atTerminator : a.pc = block.insts.length)
    (returned : block.term = .ret ids) (readValues : ids.mapM (read a.regs) = .ok values)
    (noCaller : s.stack = []) :
    execute mode p s a = .ok {s with control := .halted (.ok values), memory := s.memory.endFrame a.owner} := by
  simp only [execute, foundFunction, foundBlock, noInstruction]
  simp only [atTerminator, bne_self_eq_false,
    Bool.false_eq_true, ↓reduceIte, returned, readValues, noCaller]
  rfl

theorem execute_ret_caller (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (ids : List VReg) (values : List Value)
    (frame : ReturnFrame) (rest : List ReturnFrame) (regs : RegisterFile)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (noInstruction : block.insts[a.pc]? = none) (atTerminator : a.pc = block.insts.length)
    (returned : block.term = .ret ids) (readValues : ids.mapM (read a.regs) = .ok values)
    (caller : s.stack = frame :: rest)
    (bound : bind frame.caller.regs frame.results values = .ok regs) :
    execute mode p s a = .ok {s with control := .running {frame.caller with regs}, stack := rest, memory := s.memory.endFrame a.owner} := by
  simp only [execute, foundFunction, foundBlock, noInstruction]
  simp only [atTerminator, bne_self_eq_false,
    Bool.false_eq_true, ↓reduceIte, returned, readValues, caller, Bind.bind, Except.bind, bound]
  rfl

theorem execute_ret_error (mode : Mode) (p : Program) (s : State) (a : Activation)
    (function : Function) (block : Block) (ids : List VReg) (fault : Core.Source.Fault)
    (foundFunction : p.functions.find? (·.name == a.function) = some function)
    (foundBlock : function.blocks.find? (·.id == a.block) = some block)
    (noInstruction : block.insts[a.pc]? = none) (atTerminator : a.pc = block.insts.length)
    (returned : block.term = .ret ids) (readValues : ids.mapM (read a.regs) = .error fault) :
    execute mode p s a = .error fault := by
  simp only [execute, foundFunction, foundBlock, noInstruction]
  simp only [atTerminator, bne_self_eq_false,
    Bool.false_eq_true, ↓reduceIte, returned, readValues]
  rfl

end EvoIR.Machine
