import EvoIR.MachineTransportJump

namespace EvoIR.Machine.Transport

theorem terminatorReturn (source target : Program) (type : Core.Source.IntType) (entry : State)
    (progress : ObservedProgress .ssa source type entry)
    (s t : State) (a b : Activation) (ctx : Context source target) (active : Active ctx a b)
    (related : StateRelated source target type entry s t)
    (runningSource : s.control = .running a) (runningTarget : t.control = .running b)
    (sourceEnd : a.pc = ctx.sourceBlock.insts.length) (targetEnd : b.pc = ctx.targetBlock.insts.length)
    (values : List Value)
    (sourceEffect : Demand.evalTerminator a.regs ctx.sourceBlock.term = .ok (.returned values))
    (targetEffect : Demand.evalTerminator b.regs ctx.targetBlock.term = .ok (.returned values)) :
    ∃ s' t', step .ssa source s = some s' ∧ step .postSSA target t = some t' ∧
      ((s'.control = .halted (.ok values) ∧ t'.control = .halted (.ok values)) ∨
        StateRelated source target type entry s' t') := by
  have sourceNoInst : ctx.sourceBlock.insts[a.pc]? = none := by simp [sourceEnd]
  have targetNoInst : ctx.targetBlock.insts[b.pc]? = none := by simp [targetEnd]
  have sourceExecuted := execute_terminator .ssa source s a ctx.sourceFunction ctx.sourceBlock
    active.foundSourceFunction active.foundSourceBlock sourceNoInst sourceEnd
  have targetExecuted := execute_terminator .postSSA target t b ctx.targetFunction ctx.targetBlock
    active.foundTargetFunction active.foundTargetBlock targetNoInst targetEnd
  rw [sourceEffect] at sourceExecuted
  rw [targetEffect] at targetExecuted
  simp only [Bind.bind, Except.bind, finishTerminator] at sourceExecuted targetExecuted
  have safe := Demand.reachable_execute_safe source type entry s progress related.reached a runningSource
  have sameMemory : s.memory.endFrame a.owner = t.memory.endFrame b.owner := by
    rw [related.memory, active.owner]
  have aligned := related.stack
  cases sourceStack : s.stack <;> cases targetStack : t.stack
  case nil.nil =>
    let sourceNext := {s with control := .halted (.ok values), memory := s.memory.endFrame a.owner}
    let targetNext := {t with control := .halted (.ok values), memory := t.memory.endFrame b.owner}
    refine ⟨sourceNext, targetNext, ?_, ?_, Or.inl ⟨rfl, rfl⟩⟩
    · simp only [sourceNext, step, runningSource, sourceExecuted, Demand.returnValues, sourceStack,
        Pure.pure, Except.pure]
    · simp only [targetNext, step, runningTarget, targetExecuted, Demand.returnValues, targetStack,
        Pure.pure, Except.pure]
  case nil.cons => simp only [FramesRelated, sourceStack, targetStack] at aligned; cases aligned
  case cons.nil => simp only [FramesRelated, sourceStack, targetStack] at aligned; cases aligned
  case cons.cons sourceFrame sourceRest targetFrame targetRest =>
    have paired : Demand.Aligned (fun before after => ∃ context : Context source target,
        Nonempty (Suspended context before after)) (sourceFrame :: sourceRest) (targetFrame :: targetRest) := by
      simpa only [FramesRelated, sourceStack, targetStack] using aligned
    cases paired with
    | cons frames remaining =>
      obtain ⟨callerContext, ⟨frame⟩⟩ := frames
      have bindSafe : ∀ message, bind sourceFrame.caller.regs sourceFrame.results values ≠ .error (.malformed message) := by
        intro message failed
        apply safe message
        rw [sourceExecuted]
        simp only [Demand.returnValues, sourceStack, failed, Bind.bind, Except.bind]
      obtain ⟨sourceRegs, sourceBound⟩ := Demand.success_of_not_malformed _
        (Demand.bind_error_malformed sourceFrame.caller.regs sourceFrame.results values) bindSafe
      obtain ⟨targetRegs, targetBound, callerActive⟩ := frame.bound values sourceRegs sourceBound
      let sourceNext := {s with control := .running {sourceFrame.caller with regs := sourceRegs}, stack := sourceRest, memory := s.memory.endFrame a.owner}
      let targetNext := {t with control := .running {targetFrame.caller with regs := targetRegs}, stack := targetRest, memory := t.memory.endFrame b.owner}
      have sourceStep : step .ssa source s = some sourceNext := by
        simp only [sourceNext, step, runningSource, sourceExecuted, Demand.returnValues, sourceStack,
          sourceBound, Bind.bind, Except.bind, Pure.pure, Except.pure]
      have targetStep : step .postSSA target t = some targetNext := by
        simp only [targetNext, step, runningTarget, targetExecuted, Demand.returnValues, targetStack,
          targetBound, Bind.bind, Except.bind, Pure.pure, Except.pure]
      refine ⟨sourceNext, targetNext, sourceStep, targetStep, Or.inr ?_⟩
      exact ⟨related.reached.trans (.next sourceStep (.refl _)), remaining, related.counter, sameMemory,
        _, _, rfl, rfl, callerContext, callerActive⟩

theorem terminator_error (source target : Program) (s t : State) (a b : Activation)
    (ctx : Context source target) (active : Active ctx a b)
    (runningSource : s.control = .running a) (runningTarget : t.control = .running b)
    (sourceEnd : a.pc = ctx.sourceBlock.insts.length) (targetEnd : b.pc = ctx.targetBlock.insts.length)
    (fault : Core.Source.Fault)
    (sourceEffect : Demand.evalTerminator a.regs ctx.sourceBlock.term = .error fault)
    (targetEffect : Demand.evalTerminator b.regs ctx.targetBlock.term = .error fault) :
    step .ssa source s = some {s with control := .halted (.error fault)} ∧
      step .postSSA target t = some {t with control := .halted (.error fault)} := by
  have sourceNoInst : ctx.sourceBlock.insts[a.pc]? = none := by simp [sourceEnd]
  have targetNoInst : ctx.targetBlock.insts[b.pc]? = none := by simp [targetEnd]
  have sourceExecuted := execute_terminator .ssa source s a ctx.sourceFunction ctx.sourceBlock
    active.foundSourceFunction active.foundSourceBlock sourceNoInst sourceEnd
  have targetExecuted := execute_terminator .postSSA target t b ctx.targetFunction ctx.targetBlock
    active.foundTargetFunction active.foundTargetBlock targetNoInst targetEnd
  simp only [sourceEffect, Bind.bind, Except.bind] at sourceExecuted
  simp only [targetEffect, Bind.bind, Except.bind] at targetExecuted
  exact ⟨by simp only [step, runningSource, sourceExecuted], by simp only [step, runningTarget, targetExecuted]⟩

end EvoIR.Machine.Transport
