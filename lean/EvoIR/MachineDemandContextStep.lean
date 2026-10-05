import EvoIR.MachineDemandCorrespondence
import EvoIR.Core.OpenCheckpoint

/-! Append a real suspended caller suffix to a standalone callee run. The
return case binds the actual full result vector and uses the already finalized
memory. A successful actual caller binding is required, not assumed from a
scalar observation. No caller continuation is executed by this bridge. -/
namespace EvoIR.Machine.Demand

def restoreCaller (values : List Value) (memory : Memory) (nextFrame : Nat)
    (stack : List ReturnFrame) : State :=
  match stack with
  | [] => ⟨.halted (.ok values), [], nextFrame, memory⟩
  | frame :: rest =>
    match bind frame.caller.regs frame.results values with
    | .ok regs => ⟨.running {frame.caller with regs}, rest, nextFrame, memory⟩
    | .error fault => ⟨.halted (.error fault), stack, nextFrame, memory⟩

def context (state : State) (stack : List ReturnFrame) : State :=
  match state.control with
  | .halted (.ok values) => restoreCaller values state.memory state.nextFrame stack
  | _ => {state with stack := state.stack ++ stack}

def CallerReady (values : List Value) (stack : List ReturnFrame) : Prop :=
  match stack with
  | [] => True
  | frame :: _ => ∃ regs, bind frame.caller.regs frame.results values = .ok regs

theorem callerReady_single_result (frame : ReturnFrame) (rest : List ReturnFrame)
    (arity : frame.results.length = 1) (value : Value) :
    CallerReady [value] (frame :: rest) := by
  have unique : (frame.results.map VReg.id).eraseDups.length = frame.results.length := by
    cases params : frame.results with
    | nil => simp [params] at arity
    | cons first tail =>
      cases tail with
      | nil => rfl
      | cons second tail => simp [params] at arity
  exact ⟨bindValues frame.caller.regs (frame.results.zip [value]),
    bind_exact frame.caller.regs frame.results [value] (by simpa using arity) unique⟩

theorem context_running (state : State) (activation : Activation) (stack : List ReturnFrame)
    (running : state.control = .running activation) :
    context state stack = {state with stack := state.stack ++ stack} := by
  simp only [context, running]

theorem context_error (state : State) (fault : Core.Source.Fault) (stack : List ReturnFrame)
    (halted : state.control = .halted (.error fault)) :
    context state stack = {state with stack := state.stack ++ stack} := by
  simp only [context, halted]

private theorem step_context_success (program : Program) (state after result : State)
    (activation : Activation) (stack : List ReturnFrame)
    (running : state.control = .running activation)
    (actual : step .ssa program state = some after)
    (original : execute .ssa program state activation = .ok result)
    (extended : execute .ssa program {state with stack := state.stack ++ stack} activation =
      .ok (context result stack)) :
    step .ssa program (context state stack) = some (context after stack) := by
  have same : result = after := by
    exact Option.some.inj ((by simp only [step, running, original] :
      step .ssa program state = some result).symm.trans actual)
  rw [← same, context_running state activation stack running]
  rw [running] at extended
  simp only [step, running, extended]

private theorem step_context_fault (program : Program) (state after : State)
    (activation : Activation) (stack : List ReturnFrame) (fault : Core.Source.Fault)
    (running : state.control = .running activation)
    (actual : step .ssa program state = some after)
    (original : execute .ssa program state activation = .error fault)
    (extended : execute .ssa program {state with stack := state.stack ++ stack} activation = .error fault) :
    step .ssa program (context state stack) = some (context after stack) := by
  have same : {state with control := .halted (.error fault)} = after := by
    exact Option.some.inj ((by simp only [step, running, original] :
      step .ssa program state = some {state with control := .halted (.error fault)}).symm.trans actual)
  rw [← same, context_running state activation stack running]
  rw [running] at extended
  simp only [step, running, extended, context]

