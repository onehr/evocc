import EvoIR.Core.Execution

/-! A kernel proof of an actual infinite source execution. This sanity check
uses neither evaluator fuel nor an axiom asserting that loops can diverge. -/
namespace EvoIR.Core.Source

private def loopPhase (locals : Locals) (k : List Frame) (functionId n : Nat) : State :=
  match n % 4 with
  | 0 => ⟨.expression (.literal .int 1), locals, .loopTest (.literal .int 1) none .null :: k, functionId⟩
  | 1 => ⟨.value (.ofNat .int 1), locals, .loopTest (.literal .int 1) none .null :: k, functionId⟩
  | 2 => ⟨.statement .null, locals, .loopBody (.literal .int 1) none .null :: k, functionId⟩
  | _ => ⟨.flow .normal, locals, .loopBody (.literal .int 1) none .null :: k, functionId⟩

private theorem loopPhase_step (p : Program) (locals : Locals) (k : List Frame) (functionId n : Nat) :
    Step p (loopPhase locals k functionId n) (loopPhase locals k functionId (n + 1)) := by
  have cases : n % 4 = 0 ∨ n % 4 = 1 ∨ n % 4 = 2 ∨ n % 4 = 3 := by omega
  rcases cases with h | h | h | h
  · have next : (n + 1) % 4 = 1 := by omega
    simp only [loopPhase, h, next]
    rfl
  · have next : (n + 1) % 4 = 2 := by omega
    simp only [loopPhase, h, next]
    rfl
  · have next : (n + 1) % 4 = 3 := by omega
    simp only [loopPhase, h, next]
    rfl
  · have next : (n + 1) % 4 = 0 := by omega
    simp only [loopPhase, h, next]
    rfl

theorem while_true_diverges (p : Program) (locals : Locals) (k : List Frame) (functionId : Nat) :
    Diverges p ⟨.statement (.while (.literal .int 1) .null), locals, k, functionId⟩ := by
  let trace : Nat → State
    | 0 => ⟨.statement (.while (.literal .int 1) .null), locals, k, functionId⟩
    | n + 1 => loopPhase locals k functionId n
  refine ⟨trace, rfl, ?_⟩
  intro n
  cases n with
  | zero => rfl
  | succ n => exact loopPhase_step p locals k functionId n

end EvoIR.Core.Source
