import EvoIR.MachineSelectionBank

/-!
Scalar diagrams may execute in a real bank that also contains private
pointers. The relation records only checked scalar/flag bindings. It never
encodes an unbounded object identity as a 64-bit integer. Actual private
pointer bindings remain separate and survive only writes to other registers.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

def Bank.Models {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (registers : EvoIR.Machine.RegisterFile) : Prop :=
  ∀ id slot, bank.lookup id = some slot → registers.lookup id = some (slot.denote σ)

theorem bank_models_self {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ) :
    bank.Models σ (bankDenote σ bank) := by
  intro id slot found
  rw [bank_lookup, found]
  rfl

theorem bank_models_write {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (registers : EvoIR.Machine.RegisterFile) (models : bank.Models σ registers)
    (reg : EvoIR.Machine.VReg) (slot : BankSlot Γ) :
    (bankWrite bank reg slot).Models σ
      (EvoIR.Machine.writeSlot registers reg (slot.denote σ)) := by
  intro id value found
  by_cases same : id = reg.id
  · subst id
    have equal : slot = value := by simpa [bankWrite, List.lookup_cons] using found
    subst value
    simp [EvoIR.Machine.writeSlot, List.lookup_cons]
  · have old : bank.lookup id = some value := by
      simpa [bankWrite, List.lookup_cons, beq_eq_false_iff_ne.mpr same] using found
    simpa [EvoIR.Machine.writeSlot, List.lookup_cons, beq_eq_false_iff_ne.mpr same] using models id value old

theorem bank_models_writes {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (registers : EvoIR.Machine.RegisterFile) (models : bank.Models σ registers)
    (writes : SymbolicWrites Γ) :
    (applySymbolicWrites bank writes).Models σ
      (EvoIR.Machine.applyWrites registers (writesDenote σ writes)) := by
  induction writes generalizing bank registers with
  | nil => exact models
  | cons write rest ih =>
    rcases write with ⟨reg, slot⟩
    exact ih (bankWrite bank reg slot)
      (EvoIR.Machine.writeSlot registers reg (slot.denote σ))
      (bank_models_write σ bank registers models reg slot)

theorem bank_models_operand {Γ : Var → Nat} (σ : Subst Γ) (bank : Bank Γ)
    (registers : EvoIR.Machine.RegisterFile) (models : bank.Models σ registers)
    (reg : EvoIR.Machine.VReg) (present : (bank.lookup reg.id).isSome = true) :
    EvoIR.Machine.readOperand registers reg = EvoIR.Machine.readOperand (bankDenote σ bank) reg := by
  cases found : bank.lookup reg.id with
  | none => simp [found] at present
  | some slot =>
    have actual := models reg.id slot found
    simp [EvoIR.Machine.readOperand, EvoIR.Machine.readSlot, actual, bank_lookup, found]

/-- The additional test is a finite use-list check, not a trusted dataflow
annotation. The evaluator's use-footprint theorem is proved by constructor
cases over the actual instruction semantics. -/
def modeledInstruction {Γ : Var → Nat} (bank : Bank Γ) (inst : EvoIR.Machine.Instruction) :
    Option {next : Bank Γ // ∀ σ registers owner memory,
      bank.Models σ registers →
      ∃ output, EvoIR.Machine.evalInstruction owner inst registers memory = .ok (output, memory) ∧
        next.Models σ output} := do
  if initialized : inst.uses.all (fun reg => (bank.lookup reg.id).isSome) then
    let writes ← machineOperands bank inst
    return ⟨applySymbolicWrites bank writes.val, by
      intro σ registers owner memory models
      have agrees : ∀ reg ∈ inst.uses,
          EvoIR.Machine.readOperand registers reg =
            EvoIR.Machine.readOperand (bankDenote σ bank) reg := by
        intro reg member
        exact bank_models_operand σ bank registers models reg
          (List.all_eq_true.mp initialized reg member)
      have same := EvoIR.Machine.evalInstructionOperands_mapUses owner inst id
        (EvoIR.Machine.readOperand (bankDenote σ bank))
        (EvoIR.Machine.readOperand registers) memory agrees
      have identity : inst.mapUses id = inst := by
        cases inst <;> simp [EvoIR.Machine.Instruction.mapUses]
      rw [identity, writes.property σ owner memory] at same
      refine ⟨EvoIR.Machine.applyWrites registers (writesDenote σ writes.val), ?_,
        bank_models_writes σ bank registers models writes.val⟩
      simp [EvoIR.Machine.evalInstruction, same, Functor.map, Except.map]⟩
  else none

end EvoIR.Core.Symbolic.MachineSelection