theorem step_context (program : Program) (state after : State)
    (activation : Activation) (stack : List ReturnFrame)
    (running : state.control = .running activation)
    (actual : step .ssa program state = some after)
    (ready : ∀ values, after.control = .halted (.ok values) → CallerReady values stack) :
    step .ssa program (context state stack) = some (context after stack) := by
  cases foundFunction : program.functions.find? (·.name == activation.function) with
  | none =>
    apply step_context_fault program state after activation stack
      (.malformed "MIR missing function") running actual <;>
      simp [execute, foundFunction, MonadExcept.throw, throwThe, MonadExceptOf.throw]
  | some function =>
    cases foundBlock : function.blocks.find? (·.id == activation.block) with
    | none =>
      apply step_context_fault program state after activation stack
        (.malformed "MIR missing block") running actual <;>
        simp [execute, foundFunction, foundBlock, MonadExcept.throw, throwThe, MonadExceptOf.throw]
    | some block =>
      cases foundInstruction : block.insts[activation.pc]? with
      | some inst =>
        by_cases isCall : ∃ symbol args results, inst = .bl symbol args results
        · obtain ⟨symbol, args, results, rfl⟩ := isCall
          cases readArgs : args.mapM (read activation.regs) with
          | error fault =>
            apply step_context_fault program state after activation stack fault running actual <;>
              simp [execute, foundFunction, foundBlock, foundInstruction, readArgs,
                Bind.bind, Except.bind]
          | ok values =>
            cases foundCallee : program.functions.find? (·.name == symbol) with
            | none =>
              apply step_context_fault program state after activation stack
                (.malformed "MIR external call") running actual <;>
                simp [execute, foundFunction, foundBlock, foundInstruction, readArgs, foundCallee,
                  Bind.bind, Except.bind, MonadExcept.throw, throwThe, MonadExceptOf.throw]
            | some callee =>
              cases entered : enter callee values state.nextFrame with
              | error fault =>
                apply step_context_fault program state after activation stack fault running actual <;>
                  simp [execute, foundFunction, foundBlock, foundInstruction, readArgs, foundCallee,
                    entered, Bind.bind, Except.bind]
              | ok calleeActivation =>
                have original := execute_bl .ssa program state activation function block symbol args
                  results values callee calleeActivation foundFunction foundBlock foundInstruction
                  readArgs foundCallee entered
                apply step_context_success program state after _ activation stack running actual original
                have extended := execute_bl .ssa program {state with stack := state.stack ++ stack}
                  activation function block symbol args results values callee calleeActivation
                  foundFunction foundBlock foundInstruction readArgs foundCallee entered
                simpa only [context, List.cons_append] using extended
        · have notCall : ∀ symbol args results, inst ≠ .bl symbol args results := by
            intro symbol args results same
            exact isCall ⟨symbol, args, results, same⟩
          cases evaluated : evalInstruction activation.owner inst activation.regs state.memory with
          | error fault =>
            exact step_context_fault program state after activation stack fault running actual
              (execute_evalInstruction_error .ssa program state activation function block inst fault
                foundFunction foundBlock foundInstruction notCall evaluated)
              (execute_evalInstruction_error .ssa program {state with stack := state.stack ++ stack}
                activation function block inst fault foundFunction foundBlock foundInstruction notCall evaluated)
          | ok outputs =>
            rcases outputs with ⟨regs, memory⟩
            have original := execute_evalInstruction .ssa program state activation function block inst
              regs memory foundFunction foundBlock foundInstruction notCall evaluated
            apply step_context_success program state after _ activation stack running actual original
            exact execute_evalInstruction .ssa program {state with stack := state.stack ++ stack}
              activation function block inst regs memory foundFunction foundBlock foundInstruction notCall evaluated
      | none =>
        by_cases atTerminator : activation.pc = block.insts.length
        · have original := execute_terminator program state activation function block foundFunction
            foundBlock foundInstruction atTerminator
          have extended := execute_terminator program {state with stack := state.stack ++ stack}
            activation function block foundFunction foundBlock foundInstruction atTerminator
          cases selected : evalTerminator activation.regs block.term with
          | error fault =>
            apply step_context_fault program state after activation stack fault running actual
            · simpa only [selected, Bind.bind, Except.bind] using original
            · simpa only [selected, Bind.bind, Except.bind] using extended
          | ok effect =>
            cases effect with
            | edge destination args =>
              cases jumped : jump .ssa function activation destination args with
              | error fault =>
                apply step_context_fault program state after activation stack fault running actual
                · simpa only [selected, jumped, finishTerminator, resume,
                    Bind.bind, Except.bind] using original
                · simpa only [selected, jumped, finishTerminator, resume,
                    Bind.bind, Except.bind] using extended
              | ok nextActivation =>
                apply step_context_success program state after {state with control := .running nextActivation}
                  activation stack running actual
                · simpa only [selected, jumped, finishTerminator, resume, Bind.bind, Except.bind,
                    Pure.pure, Except.pure] using original
                · simpa only [selected, jumped, finishTerminator, resume, Bind.bind, Except.bind,
                    Pure.pure, Except.pure, context] using extended
            | returned values =>
              cases ownStack : state.stack with
              | nil =>
                let result : State := {state with control := .halted (.ok values), memory := state.memory.endFrame activation.owner}
                have originalResult : execute .ssa program state activation = .ok result := by
                  simpa only [result, selected, finishTerminator, returnValues, ownStack,
                    Bind.bind, Except.bind, Pure.pure, Except.pure] using original
                have resultAfter : result = after := by
                  exact Option.some.inj ((by simp only [step, running, originalResult] :
                    step .ssa program state = some result).symm.trans actual)
                have extraReady := ready values (by simpa only [← resultAfter, result])
                apply step_context_success program state after result activation stack running actual originalResult
                cases callerStack : stack with
                | nil =>
                  simpa only [selected, finishTerminator, returnValues, ownStack,
                    List.nil_append, callerStack, context, result, restoreCaller,
                    Bind.bind, Except.bind, Pure.pure, Except.pure] using extended
                | cons frame rest =>
                  simp only [CallerReady, callerStack] at extraReady
                  obtain ⟨regs, bound⟩ := extraReady
                  simpa only [selected, finishTerminator, returnValues, ownStack,
                    List.nil_append, callerStack, context, result, restoreCaller, bound,
                    Bind.bind, Except.bind, Pure.pure, Except.pure] using extended
              | cons frame rest =>
                cases bound : bind frame.caller.regs frame.results values with
                | error fault =>
                  apply step_context_fault program state after activation stack fault running actual
                  · simpa only [selected, finishTerminator, returnValues, ownStack, bound,
                      Bind.bind, Except.bind] using original
                  · simpa only [selected, finishTerminator, returnValues, ownStack, List.cons_append,
                      bound, Bind.bind, Except.bind] using extended
                | ok regs =>
                  let result : State := {state with control := .running {frame.caller with regs}, stack := rest, memory := state.memory.endFrame activation.owner}
                  apply step_context_success program state after result activation stack running actual
                  · simpa only [selected, finishTerminator, returnValues, ownStack, bound,
                      Bind.bind, Except.bind, Pure.pure, Except.pure] using original
                  · simpa only [selected, finishTerminator, returnValues, ownStack, List.cons_append,
                      bound, Bind.bind, Except.bind, Pure.pure, Except.pure, result, context] using extended
        · apply step_context_fault program state after activation stack
            (.malformed "MIR invalid program counter") running actual <;>
            simp only [execute, foundFunction, foundBlock, foundInstruction, bne_iff_ne,
              if_pos atTerminator, MonadExcept.throw, throwThe, MonadExceptOf.throw,
              Bind.bind, Except.bind]

end EvoIR.Machine.Demand
