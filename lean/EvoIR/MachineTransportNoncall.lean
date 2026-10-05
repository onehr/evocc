import EvoIR.MachineTransportPrepareState

namespace EvoIR.Machine.Transport

theorem matched_notCall (source target : Instruction) (sourceEnv targetEnv : Origins)
    (matched : matchOrigins source target sourceEnv targetEnv = true)
    (notCall : ∀ name args results, source ≠ .bl name args results) :
    ∀ name args results, target ≠ .bl name args results := by
  have exactShape := checkRetained_shape (useCorrespondence source target) source target
    (matchOrigins_retained source target sourceEnv targetEnv matched)
  cases source <;> simp only [Instruction.mapUses] at exactShape
  all_goals try exact False.elim (notCall _ _ _ rfl)
  all_goals intros name args results bad; rw [bad] at exactShape; cases exactShape

theorem halted_observed (source target : Program) (type : Core.Source.IntType) (entry : State)
    (progress : ObservedProgress .ssa source type entry) (s t : State)
    (outcome : Except Core.Source.Fault (List Value))
    (reachable : Core.Transition.Reaches (step .ssa source) entry s)
    (sourceHalt : s.control = .halted outcome) (targetHalt : t.control = .halted outcome) :
    ∃ observation, step .ssa source s = none ∧ step .postSSA target t = none ∧
      observe type s = some observation ∧ observe type t = some observation := by
  have sourceStop : step .ssa source s = none := by simp [step, sourceHalt]
  have targetStop : step .postSSA target t = none := by simp [step, targetHalt]
  rcases progress s reachable with ⟨next, continues⟩ | ⟨observation, observed⟩
  · simp only [sourceStop] at continues; contradiction
  · refine ⟨observation, sourceStop, targetStop, observed, ?_⟩
    simpa only [observe, sourceHalt, targetHalt] using observed

theorem retained_success (source target : Program) (type : Core.Source.IntType) (entry : State)
    (s t : State) (a b : Activation) (ctx : Context source target) (active : Active ctx a b)
    (related : StateRelated source target type entry s t)
    (runningSource : s.control = .running a) (runningTarget : t.control = .running b)
    (inst candidate : Instruction) (rest : List Instruction) (positions : List (Option Nat))
    (suffix : ctx.sourceBlock.insts.drop a.pc = inst :: rest)
    (candidateAt : ctx.targetBlock.insts[b.pc]? = some candidate)
    (matched : matchOrigins inst candidate active.env.source active.env.target = true)
    (checked : checkBodyAux ctx.sourceBlock.id ctx.plans.needed ctx.targetBlock.insts (a.pc+1) (b.pc+1)
      rest positions
      {source := resultOrigins ctx.sourceBlock.id a.pc 0 inst.defs active.env.source,
       target := resultOrigins ctx.sourceBlock.id a.pc 0 candidate.defs active.env.target,
       cuts := ⟨a.pc+1, b.pc+1, resultOrigins ctx.sourceBlock.id a.pc 0 inst.defs active.env.source,
         resultOrigins ctx.sourceBlock.id a.pc 0 candidate.defs active.env.target⟩ :: active.env.cuts} = some ctx.final)
    (notCall : ∀ name args results, inst ≠ .bl name args results)
    (writes : Writes) (memory : Memory)
    (evaluated : evalInstructionOperands a.owner inst (readOperand a.regs) s.memory = .ok (writes, memory)) :
    ∃ s' t', step .ssa source s = some s' ∧ step .postSSA target t = some t' ∧
      StateRelated source target type entry s' t' := by
  have sourceAt : ctx.sourceBlock.insts[a.pc]? = some inst := by
    have first := congrArg (fun list : List Instruction => list[0]?) suffix
    simpa only [List.getElem?_drop, Nat.add_zero, List.getElem?_cons_zero] using first
  have sourceInside := (List.getElem?_eq_some_iff.mp sourceAt).1
  have targetInside := (List.getElem?_eq_some_iff.mp candidateAt).1
  have shape : instructionShapeValid inst = true := List.all_eq_true.mp ctx.classes inst
    (List.mem_of_getElem? sourceAt)
  obtain ⟨targetEvaluated, sourceReal, targetReal⟩ := matched_results ctx.sourceBlock.id a.pc inst candidate
    active.env.source active.env.target active.valuation a.regs b.regs a.owner s.memory matched
    active.sourceReal active.targetReal
    (futureFresh_of_before _ _ _ _ active.sourceBefore)
    (futureFresh_of_before _ _ _ _ active.targetBefore) writes memory evaluated
  have sourceEval : evalInstruction a.owner inst a.regs s.memory = .ok (applyWrites a.regs writes, memory) := by
    simp only [evalInstruction, evaluated, Bind.bind, Except.bind, Pure.pure, Except.pure]
  have targetEval : evalInstruction b.owner candidate b.regs t.memory = .ok (applyWrites b.regs writes, memory) := by
    have actual := targetEvaluated
    rw [active.owner, related.memory] at actual
    simp only [evalInstruction, actual, Bind.bind, Except.bind, Pure.pure, Except.pure]
  let sourceNext := {s with control := .running {a with pc := a.pc+1, regs := applyWrites a.regs writes}, memory}
  let targetNext := {t with control := .running {b with pc := b.pc+1, regs := applyWrites b.regs writes}, memory}
  have sourceStep := step_evalInstruction .ssa source s a ctx.sourceFunction ctx.sourceBlock inst
    _ memory runningSource active.foundSourceFunction active.foundSourceBlock sourceAt notCall sourceEval
  have targetStep := step_evalInstruction .postSSA target t b ctx.targetFunction ctx.targetBlock candidate
    _ memory runningTarget active.foundTargetFunction active.foundTargetBlock candidateAt
    (matched_notCall inst candidate active.env.source active.env.target matched notCall) targetEval
  let nextActive : Active ctx {a with pc := a.pc+1, regs := applyWrites a.regs writes}
      {b with pc := b.pc+1, regs := applyWrites b.regs writes} := {
    sourceName := active.sourceName, targetName := active.targetName,
    sourceId := active.sourceId, targetId := active.targetId, owner := active.owner, incoming := active.incoming,
    positions,
    env := {
      source := resultOrigins ctx.sourceBlock.id a.pc 0 inst.defs active.env.source,
      target := resultOrigins ctx.sourceBlock.id a.pc 0 candidate.defs active.env.target,
      cuts := ⟨a.pc+1, b.pc+1, resultOrigins ctx.sourceBlock.id a.pc 0 inst.defs active.env.source,
        resultOrigins ctx.sourceBlock.id a.pc 0 candidate.defs active.env.target⟩ :: active.env.cuts},
    valuation := writeResultValues ctx.sourceBlock.id a.pc 0 writes active.valuation,
    sourceBound := by dsimp; omega, targetBound := by dsimp; omega,
    accepted := by simpa only [List.drop_add_one_eq_tail_drop, suffix, List.tail_cons] using checked,
    sourceReal, targetReal,
    sourceBefore := knownBefore_results _ _ _ _ _ active.sourceBefore,
    targetBefore := knownBefore_results _ _ _ _ _ active.targetBefore,
    normalized := writeResultValues_normalized _ _ _ _ _ active.normalized
      (checked_writes_normalized _ _ _ _ _ _ shape evaluated)}
  refine ⟨sourceNext, targetNext, sourceStep, targetStep, ?_⟩
  exact ⟨related.reached.trans (.next sourceStep (.refl _)), related.stack, related.counter, rfl,
    _, _, rfl, rfl, ctx, ⟨nextActive⟩⟩

