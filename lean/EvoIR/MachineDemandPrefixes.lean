import EvoIR.MachineDemandCorrespondence
import EvoIR.Core.OpenCheckpoint

/-! Equal-count prefix transport exposes demand's full state relation, not
only its scalar observation. This is the target-replacement interface for an
already checked callee fragment, including its returned memory boundary. -/
namespace EvoIR.Machine.Demand

theorem ActivationRelated.symmetric (plan : Plan) (source target : Activation)
    (related : ActivationRelated plan source target) : ActivationRelated plan target source := by
  refine ⟨related.function.symm, related.block.symm, related.pc.symm, related.owner.symm, ?_⟩
  intro id used
  exact (related.regs id (by simpa only [related.function] using used)).symm

theorem FrameRelated.symmetric (plan : Plan) (source target : ReturnFrame)
    (related : FrameRelated plan source target) : FrameRelated plan target source :=
  ⟨ActivationRelated.symmetric plan source.caller target.caller related.caller, related.results.symm⟩

theorem ControlRelated.symmetric (plan : Plan) (source target : Control)
    (related : ControlRelated plan source target) : ControlRelated plan target source := by
  cases related with
  | running acts => exact .running (ActivationRelated.symmetric plan _ _ acts)
  | halted result => exact .halted result

theorem StateRelated.symmetric (plan : Plan) (source target : State)
    (related : StateRelated plan source target) : StateRelated plan target source := by
  refine ⟨ControlRelated.symmetric plan source.control target.control related.control, ?_,
    related.nextFrame.symm, related.memory.symm⟩
  have swapped : ∀ left right, Aligned (FrameRelated plan) left right →
      Aligned (FrameRelated plan) right left := by
    intro left right stack
    induction stack with
    | nil => exact .nil
    | cons frames tail ih => exact .cons (FrameRelated.symmetric plan _ _ frames) ih
  exact swapped _ _ related.stack

theorem step_related_forward (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry source target afterSource : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (related : RelatedFrom program plan sourceEntry targetEntry source target)
    (actual : step .ssa program source = some afterSource) :
    ∃ afterTarget, step .ssa program target = some afterTarget ∧
      RelatedFrom program plan sourceEntry targetEntry afterSource afterTarget := by
  obtain ⟨afterTarget, next, states⟩ := step_related_reachable program plan accepted type
    targetEntry sourceEntry target source afterSource targetProgress sourceProgress
    related.2.2 related.2.1 (StateRelated.symmetric plan source target related.1) actual
  exact ⟨afterTarget, next, StateRelated.symmetric plan afterTarget afterSource states,
    related.2.1.trans (.next actual (.refl _)), related.2.2.trans (.next next (.refl _))⟩

theorem prefixes_forward (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    {n : Nat} {source afterSource target : State}
    (related : RelatedFrom program plan sourceEntry targetEntry source target)
    (path : Core.Transition.Path (step .ssa program) n source afterSource) :
    ∃ afterTarget, Core.Transition.Path (step .ssa program) n target afterTarget ∧
      RelatedFrom program plan sourceEntry targetEntry afterSource afterTarget := by
  induction path generalizing target with
  | refl => exact ⟨target, .refl _, related⟩
  | next edge tail ih =>
    obtain ⟨next, actual, nextRelated⟩ := step_related_forward program plan accepted type
      sourceEntry targetEntry _ target _ sourceProgress targetProgress related edge
    obtain ⟨after, rest, finalRelated⟩ := ih nextRelated
    exact ⟨after, .next actual rest, finalRelated⟩

theorem StateRelated.halted_equal (plan : Plan) (source target : State)
    (related : StateRelated plan source target) (result : Except Core.Source.Fault (List Value))
    (halted : source.control = .halted result) (noCaller : source.stack = []) : source = target := by
  have controls := related.control
  rw [halted] at controls
  have targetHalted := ControlRelated.halted_inv plan result target.control controls
  have noTargetCaller : target.stack = [] := by
    have lengths := related.stack.length_eq
    simpa only [noCaller, List.length_nil, List.length_eq_zero_iff] using lengths.symm
  have frames := related.nextFrame
  have memory := related.memory
  cases source
  cases target
  simp_all only [State.mk.injEq]

theorem replace_target {S O : Type} (sourceStep : S → Option S) (sourceObserve : S → Option O)
    (targetObserve : State → Option O) (program : Program) (plan : Plan)
    (accepted : checkStructure program plan = true) (type : Core.Source.IntType)
    (sourceEntry targetEntry : State)
    (sourceProgress : ObservedProgress .ssa program type sourceEntry)
    (targetProgress : ObservedProgress .ssa program type targetEntry)
    (entries : StateRelated plan sourceEntry targetEntry)
    (observeAgreement : ∀ left right, StateRelated plan left right → targetObserve left = targetObserve right)
    (boundary nextBoundary : S → State → Prop) (entry : S)
    (fragment : Core.Transition.OpenCheckpoint sourceStep (step .ssa program)
      sourceObserve targetObserve boundary entry sourceEntry)
    (boundaryAgreement : ∀ source left right, boundary source left →
      RelatedFrom program plan sourceEntry targetEntry left right → nextBoundary source right) :
    Core.Transition.OpenCheckpoint sourceStep (step .ssa program)
      sourceObserve targetObserve nextBoundary entry targetEntry := by
  obtain ⟨original, atEntry, advance⟩ := fragment
  let joined := fun source target => ∃ old,
    original source old ∧ RelatedFrom program plan sourceEntry targetEntry old target
  refine ⟨joined, ⟨sourceEntry, atEntry, entries, .refl _, .refl _⟩, ?_⟩
  rintro source target ⟨old, included, related⟩
  rcases advance source old included with ⟨sf, tf, observation, sourcePath, targetPath,
      sourceStop, targetStop, sourceObserved, targetObserved⟩ | ⟨sf, tf, sourcePath, targetPath, next⟩
  · obtain ⟨n, counted⟩ := targetPath.counted
    obtain ⟨matched, matchedPath, states⟩ := prefixes_forward program plan accepted type
      sourceEntry targetEntry sourceProgress targetProgress related counted
    have matchedStop := StateRelated.stopped program plan matched tf
      (StateRelated.symmetric plan tf matched states.1) targetStop
    exact .inl ⟨sf, matched, observation, sourcePath, matchedPath.reaches,
      sourceStop, matchedStop, sourceObserved,
      (observeAgreement tf matched states.1).symm.trans targetObserved⟩
  · obtain ⟨n, positive, counted⟩ := Core.Transition.positive_counted targetPath
    obtain ⟨matched, matchedPath, states⟩ := prefixes_forward program plan accepted type
      sourceEntry targetEntry sourceProgress targetProgress related counted
    refine .inr ⟨sf, matched, sourcePath, matchedPath.positive positive, ?_⟩
    rcases next with repeated | returned
    · exact .inl ⟨tf, repeated, states⟩
    · exact .inr (boundaryAgreement sf tf matched returned states)

end EvoIR.Machine.Demand
