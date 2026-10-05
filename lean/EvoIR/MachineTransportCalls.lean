import EvoIR.MachineTransportNoncall

namespace EvoIR.Machine.Transport

theorem retained_call (source target : Program) (annotations : ProgramPlan)
    (acceptedProgram : checkProgram source target annotations = true)
    (type : Core.Source.IntType) (entry : State) (progress : ObservedProgress .ssa source type entry)
    (s t : State) (a b : Activation) (ctx : Context source target) (active : Active ctx a b)
    (related : StateRelated source target type entry s t)
    (runningSource : s.control = .running a) (runningTarget : t.control = .running b)
    (symbol : String) (args results : List VReg) (candidate : Instruction)
    (rest : List Instruction) (positions : List (Option Nat))
    (suffix : ctx.sourceBlock.insts.drop a.pc = .bl symbol args results :: rest)
    (candidateAt : ctx.targetBlock.insts[b.pc]? = some candidate)
    (matched : matchOrigins (.bl symbol args results) candidate active.env.source active.env.target = true)
    (checked : checkBodyAux ctx.sourceBlock.id ctx.plans.needed ctx.targetBlock.insts (a.pc+1) (b.pc+1)
      rest positions
      {source := resultOrigins ctx.sourceBlock.id a.pc 0 results active.env.source,
       target := resultOrigins ctx.sourceBlock.id a.pc 0 candidate.defs active.env.target,
       cuts := ⟨a.pc+1, b.pc+1, resultOrigins ctx.sourceBlock.id a.pc 0 results active.env.source,
         resultOrigins ctx.sourceBlock.id a.pc 0 candidate.defs active.env.target⟩ :: active.env.cuts} = some ctx.final) :
    ∃ s' t', step .ssa source s = some s' ∧ step .postSSA target t = some t' ∧
      StateRelated source target type entry s' t' := by
  have sourceAt : ctx.sourceBlock.insts[a.pc]? = some (.bl symbol args results) := by
    have first := congrArg (fun list : List Instruction => list[0]?) suffix
    simpa only [List.getElem?_drop, Nat.add_zero, List.getElem?_cons_zero] using first
  have sourceInside := (List.getElem?_eq_some_iff.mp sourceAt).1
  have targetInside := (List.getElem?_eq_some_iff.mp candidateAt).1
  have safe := Demand.reachable_execute_safe source type entry s progress related.reached a runningSource
  obtain ⟨values, sourceReads⟩ := Demand.call_reads_success source s a ctx.sourceFunction ctx.sourceBlock
    symbol args results active.foundSourceFunction active.foundSourceBlock sourceAt safe
  obtain ⟨candidateShape, equalReads⟩ := matchOrigins_bl_args symbol args results candidate
    active.env.source active.env.target matched active.valuation a.regs b.regs active.sourceReal active.targetReal
  let targetArgs := args.map (rename (useCorrespondence (.bl symbol args results) candidate))
  have targetAt : ctx.targetBlock.insts[b.pc]? = some (.bl symbol targetArgs results) := by
    exact candidateAt.trans (congrArg some candidateShape)
  have candidateDefs : candidate.defs = results := by
    simpa only [Instruction.defs] using checkRetained_defs _ _ _
      (matchOrigins_retained _ _ _ _ matched)
  have targetReads : targetArgs.mapM (read b.regs) = .ok values := equalReads.symm.trans sourceReads
  cases calleeAt : source.functions.find? (·.name == symbol) with
  | none =>
    exfalso
    apply safe "MIR external call"
    simp only [execute, active.foundSourceFunction, active.foundSourceBlock, sourceAt,
      sourceReads, calleeAt, Bind.bind, Except.bind, MonadExcept.throw, throwThe, MonadExceptOf.throw]
  | some callee =>
    obtain ⟨entered, sourceEntered⟩ := Demand.call_enter_success source s a ctx.sourceFunction ctx.sourceBlock
      symbol args results values callee active.foundSourceFunction active.foundSourceBlock sourceAt
      sourceReads calleeAt safe
    cases entryAt : findBlock callee 0 with
    | none =>
      change callee.blocks.find? (·.id == 0) = none at entryAt
      simp [enter, entryAt, Bind.bind, Except.bind, MonadExcept.throw,
        throwThe, MonadExceptOf.throw] at sourceEntered
    | some entryBlock =>
      obtain ⟨calleeContext, calleeFunction, calleeBlock⟩ := contextForBlock source target annotations acceptedProgram
        symbol callee 0 entryBlock calleeAt entryAt
      have calleeId : calleeContext.sourceBlock.id = 0 := by
        rw [calleeBlock]
        simpa [findBlock] using List.find?_some entryAt
      have enters : enter calleeContext.sourceFunction values s.nextFrame = .ok entered := by
        simpa only [calleeFunction] using sourceEntered
      obtain ⟨targetEntered, nextActive⟩ := activeEntry_sameBank calleeContext values s.nextFrame entered calleeId enters
      have targetCallee : target.functions.find? (·.name == symbol) = some calleeContext.targetFunction := by
        have name : callee.name = symbol := by simpa using List.find?_some calleeAt
        simpa only [calleeFunction, name] using calleeContext.targetFunctionAt
      have targetEnters : enter calleeContext.targetFunction values t.nextFrame = .ok entered := by
        simpa only [← related.counter] using targetEntered
      let sourceFrame : ReturnFrame := ⟨{a with pc := a.pc+1}, results⟩
      let targetFrame : ReturnFrame := ⟨{b with pc := b.pc+1}, results⟩
      let saved : Suspended ctx sourceFrame targetFrame := {
        sourceName := active.sourceName, targetName := active.targetName,
        sourceId := active.sourceId, targetId := active.targetId, owner := active.owner,
        incoming := active.incoming, results := rfl, callPC := a.pc, sourcePC := rfl,
        positions, env := active.env, valuation := active.valuation,
        sourceBound := by dsimp [sourceFrame]; omega,
        targetBound := by dsimp [targetFrame]; omega,
        accepted := by
          simpa only [sourceFrame, targetFrame, List.drop_add_one_eq_tail_drop,
            suffix, List.tail_cons, candidateDefs] using checked,
        sourceReal := active.sourceReal, targetReal := active.targetReal,
        sourceBefore := active.sourceBefore, targetBefore := active.targetBefore, normalized := active.normalized}
      let sourceNext := {s with control := .running entered, stack := sourceFrame :: s.stack, nextFrame := s.nextFrame+1}
      let targetNext := {t with control := .running entered, stack := targetFrame :: t.stack, nextFrame := t.nextFrame+1}
      have sourceStep := step_bl .ssa source s a ctx.sourceFunction ctx.sourceBlock symbol args results values
        callee entered runningSource active.foundSourceFunction active.foundSourceBlock sourceAt sourceReads calleeAt sourceEntered
      have targetStep := step_bl .postSSA target t b ctx.targetFunction ctx.targetBlock symbol targetArgs results values
        calleeContext.targetFunction entered runningTarget active.foundTargetFunction active.foundTargetBlock targetAt
        targetReads targetCallee targetEnters
      refine ⟨sourceNext, targetNext, sourceStep, targetStep, ?_⟩
      exact ⟨related.reached.trans (.next sourceStep (.refl _)),
        Demand.Aligned.cons ⟨ctx, ⟨saved⟩⟩ related.stack,
        congrArg (·+1) related.counter, related.memory,
        entered, entered, rfl, rfl, calleeContext, nextActive⟩

end EvoIR.Machine.Transport
