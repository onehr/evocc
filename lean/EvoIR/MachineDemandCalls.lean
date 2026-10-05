import EvoIR.MachineDemandExecution

/-! Actual SSA call and edge bindings under checked demand closure. -/
namespace EvoIR.Machine.Demand

theorem enter_error_malformed (function : Function) (values : List Value) (owner : Nat)
    (fault : Core.Source.Fault) (failed : enter function values owner = .error fault) :
    ∃ message, fault = .malformed message := by
  cases found : function.blocks.find? (·.id == 0) with
  | none =>
    simp only [enter, found, MonadExcept.throw, throwThe, MonadExceptOf.throw] at failed
    exact ⟨_, (Except.error.inj failed).symm⟩
  | some block =>
    cases bound : bind [] block.args values with
    | error fault' =>
      simp only [enter, found, bound, Bind.bind, Except.bind, Except.error.injEq] at failed
      subst fault
      exact bind_error_malformed [] block.args values fault' bound
    | ok regs =>
      simp only [enter, found, bound, Bind.bind, Except.bind, Pure.pure, Except.pure] at failed
      contradiction

theorem jump_error_malformed (function : Function) (activation : Activation)
    (destination : Nat) (args : List VReg) (fault : Core.Source.Fault)
    (failed : jump .ssa function activation destination args = .error fault) :
    ∃ message, fault = .malformed message := by
  cases found : function.blocks.find? (·.id == destination) with
  | none =>
    simp only [jump, found, MonadExcept.throw, throwThe, MonadExceptOf.throw] at failed
    exact ⟨_, (Except.error.inj failed).symm⟩
  | some block =>
    simp only [jump, found] at failed
    cases reads : args.mapM (read activation.regs) with
    | error fault' =>
      simp only [reads, Bind.bind, Except.bind, Except.error.injEq] at failed
      subst fault
      exact reads_error_malformed activation.regs args fault' reads
    | ok values =>
      simp only [reads, Bind.bind, Except.bind] at failed
      cases bound : bind [] block.args values with
      | error fault' =>
        simp only [bound, Except.error.injEq] at failed
        subst fault
        exact bind_error_malformed _ block.args values fault' bound
      | ok regs =>
        simp only [bound, Bind.bind, Except.bind, Pure.pure, Except.pure] at failed
        contradiction

theorem success_of_not_malformed (result : Except Core.Source.Fault α)
    (failures : ∀ fault, result = .error fault → ∃ message, fault = .malformed message)
    (safe : ∀ message, result ≠ .error (.malformed message)) :
    ∃ value, result = .ok value := by
  cases result with
  | ok value => exact ⟨value, rfl⟩
  | error fault =>
    obtain ⟨message, rfl⟩ := failures fault rfl
    exact False.elim (safe message rfl)

theorem enter_success_related (plan : Plan) (function : Function) (block : Block)
    (found : function.blocks.find? (·.id == 0) = some block) (owner : Nat)
    (sourceValues targetValues : List Value)
    (values : BindingAgreement (needed plan function.name)
      (block.args.zip sourceValues) (block.args.zip targetValues))
    (source target : Activation)
    (sourceEntered : enter function sourceValues owner = .ok source)
    (targetEntered : enter function targetValues owner = .ok target) :
    ActivationRelated plan source target := by
  simp only [enter, found] at sourceEntered targetEntered
  cases sourceBound : bind [] block.args sourceValues <;>
    cases targetBound : bind [] block.args targetValues <;>
    simp only [sourceBound, targetBound, Bind.bind, Except.bind,
      Pure.pure, Except.pure, Except.ok.injEq] at sourceEntered targetEntered
  all_goals try contradiction
  next sourceRegs targetRegs =>
    subst source
    subst target
    exact ⟨rfl, rfl, rfl, rfl, bind_success_agree _ [] [] (by intro _ _; rfl)
      block.args sourceValues targetValues values sourceRegs targetRegs sourceBound targetBound⟩

