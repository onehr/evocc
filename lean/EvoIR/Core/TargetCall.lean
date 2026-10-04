import EvoIR.Core.IRExecution

/-! Exact operational call/return boundaries for later closure composition.
The supplied source instruction, callee, argument/result bindings and active
frame are checked premises; no callee body or target effect is invented. -/
namespace EvoIR.Core.IR

theorem call_enters (p : Program) (s : State) (a entered : Activation)
    (f callee : Function) (b : Block) (inst : Instruction) (args : List Value)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atPc : b.instructions[a.pc]? = some inst)
    (call : inst.op = .call callee.name)
    (found : p.functions.find? (·.name == callee.name) = some callee)
    (arguments : inst.operands.mapM (lookup a.values) = .ok args)
    (results : inst.results.map Binding.type = callee.returns)
    (entry : enter callee args s.nextFrame = .ok entered) :
    Step p s ⟨.running entered,
      ⟨{a with pc := a.pc+1}, inst.results⟩ :: s.stack, s.nextFrame+1⟩ := by
  apply Step.execute
  simp [step, running, execute, function, block, atPc, arguments, call, found,
    results, entry, Bind.bind, Except.bind, pure, Except.pure]

/-- Function exit ends precisely this activation's objects in each returned
memory snapshot. The caller's memory token is never fabricated or reset. -/
def returnedValues (owner : Nat) (values : List Value) : List Value :=
  values.map fun value => match value with
    | .memory m => .memory (m.endFrame owner)
    | _ => value

theorem call_returns (p : Program) (s : State) (a : Activation)
    (f : Function) (b : Block) (ids : List Nat) (values : List Value)
    (frame : ReturnFrame) (rest : List ReturnFrame) (bound : Bindings)
    (running : s.control = .running a)
    (function : p.functions.find? (·.name == a.function) = some f)
    (block : f.blocks.find? (·.name == a.block) = some b)
    (atEnd : a.pc = b.instructions.length)
    (terminator : b.terminator = .return ids)
    (returned : ids.mapM (lookup a.values) = .ok values)
    (types : values.map Value.type = f.returns)
    (stack : s.stack = frame :: rest)
    (bindings : appendResults frame.caller.values frame.results
      (returnedValues a.owner values) = .ok bound) :
    Step p s {s with control := .running {frame.caller with values := bound}, stack := rest} := by
  apply Step.execute
  simp [step, running, execute, function, block, atEnd, terminator, returned,
    types, stack, Bind.bind, Except.bind, pure, Except.pure]
  generalize closed : List.map _ values = finalized
  have same : finalized = returnedValues a.owner values := by
    rw [← closed]
    apply List.map_congr_left
    intro value _
    cases value <;> rfl
  rw [same]
  rw [bindings]

end EvoIR.Core.IR
