import EvoIR.MachineExecution

/-!
Actual result-write normalization. Pointer values are permitted in either
general-register class; gpr64 is not silently interpreted as an integer.
Destination channel legality is checked separately from operand readiness.
-/
namespace EvoIR.Machine

def OutputClassesValid : Instruction → Prop
  | .flagBinary _ dst flags _ _ _ => dst.class ≠ .flags ∧ flags.class = .flags
  | .cmp dst _ _ _ | .cmpImm dst _ _ _ | .tstImm dst _ _ _ => dst.class = .flags
  | .mov dst _ | .movImm dst _ | .binary _ dst _ _ _ | .binaryImm _ dst _ _ _
  | .wideMultiply _ dst _ _ | .msub dst _ _ _ _ | .madd dst _ _ _ _
  | .extend _ dst _ | .bfx _ dst _ _ _ _ | .cset dst _ _ | .csel dst _ _ _ _ _
  | .load _ dst _ _ _ _ | .frameSlot dst _ _ _ => dst.class ≠ .flags
  | .store _ _ _ _ _ _ | .stackRelease _ | .bl _ _ _ => True

def Slot.WrittenAt (reg : VReg) : Slot → Prop
  | .value concrete => reg.class ≠ .flags ∧ normalize reg.class concrete = concrete
  | .flags _ => reg.class = .flags

def WritesNormalized (writes : Writes) : Prop :=
  ∀ entry ∈ writes, entry.2.WrittenAt entry.1

theorem normalize_idempotent (regClass : RegClass) (value : Value) :
    normalize regClass (normalize regClass value) = normalize regClass value := by
  cases regClass <;> cases value <;>
    simp [normalize, BitVec.setWidth_setWidth_of_le]

theorem valueWrite_normalized (dst : VReg) (value : Value) (general : dst.class ≠ .flags) :
    WritesNormalized (valueWrite dst value) := by
  intro entry member
  have same : entry = (dst, .value (normalize dst.class value)) := by
    simpa only [valueWrite, List.mem_singleton] using member
  subst entry
  exact ⟨general, normalize_idempotent dst.class value⟩

theorem evalInstructionOperands_writes_normalized (owner : Nat) (inst : Instruction)
    (get : OperandReader) (memory : Memory) (writes : Writes) (nextMemory : Memory)
    (classes : OutputClassesValid inst)
    (evaluated : evalInstructionOperands owner inst get memory = .ok (writes, nextMemory)) :
    WritesNormalized writes := by
  have footprint := evalInstructionOperands_resultRegs owner inst get memory writes nextMemory evaluated
  cases inst <;>
    simp only [evalInstructionOperands, Bind.bind, Except.bind, Pure.pure, Except.pure] at evaluated
  all_goals repeat' split at evaluated
  all_goals simp_all [OutputClassesValid, Instruction.defs]
  all_goals rcases evaluated with ⟨writesEq, _⟩
  all_goals try rw [← writesEq]
  all_goals try clear writesEq
  all_goals simp_all [WritesNormalized, Slot.WrittenAt, valueWrite, normalize_idempotent]

end EvoIR.Machine
