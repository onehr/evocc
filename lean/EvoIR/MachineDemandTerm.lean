import EvoIR.MachineDemandCalls

/-!
Factor the actual terminator dispatcher into its control decision and its
state effect. `execute_terminator` proves the factorization for every actual
constructor, so the demand proof does not trust a terminator uses table.
-/
namespace EvoIR.Machine.Demand

inductive TermEffect where
  | edge (destination : Nat) (args : List VReg)
  | returned (values : List Value)

def evalTerminator (regs : RegisterFile) : Terminator → Except Core.Source.Fault TermEffect
  | .b target args => .ok (.edge target args)
  | .bCond flags cond taken takenArgs untaken untakenArgs => do
    let yes ← condition (← readFlags regs flags) cond
    return .edge (if yes then taken else untaken) (if yes then takenArgs else untakenArgs)
  | .cb nonzero src taken takenArgs untaken untakenArgs => do
    let some width := src.class.width | throw (.malformed "MIR flags tested as bits")
    let raw := (← readBits regs src).setWidth width.bits
    let yes := (raw != 0) == nonzero
    return .edge (if yes then taken else untaken) (if yes then takenArgs else untakenArgs)
  | .tb nonzero src bit taken takenArgs untaken untakenArgs => do
    if bit ≥ 64 then throw (.malformed "MIR test bit outside register")
    let yes := (← readBits regs src).getLsbD bit == nonzero
    return .edge (if yes then taken else untaken) (if yes then takenArgs else untakenArgs)
  | .brk imm => .error (.diagnostic imm)
  | .ret ids => do return .returned (← ids.mapM (read regs))

def resume (state : State) (action : Except Core.Source.Fault Activation) :
    Except Core.Source.Fault State := do
  let entered ← action
  return {state with control := .running entered}

def returnValues (state : State) (activation : Activation) (values : List Value) :
    Except Core.Source.Fault State := do
  let memory := state.memory.endFrame activation.owner
  match state.stack with
  | [] => return {state with control := .halted (.ok values), memory}
  | frame :: rest =>
    let regs ← bind frame.caller.regs frame.results values
    return {state with control := .running {frame.caller with regs}, stack := rest, memory}

def finishTerminator (function : Function) (state : State) (activation : Activation) :
    TermEffect → Except Core.Source.Fault State
  | .edge target args => resume state (jump .ssa function activation target args)
  | .returned values => returnValues state activation values

