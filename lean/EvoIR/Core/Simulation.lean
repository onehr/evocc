import EvoIR.Core.Transition

/-!
Local simulation obligations for result checking, independent of a particular
compiler pass. A certificate may propose a relation and an administrative rank;
neither is trusted. The caller must prove each local obligation from the two
actual programs. This module is not itself a program-pair admission procedure.

Positive source paths permit different machine granularities. A target-only
step must strictly decrease a natural rank, including across loop edges. This
rules out introducing an infinite administrative execution. No execution fuel
or finite test population appears in these statements.
-/
namespace EvoIR.Core.Transition

def Predecessor (step : α → Option α) (next current : α) : Prop :=
  step current = some next

theorem Reaches.trans {step : α → Option α} {a b c : α}
    (ab : Reaches step a b) (bc : Reaches step b c) : Reaches step a c := by
  induction ab with
  | refl => exact bc
  | next h _ ih => exact .next h (ih bc)

theorem Reaches.infinite_suffix {step : α → Option α} {a b : α}
    (path : Reaches step a b) (infinite : Infinite step a) : Infinite step b := by
  induction path with
  | refl => exact infinite
  | @next a next b edge rest ih =>
    obtain ⟨trace, entry, edges⟩ := infinite
    have same : next = trace 1 := Option.some.inj (edge.symm.trans (by simpa [entry] using edges 0))
    exact ih ⟨fun n => trace (n+1), same.symm, fun n => edges (n+1)⟩

theorem Reaches.infinite_prefix {step : α → Option α} {a b : α}
    (path : Reaches step a b) (infinite : Infinite step b) : Infinite step a := by
  induction path with
  | refl => exact infinite
  | @next a next b edge rest ih =>
    obtain ⟨trace, entry, edges⟩ := ih infinite
    refine ⟨(fun n => match n with | 0 => a | n+1 => trace n), rfl, ?_⟩
    intro n
    cases n with
    | zero => simpa only [entry] using edge
    | succ n => exact edges n

/-- Attach finite, actual prefixes to an already proved continuation. One
machine may already be at the call site, so either prefix may have zero steps;
all subsequent divergence and progress obligations still come from `tail`. -/
theorem Refines.prepend {sourceStep : S → Option S} {targetStep : T → Option T}
    {sourceObserve : S → Option O} {targetObserve : T → Option O} {s s' : S} {t t' : T}
    (tail : Refines sourceStep targetStep sourceObserve targetObserve s' t')
    (sourcePath : Reaches sourceStep s s') (targetPath : Reaches targetStep t t') :
    Refines sourceStep targetStep sourceObserve targetObserve s t := by
  have terminal : ∀ final, Reaches targetStep t final → targetStep final = none →
      ∃ sf o, Reaches sourceStep s sf ∧ sourceStep sf = none ∧
        sourceObserve sf = some o ∧ targetObserve final = some o := by
    intro final path stop
    obtain ⟨sf, o, path, ss, so, to⟩ := tail.terminal final (targetPath.finish_from_prefix path stop) stop
    exact ⟨sf, o, sourcePath.trans path, ss, so, to⟩
  refine ⟨terminal, ?_, ?_⟩
  · intro infinite
    exact sourcePath.infinite_prefix (tail.divergence (targetPath.infinite_suffix infinite))
  · intro final path
    cases h : targetStep final with
    | some next => exact .inl ⟨next, rfl⟩
    | none =>
      obtain ⟨_, o, _, _, _, observed⟩ := terminal final path h
      exact .inr ⟨o, observed⟩

theorem reaches_of_positive {step : α → Option α} {a b : α}
    (h : Relation.TransGen (Predecessor step) b a) : Reaches step a b := by
  induction h with
  | single edge => exact .next edge (.refl _)
  | tail _ edge ih => exact .next edge ih

theorem not_infinite_of_accessible {step : α → Option α} {s : α}
    (h : Acc (Predecessor step) s) : ¬ Infinite step s := by
  induction h with
  | intro s _ ih =>
    rintro ⟨trace, entry, edges⟩
    have first : step s = some (trace 1) := by simpa [entry] using edges 0
    exact ih (trace 1) first ⟨fun n => trace (n+1), rfl, fun n => edges (n+1)⟩

private def trajectory (step : α → Option α) (s : α) : Nat → α
  | 0 => s
  | n+1 => (step (trajectory step s n)).getD (trajectory step s n)

theorem infinite_of_not_accessible {step : α → Option α} {s : α}
    (h : ¬ Acc (Predecessor step) s) : Infinite step s := by
  classical
  have next : ∀ a, ¬ Acc (Predecessor step) a →
      ∃ b, step a = some b ∧ ¬ Acc (Predecessor step) b := by
    intro a notAcc
    cases edge : step a with
    | none =>
      apply False.elim
      apply notAcc
      exact .intro a (by intro b hb; simp [Predecessor, edge] at hb)
    | some b =>
      refine ⟨b, rfl, ?_⟩
      intro acc
      apply notAcc
      refine .intro a ?_
      intro c hc
      have same : b = c := Option.some.inj (edge.symm.trans hc)
      simpa [← same] using acc
  have all : ∀ n, ¬ Acc (Predecessor step) (trajectory step s n) := by
    intro n
    induction n with
    | zero => exact h
    | succ n ih =>
      obtain ⟨b, edge, hb⟩ := next _ ih
      simpa [trajectory, edge] using hb
  refine ⟨trajectory step s, rfl, ?_⟩
  intro n
  obtain ⟨b, edge, _⟩ := next _ (all n)
  simp [trajectory, edge]

