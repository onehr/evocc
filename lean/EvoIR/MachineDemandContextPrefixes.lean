import EvoIR.MachineDemandContext
import EvoIR.MachineDemandPrefixes

/-! Target replacement under a literal suspended caller. Before the callee
returns, checked demand preserves its actual equal-count prefixes. At the
top return the entire resumed states are equal, including memory and frame
counters. Subsequent prefixes then use literal state equality; no progress
or correctness hypothesis about the caller continuation is required. -/
namespace EvoIR.Machine.Demand

def ContextCoupled (program : Program) (plan : Plan) (sourceEntry targetEntry : State)
    (stack : List ReturnFrame) (source target : State) : Prop :=
  (∃ original candidate, RelatedFrom program plan sourceEntry targetEntry original candidate ∧
    source = context original stack ∧ target = context candidate stack) ∨ source = target

theorem contexts_halted_equal (plan : Plan) (source target : State)
    (related : StateRelated plan source target) (stack : List ReturnFrame)
    (values : List Value) (halted : source.control = .halted (.ok values)) :
    context source stack = context target stack := by
  have controls := related.control
  rw [halted] at controls
  have targetHalted := ControlRelated.halted_inv plan (.ok values) target.control controls
  simp only [context, halted, targetHalted, ← related.memory, ← related.nextFrame]

theorem ContextCoupled.eq_of_short_stack (program : Program) (plan : Plan)
    (sourceEntry targetEntry : State) (stack : List ReturnFrame) (source target : State)
    (related : ContextCoupled program plan sourceEntry targetEntry stack source target)
    (returned : source.stack.length < stack.length) : source = target := by
  rcases related with ⟨original, candidate, states, rfl, rfl⟩ | same
  · cases control : original.control with
    | running activation =>
      simp only [context, control, List.length_append] at returned
      omega
    | halted result =>
      cases result with
      | error fault =>
        simp only [context, control, List.length_append] at returned
        omega
      | ok values => exact contexts_halted_equal plan original candidate states.1 stack values control
  · exact same

theorem ContextCoupled.observation_eq (program : Program) (plan : Plan)
    (sourceEntry targetEntry : State) (stack : List ReturnFrame) (source target : State)
    (related : ContextCoupled program plan sourceEntry targetEntry stack source target)
    (type : Core.Source.IntType) : observe type source = observe type target := by
  rcases related with ⟨original, candidate, states, rfl, rfl⟩ | rfl
  · have controls := states.1.control
    cases control : original.control with
    | running activation =>
      rw [control] at controls
      obtain ⟨next, targetControl, _⟩ := ControlRelated.running_inv plan activation candidate.control controls
      simp only [context, control, targetControl, observe]
    | halted result =>
      rw [control] at controls
      have targetControl := ControlRelated.halted_inv plan result candidate.control controls
      cases result with
      | error fault => simp only [context, control, targetControl, observe]
      | ok values => rw [contexts_halted_equal plan original candidate states.1 stack values control]
  · rfl

theorem ContextCoupled.stopped (program : Program) (plan : Plan)
    (sourceEntry targetEntry : State) (stack : List ReturnFrame) (source target : State)
    (related : ContextCoupled program plan sourceEntry targetEntry stack source target)
    (stopped : step .ssa program source = none) : step .ssa program target = none := by
  rcases related with ⟨original, candidate, states, rfl, rfl⟩ | rfl
  · have controls := states.1.control
    cases control : original.control with
    | running activation =>
      simp only [context, control, step] at stopped
      contradiction
    | halted result =>
      rw [control] at controls
      have targetControl := ControlRelated.halted_inv plan result candidate.control controls
      cases result with
      | error fault => simp only [context, targetControl, step]
      | ok values =>
        rw [← contexts_halted_equal plan original candidate states.1 stack values control]
        exact stopped
  · exact stopped