theorem jump_success_related (plan : Plan) (function : Function)
    (source target : Activation) (related : ActivationRelated plan source target)
    (functionName : function.name = source.function) (destination : Nat) (args : List VReg)
    (block : Block) (found : function.blocks.find? (·.id == destination) = some block)
    (closed : checkParameters (needed plan function.name) (needed plan function.name) block.args args = true)
    (sourceAfter targetAfter : Activation)
    (sourceJump : jump .ssa function source destination args = .ok sourceAfter)
    (targetJump : jump .ssa function target destination args = .ok targetAfter) :
    ActivationRelated plan sourceAfter targetAfter := by
  simp only [jump, found] at sourceJump targetJump
  cases sourceRead : args.mapM (read source.regs) <;>
    cases targetRead : args.mapM (read target.regs) <;>
    simp only [sourceRead, targetRead, Bind.bind, Except.bind] at sourceJump targetJump
  all_goals try contradiction
  next sourceValues targetValues =>
    have base : RegistersAgree (needed plan function.name) source.regs target.regs := by
      simpa only [functionName] using related.regs
    have values := read_parameter_agreement _ _ source.regs target.regs base block.args args
      (checkParameters_sound _ _ block.args args closed) sourceValues targetValues sourceRead targetRead
    change (do let regs ← bind [] block.args sourceValues
               pure {source with block := destination, pc := 0, regs}) = _ at sourceJump
    change (do let regs ← bind [] block.args targetValues
               pure {target with block := destination, pc := 0, regs}) = _ at targetJump
    cases sourceBound : bind [] block.args sourceValues <;>
      cases targetBound : bind [] block.args targetValues <;>
      simp only [sourceBound, targetBound, Bind.bind, Except.bind, Pure.pure,
        Except.pure, Except.ok.injEq] at sourceJump targetJump
    all_goals try contradiction
    next sourceRegs targetRegs =>
      subst sourceAfter
      subst targetAfter
      have banks := bind_success_agree _ [] [] (by intro _ _; rfl) block.args
        sourceValues targetValues values sourceRegs targetRegs sourceBound targetBound
      exact ⟨related.function, rfl, rfl, related.owner, by simpa only [functionName] using banks⟩

theorem jump_safe_related (plan : Plan) (function : Function) (source target : Activation)
    (related : ActivationRelated plan source target) (functionName : function.name = source.function)
    (destination : Nat) (args : List VReg) (block : Block)
    (found : function.blocks.find? (·.id == destination) = some block)
    (closed : checkParameters (needed plan function.name) (needed plan function.name) block.args args = true)
    (sourceSafe : ∀ message, jump .ssa function source destination args ≠ .error (.malformed message))
    (targetSafe : ∀ message, jump .ssa function target destination args ≠ .error (.malformed message)) :
    OutcomeRelated (ActivationRelated plan) (jump .ssa function source destination args)
      (jump .ssa function target destination args) := by
  obtain ⟨sourceAfter, sourceJump⟩ := success_of_not_malformed _
    (jump_error_malformed function source destination args) sourceSafe
  obtain ⟨targetAfter, targetJump⟩ := success_of_not_malformed _
    (jump_error_malformed function target destination args) targetSafe
  rw [sourceJump, targetJump]
  exact jump_success_related plan function source target related functionName destination args
    block found closed sourceAfter targetAfter sourceJump targetJump

