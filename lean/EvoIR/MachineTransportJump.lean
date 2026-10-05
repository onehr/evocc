import EvoIR.MachineTransportCalls

namespace EvoIR.Machine.Transport

theorem terminatorEdge (source target : Program) (type : Core.Source.IntType) (entry : State)
    (progress : ObservedProgress .ssa source type entry)
    (s t : State) (a b : Activation) (ctx : Context source target) (active : Active ctx a b)
    (related : StateRelated source target type entry s t)
    (runningSource : s.control = .running a) (runningTarget : t.control = .running b)
    (sourceEnd : a.pc = ctx.sourceBlock.insts.length) (targetEnd : b.pc = ctx.targetBlock.insts.length)
    (envFinal : active.env = ctx.final) (original actual : Nat × List VReg)
    (sourceEffect : Demand.evalTerminator a.regs ctx.sourceBlock.term = .ok (.edge original.1 original.2))
    (targetEffect : Demand.evalTerminator b.regs ctx.targetBlock.term = .ok (.edge actual.1 actual.2))
    (paired : (original, actual) ∈ ctx.sourceBlock.term.edges.zip ctx.targetBlock.term.edges) :
    ∃ s' t', step .ssa source s = some s' ∧
      Relation.TransGen (Core.Transition.Predecessor (step .postSSA target)) t' t ∧
      StateRelated source target type entry s' t' := by
  have edgeAccepted := ctx.edges (original, actual) paired
  obtain ⟨destination, plan, transported, destinationAt, planAt, arity, cleared, routed, parameters⟩ :=
    edgeValid_data ctx.sourceFunction ctx.targetFunction ctx.plans ctx.final original actual edgeAccepted
  have actualDestinationAt : ctx.sourceFunction.blocks.find? (·.id == original.1) = some destination := destinationAt
  have sourceNoInst : ctx.sourceBlock.insts[a.pc]? = none := by simp [sourceEnd]
  have targetNoInst : ctx.targetBlock.insts[b.pc]? = none := by simp [targetEnd]
  have sourceExecuted := execute_terminator .ssa source s a ctx.sourceFunction ctx.sourceBlock
    active.foundSourceFunction active.foundSourceBlock sourceNoInst sourceEnd
  rw [sourceEffect] at sourceExecuted
  simp only [Bind.bind, Except.bind, finishTerminator] at sourceExecuted
  have safe := Demand.reachable_execute_safe source type entry s progress related.reached a runningSource
  have jumpSafe : ∀ message, jump .ssa ctx.sourceFunction a original.1 original.2 ≠ .error (.malformed message) := by
    intro message failed
    apply safe message
    rw [sourceExecuted, failed]
    rfl
  obtain ⟨entered, jumped⟩ := Demand.success_of_not_malformed _
    (Demand.jump_error_malformed ctx.sourceFunction a original.1 original.2) jumpSafe
  cases capturedAt : original.2.mapM (read a.regs) with
  | error fault => simp [jump, actualDestinationAt, capturedAt, Bind.bind, Except.bind] at jumped
  | ok captured =>
    cases bound : bind [] destination.args captured with
    | error fault => simp [jump, actualDestinationAt, capturedAt, bound, Bind.bind, Except.bind] at jumped
    | ok sourceRegs =>
      have enteredShape : {a with block := original.1, pc := 0, regs := sourceRegs} = entered := by
        simpa [jump, actualDestinationAt, capturedAt, bound, Bind.bind, Except.bind, Pure.pure, Except.pure] using jumped
      subst entered
      obtain ⟨nextContext, sourceFunction, targetFunction, plans, sourceBlock⟩ :=
        contextInFunction ctx original.1 destination destinationAt
      have nextId : nextContext.sourceBlock.id = original.1 := by
        rw [sourceBlock]
        simpa [findBlock] using List.find?_some destinationAt
      obtain ⟨startingBlock, startingAt⟩ := routeOrigins_found ctx.sourceFunction ctx.targetFunction ctx.plans
        original.1 (ctx.targetFunction.blocks.length+1) actual.1 ctx.final.target transported routed
      have actualStartingAt : ctx.targetFunction.blocks.find? (·.id == actual.1) = some startingBlock := startingAt
      have targetJump : jump .postSSA ctx.targetFunction b actual.1 actual.2 =
          .ok {b with block := actual.1, pc := 0} := by
        simp [jump, cleared, actualStartingAt, Bind.bind, Except.bind, Pure.pure, Except.pure]
      have targetExecuted := execute_terminator .postSSA target t b ctx.targetFunction ctx.targetBlock
        active.foundTargetFunction active.foundTargetBlock targetNoInst targetEnd
      rw [targetEffect] at targetExecuted
      simp only [Bind.bind, Except.bind, finishTerminator] at targetExecuted
      let sourceNext := {s with control := .running {a with block := original.1, pc := 0, regs := sourceRegs}}
      let targetJumpState := {t with control := .running {b with block := actual.1, pc := 0}}
      have sourceStep : step .ssa source s = some sourceNext := by
        simp only [sourceNext, step, runningSource, sourceExecuted, jumped, Bind.bind, Except.bind,
          finishTerminator, Demand.resume, Pure.pure, Except.pure]
      have targetStep : step .postSSA target t = some targetJumpState := by
        simp only [targetJumpState, step, runningTarget, targetExecuted, targetJump, Bind.bind, Except.bind,
          finishTerminator, Demand.resume, Pure.pure, Except.pure]
      have sourceReal : Realizes ctx.final.source active.valuation a.regs := by simpa only [← envFinal] using active.sourceReal
      have targetReal : Realizes ctx.final.target active.valuation b.regs := by simpa only [← envFinal] using active.targetReal
      obtain ⟨routedBlock, routedPlan, targetRegs, targetAt, routedPlanAt, _, path, transportedReal⟩ :=
        routeOrigins_steps ctx.sourceFunction ctx.targetFunction ctx.plans original.1
          (ctx.targetFunction.blocks.length+1) actual.1 ctx.final.target transported routed
          target targetJumpState {b with block := actual.1, pc := 0} rfl active.foundTargetFunction rfl rfl
          active.valuation targetReal active.normalized
      have nextTargetAt : findBlock ctx.targetFunction original.1 = some nextContext.targetBlock := by
        simpa only [targetFunction, nextId] using nextContext.targetBlockAt
      have blockSame : routedBlock = nextContext.targetBlock := Option.some.inj (targetAt.symm.trans nextTargetAt)
      have nextPlanAt : findPlan ctx.plans original.1 = some nextContext.plan := by
        simpa only [plans, nextId] using nextContext.planAt
      have planSame : routedPlan = nextContext.plan := Option.some.inj (routedPlanAt.symm.trans nextPlanAt)
      have originalPlanSame : plan = nextContext.plan := Option.some.inj (planAt.symm.trans nextPlanAt)
      obtain ⟨_, _, _, nextBindings, _⟩ := checkBody_data nextContext.sourceBlock nextContext.targetBlock
        nextContext.plan nextContext.plans.needed nextContext.final nextContext.body
      have bindings : bindingsValid destination plan ctx.plans.needed = true := by
        simpa only [sourceBlock, plans, ← originalPlanSame] using nextBindings
      have ready := edge_entryReady destination plan ctx.plans.needed original.2 captured a.regs targetRegs
        sourceRegs ctx.final.source transported active.valuation arity bindings parameters capturedAt bound
        sourceReal transportedReal active.normalized
      have nextActive := activeEntry nextContext {a with block := original.1, pc := 0, regs := sourceRegs}
        {b with block := original.1, pc := nextContext.plan.prefixLength, regs := targetRegs}
        (by simpa only [sourceFunction] using active.sourceName)
        (by simpa only [sourceFunction] using active.targetName)
        nextId.symm nextId.symm active.owner active.incoming rfl rfl
        (by simpa only [sourceBlock, ← originalPlanSame] using ready)
      let targetNext := {t with control := .running {b with block := original.1, pc := nextContext.plan.prefixLength, regs := targetRegs}}
      have targetPath : Core.Transition.Reaches (step .postSSA target) targetJumpState targetNext := by
        simpa only [targetJumpState, targetNext, planSame] using path
      refine ⟨sourceNext, targetNext, sourceStep, targetPath.after_step targetStep, ?_⟩
      exact ⟨related.reached.trans (.next sourceStep (.refl _)), related.stack, related.counter, related.memory,
        _, _, rfl, rfl, nextContext, nextActive⟩

end EvoIR.Machine.Transport
