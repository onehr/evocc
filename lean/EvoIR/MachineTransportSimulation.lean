import EvoIR.MachineTransportReturn

/-! Composition of the actual dispatcher cases into a cyclic simulation.
Both sides advance positively after finite DCE/copy preparation. Original
loops and recursive calls are consequently covered without execution fuel. -/
namespace EvoIR.Machine.Transport

theorem termEffects_cases (source target : Terminator)
    (before after : Except Core.Source.Fault Demand.TermEffect)
    (related : TermEffectsRelated source target before after) :
    (∃ fault, before = .error fault ∧ after = .error fault) ∨
    (∃ original actual : Nat × List VReg,
      before = .ok (.edge original.1 original.2) ∧ after = .ok (.edge actual.1 actual.2) ∧
      (original, actual) ∈ source.edges.zip target.edges) ∨
    (∃ values, before = .ok (.returned values) ∧ after = .ok (.returned values)) := by
  cases before with
  | error fault =>
    cases after with
    | error targetFault => exact Or.inl ⟨fault, rfl, congrArg Except.error related.symm⟩
    | ok effect => cases effect <;> contradiction
  | ok sourceEffect =>
    cases after with
    | error fault => cases sourceEffect <;> contradiction
    | ok targetEffect =>
      cases sourceEffect with
      | edge sourceDst sourceArgs =>
        cases targetEffect with
        | returned values => contradiction
        | edge targetDst targetArgs =>
          exact Or.inr (Or.inl ⟨(sourceDst, sourceArgs), (targetDst, targetArgs), rfl, rfl, related⟩)
      | returned sourceValues =>
        cases targetEffect with
        | edge targetDst targetArgs => contradiction
        | returned targetValues =>
          exact Or.inr (Or.inr ⟨sourceValues, rfl, congrArg (fun values => Except.ok (Demand.TermEffect.returned values)) related.symm⟩)

