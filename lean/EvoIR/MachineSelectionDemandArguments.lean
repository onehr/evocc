import EvoIR.MachineSelectionCallEntry
import EvoIR.MachineDemandCalls

/-! Check the actual complete BL vector modulo a previously proved demand
set. Undemanded positions may contain the lowerer's literal zero substitution,
but their positions, widths and total arity remain in the vector. The result
also supplies the auxiliary all-argument callee vector needed for progress;
it does not assert that demand analysis or a callee body is correct. -/
namespace EvoIR.Core.Symbolic.MachineSelection

structure DemandArgumentMatch (Γ : Var → Nat) (needed : List Nat)
    (parameters : List EvoIR.Machine.VReg) (source : List (Word Γ))
    (target : List (WordExpr Γ)) (checks : List (Guard Γ)) where
  substituted : List (Word Γ)
  widths : substituted.map Sigma.fst = source.map Sigma.fst
  actual : ∀ σ, guards σ checks = .ok () →
    target.map (fun word => EvoIR.Machine.Value.int (word.denote σ)) = machineInputs σ substituted
  agreement : ∀ σ, EvoIR.Machine.BindingAgreement needed
    (parameters.zip (machineInputs σ source)) (parameters.zip (machineInputs σ substituted))

def checkDemandArguments {Γ : Var → Nat} (needed : List Nat) (checks : List (Guard Γ)) :
    (parameters : List EvoIR.Machine.VReg) → (source : List (Word Γ)) →
    (target : List (WordExpr Γ)) → Option (DemandArgumentMatch Γ needed parameters source target checks)
  | [], [], [] => some ⟨[], rfl, by intro σ passed; rfl, by intro σ; exact .nil⟩
  | param :: parameters, word :: source, actual :: target => do
    let selected : Word Γ := if param.id ∈ needed then word else ⟨word.1, .const word.1 0⟩
    let head ← checkWordUnder checks selected actual
    let rest ← checkDemandArguments needed checks parameters source target
    return {
      substituted := selected :: rest.substituted
      widths := by
        simp only [List.map_cons, selected]
        split <;> exact congrArg (_ :: ·) rest.widths
      actual := by
        intro σ passed
        simp only [List.map_cons, machineInputs, head.down σ passed]
        exact congrArg (EvoIR.Machine.Value.int ((denoteTerm σ selected.2).setWidth 64) :: ·)
          (rest.actual σ passed)
      agreement := by
        intro σ
        simp only [machineInputs, List.map_cons, List.zip_cons_cons]
        apply EvoIR.Machine.BindingAgreement.cons param
        · intro used
          have chosen : selected = word := by simp [selected, used]
          rw [chosen]
        · exact rest.agreement σ }
  | _, _, _ => none

end EvoIR.Core.Symbolic.MachineSelection