theorem call_reads_success (program : Program) (state : State) (activation : Activation)
    (function : Function) (block : Block) (symbol : String) (args results : List VReg)
    (foundFunction : program.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (foundInstruction : block.insts[activation.pc]? = some (.bl symbol args results))
    (safe : ∀ message, execute .ssa program state activation ≠ .error (.malformed message)) :
    ∃ values, args.mapM (read activation.regs) = .ok values := by
  apply success_of_not_malformed _ (reads_error_malformed activation.regs args)
  intro message failed
  apply safe message
  simp only [execute, foundFunction, foundBlock, foundInstruction, failed,
    Bind.bind, Except.bind]

theorem call_enter_success (program : Program) (state : State) (activation : Activation)
    (function : Function) (block : Block) (symbol : String) (args results : List VReg)
    (values : List Value) (callee : Function)
    (foundFunction : program.functions.find? (·.name == activation.function) = some function)
    (foundBlock : function.blocks.find? (·.id == activation.block) = some block)
    (foundInstruction : block.insts[activation.pc]? = some (.bl symbol args results))
    (readValues : args.mapM (read activation.regs) = .ok values)
    (foundCallee : program.functions.find? (·.name == symbol) = some callee)
    (safe : ∀ message, execute .ssa program state activation ≠ .error (.malformed message)) :
    ∃ entered, enter callee values state.nextFrame = .ok entered := by
  apply success_of_not_malformed _ (enter_error_malformed callee values state.nextFrame)
  intro message failed
  apply safe message
  simp only [execute, foundFunction, foundBlock, foundInstruction, readValues,
    foundCallee, Bind.bind, Except.bind, failed]

theorem call_execute_related (program : Program) (plan : Plan)
    (sourceState targetState : State) (states : StateRelated plan sourceState targetState)
    (source target : Activation) (related : ActivationRelated plan source target)
    (function : Function) (functionName : function.name = source.function)
    (block : Block) (symbol : String) (args results : List VReg)
    (foundFunction : program.functions.find? (·.name == source.function) = some function)
    (foundBlock : function.blocks.find? (·.id == source.block) = some block)
    (foundInstruction : block.insts[source.pc]? = some (.bl symbol args results))
    (callee : Function) (foundCallee : program.functions.find? (·.name == symbol) = some callee)
    (entry : Block) (foundEntry : callee.blocks.find? (·.id == 0) = some entry)
    (closed : checkParameters (needed plan callee.name) (needed plan function.name) entry.args args = true)
    (sourceSafe : ∀ message, execute .ssa program sourceState source ≠ .error (.malformed message))
    (targetSafe : ∀ message, execute .ssa program targetState target ≠ .error (.malformed message)) :
    OutcomeRelated (StateRelated plan) (execute .ssa program sourceState source)
      (execute .ssa program targetState target) := by
  have targetFunction : program.functions.find? (·.name == target.function) = some function := by
    simpa only [← related.function] using foundFunction
  have targetBlock : function.blocks.find? (·.id == target.block) = some block := by
    simpa only [← related.block] using foundBlock
  have targetInstruction : block.insts[target.pc]? = some (.bl symbol args results) := by
    simpa only [← related.pc] using foundInstruction
  obtain ⟨sourceValues, sourceRead⟩ := call_reads_success program sourceState source function
    block symbol args results foundFunction foundBlock foundInstruction sourceSafe
  obtain ⟨targetValues, targetRead⟩ := call_reads_success program targetState target function
    block symbol args results targetFunction targetBlock targetInstruction targetSafe
  have banks : RegistersAgree (needed plan function.name) source.regs target.regs := by
    simpa only [functionName] using related.regs
  have values := read_parameter_agreement (needed plan callee.name) (needed plan function.name)
    source.regs target.regs banks entry.args args (checkParameters_sound _ _ entry.args args closed)
    sourceValues targetValues sourceRead targetRead
  obtain ⟨sourceEntered, sourceEnter⟩ := call_enter_success program sourceState source function
    block symbol args results sourceValues callee foundFunction foundBlock foundInstruction
    sourceRead foundCallee sourceSafe
  obtain ⟨targetEntered, targetEnter⟩ := call_enter_success program targetState target function
    block symbol args results targetValues callee targetFunction targetBlock targetInstruction
    targetRead foundCallee targetSafe
  have sameOwnerTarget : enter callee targetValues sourceState.nextFrame = .ok targetEntered := by
    simpa only [states.nextFrame] using targetEnter
  have entered := enter_success_related plan callee entry foundEntry sourceState.nextFrame
    sourceValues targetValues values sourceEntered targetEntered sourceEnter sameOwnerTarget
  rw [execute_bl .ssa program sourceState source function block symbol args results sourceValues
    callee sourceEntered foundFunction foundBlock foundInstruction sourceRead foundCallee sourceEnter,
    execute_bl .ssa program targetState target function block symbol args results targetValues
      callee targetEntered targetFunction targetBlock targetInstruction targetRead foundCallee targetEnter]
  exact ⟨.running entered, .cons ⟨ActivationRelated.saved plan source target related, rfl⟩
    states.stack, congrArg (·+1) states.nextFrame, states.memory⟩

end EvoIR.Machine.Demand
