import EvoIR.Core.CheckpointSimulation

/-! Open, productive simulation fragments. A fragment may return to a caller
cutpoint without assuming that the caller already refines. Closing the table
checks every such boundary. Every boundary transfer makes positive progress on
both actual machines; an identity fragment cannot certify a cyclic invariant. -/
namespace EvoIR.Core.Transition

def OpenCheckpoint (sourceStep : S → Option S) (targetStep : T → Option T)
    (sourceObserve : S → Option O) (targetObserve : T → Option O)
    (boundary : S → T → Prop) (s : S) (t : T) : Prop :=
  ∃ related : S → T → Prop, related s t ∧ ∀ a b, related a b →
    (∃ sf tf o, Reaches sourceStep a sf ∧ Reaches targetStep b tf ∧
      sourceStep sf = none ∧ targetStep tf = none ∧
      sourceObserve sf = some o ∧ targetObserve tf = some o) ∨
    (∃ a' b', Relation.TransGen (Predecessor sourceStep) a' a ∧
      Relation.TransGen (Predecessor targetStep) b' b ∧
      (related a' b' ∨ boundary a' b'))

namespace OpenCheckpoint

variable {sourceStep : S → Option S} {targetStep : T → Option T}
  {sourceObserve : S → Option O} {targetObserve : T → Option O}
  {boundary nextBoundary : S → T → Prop} {s s' : S} {t t' : T}

theorem of_refines (proof : Refines sourceStep targetStep sourceObserve targetObserve s t) :
    OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s t := by
  refine ⟨Refines sourceStep targetStep sourceObserve targetObserve, proof, ?_⟩
  intro a b related
  rcases (refinement_simulation sourceStep targetStep sourceObserve targetObserve).advance a b related
      with terminal | ⟨a', b', sp, tp, next⟩
  · exact .inl terminal
  · exact .inr ⟨a', b', sp, tp, .inl next⟩

