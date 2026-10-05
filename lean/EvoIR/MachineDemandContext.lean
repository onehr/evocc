import EvoIR.MachineDemandContextStep

/-! Context-parametric demand noninterference. Progress is needed only for
the two standalone callee runs already certified for all logical arguments.
Their actual returns transfer equal values, memory and frame counters into
an open caller boundary. The caller continuation is not assumed correct. -/
namespace EvoIR.Machine.Demand

def SingleCaller (stack : List ReturnFrame) : Prop :=
  ∀ frame rest, stack = frame :: rest → frame.results.length = 1

theorem progress_return_single (program : Program) (type : Core.Source.IntType)
    (entry state : State) (progress : ObservedProgress .ssa program type entry)
    (reachable : Core.Transition.Reaches (step .ssa program) entry state)
    (values : List Value) (halted : state.control = .halted (.ok values)) :
    ∃ bits, values = [.int bits] := by
  rcases progress state reachable with ⟨next, advances⟩ | ⟨observation, observed⟩
  · simp only [step, halted] at advances
    contradiction
  · simp only [observe, halted] at observed
    cases values with
    | nil => contradiction
    | cons first rest =>
      cases first with
      | pointer => cases rest <;> contradiction
      | int bits =>
        cases rest with
        | nil => exact ⟨bits, rfl⟩
        | cons => contradiction

theorem progress_caller_ready (program : Program) (type : Core.Source.IntType)
    (entry state : State) (progress : ObservedProgress .ssa program type entry)
    (reachable : Core.Transition.Reaches (step .ssa program) entry state)
    (stack : List ReturnFrame) (single : SingleCaller stack) :
    ∀ values, state.control = .halted (.ok values) → CallerReady values stack := by
  intro values halted
  obtain ⟨bits, rfl⟩ := progress_return_single program type entry state progress reachable values halted
  cases shape : stack with
  | nil => trivial
  | cons frame rest => exact callerReady_single_result frame rest (single frame rest shape) (.int bits)

def ContextReturn (sourceStack targetStack : List ReturnFrame) (source target : State) : Prop :=
  ∃ values memory nextFrame,
    source = restoreCaller values memory nextFrame sourceStack ∧
    target = restoreCaller values memory nextFrame targetStack

def ContextRelated (program : Program) (plan : Plan) (sourceEntry targetEntry : State)
    (sourceStack targetStack : List ReturnFrame) (source target : State) : Prop :=
  ∃ original candidate, RelatedFrom program plan sourceEntry targetEntry original candidate ∧
    (∃ activation, original.control = .running activation) ∧
    source = context original sourceStack ∧ target = context candidate targetStack

theorem context_observe_error (type : Core.Source.IntType) (state : State)
    (stack : List ReturnFrame) (fault : Core.Source.Fault)
    (halted : state.control = .halted (.error fault)) :
    observe type (context state stack) = observe type state := by
  simp only [context, halted, observe]

theorem observe_error_type (sourceType targetType : Core.Source.IntType) (state : State)
    (fault : Core.Source.Fault) (halted : state.control = .halted (.error fault)) :
    observe sourceType state = observe targetType state := by
  cases fault <;> simp only [observe, halted]

