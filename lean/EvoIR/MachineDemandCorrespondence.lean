import EvoIR.MachineDemandLockstep

/-!
Proof-valued demand admission for an actual SSA program. The returned checked
closure proves full finite and divergent same-program noninterference whenever
both concrete argument vectors have the progress supplied by the preceding
all-argument selection theorem. No finite execution bound occurs here.
-/
namespace EvoIR.Machine.Demand

def RelatedFrom (program : Program) (plan : Plan) (sourceEntry targetEntry : State)
    (source target : State) : Prop :=
  StateRelated plan source target ∧
    Core.Transition.Reaches (step .ssa program) sourceEntry source ∧
    Core.Transition.Reaches (step .ssa program) targetEntry target

theorem StateRelated.observation_eq (plan : Plan) (type : Core.Source.IntType)
    (source target : State) (states : StateRelated plan source target) :
    observe type source = observe type target := by
  have controls := states.control
  cases sourceControl : source.control with
  | running activation =>
    rw [sourceControl] at controls
    obtain ⟨targetActivation, targetControl, _⟩ :=
      ControlRelated.running_inv plan activation target.control controls
    simp only [observe, sourceControl, targetControl]
  | halted result =>
    rw [sourceControl] at controls
    have targetControl := ControlRelated.halted_inv plan result target.control controls
    simp only [observe, sourceControl, targetControl]

theorem StateRelated.stopped (program : Program) (plan : Plan) (source target : State)
    (states : StateRelated plan source target) (stopped : step .ssa program target = none) :
    step .ssa program source = none := by
  have controls := states.control
  cases sourceControl : source.control with
  | halted result => simp only [step, sourceControl]
  | running activation =>
    rw [sourceControl] at controls
    obtain ⟨targetActivation, targetControl, _⟩ :=
      ControlRelated.running_inv plan activation target.control controls
    simp only [step, targetControl] at stopped
    contradiction

theorem ranked_simulation (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry) :
    Core.Transition.RankedSimulation (step .ssa program) (step .ssa program)
      (observe type) (observe type) (RelatedFrom program plan sourceEntry targetEntry)
      (fun _ _ => 0) := by
  constructor
  · intro source target targetAfter related targetStep
    obtain ⟨sourceAfter, sourceStep, states⟩ := step_related_reachable program plan accepted type
      sourceEntry targetEntry source target targetAfter sourceProgress targetProgress
      related.2.1 related.2.2 related.1 targetStep
    refine .inl ⟨sourceAfter, .single sourceStep, states, ?_, ?_⟩
    · exact related.2.1.trans (.next sourceStep (.refl _))
    · exact related.2.2.trans (.next targetStep (.refl _))
  · intro source target related stopped
    have sourceStopped := StateRelated.stopped program plan source target related.1 stopped
    rcases targetProgress target related.2.2 with ⟨next, continues⟩ | ⟨value, observed⟩
    · rw [stopped] at continues
      contradiction
    · exact ⟨source, value, .refl _, sourceStopped,
        (StateRelated.observation_eq plan type source target related.1).trans observed, observed⟩
  · intro source target related
    exact targetProgress target related.2.2

theorem demand_refines (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (entries : StateRelated plan sourceEntry targetEntry) :
    Core.Transition.Refines (step .ssa program) (step .ssa program)
      (observe type) (observe type) sourceEntry targetEntry :=
  (ranked_simulation program plan accepted type sourceEntry targetEntry sourceProgress targetProgress).sound
    ⟨entries, .refl _, .refl _⟩

structure Checked (program : Program) (plan : Plan) : Type where
  accepted : checkStructure program plan = true

def checkDemand (program : Program) (plan : Plan) : Option (Checked program plan) :=
  if accepted : checkStructure program plan = true then some ⟨accepted⟩ else none

theorem Checked.refines (program : Program) (plan : Plan) (checked : Checked program plan)
    (type : Core.Source.IntType) (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (entries : StateRelated plan sourceEntry targetEntry) :
    Core.Transition.Refines (step .ssa program) (step .ssa program)
      (observe type) (observe type) sourceEntry targetEntry :=
  demand_refines program plan checked.accepted type sourceEntry targetEntry sourceProgress targetProgress entries

end EvoIR.Machine.Demand
