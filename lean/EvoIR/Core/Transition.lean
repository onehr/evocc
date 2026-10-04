/-!
Deterministic transition facts used by the compilation correspondence proofs.
Finite paths and infinite executions are relations, not evaluator timeouts.
A terminal witness rules out all alternative outcomes and infinite traces.
-/
namespace EvoIR.Core.Transition

inductive Reaches (step : α → Option α) : α → α → Prop where
  | refl (s) : Reaches step s s
  | next {s t u} : step s = some t → Reaches step t u → Reaches step s u

def Infinite (step : α → Option α) (s : α) : Prop :=
  ∃ trace : Nat → α, trace 0 = s ∧ ∀ n, step (trace n) = some (trace (n+1))

theorem Reaches.from_terminal {step : α → Option α} {s t : α}
    (h : Reaches step s t) (stop : step s = none) : t = s := by
  cases h with
  | refl => rfl
  | next first _ => simp [stop] at first

/-- Every prefix of a deterministic terminating execution can finish at the
same terminal state. In particular, no reachable malformed terminal is hidden
by the existence of a different successful run. -/
theorem Reaches.finish_from_prefix {step : α → Option α} {s t u : α}
    (path : Reaches step s t) (finish : Reaches step s u)
    (stop : step u = none) : Reaches step t u := by
  induction path with
  | refl => exact finish
  | @next s next t first rest ih =>
    cases finish with
    | refl => simp [stop] at first
    | @next _ other _ first' rest' =>
      have same : next = other := Option.some.inj (first.symm.trans first')
      subst other
      exact ih rest'

theorem Reaches.terminal_unique {step : α → Option α} {s t u : α}
    (left : Reaches step s t) (right : Reaches step s u)
    (ht : step t = none) (hu : step u = none) : t = u := by
  exact ((left.finish_from_prefix right hu).from_terminal ht).symm

theorem Reaches.not_infinite {step : α → Option α} {s t : α}
    (finish : Reaches step s t) (stop : step t = none) : ¬ Infinite step s := by
  induction finish with
  | refl =>
    rintro ⟨trace, entry, edges⟩
    have h := edges 0
    simp [entry, stop] at h
  | @next s next t first rest ih =>
    rintro ⟨trace, entry, edges⟩
    have h := edges 0
    rw [entry, first] at h
    have same : next = trace 1 := Option.some.inj h
    apply ih stop
    exact ⟨fun n => trace (n+1), same.symm, fun n => edges (n+1)⟩

/-- Backward observational refinement with progress. An observation is only a
real return or supported diagnostic, never malformed execution or a timeout.
This does not assume source/target steps run at the same rate. -/
structure Refines (sourceStep : S → Option S) (targetStep : T → Option T)
    (sourceObserve : S → Option O) (targetObserve : T → Option O)
    (sourceEntry : S) (targetEntry : T) : Prop where
  terminal : ∀ t, Reaches targetStep targetEntry t → targetStep t = none →
    ∃ s o, Reaches sourceStep sourceEntry s ∧ sourceStep s = none ∧
      sourceObserve s = some o ∧ targetObserve t = some o
  divergence : Infinite targetStep targetEntry → Infinite sourceStep sourceEntry
  progress : ∀ t, Reaches targetStep targetEntry t →
    (∃ next, targetStep t = some next) ∨ ∃ o, targetObserve t = some o

/-- A universally proved terminating fragment needs no arbitrary execution
bound in its refinement statement. Determinism closes all finite/infinite runs,
not merely the particular witnesses used to establish termination. -/
theorem refines_of_terminals {sourceStep : S → Option S} {targetStep : T → Option T}
    {sourceObserve : S → Option O} {targetObserve : T → Option O}
    {sourceEntry sourceFinal : S} {targetEntry targetFinal : T} {o : O}
    (hs : Reaches sourceStep sourceEntry sourceFinal)
    (ht : Reaches targetStep targetEntry targetFinal)
    (ss : sourceStep sourceFinal = none) (st : targetStep targetFinal = none)
    (os : sourceObserve sourceFinal = some o) (ot : targetObserve targetFinal = some o) :
    Refines sourceStep targetStep sourceObserve targetObserve sourceEntry targetEntry := by
  constructor
  · intro t path stop
    have same := path.terminal_unique ht stop st
    subst t
    exact ⟨sourceFinal, o, hs, ss, os, ot⟩
  · intro infinite
    exact False.elim (ht.not_infinite st infinite)
  · intro t path
    cases h : targetStep t with
    | some next => exact Or.inl ⟨next, rfl⟩
    | none =>
      have same := path.terminal_unique ht h st
      subst t
      exact Or.inr ⟨o, ot⟩

end EvoIR.Core.Transition
