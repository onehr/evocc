import EvoIR.Core.Simulation

/-! Kernel-checked regressions for the local simulation contract. These test
the proof interface, not admission of additional compiler inputs. -/
namespace EvoIR.Core.Transition.SimulationTests

private def count : Nat → Option Nat
  | 0 => none
  | n+1 => some n

private def delayed : Nat × Bool → Option (Nat × Bool)
  | (0, _) => none
  | (n+1, false) => some (n+1, true)
  | (n+1, true) => some (n, false)

private def observed (n : Nat) : Option Nat := if n == 0 then some 0 else none
private def related (n : Nat) (t : Nat × Bool) : Prop := n = t.1
private def rank (_ : Nat) (t : Nat × Bool) : Nat := if t.2 then 0 else 1

/-- An arbitrary number of source steps, each with one extra target-only
step, is accepted. The administrative rank may reset after source progress. -/
theorem delayed_simulation : RankedSimulation count delayed observed
    (fun t => observed t.1) related rank := by
  constructor
  · intro s t t' hr edge
    rcases t with ⟨n, phase⟩
    cases hr
    cases n with
    | zero => simp [delayed] at edge
    | succ n =>
      cases phase with
      | false =>
        simp only [delayed, Option.some.injEq] at edge
        subst t'
        exact .inr ⟨rfl, by simp [rank]⟩
      | true =>
        simp only [delayed, Option.some.injEq] at edge
        subst t'
        exact .inl ⟨n, .single rfl, rfl⟩
  · intro s t hr stopped
    rcases t with ⟨n, phase⟩
    cases hr
    cases n with
    | zero => exact ⟨0, 0, .refl _, rfl, rfl, rfl⟩
    | succ n => cases phase <;> simp [delayed] at stopped
  · intro s t _
    rcases t with ⟨n, phase⟩
    cases n with
    | zero => exact .inr ⟨0, rfl⟩
    | succ n =>
      cases phase with
      | false => exact .inl ⟨(n+1, true), rfl⟩
      | true => exact .inl ⟨(n, false), rfl⟩

theorem delayed_refines (n : Nat) :
    Refines count delayed observed (fun t => observed t.1) n (n, false) :=
  delayed_simulation.sound rfl

private def stopped (_ : Unit) : Option Unit := none
private def spinning (_ : Unit) : Option Unit := some ()

/-- Without the strict rank decrease, a target-only infinite loop could pass
the ordinary successor relation. No rank assignment can certify this cycle. -/
theorem cannot_add_infinite_stutter (rank : Unit → Unit → Nat) :
    ¬ RankedSimulation stopped spinning (fun _ => some (0 : Nat))
      (fun _ => none) (fun _ _ => True) rank := by
  intro sim
  rcases sim.transfer () () () trivial rfl with ⟨s', path, _⟩ | ⟨_, decrease⟩
  · cases path with
    | single h => cases h
    | tail _ h => cases h
  · exact Nat.lt_irrefl _ decrease

/-- Matching halted states is insufficient when their observations differ. -/
theorem cannot_change_terminal_observation :
    ¬ RankedSimulation stopped stopped (fun _ => some (3004 : Nat))
      (fun _ => some 0) (fun _ _ => True) (fun _ _ => 0) := by
  intro sim
  obtain ⟨_, o, _, _, left, right⟩ := sim.terminal () () trivial rfl
  have eq : (3004 : Nat) = 0 := Option.some.inj (left.trans right.symm)
  contradiction

/-- A false entry relation cannot turn vacuous local obligations into proof
admission. The soundness theorem requires membership of the actual entry. -/
example : ¬ (fun (_ : Unit) (_ : Unit) => False) () () := by simp

end EvoIR.Core.Transition.SimulationTests
