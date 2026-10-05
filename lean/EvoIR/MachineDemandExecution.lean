import EvoIR.MachineDemand
import EvoIR.MachineProgress

/-!
Actual same-program SSA demand execution. The progress hypotheses below are
semantic facts about both complete entry runs, discharged by the preceding
selection theorem. They are not producer-supplied type or use/def claims.
-/
namespace EvoIR.Machine.Demand

def OutcomeRelated (related : α → β → Prop) :
    Except Core.Source.Fault α → Except Core.Source.Fault β → Prop
  | .ok source, .ok target => related source target
  | .error source, .error target => source = target
  | _, _ => False

theorem reachable_execute_safe (program : Program) (type : Core.Source.IntType)
    (entry state : State) (progress : ObservedProgress .ssa program type entry)
    (reachable : Core.Transition.Reaches (step .ssa program) entry state)
    (activation : Activation) (running : state.control = .running activation) :
    ∀ message, execute .ssa program state activation ≠ .error (.malformed message) := by
  intro message failed
  have edge : step .ssa program state =
      some {state with control := .halted (.error (.malformed message))} := by
    simp only [step, running, failed]
  have reached := reachable.trans (.next edge (.refl _))
  rcases progress _ reached with ⟨next, continues⟩ | ⟨value, observed⟩
  · simp [step] at continues
  · simp [observe] at observed

theorem read_error_malformed (regs : RegisterFile) (reg : VReg)
    (fault : Core.Source.Fault) (failed : read regs reg = .error fault) :
    ∃ message, fault = .malformed message := by
  simp only [read] at failed
  split at failed <;> try contradiction
  all_goals exact ⟨_, (Except.error.inj failed).symm⟩

theorem reads_error_malformed (regs : RegisterFile) (args : List VReg)
    (fault : Core.Source.Fault) (failed : args.mapM (read regs) = .error fault) :
    ∃ message, fault = .malformed message := by
  induction args generalizing fault with
  | nil =>
    simp only [List.mapM_nil, Pure.pure, Except.pure] at failed
    contradiction
  | cons reg args ih =>
    simp only [List.mapM_cons, Bind.bind, Except.bind] at failed
    cases head : read regs reg with
    | error headFault =>
      simp only [head, Except.error.injEq] at failed
      subst fault
      exact read_error_malformed regs reg headFault head
    | ok value =>
      simp only [head] at failed
      cases tail : args.mapM (read regs) with
      | error tailFault =>
        simp only [tail, Except.error.injEq] at failed
        subst fault
        exact ih tailFault tail
      | ok values =>
        simp only [tail, Pure.pure, Except.pure] at failed
        contradiction

theorem bind_error_malformed (regs : RegisterFile) (params : List VReg)
    (values : List Value) (fault : Core.Source.Fault)
    (failed : bind regs params values = .error fault) :
    ∃ message, fault = .malformed message := by
  simp only [bind, MonadExcept.throw, throwThe, MonadExceptOf.throw,
    Bind.bind, Except.bind, Pure.pure, Except.pure] at failed
  repeat' split at failed
  all_goals try simp only [Except.error.injEq] at failed
  all_goals simp_all
  all_goals try contradiction
  all_goals exact ⟨_, failed.symm⟩

theorem bind_success_values (regs : RegisterFile) (params : List VReg)
    (values : List Value) (next : RegisterFile)
    (succeeded : bind regs params values = .ok next) :
    next = bindValues regs (params.zip values) := by
  simp only [bind, MonadExcept.throw, throwThe, MonadExceptOf.throw,
    Bind.bind, Except.bind, Pure.pure, Except.pure] at succeeded
  repeat' split at succeeded <;> simp_all [bindValues]

