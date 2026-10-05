import EvoIR.MachineDemandTerm

/-!
Demand closure is discharged on the actual instructions and successors of
the program. This module lifts those checked local facts through the actual
SSA call-stack machine, without a bounded execution or a typing axiom.
-/
namespace EvoIR.Machine.Demand

theorem checked_edge_target (function : Function) (ids : List Nat)
    (destination : Nat) (args : List VReg)
    (accepted : checkEdge function ids (destination, args) = true) :
    ∃ block, function.blocks.find? (·.id == destination) = some block ∧
      checkParameters ids ids block.args args = true := by
  cases found : function.blocks.find? (·.id == destination) with
  | none => simp [checkEdge, found] at accepted
  | some block => exact ⟨block, rfl, by simpa only [checkEdge, found] using accepted⟩

theorem checked_call_target (program : Program) (plan : Plan) (function : Function)
    (symbol : String) (args : List VReg)
    (accepted : checkCall program plan function symbol args = true) :
    ∃ callee entry, program.functions.find? (·.name == symbol) = some callee ∧
      callee.blocks.find? (·.id == 0) = some entry ∧
      checkParameters (needed plan callee.name) (needed plan function.name) entry.args args = true := by
  cases found : program.functions.find? (·.name == symbol) with
  | none => simp [checkCall, found] at accepted
  | some callee =>
    cases foundEntry : callee.blocks.find? (·.id == 0) with
    | none => simp [checkCall, found, foundEntry] at accepted
    | some entry =>
      exact ⟨callee, entry, rfl, foundEntry, by
        simpa only [checkCall, found, foundEntry] using accepted⟩

theorem noncall_checked (program : Program) (plan : Plan) (function : Function)
    (inst : Instruction) (notCall : ∀ symbol args results, inst ≠ .bl symbol args results)
    (accepted : checkInstruction program plan function inst = true) :
    InstructionClosed (needed plan function.name) inst := by
  apply checkLocalInstruction_sound
  cases inst <;> try exact accepted
  next symbol args results => exact False.elim (notCall symbol args results rfl)

theorem terminator_execute_related (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true)
    (sourceState targetState : State) (states : StateRelated plan sourceState targetState)
    (source target : Activation) (related : ActivationRelated plan source target)
    (function : Function) (block : Block)
    (foundFunction : program.functions.find? (·.name == source.function) = some function)
    (foundBlock : function.blocks.find? (·.id == source.block) = some block)
    (noInstruction : block.insts[source.pc]? = none)
    (atTerminator : source.pc = block.insts.length)
    (sourceSafe : ∀ message, execute .ssa program sourceState source ≠ .error (.malformed message))
    (targetSafe : ∀ message, execute .ssa program targetState target ≠ .error (.malformed message)) :
    OutcomeRelated (StateRelated plan) (execute .ssa program sourceState source)
      (execute .ssa program targetState target) := by
  have functionName : function.name = source.function := by simpa using List.find?_some foundFunction
  have targetFunction : program.functions.find? (·.name == target.function) = some function := by
    simpa only [← related.function] using foundFunction
  have targetBlock : function.blocks.find? (·.id == target.block) = some block := by
    simpa only [← related.block] using foundBlock
  have targetNoInstruction : block.insts[target.pc]? = none := by
    simpa only [← related.pc] using noInstruction
  have targetAtTerminator : target.pc = block.insts.length := related.pc.symm.trans atTerminator
  have sourceDispatch := execute_terminator program sourceState source function block foundFunction
    foundBlock noInstruction atTerminator
  have targetDispatch := execute_terminator program targetState target function block targetFunction
    targetBlock targetNoInstruction targetAtTerminator
  have used := checkStructure_controls program plan accepted function
    (List.mem_of_find?_eq_some foundFunction) block (List.mem_of_find?_eq_some foundBlock)
  have banks : RegistersAgree (needed plan function.name) source.regs target.regs := by
    simpa only [functionName] using related.regs
  have choices := evalTerminator_agree (needed plan function.name) source.regs target.regs banks
    block.term used
  cases selected : evalTerminator source.regs block.term with
  | error fault =>
    have targetSelected : evalTerminator target.regs block.term = .error fault := choices.symm.trans selected
    rw [sourceDispatch, targetDispatch, selected, targetSelected]
    rfl
  | ok effect =>
    have targetSelected : evalTerminator target.regs block.term = .ok effect := choices.symm.trans selected
    have sourceFinishSafe : ∀ message,
        finishTerminator function sourceState source effect ≠ .error (.malformed message) := by
      intro message failed
      apply sourceSafe message
      rw [sourceDispatch, selected]
      exact failed
    have targetFinishSafe : ∀ message,
        finishTerminator function targetState target effect ≠ .error (.malformed message) := by
      intro message failed
      apply targetSafe message
      rw [targetDispatch, targetSelected]
      exact failed
    rw [sourceDispatch, targetDispatch, selected, targetSelected]
    change OutcomeRelated (StateRelated plan) (finishTerminator function sourceState source effect)
      (finishTerminator function targetState target effect)
    cases effect with
    | returned values => exact returnValues_related plan sourceState targetState states source target related values
    | edge destination args =>
      have edgeMember := evalTerminator_edge source.regs block.term destination args selected
      have checked := checkStructure_edge program plan accepted function
        (List.mem_of_find?_eq_some foundFunction) block (List.mem_of_find?_eq_some foundBlock)
        (destination, args) edgeMember
      obtain ⟨successor, foundSuccessor, parameters⟩ := checked_edge_target function
        (needed plan function.name) destination args checked
      have sourceJumpSafe := resume_safe sourceState (jump .ssa function source destination args) sourceFinishSafe
      have targetJumpSafe := resume_safe targetState (jump .ssa function target destination args) targetFinishSafe
      exact resume_related plan sourceState targetState states _ _
        (jump_safe_related plan function source target related functionName destination args successor
          foundSuccessor parameters sourceJumpSafe targetJumpSafe)

