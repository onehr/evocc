import EvoIR.Core.ValidatorStatementFacts

/-! Inversion of the existing source checker's for-loop branch. Each equation
below is a component of the accepted checkStmt computation, not a replacement
validator or a condition imposed on future execution. -/
namespace EvoIR.Core.Source

def validatorForInitCheck (callees : List Function) (fuel : Nat)
    (input : LocalState) (init : ForInit) : Except String LocalState :=
  match init with
  | .empty => pure input
  | .expression e => expressionState callees fuel input e
  | .declarations objects => objects.foldlM (declare callees fuel) input

def validatorForConditionCheck (callees : List Function) (fuel : Nat)
    (start : LocalState) (condition : Option Expr) : Except String LocalState :=
  match condition with
  | none => pure start
  | some e => expressionState callees fuel start e

theorem checked_for_statement (callees : List Function) (result : QualifiedType)
    (fuel depth : Nat) (input : LocalState) (init : ForInit)
    (condition increment : Option Expr) (body : Stmt) (facts : FlowFacts)
    (accepted : checkStmt callees result fuel depth input
      (.for init condition increment body) = .ok facts) :
    ∃ innerFuel start incoming bodyFacts, fuel = innerFuel + 1 ∧
      validatorForInitCheck callees innerFuel input init = .ok start ∧
      validatorForConditionCheck callees innerFuel start condition = .ok incoming ∧
      checkStmt callees result innerFuel (depth + 1) incoming body = .ok bodyFacts ∧
      (∀ e, increment = some e →
        ∃ out, expressionState callees innerFuel incoming e = .ok out) ∧
      facts = ⟨restoreScope input incoming, true⟩ := by
  cases fuel with
  | zero => simp [checkStmt] at accepted
  | succ fuel =>
    have expanded : checkStmt callees result (fuel + 1) depth input
        (.for init condition increment body) = (do
          let start ← validatorForInitCheck callees fuel input init
          let incoming ← validatorForConditionCheck callees fuel start condition
          let _ ← checkStmt callees result fuel (depth + 1) incoming body
          match increment with
          | none => pure ⟨restoreScope input incoming, true⟩
          | some e => do
            let _ ← expressionState callees fuel incoming e
            pure ⟨restoreScope input incoming, true⟩) := by
      cases init <;> cases condition <;> cases increment <;> rfl
    rw [expanded] at accepted
    obtain ⟨start, startChecked, remaining⟩ := except_bind_success _ _ _ accepted
    obtain ⟨incoming, conditionChecked, remaining⟩ := except_bind_success _ _ _ remaining
    obtain ⟨bodyFacts, bodyChecked, remaining⟩ := except_bind_success _ _ _ remaining
    refine ⟨fuel, start, incoming, bodyFacts, rfl, startChecked, conditionChecked,
      bodyChecked, ?_⟩
    cases increment with
    | none =>
      constructor
      · intro e impossible
        cases impossible
      · simpa [pure, Except.pure, bind, Except.bind] using remaining.symm
    | some e =>
      obtain ⟨out, incrementChecked, returned⟩ := except_bind_success _ _ _ remaining
      constructor
      · intro other same
        cases same
        exact ⟨out, incrementChecked⟩
      · simpa [pure, Except.pure, bind, Except.bind] using returned.symm

end EvoIR.Core.Source
