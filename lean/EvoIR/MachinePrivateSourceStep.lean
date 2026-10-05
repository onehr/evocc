import EvoIR.Core.PrivateTarget

/-! Lift a concrete private IR row to the actual dispatcher. These lemmas
preserve the complete value bank, caller stack and frame counter. Both success
and the first failure are explicit; a failed row cannot be skipped by a span. -/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem private_source_instruction_step {Γ : Var → Nat} (program : IR.Program)
    (function : IR.Function) (block : IR.Block) (pc owner frame : Nat)
    (before after : PrivateConfig Γ) (σ : Subst Γ) (values nextValues : List IR.Value)
    (stack : List IR.ReturnFrame) (inst : IR.Instruction)
    (functionFound : program.functions.find? (·.name == function.name) = some function)
    (blockFound : function.blocks.find? (·.name == block.name) = some block)
    (atPc : block.instructions[pc]? = some inst)
    (noncall : ∀ name, inst.op ≠ .call name)
    (evaluated : concreteInstruction owner (before.scope.denote σ (privateToken values)) inst =
      .ok (after.scope.denote σ (privateToken nextValues))) :
    IR.step program (privateState σ values function block.name pc owner frame before stack) =
      some (privateState σ nextValues function block.name (pc+1) owner frame after stack) := by
  let start := privateState σ values function block.name pc owner frame before stack
  have dispatched := execute_instruction (p := program) (s := start)
    (a := ⟨function.name, block.name, pc, before.scope.denote σ (privateToken values), owner⟩)
    functionFound blockFound atPc noncall
  dsimp only [start, privateState] at dispatched
  simp only [IR.step, privateState]
  rw [dispatched, evaluated]
  rfl

theorem private_source_instruction_fault {Γ : Var → Nat} (program : IR.Program)
    (function : IR.Function) (block : IR.Block) (pc owner frame : Nat)
    (before : PrivateConfig Γ) (σ : Subst Γ) (values : List IR.Value)
    (stack : List IR.ReturnFrame) (inst : IR.Instruction) (fault : Source.Fault)
    (functionFound : program.functions.find? (·.name == function.name) = some function)
    (blockFound : function.blocks.find? (·.name == block.name) = some block)
    (atPc : block.instructions[pc]? = some inst)
    (noncall : ∀ name, inst.op ≠ .call name)
    (evaluated : concreteInstruction owner (before.scope.denote σ (privateToken values)) inst =
      .error fault) :
    IR.step program (privateState σ values function block.name pc owner frame before stack) =
      some ⟨.halted (.error fault), stack, frame⟩ := by
  let start := privateState σ values function block.name pc owner frame before stack
  have dispatched := execute_instruction (p := program) (s := start)
    (a := ⟨function.name, block.name, pc, before.scope.denote σ (privateToken values), owner⟩)
    functionFound blockFound atPc noncall
  dsimp only [start, privateState] at dispatched
  simp only [IR.step, privateState]
  rw [dispatched, evaluated]
  rfl

end EvoIR.Core.Symbolic.MachineSelection