/-- Pass-local proof obligations. Unknown effects, malformed states and wrong
diagnostics cannot be hidden by choosing an empty relation: entry membership,
successor closure, terminal observations and progress all remain obligations. -/
structure RankedSimulation (sourceStep : S → Option S) (targetStep : T → Option T)
    (sourceObserve : S → Option O) (targetObserve : T → Option O)
    (related : S → T → Prop) (rank : S → T → Nat) : Prop where
  transfer : ∀ s t t', related s t → targetStep t = some t' →
    (∃ s', Relation.TransGen (Predecessor sourceStep) s' s ∧ related s' t') ∨
    (related s t' ∧ rank s t' < rank s t)
  terminal : ∀ s t, related s t → targetStep t = none →
    ∃ s' o, Reaches sourceStep s s' ∧ sourceStep s' = none ∧
      sourceObserve s' = some o ∧ targetObserve t = some o
  progress : ∀ s t, related s t →
    (∃ t', targetStep t = some t') ∨ ∃ o, targetObserve t = some o

namespace RankedSimulation

variable {sourceStep : S → Option S} {targetStep : T → Option T}
  {sourceObserve : S → Option O} {targetObserve : T → Option O}
  {related : S → T → Prop} {rank : S → T → Nat}
  (sim : RankedSimulation sourceStep targetStep sourceObserve targetObserve related rank)

include sim

theorem prefixes {s : S} {t t' : T} (entry : related s t)
    (path : Reaches targetStep t t') :
    ∃ s', Reaches sourceStep s s' ∧ related s' t' := by
  induction path generalizing s with
  | refl => exact ⟨s, .refl s, entry⟩
  | @next t u v edge _ ih =>
    rcases sim.transfer s t u entry edge with ⟨s', path, hr⟩ | ⟨hr, _⟩
    · obtain ⟨s'', rest, last⟩ := ih hr
      exact ⟨s'', (reaches_of_positive path).trans rest, last⟩
    · exact ih hr

theorem accessible {s : S} {t : T} (entry : related s t)
    (sourceAcc : Acc (Predecessor sourceStep) s) : Acc (Predecessor targetStep) t := by
  have strong := sourceAcc.transGen
  clear sourceAcc
  induction strong generalizing t with
  | intro s _ ih =>
    revert entry
    refine (InvImage.wf (rank s) Nat.lt_wfRel.wf).induction
      (C := fun u => related s u → Acc (Predecessor targetStep) u) t ?_
    intro t smaller entry
    refine Acc.intro t ?_
    intro t' edge
    rcases sim.transfer s t t' entry edge with ⟨s', path, hr⟩ | ⟨hr, lower⟩
    · exact ih s' path hr
    · exact smaller t' lower hr

theorem sound {s : S} {t : T} (entry : related s t) :
    Refines sourceStep targetStep sourceObserve targetObserve s t := by
  constructor
  · intro t' path stopped
    obtain ⟨s', before, hr⟩ := sim.prefixes entry path
    obtain ⟨s'', o, suffix, stop, os, ot⟩ := sim.terminal s' t' hr stopped
    exact ⟨s'', o, before.trans suffix, stop, os, ot⟩
  · intro infinite
    classical
    apply Classical.byContradiction
    intro notInfinite
    have sourceAcc : Acc (Predecessor sourceStep) s := by
      apply Classical.byContradiction
      intro notAcc
      exact notInfinite (infinite_of_not_accessible notAcc)
    exact not_infinite_of_accessible (sim.accessible entry sourceAcc) infinite
  · intro t' path
    obtain ⟨s', _, hr⟩ := sim.prefixes entry path
    exact sim.progress s' t' hr

end RankedSimulation

/-- Compose independently checked stages without assuming matching step counts
or silently dropping diagnostic/divergence obligations at an intermediate IR. -/
theorem Refines.trans
    {firstStep : A → Option A} {middleStep : B → Option B} {lastStep : C → Option C}
    {firstObserve : A → Option O} {middleObserve : B → Option O} {lastObserve : C → Option O}
    {a : A} {b : B} {c : C}
    (ab : Refines firstStep middleStep firstObserve middleObserve a b)
    (bc : Refines middleStep lastStep middleObserve lastObserve b c) :
    Refines firstStep lastStep firstObserve lastObserve a c := by
  constructor
  · intro c' path stop
    obtain ⟨b', o, bp, bs, bo, co⟩ := bc.terminal c' path stop
    obtain ⟨a', o', ap, ast, ao, bo'⟩ := ab.terminal b' bp bs
    have same : o' = o := Option.some.inj (bo'.symm.trans bo)
    exact ⟨a', o, ap, ast, same ▸ ao, co⟩
  · exact fun h => ab.divergence (bc.divergence h)
  · exact bc.progress

end EvoIR.Core.Transition