theorem execute_related_reachable (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry sourceState targetState : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (sourceReachable : Core.Transition.Reaches (step .ssa program) sourceEntry sourceState)
    (targetReachable : Core.Transition.Reaches (step .ssa program) targetEntry targetState)
    (states : StateRelated plan sourceState targetState) (source target : Activation)
    (related : ActivationRelated plan source target)
    (sourceRunning : sourceState.control = .running source)
    (targetRunning : targetState.control = .running target) :
    OutcomeRelated (StateRelated plan) (execute .ssa program sourceState source)
      (execute .ssa program targetState target) := by
  classical
  have sourceSafe := reachable_execute_safe program type sourceEntry sourceState sourceProgress
    sourceReachable source sourceRunning
  have targetSafe := reachable_execute_safe program type targetEntry targetState targetProgress
    targetReachable target targetRunning
  cases foundFunction : program.functions.find? (·.name == source.function) with
  | none =>
    have targetFunction : program.functions.find? (·.name == target.function) = none := by
      simpa only [← related.function] using foundFunction
    simp only [execute, foundFunction, targetFunction, OutcomeRelated]
    rfl
  | some function =>
    cases foundBlock : function.blocks.find? (·.id == source.block) with
    | none =>
      have targetFunction : program.functions.find? (·.name == target.function) = some function := by
        simpa only [← related.function] using foundFunction
      have targetBlock : function.blocks.find? (·.id == target.block) = none := by
        simpa only [← related.block] using foundBlock
      simp only [execute, foundFunction, targetFunction, foundBlock, targetBlock, OutcomeRelated]
      rfl
    | some block =>
      have functionName : function.name = source.function := by simpa using List.find?_some foundFunction
      have targetFunction : program.functions.find? (·.name == target.function) = some function := by
        simpa only [← related.function] using foundFunction
      have targetBlock : function.blocks.find? (·.id == target.block) = some block := by
        simpa only [← related.block] using foundBlock
      cases foundInstruction : block.insts[source.pc]? with
      | none =>
        by_cases atTerminator : source.pc = block.insts.length
        · exact terminator_execute_related program plan accepted sourceState targetState states
            source target related function block foundFunction foundBlock foundInstruction atTerminator
            sourceSafe targetSafe
        · have targetInstruction : block.insts[target.pc]? = none := by
            simpa only [← related.pc] using foundInstruction
          simp only [execute, foundFunction, targetFunction, foundBlock, targetBlock,
            foundInstruction, targetInstruction]
          simp only [← related.pc, bne_iff_ne, if_pos atTerminator,
            MonadExcept.throw, throwThe, MonadExceptOf.throw, Bind.bind, Except.bind, OutcomeRelated]
      | some inst =>
        have checked := checkStructure_instruction program plan accepted function
          (List.mem_of_find?_eq_some foundFunction) block (List.mem_of_find?_eq_some foundBlock)
          inst (List.mem_of_getElem? foundInstruction)
        by_cases isCall : ∃ symbol args results, inst = .bl symbol args results
        · obtain ⟨symbol, args, results, rfl⟩ := isCall
          obtain ⟨callee, entry, foundCallee, foundEntry, parameters⟩ :=
            checked_call_target program plan function symbol args checked
          exact call_execute_related program plan sourceState targetState states source target related
            function functionName block symbol args results foundFunction foundBlock foundInstruction
            callee foundCallee entry foundEntry parameters sourceSafe targetSafe
        · have notCall : ∀ symbol args results, inst ≠ .bl symbol args results := by
            intro symbol args results same
            exact isCall ⟨symbol, args, results, same⟩
          have closed := noncall_checked program plan function inst notCall checked
          have evaluations : EvaluationRelated (needed plan source.function)
              (evalInstruction source.owner inst source.regs sourceState.memory)
              (evalInstruction source.owner inst target.regs sourceState.memory) := by
            rcases closed with ⟨pure, outside⟩ | used
            · obtain ⟨sourceRegs, sourceEval⟩ := reachable_pure_success .ssa program type sourceEntry
                sourceState sourceProgress sourceReachable source function block inst sourceRunning
                foundFunction foundBlock foundInstruction pure
              have targetInstruction : block.insts[target.pc]? = some inst := by
                simpa only [← related.pc] using foundInstruction
              obtain ⟨targetRegs, targetEval⟩ := reachable_pure_success .ssa program type targetEntry
                targetState targetProgress targetReachable target function block inst targetRunning
                targetFunction targetBlock targetInstruction pure
              have targetEval' : evalInstruction source.owner inst target.regs sourceState.memory =
                  .ok (targetRegs, sourceState.memory) := by
                simpa only [← related.owner, ← states.memory] using targetEval
              rw [sourceEval, targetEval']
              exact ⟨rfl, instruction_successes_dead source.owner inst (needed plan source.function)
                source.regs target.regs sourceState.memory sourceRegs targetRegs sourceEval targetEval'
                (by simpa only [functionName] using outside) related.regs⟩
            · exact instruction_reads_noninterference source.owner inst (needed plan source.function)
                source.regs target.regs sourceState.memory (by simpa only [functionName] using used) related.regs
          exact noncall_execute_related program plan sourceState targetState states source target related
            function block inst foundFunction foundBlock foundInstruction notCall evaluations

theorem ControlRelated.running_inv (plan : Plan) (source : Activation) (control : Control)
    (related : ControlRelated plan (.running source) control) :
    ∃ target, control = .running target ∧ ActivationRelated plan source target := by
  cases related with | running acts => exact ⟨_, rfl, acts⟩

theorem ControlRelated.halted_inv (plan : Plan) (result : Except Core.Source.Fault (List Value))
    (control : Control) (related : ControlRelated plan (.halted result) control) :
    control = .halted result := by
  cases related
  rfl

theorem step_related_reachable (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry sourceState targetState targetAfter : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (sourceReachable : Core.Transition.Reaches (step .ssa program) sourceEntry sourceState)
    (targetReachable : Core.Transition.Reaches (step .ssa program) targetEntry targetState)
    (states : StateRelated plan sourceState targetState)
    (targetStep : step .ssa program targetState = some targetAfter) :
    ∃ sourceAfter, step .ssa program sourceState = some sourceAfter ∧
      StateRelated plan sourceAfter targetAfter := by
  have controls := states.control
  cases sourceRunning : sourceState.control with
  | halted result =>
    rw [sourceRunning] at controls
    have targetHalted := ControlRelated.halted_inv plan result targetState.control controls
    simp only [step, targetHalted] at targetStep
    contradiction
  | running source =>
    rw [sourceRunning] at controls
    obtain ⟨target, targetRunning, related⟩ := ControlRelated.running_inv plan source targetState.control controls
    have executes := execute_related_reachable program plan accepted type sourceEntry targetEntry sourceState
      targetState sourceProgress targetProgress sourceReachable targetReachable states source target related
      sourceRunning targetRunning
    cases sourceExecute : execute .ssa program sourceState source with
    | error fault =>
      cases targetExecute : execute .ssa program targetState target with
      | ok output => simp [OutcomeRelated, sourceExecute, targetExecute] at executes
      | error targetFault =>
        have same : fault = targetFault := by
          simpa only [OutcomeRelated, sourceExecute, targetExecute] using executes
        simp only [step, targetRunning, targetExecute, Option.some.injEq] at targetStep
        subst targetAfter
        refine ⟨{sourceState with control := .halted (.error fault)}, ?_, ?_⟩
        · simp only [step, sourceRunning, sourceExecute]
        · rw [same]
          exact ⟨.halted _, states.stack, states.nextFrame, states.memory⟩
    | ok sourceAfter =>
      cases targetExecute : execute .ssa program targetState target with
      | error fault => simp [OutcomeRelated, sourceExecute, targetExecute] at executes
      | ok targetNext =>
        simp only [step, targetRunning, targetExecute, Option.some.injEq] at targetStep
        subst targetAfter
        refine ⟨sourceAfter, ?_, ?_⟩
        · simp only [step, sourceRunning, sourceExecute]
        · simpa only [OutcomeRelated, sourceExecute, targetExecute] using executes

end EvoIR.Machine.Demand
