import EvoIR.Core.PrivateChecker
import EvoIR.Core.CallingCutpoint

/-! Memory-aware target span endpoints. A bank belongs to one inspected span;
successors quantify fresh banks, so the bank length never bounds execution. -/
namespace EvoIR.Core.Symbolic

inductive PrivateExit (Γ : Var → Nat) where
  | next (label : String) (pc : Nat) (config : PrivateConfig Γ)
  | returned (word : CWord Γ) (config : PrivateConfig Γ)

def privateResults {Γ : Var → Nat} (tokens : Bool) (config : PrivateConfig Γ)
    (word : CWord Γ) : List (Slot Γ) :=
  [.word word.word] ++ if tokens then [.opaque config.memory, .opaque config.world] else []

def privateReturnValues (tokens : Bool) (value : Source.Value) (memory : IR.Memory)
    (owner : Nat) : List IR.Value :=
  [sourceValueIR value] ++ if tokens then [.memory (memory.endFrame owner), .world] else []

def PrivateExit.Realizes {Γ : Var → Nat} (σ : Subst Γ) (incoming : IR.Memory)
    (tokens : Bool) (f : IR.Function) (owner : Nat) (stack : List IR.ReturnFrame)
    (exit : PrivateExit Γ) (final : IR.State) : Prop :=
  match exit with
  | .next label pc config => ∃ values frame,
      config.Realizes σ values incoming owner frame ∧
      final = privateState σ values f label pc owner frame config stack
  | .returned word config => ∃ values memory frame,
      config.Realizes σ values incoming owner frame ∧ config.memoryValue values = some memory ∧
      final = cutResume (privateReturnValues tokens (word.denote σ) memory owner) frame stack

def PrivateCutRuns {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (tokens : Bool)
    (advance : Bool) (label : String) (pc : Nat) (config : PrivateConfig Γ)
    (tree : CutTree Γ (PrivateExit Γ)) : Prop :=
  ∀ σ values incoming owner frame stack, config.Realizes σ values incoming owner frame →
    ∃ final, IR.Steps p (privateState σ values f label pc owner frame config stack) final ∧
      (advance = true → Relation.TransGen (Transition.Predecessor (IR.step p)) final
        (privateState σ values f label pc owner frame config stack)) ∧
      match tree.eval σ with
      | .error fault => final.control = .halted (.error fault)
      | .ok exit => exit.Realizes σ incoming tokens f owner stack final

theorem private_rescope_realizes {Γ : Var → Nat} (σ : Subst Γ) (values : List IR.Value)
    (incoming : IR.Memory) (owner frame : Nat) (before : PrivateConfig Γ) (scope : Scope Γ)
    (related : before.Realizes σ values incoming owner frame)
    (shape : ({before with scope} : PrivateConfig Γ).WellShaped = true) :
    ({before with scope} : PrivateConfig Γ).Realizes σ values incoming owner frame := by
  exact ⟨shape, related.2⟩

end EvoIR.Core.Symbolic
