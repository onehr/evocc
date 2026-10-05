import EvoIR.MachineTransportActive

namespace EvoIR.Machine.Transport

theorem bodyReady_suffix (source target : Block) (needed : List Nat) (final env : BodyResult)
    (sourcePC targetPC : Nat) (ready : BodyReady source target needed final sourcePC targetPC env) :
    sourcePC ≤ source.insts.length ∧ targetPC ≤ target.insts.length ∧
      ∃ positions, checkBodyAux source.id needed target.insts sourcePC targetPC
        (source.insts.drop sourcePC) positions env = some final := by
  rcases ready with ⟨sourceEnd, targetEnd, rfl⟩ | ⟨inst, rest, positions, candidate, suffix, candidateAt, matched, checked⟩
  · refine ⟨by omega, by omega, [], ?_⟩
    simp [sourceEnd, targetEnd, checkBodyAux, copyOrigins]
  · have sourceAt : source.insts[sourcePC]? = some inst := by
      have first := congrArg (fun list : List Instruction => list[0]?) suffix
      simpa only [List.getElem?_drop, Nat.add_zero, List.getElem?_cons_zero] using first
    have sourceBound := (List.getElem?_eq_some_iff.mp sourceAt).1
    have targetBound := (List.getElem?_eq_some_iff.mp candidateAt).1
    refine ⟨by omega, by omega, some targetPC :: positions, ?_⟩
    simpa [suffix, checkBodyAux, copyOrigins, candidateAt, matched] using checked

theorem prepareState (source target : Program) (type : Core.Source.IntType) (entry : State)
    (progress : ObservedProgress .ssa source type entry)
    (s t : State) (related : StateRelated source target type entry s t) :
    ∃ s' t' a b, ∃ ctx : Context source target, ∃ active : Active ctx a b,
      Core.Transition.Reaches (step .ssa source) s s' ∧
      Core.Transition.Reaches (step .postSSA target) t t' ∧
      StateRelated source target type entry s' t' ∧ s'.control = .running a ∧ t'.control = .running b ∧
      BodyReady ctx.sourceBlock ctx.targetBlock ctx.plans.needed ctx.final a.pc b.pc active.env := by
  obtain ⟨a, b, runningSource, runningTarget, ctx, ⟨active⟩⟩ := related.active
  have remainingClasses : (ctx.sourceBlock.insts.drop a.pc).all instructionShapeValid = true := by
    apply List.all_eq_true.mpr
    intro inst member
    exact List.all_eq_true.mp ctx.classes inst (List.mem_of_mem_drop member)
  obtain ⟨sourcePC, targetPC, sourceRegs, targetRegs, valuation, env,
    sourcePath, targetPath, sourceReal, targetReal, normalized, sourceBefore, targetBefore, ready⟩ :=
    checkBodyAux_prepares source target type entry ctx.sourceFunction ctx.targetFunction ctx.sourceBlock
      ctx.targetBlock ctx.plans.needed ctx.final (ctx.sourceBlock.insts.drop a.pc) active.positions
      s t a b active.env active.valuation active.accepted rfl active.sourceBound active.targetBound remainingClasses
      runningSource runningTarget active.foundSourceFunction active.foundTargetFunction
      active.foundSourceBlock active.foundTargetBlock active.sourceReal active.targetReal
      active.sourceBefore active.targetBefore active.normalized progress related.reached
  obtain ⟨sourceBound, targetBound, positions, checked⟩ := bodyReady_suffix ctx.sourceBlock ctx.targetBlock
    ctx.plans.needed ctx.final env sourcePC targetPC ready
  let nextSource := {a with pc := sourcePC, regs := sourceRegs}
  let nextTarget := {b with pc := targetPC, regs := targetRegs}
  let nextActive : Active ctx nextSource nextTarget := {
    sourceName := active.sourceName, targetName := active.targetName,
    sourceId := active.sourceId, targetId := active.targetId, owner := active.owner, incoming := active.incoming,
    positions, env, valuation, sourceBound, targetBound, accepted := checked,
    sourceReal, targetReal, sourceBefore, targetBefore, normalized}
  refine ⟨atState s a sourcePC sourceRegs, atState t b targetPC targetRegs,
    nextSource, nextTarget, ctx, nextActive, sourcePath, targetPath, ?_, rfl, rfl, ready⟩
  exact ⟨related.reached.trans sourcePath, related.stack, related.counter, related.memory,
    nextSource, nextTarget, rfl, rfl, ctx, ⟨nextActive⟩⟩

end EvoIR.Machine.Transport
