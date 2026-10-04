import EvoIR.Core.Simulation

/-! Cyclic checking at paired source/target cutpoints. Each nonterminal edge
must execute a positive finite path on both actual machines. This permits
different instruction counts without accepting an infinite administrative
cycle. Bounds below count proof paths, not a bounded program execution test. -/
namespace EvoIR.Core.Transition

inductive Path (step : α → Option α) : Nat → α → α → Prop where
  | refl (s) : Path step 0 s s
  | next {n s t u} : step s = some t → Path step n t u → Path step (n+1) s u

theorem Path.reaches {step : α → Option α} {n : Nat} {s t : α}
    (path : Path step n s t) : Reaches step s t := by
  induction path with
  | refl => exact .refl _
  | next edge _ ih => exact .next edge ih

theorem Reaches.counted {step : α → Option α} {s t : α}
    (path : Reaches step s t) : ∃ n, Path step n s t := by
  induction path with
  | refl => exact ⟨0, .refl _⟩
  | next edge _ ih =>
    obtain ⟨n, tail⟩ := ih
    exact ⟨n+1, .next edge tail⟩

theorem Path.positive {step : α → Option α} {n : Nat} {s t : α}
    (path : Path step n s t) (nonempty : 0 < n) :
    Relation.TransGen (Predecessor step) t s := by
  induction path with
  | refl => omega
  | @next n s t u edge rest ih =>
    cases n with
    | zero => cases rest; exact .single edge
    | succ n => exact .tail (ih (by omega)) edge

theorem positive_counted {step : α → Option α} {s t : α}
    (path : Relation.TransGen (Predecessor step) t s) :
    ∃ n, 0 < n ∧ Path step n s t := by
  induction path with
  | single edge => exact ⟨1, by decide, .next edge (.refl _)⟩
  | tail _ edge ih =>
    obtain ⟨n, _, path⟩ := ih
    exact ⟨n+1, by omega, .next edge path⟩

theorem Reaches.positive_of_ne {step : α → Option α} {s t : α}
    (path : Reaches step s t) (different : s ≠ t) :
    Relation.TransGen (Predecessor step) t s := by
  obtain ⟨n, path⟩ := path.counted
  cases n with
  | zero => cases path; exact False.elim (different rfl)
  | succ n => exact path.positive (by omega)

theorem Reaches.then_positive {step : α → Option α} {s t u : α}
    (path : Reaches step s t) (rest : Relation.TransGen (Predecessor step) u t) :
    Relation.TransGen (Predecessor step) u s := by
  induction path with
  | refl => exact rest
  | next edge _ ih => exact .tail (ih rest) edge

theorem Reaches.after_step {step : α → Option α} {s t u : α}
    (path : Reaches step t u) (first : step s = some t) :
    Relation.TransGen (Predecessor step) u s := by
  induction path generalizing s with
  | refl => exact .single first
  | next edge _ ih => exact .tail (ih edge) first

theorem positive_then_reaches {step : α → Option α} {s t u : α}
    (first : Relation.TransGen (Predecessor step) t s) (rest : Reaches step t u) :
    Relation.TransGen (Predecessor step) u s := by
  induction first with
  | single edge => exact rest.after_step edge
  | tail _ edge ih => exact .tail ih edge

/-- A finite checked chunk cannot run past a terminal of the same deterministic
machine. Its residual path is strictly shorter when the chunk is nonempty. -/
theorem Path.before_terminal {step : α → Option α} {k n : Nat} {s t final : α}
    (chunk : Path step k s t) (finish : Path step n s final) (stop : step final = none) :
    k ≤ n ∧ Path step (n-k) t final := by
  induction chunk generalizing n final with
  | refl => exact ⟨by omega, by simpa using finish⟩
  | @next k s next t edge tail ih =>
    cases finish with
    | refl => simp [stop] at edge
    | @next n _ other final first rest =>
      have same : next = other := Option.some.inj (edge.symm.trans first)
      subst other
      obtain ⟨length, path⟩ := ih rest stop
      exact ⟨by omega, by simpa using path⟩

