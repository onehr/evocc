import EvoIR.MachineSelectionSpan

/-! A finite source-EvoIR span fetched from the actual paired input. The
ordered checks are computed by the existing proved IR symbolic interpreter;
they are not obligations supplied by the lowerer or facts about sampled inputs.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def sourceState {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value) (f : IR.Function)
    (b : IR.Block) (pc : Nat) (scope : Scope Γ) (owner nextFrame : Nat)
    (stack : List IR.ReturnFrame) : IR.State :=
  ⟨.running ⟨f.name, b.name, pc, scope.denote σ ρ, owner⟩, stack, nextFrame⟩

structure SourceSpanResult (Γ : Var → Nat) where
  checks : List (Guard Γ)
  scope : Scope Γ

def SourceSpanDiagram {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (b : IR.Block)
    (pc count : Nat) (scope : Scope Γ) (result : SourceSpanResult Γ) : Prop :=
  ∀ σ ρ owner nextFrame stack, ∃ last,
    IR.Steps p (sourceState σ ρ f b pc scope owner nextFrame stack) last ∧
      match guards σ result.checks with
      | .error fault => last.control = .halted (.error fault)
      | .ok _ => last = sourceState σ ρ f b (pc+count) result.scope owner nextFrame stack

def sourceSpan {Γ : Var → Nat} (p : IR.Program) (f : IR.Function) (b : IR.Block)
    (count pc : Nat) (scope : Scope Γ) :
    Option {result : SourceSpanResult Γ // SourceSpanDiagram p f b pc count scope result} := do
  if function : p.functions.find? (·.name == f.name) = some f then
    if block : f.blocks.find? (·.name == b.name) = some b then
      if within : pc+count ≤ b.instructions.length then
        let code := (b.instructions.drop pc).take count
        let computed : {result // instructions scope code = some result} ←
          match found : instructions scope code with
          | none => none
          | some result => some ⟨result, found⟩
        return ⟨⟨computed.val.1, computed.val.2⟩, by
          intro σ ρ owner nextFrame stack
          have codeLength : code.length = count := by
            simp only [code, List.length_take, List.length_drop]
            exact Nat.min_eq_left (by omega)
          have codeAt : ∀ i inst, code[i]? = some inst → b.instructions[pc+i]? = some inst := by
            intro i inst found
            have bounded : i < code.length := by
              by_cases inside : i < code.length
              · exact inside
              · have missing := List.getElem?_eq_none (by omega : code.length ≤ i)
                rw [missing] at found
                contradiction
            have inside : i < count := by omega
            simpa only [code, List.getElem?_take_of_lt inside, List.getElem?_drop] using found
          obtain ⟨last, path, outcome⟩ := segment_execution_sound σ ρ
            (sourceState σ ρ f b pc scope owner nextFrame stack)
            ⟨f.name, b.name, pc, scope.denote σ ρ, owner⟩ f b scope computed.val.2
            code computed.val.1 rfl function block codeAt rfl computed.property
          refine ⟨last, path, ?_⟩
          cases checked : guards σ computed.val.1 with
          | error fault => simpa only [checked, codeLength, sourceState] using outcome
          | ok value => cases value; simpa only [checked, codeLength, sourceState] using outcome⟩
      else none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
