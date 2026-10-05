import EvoIR.Asm.Steps

/-!
Facts about the allocated EvoMIR semantics that the frame proof uses. They
are about the actual definitions of `EvoIR.Alloc.Semantics`:

* location codes decode to their location;
* reads and writes of general register locations;
* the admitted non-call forms fail only with malformed faults, never with a
  program diagnostic.
-/
set_option linter.constructorNameAsVariable false

namespace EvoIR.Asm.Proof

open EvoIR.Alloc
open Core.Source (Fault)

theorem decode_code (l : Loc) : Loc.decode l.code = some l := by
  cases l with
  | gpr n v => cases v <;> simp [Loc.code, Loc.decode] <;> omega
  | flags => rfl
  | spill k v => cases v <;> simp [Loc.code, Loc.decode] <;> omega
  | incoming k v => cases v <;> simp [Loc.code, Loc.decode] <;> omega
  | outgoing k v => cases v <;> simp [Loc.code, Loc.decode] <;> omega

@[simp] theorem decode_reg (l : Loc) : Loc.decode (Loc.reg l).id = some l := decode_code l

@[simp] theorem operandReader_reg (st : Storage) (l : Loc) :
    operandReader st (Loc.reg l) = readLoc st l := by
  simp [operandReader]

@[simp] theorem applyWrites_nil (st : Storage) : applyWrites st [] = st := rfl

@[simp] theorem applyWrites_cons (st : Storage) (l : Loc) (slot : Machine.Slot)
    (rest : Machine.Writes) :
    applyWrites st ((Loc.reg l, slot) :: rest) = applyWrites (writeLoc st l slot) rest := by
  simp [applyWrites, List.foldl_cons]

/-- Only malformed faults: no program diagnostic. -/
def MalformedOnly {α : Type} (r : Except Fault α) : Prop :=
  ∀ e, r = .error e → ∃ msg, e = .malformed msg

theorem readLoc_malformed (st : Storage) (l : Loc) : MalformedOnly (readLoc st l) := by
  intro e h
  simp only [readLoc] at h
  repeat' split at h
  all_goals first | (cases h; exact ⟨_, rfl⟩) | cases h

theorem operandReader_malformed (st : Storage) (r : Machine.VReg) :
    MalformedOnly (operandReader st r) := by
  intro e h
  simp only [operandReader] at h
  split at h
  · exact readLoc_malformed st _ e h
  · cases h; exact ⟨_, rfl⟩

section
variable {get : Machine.OperandReader} (hg : ∀ r, MalformedOnly (get r))
include hg

theorem operandValue_malformed (r : Machine.VReg) : MalformedOnly (Machine.operandValue get r) := by
  intro e h
  simp only [Machine.operandValue, bind, Except.bind] at h
  split at h
  · cases h; exact hg r _ ‹_›
  · split at h
    · cases h
    · cases h; exact ⟨_, rfl⟩

theorem operandBits_malformed (r : Machine.VReg) : MalformedOnly (Machine.operandBits get r) := by
  intro e h
  simp only [Machine.operandBits, bind, Except.bind] at h
  split at h
  · cases h; exact operandValue_malformed hg r _ ‹_›
  · split at h
    · cases h
    · cases h; exact ⟨_, rfl⟩

theorem operandFlags_malformed (r : Machine.VReg) : MalformedOnly (Machine.operandFlags get r) := by
  intro e h
  simp only [Machine.operandFlags, bind, Except.bind] at h
  split at h
  · cases h; exact hg r _ ‹_›
  · split at h
    · cases h
    · cases h; exact ⟨_, rfl⟩

end

theorem evalBinaryValue_malformed (op : Machine.BinOp) (w : Machine.Width) (x y : Machine.Value) :
    MalformedOnly (Machine.evalBinaryValue op w x y) := by
  intro e h
  simp only [Machine.evalBinaryValue] at h
  repeat' split at h
  all_goals first | (cases h; exact ⟨_, rfl⟩) | cases h

theorem condition_malformed (f : Machine.Flags) (c : Machine.Cond) :
    MalformedOnly (Machine.condition f c) := by
  intro e h
  simp only [Machine.condition] at h
  repeat' split at h
  all_goals first | (cases h; exact ⟨_, rfl⟩) | cases h

/-- A bind of two malformed-only computations is malformed-only. -/
theorem bind_malformed {α β : Type} (x : Except Fault α) (f : α → Except Fault β)
    (hx : MalformedOnly x) (hf : ∀ a, x = .ok a → MalformedOnly (f a)) :
    MalformedOnly (x >>= f) := by
  intro e h
  cases hx' : x with
  | error err =>
    rw [hx'] at h
    cases h
    exact hx e hx'
  | ok a =>
    rw [hx'] at h
    exact hf a hx' e h

theorem pure_malformed {α : Type} (a : α) : MalformedOnly (pure a : Except Fault α) := by
  intro e h; cases h

/-- The pure machine forms fail only with malformed faults. -/
theorem evalPure_malformed (owner : Nat) (inst : Machine.Instruction) (get : Machine.OperandReader)
    (memory : Machine.Memory) (pure : inst.pure = true) (hg : ∀ r, MalformedOnly (get r)) :
    MalformedOnly (Machine.evalInstructionOperands owner inst get memory) := by
  have v := operandValue_malformed hg
  have b := operandBits_malformed hg
  have f := operandFlags_malformed hg
  cases inst <;> simp only [Machine.Instruction.pure, Bool.false_eq_true] at pure <;>
    simp only [Machine.evalInstructionOperands]
  all_goals
    repeat (first
      | apply bind_malformed _ _ (v _); intro _ _
      | apply bind_malformed _ _ (b _); intro _ _
      | apply bind_malformed _ _ (f _); intro _ _
      | apply bind_malformed _ _ (evalBinaryValue_malformed _ _ _ _); intro _ _
      | apply bind_malformed _ _ (condition_malformed _ _); intro _ _
      | exact pure_malformed _)

end EvoIR.Asm.Proof
