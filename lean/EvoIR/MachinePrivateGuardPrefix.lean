import EvoIR.MachinePrivateBodySpan
import EvoIR.MachineSelectionGuard

/-! Pure native guard prefixes execute in the real mixed bank. The source
private token history does not change, and each pointer survives only after
checking the actual native result footprint. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def PrivateNativePrefixDiagram {Γ : Var → Nat} (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block) (pc count : Nat)
    (before next : PrivateBank Γ) : Prop :=
  next.source = before.source ∧
  ∀ σ values incoming owner frame memory regs logicalInputs stack,
    before.Models σ values incoming owner frame memory regs →
    ∃ nextRegs,
      Transition.Path (EvoIR.Machine.step .ssa program) count
        (privateMachineState function block pc regs owner frame logicalInputs stack memory)
        (privateMachineState function block (pc+count) nextRegs owner frame logicalInputs stack memory) ∧
      next.Models σ values incoming owner frame memory nextRegs

private theorem private_prefix_notcall (owner : Nat) (inst : EvoIR.Machine.Instruction)
    (regs nextRegs : EvoIR.Machine.RegisterFile) (memory : IR.Memory)
    (evaluated : EvoIR.Machine.evalInstruction owner inst regs memory = .ok (nextRegs, memory)) :
    ∀ name args results, inst ≠ .bl name args results := by
  intro name args results called
  subst inst
  simp [EvoIR.Machine.evalInstruction, EvoIR.Machine.evalInstructionOperands,
    Functor.map, Except.map] at evaluated

def privateNativePrefix {Γ : Var → Nat} (program : EvoIR.Machine.Program)
    (function : EvoIR.Machine.Function) (block : EvoIR.Machine.Block) :
    (count pc : Nat) → (before : PrivateBank Γ) →
    Option {next : PrivateBank Γ // PrivateNativePrefixDiagram program function block pc count before next}
  | 0, pc, before =>
    if foundFunction : program.functions.find? (·.name == function.name) = some function then
      if foundBlock : function.blocks.find? (·.id == block.id) = some block then
        if within : pc ≤ block.insts.length then
          some ⟨before, rfl, by
            intro σ values incoming owner frame memory regs logicalInputs stack models
            exact ⟨regs, .refl _, models⟩⟩
        else none
      else none
    else none
  | count+1, pc, before => do
    if foundFunction : program.functions.find? (·.name == function.name) = some function then
      if foundBlock : function.blocks.find? (·.id == block.id) = some block then
        let inst : {inst // block.insts[pc]? = some inst} ←
          match found : block.insts[pc]? with
          | none => none
          | some inst => some ⟨inst, found⟩
        let native ← modeledInstruction before.words inst.val
        if separate : before.pointers.all (fun pointer =>
            inst.val.defs.all (fun dst => decide (pointer.target.id ≠ dst.id))) then
          let middle : PrivateBank Γ := {before with words := native.val}
          let rest ← privateNativePrefix program function block count (pc+1) middle
          return ⟨rest.val, rest.property.1, by
            intro σ values incoming owner frame memory regs logicalInputs stack models
            obtain ⟨middleRegs, evaluated, words⟩ := native.property σ regs owner memory models.words
            have middleModels : middle.Models σ values incoming owner frame memory middleRegs := by
              refine ⟨models.source, words, ?_, models.memory⟩
              apply pointer_target_preserved before.pointers _ regs middleRegs models.pointers
              intro pointer member
              apply eval_pointer_preserved owner inst.val regs middleRegs memory memory evaluated pointer.target
              intro dst defined
              exact of_decide_eq_true (List.all_eq_true.mp
                (List.all_eq_true.mp separate pointer member) dst defined)
            obtain ⟨nextRegs, path, nextModels⟩ := rest.property.2 σ values incoming owner frame memory
              middleRegs logicalInputs stack middleModels
            have stepped := EvoIR.Machine.step_evalInstruction .ssa program
              (privateMachineState function block pc regs owner frame logicalInputs stack memory)
              ⟨function.name, block.id, pc, regs, owner, logicalInputs⟩ function block inst.val middleRegs memory
              rfl foundFunction foundBlock inst.property
              (private_prefix_notcall owner inst.val regs middleRegs memory evaluated) evaluated
            refine ⟨nextRegs, ?_, nextModels⟩
            simpa only [privateMachineState, Nat.add_assoc, Nat.add_comm, Nat.add_left_comm]
              using Transition.Path.next stepped path⟩
        else none
      else none
    else none

end EvoIR.Core.Symbolic.MachineSelection
