import EvoIR.MachineTransportResults
import EvoIR.MachineWrites

/-! The structural check's actual destination-class row discharges the
semantic output-channel precondition. No result-kind or normalization claim
is accepted from a producer annotation. -/
namespace EvoIR.Machine.Transport

theorem instructionShapeValid_output (inst : Instruction)
    (accepted : instructionShapeValid inst = true) : OutputClassesValid inst := by
  cases inst <;> simp_all [instructionShapeValid, general, OutputClassesValid]

theorem writesNormalized_of_machine (writes : Writes)
    (written : EvoIR.Machine.WritesNormalized writes) : WritesNormalized writes := by
  intro entry member general
  have head := written entry member
  rcases entry with ⟨dst, slot⟩
  cases slot with
  | value value => exact ⟨value, rfl, head.2⟩
  | flags flags => exact False.elim (general head)

theorem checked_writes_normalized (owner : Nat) (inst : Instruction) (get : OperandReader)
    (memory : Memory) (writes : Writes) (nextMemory : Memory)
    (accepted : instructionShapeValid inst = true)
    (evaluated : evalInstructionOperands owner inst get memory = .ok (writes, nextMemory)) :
    WritesNormalized writes := by
  exact writesNormalized_of_machine writes
    (evalInstructionOperands_writes_normalized owner inst get memory writes nextMemory
      (instructionShapeValid_output inst accepted) evaluated)

end EvoIR.Machine.Transport