theorem execute_terminator (program : Program) (state : State) (activation : Activation)
    (function : Function) (block : Block)
    (foundFunction : program.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (noInstruction : block.insts[activation.pc]? = none)
    (atTerminator : activation.pc = block.insts.length) :
    execute .ssa program state activation =
      (evalTerminator activation.regs block.term >>= finishTerminator function state activation) := by
  simp only [execute, foundFunction, foundBlock, noInstruction]
  simp only [atTerminator, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
  cases term : block.term <;>
    simp only [evalTerminator, finishTerminator, resume, returnValues,
      Bind.bind, Except.bind, Pure.pure, Except.pure,
      MonadExcept.throw, throwThe, MonadExceptOf.throw]
  all_goals repeat' first | rfl | split
  all_goals simp_all

theorem evalTerminator_agree (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (term : Terminator)
    (used : ∀ reg ∈ controlUses term, reg.id ∈ ids) :
    evalTerminator source term = evalTerminator target term := by
  cases term with
  | b target args => rfl
  | bCond flags cond taken takenArgs untaken untakenArgs =>
    simp only [evalTerminator,
      readFlags_agree ids source target agree flags (used flags (by simp [controlUses]))]
  | cb nonzero src taken takenArgs untaken untakenArgs =>
    simp only [evalTerminator,
      readBits_agree ids source target agree src (used src (by simp [controlUses]))]
  | tb nonzero src bit taken takenArgs untaken untakenArgs =>
    simp only [evalTerminator,
      readBits_agree ids source target agree src (used src (by simp [controlUses]))]
  | brk imm => rfl
  | ret values =>
    simp only [evalTerminator, reads_agree ids source target agree values (by
      simpa only [controlUses] using used)]

theorem evalTerminator_edge (regs : RegisterFile) (term : Terminator)
    (destination : Nat) (args : List VReg)
    (evaluated : evalTerminator regs term = .ok (.edge destination args)) :
    (destination, args) ∈ term.edges := by
  cases term <;>
    simp only [evalTerminator, Bind.bind, Except.bind, Pure.pure, Except.pure,
      MonadExcept.throw, throwThe, MonadExceptOf.throw] at evaluated
  all_goals repeat' split at evaluated
  all_goals simp_all [Terminator.edges]

theorem resume_safe (state : State) (action : Except Core.Source.Fault Activation)
    (safe : ∀ message, resume state action ≠ .error (.malformed message)) :
    ∀ message, action ≠ .error (.malformed message) := by
  intro message failed
  apply safe message
  rw [failed]
  rfl

theorem resume_related (plan : Plan) (sourceState targetState : State)
    (states : StateRelated plan sourceState targetState)
    (source target : Except Core.Source.Fault Activation)
    (related : OutcomeRelated (ActivationRelated plan) source target) :
    OutcomeRelated (StateRelated plan) (resume sourceState source) (resume targetState target) := by
  cases source <;> cases target <;>
    simp only [OutcomeRelated] at related
  all_goals try contradiction
  next source target => exact related
  next source target => exact ⟨.running related, states.stack, states.nextFrame, states.memory⟩

theorem returnValues_related (plan : Plan) (sourceState targetState : State)
    (states : StateRelated plan sourceState targetState)
    (source target : Activation) (related : ActivationRelated plan source target)
    (values : List Value) :
    OutcomeRelated (StateRelated plan) (returnValues sourceState source values)
      (returnValues targetState target values) := by
  have sameMemory : sourceState.memory.endFrame source.owner =
      targetState.memory.endFrame target.owner := by rw [states.memory, related.owner]
  cases sourceStack : sourceState.stack with
  | nil =>
    have aligned : Aligned (FrameRelated plan) [] targetState.stack := by
      simpa only [sourceStack] using states.stack
    have targetStack : targetState.stack = [] := by
      have length := aligned.length_eq
      simpa using length.symm
    simp only [returnValues, sourceStack, targetStack, Pure.pure, Except.pure, OutcomeRelated]
    exact ⟨.halted _, by simpa only [sourceStack, targetStack] using states.stack,
      states.nextFrame, sameMemory⟩
  | cons frame rest =>
    have aligned : Aligned (FrameRelated plan) (frame :: rest) targetState.stack := by
      simpa only [sourceStack] using states.stack
    cases targetStack : targetState.stack with
    | nil => rw [targetStack] at aligned; cases aligned
    | cons targetFrame targetRest =>
      rw [targetStack] at aligned
      cases aligned with
      | cons frames remaining =>
        have results := frames.results
        have bindings := bind_same_values_related (needed plan frame.caller.function)
          frame.caller.regs targetFrame.caller.regs frames.caller.regs frame.results values
        have targetResults : bind targetFrame.caller.regs frame.results values =
            bind targetFrame.caller.regs targetFrame.results values := by rw [results]
        rw [targetResults] at bindings
        simp only [returnValues, sourceStack, targetStack]
        cases sourceBound : bind frame.caller.regs frame.results values with
        | error fault =>
          cases targetBound : bind targetFrame.caller.regs targetFrame.results values with
          | ok regs => simp [OutcomeRelated, sourceBound, targetBound] at bindings
          | error targetFault =>
            simpa only [sourceBound, targetBound, Bind.bind, Except.bind, OutcomeRelated]
              using bindings
        | ok sourceRegs =>
          cases targetBound : bind targetFrame.caller.regs targetFrame.results values with
          | error fault => simp [OutcomeRelated, sourceBound, targetBound] at bindings
          | ok targetRegs =>
            have banks : RegistersAgree (needed plan frame.caller.function) sourceRegs targetRegs := by
              simpa only [OutcomeRelated, sourceBound, targetBound] using bindings
            simp only [sourceBound, targetBound, Bind.bind, Except.bind, Pure.pure,
              Except.pure, OutcomeRelated]
            exact ⟨.running ⟨frames.caller.function, frames.caller.block, frames.caller.pc,
              frames.caller.owner, banks⟩, remaining, states.nextFrame, sameMemory⟩

end EvoIR.Machine.Demand