theorem step_context_forward (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (stack : List ReturnFrame) (single : SingleCaller stack)
    (source target afterSource : State)
    (related : ContextCoupled program plan sourceEntry targetEntry stack source target)
    (actual : step .ssa program source = some afterSource) :
    ∃ afterTarget, step .ssa program target = some afterTarget ∧
      ContextCoupled program plan sourceEntry targetEntry stack afterSource afterTarget := by
  rcases related with ⟨original, candidate, states, rfl, rfl⟩ | same
  · cases control : original.control with
    | running activation =>
      obtain ⟨nextOriginal, originalStep⟩ := running_progress .ssa program original activation control
      have originalReach := states.2.1.trans (.next originalStep (.refl _))
      have originalContext := step_context program original nextOriginal activation stack control originalStep
        (progress_caller_ready program type sourceEntry nextOriginal sourceProgress originalReach stack single)
      have same : context nextOriginal stack = afterSource := Option.some.inj (originalContext.symm.trans actual)
      obtain ⟨nextCandidate, candidateStep, nextStates⟩ := step_related_forward program plan accepted type
        sourceEntry targetEntry original candidate nextOriginal sourceProgress targetProgress states originalStep
      have controls := states.1.control
      rw [control] at controls
      obtain ⟨candidateActivation, candidateControl, _⟩ :=
        ControlRelated.running_inv plan activation candidate.control controls
      have candidateContext := step_context program candidate nextCandidate candidateActivation stack
        candidateControl candidateStep
        (progress_caller_ready program type targetEntry nextCandidate targetProgress nextStates.2.2 stack single)
      exact ⟨context nextCandidate stack, candidateContext,
        .inl ⟨nextOriginal, nextCandidate, nextStates, same.symm, rfl⟩⟩
    | halted result =>
      cases result with
      | error fault =>
        simp only [context, control, step] at actual
        contradiction
      | ok values =>
        have equal := contexts_halted_equal plan original candidate states.1 stack values control
        exact ⟨afterSource, by simpa only [← equal] using actual, .inr rfl⟩
  · exact ⟨afterSource, by simpa only [← same] using actual, .inr rfl⟩

theorem context_prefixes_forward (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (stack : List ReturnFrame) (single : SingleCaller stack)
    {n : Nat} {source afterSource target : State}
    (related : ContextCoupled program plan sourceEntry targetEntry stack source target)
    (path : Core.Transition.Path (step .ssa program) n source afterSource) :
    ∃ afterTarget, Core.Transition.Path (step .ssa program) n target afterTarget ∧
      ContextCoupled program plan sourceEntry targetEntry stack afterSource afterTarget := by
  induction path generalizing target with
  | refl => exact ⟨target, .refl _, related⟩
  | next edge tail ih =>
    obtain ⟨next, actual, nextRelated⟩ := step_context_forward program plan accepted type
      sourceEntry targetEntry sourceProgress targetProgress stack single _ target _ related edge
    obtain ⟨after, rest, finalRelated⟩ := ih nextRelated
    exact ⟨after, .next actual rest, finalRelated⟩

theorem replace_context_target {S : Type} (sourceStep : S → Option S)
    (sourceObserve : S → Option Core.Observation) (observationType : Core.Source.IntType)
    (program : Program) (plan : Plan) (accepted : checkStructure program plan = true)
    (type : Core.Source.IntType) (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (entries : StateRelated plan sourceEntry targetEntry)
    (stack : List ReturnFrame) (single : SingleCaller stack)
    (boundary : S → State → Prop) (entry : S)
    (fragment : Core.Transition.OpenCheckpoint sourceStep (step .ssa program)
      sourceObserve (observe observationType) boundary entry (context sourceEntry stack))
    (boundaryReturned : ∀ source target, boundary source target → target.stack.length < stack.length) :
    Core.Transition.OpenCheckpoint sourceStep (step .ssa program)
      sourceObserve (observe observationType) boundary entry (context targetEntry stack) := by
  obtain ⟨original, atEntry, advance⟩ := fragment
  let joined := fun source target => ∃ old,
    original source old ∧ ContextCoupled program plan sourceEntry targetEntry stack old target
  refine ⟨joined, ⟨context sourceEntry stack, atEntry,
    .inl ⟨sourceEntry, targetEntry, ⟨entries, .refl _, .refl _⟩, rfl, rfl⟩⟩, ?_⟩
  rintro source target ⟨old, included, related⟩
  rcases advance source old included with ⟨sf, tf, observation, sourcePath, targetPath,
      sourceStop, targetStop, sourceObserved, targetObserved⟩ | ⟨sf, tf, sourcePath, targetPath, next⟩
  · obtain ⟨n, counted⟩ := targetPath.counted
    obtain ⟨matched, matchedPath, states⟩ := context_prefixes_forward program plan accepted type
      sourceEntry targetEntry sourceProgress targetProgress stack single related counted
    exact .inl ⟨sf, matched, observation, sourcePath, matchedPath.reaches, sourceStop,
      ContextCoupled.stopped program plan sourceEntry targetEntry stack tf matched states targetStop,
      sourceObserved, (ContextCoupled.observation_eq program plan sourceEntry targetEntry stack tf matched states
        observationType).symm.trans targetObserved⟩
  · obtain ⟨n, positive, counted⟩ := Core.Transition.positive_counted targetPath
    obtain ⟨matched, matchedPath, states⟩ := context_prefixes_forward program plan accepted type
      sourceEntry targetEntry sourceProgress targetProgress stack single related counted
    refine .inr ⟨sf, matched, sourcePath, matchedPath.positive positive, ?_⟩
    rcases next with repeated | returned
    · exact .inl ⟨tf, repeated, states⟩
    · have same := ContextCoupled.eq_of_short_stack program plan sourceEntry targetEntry stack tf matched
        states (boundaryReturned sf tf returned)
      exact .inr (same ▸ returned)

end EvoIR.Machine.Demand