theorem checkpointSimulation (source target : Program) (annotations : ProgramPlan)
    (accepted : checkProgram source target annotations = true)
    (type : Core.Source.IntType) (entry : State)
    (progress : ObservedProgress .ssa source type entry) :
    Core.Transition.CheckpointSimulation (step .ssa source) (step .postSSA target)
      (observe type) (observe type) (StateRelated source target type entry) := by
  classical
  constructor
  intro s t related
  obtain ⟨preparedSource, preparedTarget, a, b, ctx, active, sourcePath, targetPath,
    preparedRelated, runningSource, runningTarget, ready⟩ := prepareState source target type entry progress s t related
  rcases ready with ⟨sourceEnd, targetEnd, envFinal⟩ |
    ⟨inst, rest, positions, candidate, suffix, candidateAt, matched, checked⟩
  · have sourceReal : Realizes ctx.final.source active.valuation a.regs := by
      simpa only [← envFinal] using active.sourceReal
    have targetReal : Realizes ctx.final.target active.valuation b.regs := by
      simpa only [← envFinal] using active.targetReal
    have effects := matchControls_effects ctx.sourceBlock.term ctx.targetBlock.term ctx.final.source ctx.final.target
      ctx.controls active.valuation a.regs b.regs sourceReal targetReal
    rcases termEffects_cases _ _ _ _ effects with ⟨fault, sourceEffect, targetEffect⟩ |
      ⟨original, actual, sourceEffect, targetEffect, paired⟩ | ⟨values, sourceEffect, targetEffect⟩
    · obtain ⟨sourceStep, targetStep⟩ := terminator_error source target preparedSource preparedTarget a b ctx active
        runningSource runningTarget sourceEnd targetEnd fault sourceEffect targetEffect
      obtain ⟨observation, sourceStop, targetStop, sourceObserved, targetObserved⟩ :=
        halted_observed source target type entry progress
          {preparedSource with control := .halted (.error fault)}
          {preparedTarget with control := .halted (.error fault)} (.error fault)
          (preparedRelated.reached.trans (.next sourceStep (.refl _))) rfl rfl
      exact Or.inl ⟨_, _, observation, sourcePath.trans (.next sourceStep (.refl _)),
        targetPath.trans (.next targetStep (.refl _)), sourceStop, targetStop, sourceObserved, targetObserved⟩
    · obtain ⟨sourceNext, targetNext, sourceStep, targetPositive, nextRelated⟩ := terminatorEdge source target
        type entry progress preparedSource preparedTarget a b ctx active preparedRelated runningSource runningTarget
        sourceEnd targetEnd envFinal original actual sourceEffect targetEffect paired
      exact Or.inr ⟨sourceNext, targetNext, sourcePath.then_positive (.single sourceStep),
        targetPath.then_positive targetPositive, nextRelated⟩
    · obtain ⟨sourceNext, targetNext, sourceStep, targetStep, finished⟩ := terminatorReturn source target type entry
        progress preparedSource preparedTarget a b ctx active preparedRelated runningSource runningTarget
        sourceEnd targetEnd values sourceEffect targetEffect
      rcases finished with ⟨sourceHalt, targetHalt⟩ | nextRelated
      · obtain ⟨observation, sourceStop, targetStop, sourceObserved, targetObserved⟩ := halted_observed source target
          type entry progress sourceNext targetNext (.ok values)
          (preparedRelated.reached.trans (.next sourceStep (.refl _))) sourceHalt targetHalt
        exact Or.inl ⟨sourceNext, targetNext, observation, sourcePath.trans (.next sourceStep (.refl _)),
          targetPath.trans (.next targetStep (.refl _)), sourceStop, targetStop, sourceObserved, targetObserved⟩
      · exact Or.inr ⟨sourceNext, targetNext, sourcePath.then_positive (.single sourceStep),
          targetPath.then_positive (.single targetStep), nextRelated⟩
  · by_cases call : ∃ symbol args results, inst = .bl symbol args results
    · obtain ⟨symbol, args, results, rfl⟩ := call
      obtain ⟨sourceNext, targetNext, sourceStep, targetStep, nextRelated⟩ := retained_call source target annotations
        accepted type entry progress preparedSource preparedTarget a b ctx active preparedRelated runningSource runningTarget
        symbol args results candidate rest positions suffix candidateAt matched checked
      exact Or.inr ⟨sourceNext, targetNext, sourcePath.then_positive (.single sourceStep),
        targetPath.then_positive (.single targetStep), nextRelated⟩
    · have notCall : ∀ symbol args results, inst ≠ .bl symbol args results := by
        intro symbol args results isCall
        exact call ⟨symbol, args, results, isCall⟩
      cases evaluated : evalInstructionOperands a.owner inst (readOperand a.regs) preparedSource.memory with
      | error fault =>
        have sourceAt : ctx.sourceBlock.insts[a.pc]? = some inst := by
          have first := congrArg (fun list : List Instruction => list[0]?) suffix
          simpa only [List.getElem?_drop, Nat.add_zero, List.getElem?_cons_zero] using first
        obtain ⟨sourceStep, targetStep⟩ := retained_error source target preparedSource preparedTarget a b ctx active
          preparedRelated.memory runningSource runningTarget inst candidate sourceAt candidateAt matched notCall fault evaluated
        obtain ⟨observation, sourceStop, targetStop, sourceObserved, targetObserved⟩ :=
          halted_observed source target type entry progress
            {preparedSource with control := .halted (.error fault)}
            {preparedTarget with control := .halted (.error fault)} (.error fault)
            (preparedRelated.reached.trans (.next sourceStep (.refl _))) rfl rfl
        exact Or.inl ⟨_, _, observation, sourcePath.trans (.next sourceStep (.refl _)),
          targetPath.trans (.next targetStep (.refl _)), sourceStop, targetStop, sourceObserved, targetObserved⟩
      | ok pair =>
        rcases pair with ⟨writes, memory⟩
        obtain ⟨sourceNext, targetNext, sourceStep, targetStep, nextRelated⟩ := retained_success source target type entry
          preparedSource preparedTarget a b ctx active preparedRelated runningSource runningTarget inst candidate rest positions
          suffix candidateAt matched checked notCall writes memory evaluated
        exact Or.inr ⟨sourceNext, targetNext, sourcePath.then_positive (.single sourceStep),
          targetPath.then_positive (.single targetStep), nextRelated⟩

end EvoIR.Machine.Transport