theorem exit (entry : boundary s' t')
    (sp : Relation.TransGen (Predecessor sourceStep) s' s)
    (tp : Relation.TransGen (Predecessor targetStep) t' t) :
    OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s t := by
  refine ⟨fun a b => a = s ∧ b = t, ⟨rfl, rfl⟩, ?_⟩
  rintro a b ⟨rfl, rfl⟩
  exact .inr ⟨s', t', sp, tp, .inr entry⟩

/-- Finite prefixes do not need to be positive: the fragment they lead into
already enforces positive progress before each open boundary. -/
theorem prepend (tail : OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s' t')
    (sp : Reaches sourceStep s s') (tp : Reaches targetStep t t') :
    OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s t := by
  obtain ⟨related, entry, advance⟩ := tail
  refine ⟨fun a b => (a = s ∧ b = t) ∨ related a b, .inl ⟨rfl, rfl⟩, ?_⟩
  intro a b included
  rcases included with ⟨rfl, rfl⟩ | included
  · rcases advance s' t' entry with ⟨sf, tf, o, ss, ts, stopS, stopT, so, to⟩ |
        ⟨a', b', ss, ts, next⟩
    · exact .inl ⟨sf, tf, o, sp.trans ss, tp.trans ts, stopS, stopT, so, to⟩
    · exact .inr ⟨a', b', sp.then_positive ss, tp.then_positive ts,
        next.elim (fun h => .inl (.inr h)) Or.inr⟩
  · rcases advance a b included with terminal | ⟨a', b', ss, ts, next⟩
    · exact .inl terminal
    · exact .inr ⟨a', b', ss, ts, next.elim (fun h => .inl (.inr h)) Or.inr⟩

/-- A call return either is already a caller cutpoint or enters a separately
checked suffix. This does not demand a whole-caller theorem in advance. -/
theorem bind (first : OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s t)
    (continueAt : ∀ a b, boundary a b → nextBoundary a b ∨
      OpenCheckpoint sourceStep targetStep sourceObserve targetObserve nextBoundary a b) :
    OpenCheckpoint sourceStep targetStep sourceObserve targetObserve nextBoundary s t := by
  obtain ⟨related, entry, advance⟩ := first
  let suffix := fun a b => ∃ r : S → T → Prop,
    (∀ x y, r x y →
      (∃ sf tf o, Reaches sourceStep x sf ∧ Reaches targetStep y tf ∧
        sourceStep sf = none ∧ targetStep tf = none ∧
        sourceObserve sf = some o ∧ targetObserve tf = some o) ∨
      (∃ x' y', Relation.TransGen (Predecessor sourceStep) x' x ∧
        Relation.TransGen (Predecessor targetStep) y' y ∧ (r x' y' ∨ nextBoundary x' y'))) ∧ r a b
  have atBoundary : ∀ a b, boundary a b → suffix a b ∨ nextBoundary a b := by
    intro a b h
    rcases continueAt a b h with done | ⟨r, start, step⟩
    · exact .inr done
    · exact .inl ⟨r, step, start⟩
  refine ⟨fun a b => related a b ∨ suffix a b, .inl entry, ?_⟩
  intro a b included
  rcases included with included | ⟨r, step, included⟩
  · rcases advance a b included with terminal | ⟨a', b', sp, tp, next⟩
    · exact .inl terminal
    · refine .inr ⟨a', b', sp, tp, ?_⟩
      rcases next with same | boundaryHere
      · exact .inl (.inl same)
      · exact (atBoundary a' b' boundaryHere).elim (fun h => .inl (.inr h)) Or.inr
  · rcases step a b included with terminal | ⟨a', b', sp, tp, next⟩
    · exact .inl terminal
    · exact .inr ⟨a', b', sp, tp,
        next.elim (fun h => .inl (.inr ⟨r, step, h⟩)) Or.inr⟩

/-- Close all proposed cutpoints together. Each entry must supply its own
productive fragment. Merely asserting membership in the table cannot close it. -/
theorem close (table : ∀ a b, boundary a b →
      OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary a b)
    (entry : boundary s t) :
    Refines sourceStep targetStep sourceObserve targetObserve s t := by
  let joined := fun a b => ∃ r : S → T → Prop,
    (∀ x y, r x y →
      (∃ sf tf o, Reaches sourceStep x sf ∧ Reaches targetStep y tf ∧
        sourceStep sf = none ∧ targetStep tf = none ∧
        sourceObserve sf = some o ∧ targetObserve tf = some o) ∨
      (∃ x' y', Relation.TransGen (Predecessor sourceStep) x' x ∧
        Relation.TransGen (Predecessor targetStep) y' y ∧ (r x' y' ∨ boundary x' y'))) ∧ r a b
  have includeBoundary : ∀ a b, boundary a b → joined a b := by
    intro a b h
    obtain ⟨r, start, step⟩ := table a b h
    exact ⟨r, step, start⟩
  have simulation : CheckpointSimulation sourceStep targetStep sourceObserve targetObserve joined := by
    constructor
    rintro a b ⟨r, step, included⟩
    rcases step a b included with terminal | ⟨a', b', sp, tp, next⟩
    · exact .inl terminal
    · exact .inr ⟨a', b', sp, tp,
        next.elim (fun h => ⟨r, step, h⟩) (includeBoundary a' b')⟩
  exact simulation.sound (includeBoundary s t entry)

/-- The historical whole-suffix interface is a special case of open
composition, so it need not maintain a second call-entry proof. -/
theorem finish (fragment : OpenCheckpoint sourceStep targetStep sourceObserve targetObserve boundary s t)
    (continuation : ∀ a b, boundary a b →
      Refines sourceStep targetStep sourceObserve targetObserve a b) :
    Refines sourceStep targetStep sourceObserve targetObserve s t := by
  have closed := fragment.bind (nextBoundary := fun _ _ => False)
    (fun a b h => .inr (.of_refines (continuation a b h)))
  obtain ⟨related, entry, advance⟩ := closed
  have simulation : CheckpointSimulation sourceStep targetStep sourceObserve targetObserve related := by
    constructor
    intro a b h
    rcases advance a b h with terminal | ⟨a', b', sp, tp, next⟩
    · exact .inl terminal
    · exact .inr ⟨a', b', sp, tp, next.resolve_right id⟩
  exact simulation.sound entry

end OpenCheckpoint
end EvoIR.Core.Transition
