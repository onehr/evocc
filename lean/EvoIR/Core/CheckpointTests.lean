import EvoIR.Core.CheckpointSimulation

/-! Cyclic simulation regressions, quantified over unbounded path lengths. -/
namespace EvoIR.Core.Transition.CheckpointTests

private def countdown : Nat → Option Nat
  | 0 => none
  | n+1 => some n

private def observed (n : Nat) : Option Nat := if n == 0 then some 0 else none

/-- A target chunk takes two source steps, without requiring a finite bound
on the incoming counter. -/
theorem two_step_chunks : CheckpointSimulation countdown countdown observed observed
    (fun s t => s = t) := by
  constructor
  intro s t same
  subst t
  cases s with
  | zero => exact .inl ⟨0, 0, 0, .refl _, .refl _, rfl, rfl, rfl, rfl⟩
  | succ n =>
    cases n with
    | zero => exact .inr ⟨0, 0, .single rfl, .single rfl, rfl⟩
    | succ n =>
      have path : Relation.TransGen (Predecessor countdown) n (n+2) :=
        .tail (.single (show countdown (n+1) = some n from rfl)) rfl
      exact .inr ⟨n, n, path, path, rfl⟩

theorem all_counters_refine (n : Nat) :
    Refines countdown countdown observed observed n n := two_step_chunks.sound rfl

private def stopped (_ : Unit) : Option Unit := none
private def spinning (_ : Unit) : Option Unit := some ()

theorem cannot_add_infinite_stutter :
    ¬ CheckpointSimulation stopped spinning (fun _ => some (0 : Nat))
      (fun _ => none) (fun _ _ => True) := by
  intro sim
  rcases sim.advance () () trivial with ⟨_, _, _, _, _, _, halt, _, _⟩ |
    ⟨s, _, positive, _, _⟩
  · cases halt
  · cases positive with
    | single h => cases h
    | tail _ h => cases h

theorem cannot_change_diagnostic :
    ¬ CheckpointSimulation stopped stopped (fun _ => some (3004 : Nat))
      (fun _ => some 0) (fun _ _ => True) := by
  intro sim
  rcases sim.advance () () trivial with ⟨_, _, _, _, _, _, _, left, right⟩ |
    ⟨s, _, positive, _, _⟩
  · have same : (3004 : Nat) = 0 := Option.some.inj (left.trans right.symm)
    contradiction
  · cases positive with
    | single h => cases h
    | tail _ h => cases h

example : ¬ (fun (_ : Unit) (_ : Unit) => False) () () := by simp

end EvoIR.Core.Transition.CheckpointTests