/-- A table of paired cutpoints is accepted only with entry membership and
universally proved outgoing diagrams. The table may have cycles; the proofs
never assume that a reached cutpoint terminates. -/
structure CheckpointSimulation (sourceStep : S → Option S) (targetStep : T → Option T)
    (sourceObserve : S → Option O) (targetObserve : T → Option O)
    (related : S → T → Prop) : Prop where
  advance : ∀ s t, related s t →
    (∃ sf tf o, Reaches sourceStep s sf ∧ Reaches targetStep t tf ∧
      sourceStep sf = none ∧ targetStep tf = none ∧
      sourceObserve sf = some o ∧ targetObserve tf = some o) ∨
    (∃ s' t', Relation.TransGen (Predecessor sourceStep) s' s ∧
      Relation.TransGen (Predecessor targetStep) t' t ∧ related s' t')

namespace CheckpointSimulation

variable {sourceStep : S → Option S} {targetStep : T → Option T}
  {sourceObserve : S → Option O} {targetObserve : T → Option O}
  {related : S → T → Prop}
  (sim : CheckpointSimulation sourceStep targetStep sourceObserve targetObserve related)

include sim

theorem terminal {s : S} {t final : T} {n : Nat} (entry : related s t)
    (path : Path targetStep n t final) (stop : targetStep final = none) :
    ∃ sf o, Reaches sourceStep s sf ∧ sourceStep sf = none ∧
      sourceObserve sf = some o ∧ targetObserve final = some o := by
  induction n using Nat.strongRecOn generalizing s t with
  | ind n ih =>
    rcases sim.advance s t entry with ⟨sf, tf, o, sp, tp, ss, ts, so, to⟩ |
      ⟨s', t', sp, tp, next⟩
    · have same : final = tf := path.reaches.terminal_unique tp stop ts
      exact ⟨sf, o, sp, ss, so, same ▸ to⟩
    · obtain ⟨k, positive, chunk⟩ := positive_counted tp
      obtain ⟨length, rest⟩ := chunk.before_terminal path stop
      obtain ⟨sf, o, suffix, ss, so, to⟩ := ih (n-k) (by omega) next rest
      exact ⟨sf, o, (reaches_of_positive sp).trans suffix, ss, so, to⟩

theorem target_terminates {s : S} {t : T} (entry : related s t)
    (sourceAcc : Acc (Predecessor sourceStep) s) :
    ∃ final, Reaches targetStep t final ∧ targetStep final = none := by
  have strong := sourceAcc.transGen
  clear sourceAcc
  induction strong generalizing t with
  | intro s _ ih =>
    rcases sim.advance s t entry with ⟨sf, tf, o, sp, tp, ss, ts, so, to⟩ |
      ⟨s', t', sp, tp, next⟩
    · exact ⟨tf, tp, ts⟩
    · obtain ⟨final, tail, stopped⟩ := ih s' sp next
      exact ⟨final, (reaches_of_positive tp).trans tail, stopped⟩

theorem sound {s : S} {t : T} (entry : related s t) :
    Refines sourceStep targetStep sourceObserve targetObserve s t := by
  constructor
  · intro final path stop
    obtain ⟨n, counted⟩ := path.counted
    exact sim.terminal entry counted stop
  · intro infinite
    classical
    apply Classical.byContradiction
    intro notInfinite
    have sourceAcc : Acc (Predecessor sourceStep) s := by
      apply Classical.byContradiction
      intro notAcc
      exact notInfinite (infinite_of_not_accessible notAcc)
    obtain ⟨final, path, stop⟩ := sim.target_terminates entry sourceAcc
    exact path.not_infinite stop infinite
  · intro final path
    cases step : targetStep final with
    | some next => exact .inl ⟨next, rfl⟩
    | none =>
      obtain ⟨n, counted⟩ := path.counted
      obtain ⟨sf, o, _, _, _, observed⟩ := sim.terminal entry counted step
      exact .inr ⟨o, observed⟩

/-- An actual positive entry prefix may precede a checked simulation. The
new relation includes that entry; it cannot be justified by a zero-step loop. -/
theorem prepend {s t s' t'} (entry : related s' t')
    (sourcePath : Relation.TransGen (Predecessor sourceStep) s' s)
    (targetPath : Relation.TransGen (Predecessor targetStep) t' t) :
    Refines sourceStep targetStep sourceObserve targetObserve s t := by
  let extended := fun a b => (a = s ∧ b = t) ∨ related a b
  have joined : CheckpointSimulation sourceStep targetStep sourceObserve targetObserve extended := by
    constructor
    intro a b accepted
    rcases accepted with ⟨rfl, rfl⟩ | existing
    · exact .inr ⟨s', t', sourcePath, targetPath, Or.inr entry⟩
    · rcases sim.advance a b existing with terminal | ⟨a', b', ap, bp, next⟩
      · exact .inl terminal
      · exact .inr ⟨a', b', ap, bp, Or.inr next⟩
  exact joined.sound (Or.inl ⟨rfl, rfl⟩)

end CheckpointSimulation

theorem terminal_of_accessible {step : α → Option α} {s : α}
    (accessible : Acc (Predecessor step) s) :
    ∃ final, Reaches step s final ∧ step final = none := by
  induction accessible with
  | intro current _ ih =>
    cases edge : step current with
    | none => exact ⟨current, .refl _, edge⟩
    | some next =>
      obtain ⟨final, tail, stop⟩ := ih next edge
      exact ⟨final, .next edge tail, stop⟩

/-- Already proved continuations can be composed through the same cyclic
simulation interface. The relation is refinement itself; no search result or
finite execution sample is used to manufacture a continuation proof. -/
theorem refinement_simulation (sourceStep : S → Option S) (targetStep : T → Option T)
    (sourceObserve : S → Option O) (targetObserve : T → Option O) :
    CheckpointSimulation sourceStep targetStep sourceObserve targetObserve
      (Refines sourceStep targetStep sourceObserve targetObserve) := by
  constructor
  intro s t proof
  cases targetEdge : targetStep t with
  | none =>
    obtain ⟨sf, o, sp, ss, so, to⟩ := proof.terminal t (.refl _) targetEdge
    exact .inl ⟨sf, t, o, sp, .refl _, ss, targetEdge, so, to⟩
  | some t' =>
    cases sourceEdge : sourceStep s with
    | some s' =>
      exact .inr ⟨s', t', .single sourceEdge, .single targetEdge,
        proof.suffix (.next sourceEdge (.refl _)) (.next targetEdge (.refl _))⟩
    | none =>
      classical
      have accessible : Acc (Predecessor targetStep) t := by
        apply Classical.byContradiction
        intro notAccessible
        exact (Reaches.refl s).not_infinite sourceEdge
          (proof.divergence (infinite_of_not_accessible notAccessible))
      obtain ⟨tf, tp, ts⟩ := terminal_of_accessible accessible
      obtain ⟨sf, o, sp, ss, so, to⟩ := proof.terminal tf tp ts
      exact .inl ⟨sf, tf, o, sp, tp, ss, ts, so, to⟩

end EvoIR.Core.Transition
