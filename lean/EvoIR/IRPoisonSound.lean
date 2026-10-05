import EvoIR.IRPoisonExecution
import EvoIR.Core.Correspondence

/-! Universal, unbounded refinement for the exact checked IR pair. An already
proved continuation supplies observability of normalized terminals, via
Refines.source_terminal. It supplies no bank invariant or equivalence: actual
IR transfer, full call stacks, faults and edges were independently proved.
The exact original IR remains the first program of the composed theorem. -/
namespace EvoIR.Core.IR.Poison

theorem terminal_related (plan : DemandPlan) (source target : Program) (type : Source.IntType)
    (before after : State) (states : StatesRelated plan before after)
    (stopped : step target after = none) :
    step source before = none ∧ observeTarget type before = observeTarget type after := by
  have controls := states.control
  cases targetControl : after.control with
  | running _ => simp [step, targetControl] at stopped
  | halted targetResult =>
    cases sourceControl : before.control with
    | running _ => simp [ControlsRelated, sourceControl, targetControl] at controls
    | halted sourceResult =>
      have same : sourceResult = targetResult := by
        simpa only [ControlsRelated, sourceControl, targetControl] using controls
      refine ⟨by simp [step, sourceControl], ?_⟩
      simp only [observeTarget, sourceControl, targetControl, same]

theorem normalization_refines {T : Type} (source normalized : Program) (plan : DemandPlan)
    (accepted : checkPair source normalized plan = true) (type : Source.IntType)
    (entry : State) (targetStep : T → Option T) (targetObserve : T → Option Observation)
    (targetEntry : T)
    (continuation : Transition.Refines (step normalized) targetStep
      (observeTarget type) targetObserve entry targetEntry) :
    Transition.Refines (step source) (step normalized)
      (observeTarget type) (observeTarget type) entry entry := by
  obtain ⟨valid, same⟩ := checkPair_data source normalized plan accepted
  subst normalized
  let related := fun before after =>
    Transition.Reaches (step (rewriteProgram plan source)) entry after ∧
      StatesRelated plan before after
  have simulation : Transition.RankedSimulation (step source) (step (rewriteProgram plan source))
      (observeTarget type) (observeTarget type) related (fun _ _ => 0) := by
    constructor
    · intro before after next states stepped
      obtain ⟨sourceNext, sourceStep, nextRelated⟩ := step_related plan source valid before after next states.2 stepped
      exact Or.inl ⟨sourceNext, .single sourceStep,
        states.1.trans (.next stepped (.refl _)), nextRelated⟩
    · intro before after states stopped
      obtain ⟨observation, observed⟩ := continuation.source_terminal after states.1 stopped
      obtain ⟨sourceStopped, sameObservation⟩ := terminal_related plan source (rewriteProgram plan source)
        type before after states.2 stopped
      exact ⟨before, observation, .refl _, sourceStopped, sameObservation.trans observed, observed⟩
    · intro before after states
      cases stepped : step (rewriteProgram plan source) after with
      | some next => exact Or.inl ⟨next, rfl⟩
      | none => exact Or.inr (continuation.source_terminal after states.1 stepped)
  exact simulation.sound ⟨.refl _, StatesRelated.refl plan entry⟩

theorem sound {T : Type} (source normalized : Program) (plan : DemandPlan)
    (accepted : checkPair source normalized plan = true) (type : Source.IntType)
    (name : String) (args : List Value) (entry : State)
    (started : initial normalized name args = .ok entry)
    (targetStep : T → Option T) (targetObserve : T → Option Observation) (targetEntry : T)
    (continuation : Transition.Refines (step normalized) targetStep
      (observeTarget type) targetObserve entry targetEntry) :
    initial source name args = .ok entry ∧
      Transition.Refines (step source) targetStep
        (observeTarget type) targetObserve entry targetEntry := by
  have originalStarted : initial source name args = .ok entry := by
    have same := (checkPair_data source normalized plan accepted).2
    rw [same, initial_rewrite] at started
    exact started
  exact ⟨originalStarted, Transition.Refines.trans
    (normalization_refines source normalized plan accepted type entry targetStep targetObserve targetEntry continuation)
    continuation⟩

end EvoIR.Core.IR.Poison