theorem retained_error (source target : Program) (s t : State) (a b : Activation)
    (ctx : Context source target) (active : Active ctx a b)
    (sameMemory : s.memory = t.memory)
    (runningSource : s.control = .running a) (runningTarget : t.control = .running b)
    (inst candidate : Instruction) (sourceAt : ctx.sourceBlock.insts[a.pc]? = some inst)
    (candidateAt : ctx.targetBlock.insts[b.pc]? = some candidate)
    (matched : matchOrigins inst candidate active.env.source active.env.target = true)
    (notCall : ∀ name args results, inst ≠ .bl name args results)
    (fault : Core.Source.Fault)
    (evaluated : evalInstructionOperands a.owner inst (readOperand a.regs) s.memory = .error fault) :
    step .ssa source s = some {s with control := .halted (.error fault)} ∧
      step .postSSA target t = some {t with control := .halted (.error fault)} := by
  have targetOperands := (matchOrigins_eval inst candidate active.env.source active.env.target matched
    active.valuation a.owner a.regs b.regs s.memory active.sourceReal active.targetReal).trans evaluated
  have sourceEval : evalInstruction a.owner inst a.regs s.memory = .error fault := by
    simp only [evalInstruction, evaluated, Bind.bind, Except.bind]
  have targetEval : evalInstruction b.owner candidate b.regs t.memory = .error fault := by
    rw [active.owner, sameMemory] at targetOperands
    simp only [evalInstruction, targetOperands, Bind.bind, Except.bind]
  exact ⟨step_evalInstruction_error .ssa source s a ctx.sourceFunction ctx.sourceBlock inst fault runningSource
    active.foundSourceFunction active.foundSourceBlock sourceAt notCall sourceEval,
    step_evalInstruction_error .postSSA target t b ctx.targetFunction ctx.targetBlock candidate fault runningTarget
      active.foundTargetFunction active.foundTargetBlock candidateAt
      (matched_notCall inst candidate active.env.source active.env.target matched notCall) targetEval⟩

end EvoIR.Machine.Transport
