import EvoIR.MachineExecution

/-!
Concrete instruction obligations for two-input SSA transport checking. A use
map is an untrusted finite correspondence. Acceptance checks the exact actual
instruction, including destinations, widths, immediates, flags, call symbols
and all memory/lifetime fields. The proof interprets that check through the
independently defined operand evaluator, not through a uses/defs-only model.

The unconditional deletion row is movImm. General pure deletion is justified
by actual reachable-source progress in MachineTransportDCE, not by a type or
discardability assertion accepted from the producer.
-/
namespace EvoIR.Machine.Transport

abbrev UseMap := List (Nat × VReg)

def rename (mapping : UseMap) (reg : VReg) : VReg :=
  (mapping.lookup reg.id).getD reg

def checkRetained (mapping : UseMap) (source target : Instruction) : Bool :=
  source.uses.all (fun reg => decide ((rename mapping reg).class = reg.class)) &&
    decide (source.mapUses (rename mapping) = target)

theorem checkRetained_shape (mapping : UseMap) (source target : Instruction)
    (accepted : checkRetained mapping source target = true) :
    source.mapUses (rename mapping) = target := by
  simp only [checkRetained, Bool.and_eq_true, decide_eq_true_eq] at accepted
  exact accepted.2

theorem checkRetained_defs (mapping : UseMap) (source target : Instruction)
    (accepted : checkRetained mapping source target = true) : target.defs = source.defs := by
  rw [← checkRetained_shape mapping source target accepted]
  exact Instruction.defs_mapUses _ _

/-- Every effect, result slot and fault is preserved. The only hypothesis is
the incoming register-value relation, which a whole-function checker must
establish at entry and propagate on every edge, including backedges. -/
theorem checkRetained_eval (mapping : UseMap) (source target : Instruction)
    (accepted : checkRetained mapping source target = true)
    (owner : Nat) (sourceRegs targetRegs : RegisterFile) (memory : Memory)
    (related : ∀ reg, reg ∈ source.uses →
      readOperand targetRegs (rename mapping reg) = readOperand sourceRegs reg) :
    evalInstructionOperands owner target (readOperand targetRegs) memory =
      evalInstructionOperands owner source (readOperand sourceRegs) memory := by
  rw [← checkRetained_shape mapping source target accepted]
  exact evalInstructionOperands_mapUses owner source (rename mapping)
    (readOperand sourceRegs) (readOperand targetRegs) memory related

def checkDeleted (live : List Nat) : Instruction → Bool
  | .movImm dst _ => decide (dst.class ≠ .flags) &&
      live.all (fun reg => decide (reg ≠ dst.id))
  | _ => false

/-- General baseline DCE syntax/demand obligation. Unlike checkDeleted, its
totality proof uses actual reachable-source progress (or checked typed reads)
and is not unconditional for an arbitrary register file. -/
def checkDCE (live : List Nat) (inst : Instruction) : Bool :=
  inst.pure && inst.defs.all (fun dst => live.all (fun reg => decide (reg ≠ dst.id)))

theorem movImm_eval (owner : Nat) (dst : VReg) (imm : Nat)
    (regs : RegisterFile) (memory : Memory) :
    evalInstruction owner (.movImm dst imm) regs memory =
      .ok (write regs dst (.int (BitVec.ofNat 64 imm)), memory) := by
  rfl

/-- The actual machine executes an accepted deletion without any read or
fault, retains the exact memory, and preserves every checked live register.
No precondition assumes that the source/candidate executions are equivalent. -/
theorem checkDeleted_sound (live : List Nat) (inst : Instruction)
    (accepted : checkDeleted live inst = true)
    (owner : Nat) (regs : RegisterFile) (memory : Memory) :
    ∃ nextRegs, evalInstruction owner inst regs memory = .ok (nextRegs, memory) ∧
      ∀ reg ∈ live, ∀ regClass,
        readSlot nextRegs ⟨reg, regClass⟩ = readSlot regs ⟨reg, regClass⟩ := by
  cases inst <;> try simp [checkDeleted] at accepted
  case movImm dst imm =>
    refine ⟨write regs dst (.int (BitVec.ofNat 64 imm)), movImm_eval _ _ _ _ _, ?_⟩
    intro reg member regClass
    have different : reg ≠ dst.id := accepted.2 reg member
    exact readSlot_writeSlot_other regs dst ⟨reg, regClass⟩ _ different

end EvoIR.Machine.Transport