theorem demand_context_open (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (entries : StateRelated plan sourceEntry targetEntry)
    (entryRunning : ∃ activation, sourceEntry.control = .running activation)
    (sourceStandalone : sourceEntry.stack = []) (targetStandalone : targetEntry.stack = [])
    (sourceStack targetStack : List ReturnFrame)
    (sourceSingle : SingleCaller sourceStack) (targetSingle : SingleCaller targetStack)
    (observationType : Core.Source.IntType := type) :
    Core.Transition.OpenCheckpoint (step .ssa program) (step .ssa program)
      (observe observationType) (observe observationType) (ContextReturn sourceStack targetStack)
      (context sourceEntry sourceStack) (context targetEntry targetStack) := by
  refine ⟨ContextRelated program plan sourceEntry targetEntry sourceStack targetStack,
    ⟨sourceEntry, targetEntry, ⟨entries, .refl _, .refl _⟩, entryRunning, rfl, rfl⟩, ?_⟩
  rintro originalContext candidateContext
    ⟨original, candidate, related, ⟨activation, running⟩, rfl, rfl⟩
  have controls := related.1.control
  rw [running] at controls
  obtain ⟨targetActivation, targetRunning, acts⟩ :=
    ControlRelated.running_inv plan activation candidate.control controls
  obtain ⟨afterTarget, targetStep⟩ := running_progress .ssa program candidate targetActivation targetRunning
  obtain ⟨afterSource, sourceStep, afterRelated⟩ := step_related_reachable program plan accepted type
    sourceEntry targetEntry original candidate afterTarget sourceProgress targetProgress
    related.2.1 related.2.2 related.1 targetStep
  have sourceReach := related.2.1.trans (.next sourceStep (.refl _))
  have targetReach := related.2.2.trans (.next targetStep (.refl _))
  have sourceContextStep := step_context program original afterSource activation sourceStack
    running sourceStep (progress_caller_ready program type sourceEntry afterSource sourceProgress
      sourceReach sourceStack sourceSingle)
  have targetContextStep := step_context program candidate afterTarget targetActivation targetStack
    targetRunning targetStep (progress_caller_ready program type targetEntry afterTarget targetProgress
      targetReach targetStack targetSingle)
  have afterControls := afterRelated.control
  cases afterControl : afterSource.control with
  | running nextActivation =>
    refine .inr ⟨context afterSource sourceStack, context afterTarget targetStack,
      .single sourceContextStep, .single targetContextStep, .inl ?_⟩
    exact ⟨afterSource, afterTarget, ⟨afterRelated, sourceReach, targetReach⟩,
      ⟨nextActivation, afterControl⟩, rfl, rfl⟩
  | halted result =>
    rw [afterControl] at afterControls
    have targetHalted := ControlRelated.halted_inv plan result afterTarget.control afterControls
    cases result with
    | error fault =>
      rcases targetProgress afterTarget targetReach with ⟨next, advances⟩ | ⟨observation, observed⟩
      · simp only [step, targetHalted] at advances
        contradiction
      · refine .inl ⟨context afterSource sourceStack, context afterTarget targetStack,
          observation, .next sourceContextStep (.refl _), .next targetContextStep (.refl _), ?_, ?_, ?_, ?_⟩
        · simp only [step, context, afterControl]
        · simp only [step, context, targetHalted]
        · rw [context_observe_error observationType afterSource sourceStack fault afterControl]
          exact (StateRelated.observation_eq plan observationType afterSource afterTarget afterRelated).trans
            ((observe_error_type observationType type afterTarget fault targetHalted).trans observed)
        · exact (context_observe_error observationType afterTarget targetStack fault targetHalted).trans
            ((observe_error_type observationType type afterTarget fault targetHalted).trans observed)
    | ok values =>
      refine .inr ⟨context afterSource sourceStack, context afterTarget targetStack,
        .single sourceContextStep, .single targetContextStep, .inr ?_⟩
      refine ⟨values, afterSource.memory, afterSource.nextFrame, ?_, ?_⟩
      · simp only [context, afterControl]
      · simp only [context, targetHalted, ← afterRelated.memory, ← afterRelated.nextFrame]

theorem Checked.contextOpen (program : Program) (plan : Plan) (checked : Checked program plan)
    (type : Core.Source.IntType) (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (entries : StateRelated plan sourceEntry targetEntry)
    (entryRunning : ∃ activation, sourceEntry.control = .running activation)
    (sourceStandalone : sourceEntry.stack = []) (targetStandalone : targetEntry.stack = [])
    (sourceStack targetStack : List ReturnFrame)
    (sourceSingle : SingleCaller sourceStack) (targetSingle : SingleCaller targetStack)
    (observationType : Core.Source.IntType := type) :
    Core.Transition.OpenCheckpoint (step .ssa program) (step .ssa program)
      (observe observationType) (observe observationType) (ContextReturn sourceStack targetStack)
      (context sourceEntry sourceStack) (context targetEntry targetStack) :=
  demand_context_open program plan checked.accepted type sourceEntry targetEntry
    sourceProgress targetProgress entries entryRunning sourceStandalone targetStandalone
    sourceStack targetStack sourceSingle targetSingle observationType

end EvoIR.Machine.Demand