theorem bind_success_agree (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (params : List VReg)
    (sourceValues targetValues : List Value)
    (values : BindingAgreement ids (params.zip sourceValues) (params.zip targetValues))
    (sourceNext targetNext : RegisterFile)
    (sourceBound : bind source params sourceValues = .ok sourceNext)
    (targetBound : bind target params targetValues = .ok targetNext) :
    RegistersAgree ids sourceNext targetNext := by
  rw [bind_success_values source params sourceValues sourceNext sourceBound,
    bind_success_values target params targetValues targetNext targetBound]
  exact bindValues_agree ids source target agree _ _ values

theorem evalInstruction_lookup_other (owner : Nat) (inst : Instruction)
    (regs : RegisterFile) (memory : Memory) (next : RegisterFile) (nextMemory : Memory)
    (evaluated : evalInstruction owner inst regs memory = .ok (next, nextMemory))
    (reg : VReg) (outside : ∀ dst ∈ inst.defs, reg.id ≠ dst.id) :
    readSlot next reg = readSlot regs reg := by
  cases effects : evalInstructionOperands owner inst (readOperand regs) memory with
  | error fault =>
    simp only [evalInstruction, effects, Bind.bind, Except.bind] at evaluated
    contradiction
  | ok output =>
    rcases output with ⟨writes, effectMemory⟩
    simp only [evalInstruction, effects, Bind.bind, Except.bind, Pure.pure,
      Except.pure, Except.ok.injEq, Prod.mk.injEq] at evaluated
    rcases evaluated with ⟨bank, memoryEq⟩
    subst next
    apply applyWrites_readSlot_other
    intro write member
    apply outside write.1
    rw [← evalInstructionOperands_resultRegs owner inst (readOperand regs) memory writes
      effectMemory effects]
    exact List.mem_map.mpr ⟨write, member, rfl⟩

theorem instruction_successes_dead (owner : Nat) (inst : Instruction) (ids : List Nat)
    (source target : RegisterFile) (memory : Memory)
    (sourceNext targetNext : RegisterFile)
    (sourceEvaluated : evalInstruction owner inst source memory = .ok (sourceNext, memory))
    (targetEvaluated : evalInstruction owner inst target memory = .ok (targetNext, memory))
    (outside : ∀ dst ∈ inst.defs, dst.id ∉ ids)
    (agree : RegistersAgree ids source target) : RegistersAgree ids sourceNext targetNext := by
  intro id used
  have disjoint : ∀ dst ∈ inst.defs, id ≠ dst.id := by
    intro dst defined same
    exact outside dst defined (same ▸ used)
  exact (evalInstruction_lookup_other owner inst source memory sourceNext memory sourceEvaluated
    ⟨id, .gpr64⟩ disjoint).trans ((agree id used).trans
      (evalInstruction_lookup_other owner inst target memory targetNext memory targetEvaluated
        ⟨id, .gpr64⟩ disjoint).symm)

theorem reads_agree (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (args : List VReg)
    (used : ∀ reg ∈ args, reg.id ∈ ids) :
    args.mapM (read source) = args.mapM (read target) := by
  induction args with
  | nil => rfl
  | cons reg rest ih =>
    have head := read_agree ids source target agree reg (used reg (by simp))
    have tail := ih (by intro r member; exact used r (by simp [member]))
    simp only [List.mapM_cons, head, tail]

theorem read_list_length (regs : RegisterFile) (args : List VReg) (values : List Value)
    (readValues : args.mapM (read regs) = .ok values) : values.length = args.length := by
  induction args generalizing values with
  | nil => simp only [List.mapM_nil, Pure.pure, Except.pure, Except.ok.injEq] at readValues
           subst values
           rfl
  | cons reg rest ih =>
    simp only [List.mapM_cons, Bind.bind, Except.bind] at readValues
    cases head : read regs reg <;> simp only [head] at readValues
    next fault => contradiction
    next value =>
      cases tail : rest.mapM (read regs) <;>
        simp only [tail, Pure.pure, Except.pure, Except.ok.injEq] at readValues
      next fault => contradiction
      next valuesTail =>
        subst values
        exact congrArg Nat.succ (ih valuesTail tail)

theorem ActivationRelated.advance (plan : Plan) (source target : Activation)
    (related : ActivationRelated plan source target) (sourceRegs targetRegs : RegisterFile)
    (regs : RegistersAgree (needed plan source.function) sourceRegs targetRegs) :
    ActivationRelated plan {source with pc := source.pc+1, regs := sourceRegs}
      {target with pc := target.pc+1, regs := targetRegs} :=
  ⟨related.function, related.block, congrArg (·+1) related.pc, related.owner, regs⟩

theorem ActivationRelated.saved (plan : Plan) (source target : Activation)
    (related : ActivationRelated plan source target) :
    ActivationRelated plan {source with pc := source.pc+1} {target with pc := target.pc+1} :=
  ActivationRelated.advance plan source target related source.regs target.regs related.regs

theorem BindingAgreement.refl (ids : List Nat) (values : List (VReg × Value)) :
    BindingAgreement ids values values := by
  induction values with
  | nil => exact .nil
  | cons value rest ih => exact .cons value.1 value.2 value.2 (by intro _; rfl) ih

theorem bind_same_values_related (ids : List Nat) (source target : RegisterFile)
    (agree : RegistersAgree ids source target) (params : List VReg) (values : List Value) :
    OutcomeRelated (RegistersAgree ids) (bind source params values) (bind target params values) := by
  simp only [bind, MonadExcept.throw, throwThe, MonadExceptOf.throw,
    Bind.bind, Except.bind, Pure.pure, Except.pure]
  repeat' split
  all_goals try exact rfl
  all_goals exact bindValues_agree ids source target agree _ _ (BindingAgreement.refl ids _)

theorem noncall_execute_related (program : Program) (plan : Plan)
    (sourceState targetState : State) (states : StateRelated plan sourceState targetState)
    (source target : Activation) (related : ActivationRelated plan source target)
    (function : Function) (block : Block) (inst : Instruction)
    (foundFunction : program.functions.find? (·.name == source.function) = some function)
    (foundBlock : function.blocks.find? (·.id == source.block) = some block)
    (foundInstruction : block.insts[source.pc]? = some inst)
    (notCall : ∀ symbol args results, inst ≠ .bl symbol args results)
    (evaluations : EvaluationRelated (needed plan source.function)
      (evalInstruction source.owner inst source.regs sourceState.memory)
      (evalInstruction source.owner inst target.regs sourceState.memory)) :
    OutcomeRelated (StateRelated plan) (execute .ssa program sourceState source)
      (execute .ssa program targetState target) := by
  have targetFunction : program.functions.find? (·.name == target.function) = some function := by
    simpa only [← related.function] using foundFunction
  have targetBlock : function.blocks.find? (·.id == target.block) = some block := by
    simpa only [← related.block] using foundBlock
  have targetInstruction : block.insts[target.pc]? = some inst := by
    simpa only [← related.pc] using foundInstruction
  cases sourceEval : evalInstruction source.owner inst source.regs sourceState.memory with
  | error fault =>
    cases targetEval : evalInstruction source.owner inst target.regs sourceState.memory with
    | ok output => simp [EvaluationRelated, sourceEval, targetEval] at evaluations
    | error targetFault =>
      have faults : fault = targetFault := by
        simpa only [EvaluationRelated, sourceEval, targetEval] using evaluations
      have actualTarget : evalInstruction target.owner inst target.regs targetState.memory =
          .error targetFault := by
        simpa only [← related.owner, ← states.memory] using targetEval
      rw [execute_evalInstruction_error .ssa program sourceState source function block inst fault
        foundFunction foundBlock foundInstruction notCall sourceEval,
        execute_evalInstruction_error .ssa program targetState target function block inst targetFault
          targetFunction targetBlock targetInstruction notCall actualTarget]
      exact faults
  | ok sourceOutput =>
    rcases sourceOutput with ⟨sourceRegs, sourceMemory⟩
    cases targetEval : evalInstruction source.owner inst target.regs sourceState.memory with
    | error fault => simp [EvaluationRelated, sourceEval, targetEval] at evaluations
    | ok targetOutput =>
      rcases targetOutput with ⟨targetRegs, targetMemory⟩
      have effect : sourceMemory = targetMemory ∧
          RegistersAgree (needed plan source.function) sourceRegs targetRegs := by
        simpa only [EvaluationRelated, sourceEval, targetEval] using evaluations
      have actualTarget : evalInstruction target.owner inst target.regs targetState.memory =
          .ok (targetRegs, targetMemory) := by
        simpa only [← related.owner, ← states.memory] using targetEval
      rw [execute_evalInstruction .ssa program sourceState source function block inst sourceRegs
        sourceMemory foundFunction foundBlock foundInstruction notCall sourceEval,
        execute_evalInstruction .ssa program targetState target function block inst targetRegs
          targetMemory targetFunction targetBlock targetInstruction notCall actualTarget]
      exact ⟨.running (ActivationRelated.advance plan source target related sourceRegs targetRegs
        effect.2), states.stack, states.nextFrame, effect.1⟩

end EvoIR.Machine.Demand
