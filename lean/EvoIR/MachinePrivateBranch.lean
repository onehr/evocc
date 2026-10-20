import EvoIR.MachinePrivateBody
import EvoIR.MachineSelectionTest
import EvoIR.MachineSelectionConditional
import EvoIR.MachineSelectionTermStep

/-!
Conditional selection in a real mixed register file. The checker reads only
the tested scalar/flag slot; successor arguments remain their full actual
vectors, including unbounded private object identities. A separate edge
checker supplies the exact SSA jump and its destination model.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

theorem private_word_read {Γ : Var → Nat} (bank : Bank Γ)
    (reg : EvoIR.Machine.VReg) (word : WordExpr Γ)
    (read : ∀ σ, EvoIR.Machine.operandBits
      (EvoIR.Machine.readOperand (bankDenote σ bank)) reg = .ok (word.denote σ))
    (present : (bank.lookup reg.id).isSome = true)
    (σ : Subst Γ) (regs : EvoIR.Machine.RegisterFile) (models : bank.Models σ regs) :
    EvoIR.Machine.readBits regs reg = .ok (word.denote σ) := by
  have equal := bank_models_operand σ bank regs models reg present
  have actual := read σ
  have lifted : EvoIR.Machine.operandBits (EvoIR.Machine.readOperand regs) reg =
      EvoIR.Machine.operandBits (EvoIR.Machine.readOperand (bankDenote σ bank)) reg := by
    simp only [EvoIR.Machine.operandBits, EvoIR.Machine.operandValue, equal]
  rw [EvoIR.Machine.operandBits_readOperand] at lifted
  exact lifted.trans actual

theorem private_flag_read {Γ : Var → Nat} (bank : Bank Γ)
    (reg : EvoIR.Machine.VReg) (flags : FlagExpr Γ)
    (read : ∀ σ, EvoIR.Machine.operandFlags
      (EvoIR.Machine.readOperand (bankDenote σ bank)) reg = .ok (flags.denote σ))
    (present : (bank.lookup reg.id).isSome = true)
    (σ : Subst Γ) (regs : EvoIR.Machine.RegisterFile) (models : bank.Models σ regs) :
    EvoIR.Machine.readFlags regs reg = .ok (flags.denote σ) := by
  have canonical := flag_operand_read bank reg flags read σ
  cases found : bank.lookup reg.id with
  | none => simp only [found, Option.isSome_none, Bool.false_eq_true] at present
  | some slot =>
    have actual := models reg.id slot found
    simpa only [EvoIR.Machine.readFlags, EvoIR.Machine.readSlot, actual,
      bank_lookup, found, Option.map_some] using canonical

def privateGuardTestUnder {Γ : Var → Nat} (checks : List (Guard Γ))
    (source : Term Γ 1) (flags : FlagExpr Γ) (cond : EvoIR.Machine.Cond) :
    Option {passWhen : Bool // ∀ σ, guards σ checks = .ok () →
      EvoIR.Machine.condition (flags.denote σ) cond =
        .ok (if (denoteTerm σ source).toNat != 0 then passWhen else !passWhen)} :=
  match guardTest source flags cond with
  | some tested => some ⟨tested.val, by intro σ _; exact tested.property σ⟩
  | none =>
    match checkFlagUnder checks source flags cond with
    | some same => some ⟨true, by
        intro σ passed
        rw [same.down σ passed, one_bit_nonzero]
        cases (denoteTerm σ source).toNat != 0 <;> rfl⟩
    | none => do
      let inverse ← checkFlagUnder checks source flags (inverseCond cond)
      return ⟨false, by
        intro σ passed
        rw [condition_inverse, inverse.down σ passed, one_bit_nonzero]
        cases (denoteTerm σ source).toNat != 0 <;> rfl⟩

structure PrivateBranchArm where
  target : Nat
  args : List EvoIR.Machine.VReg

structure PrivateBranchChoice where
  yes : PrivateBranchArm
  no : PrivateBranchArm

def PrivateBranchDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ)
    (condition : Term Γ 1) (checks : List (Guard Γ)) (choice : PrivateBranchChoice) : Prop :=
  ∀ σ regs owner nextFrame incoming stack memory entered,
    bank.Models σ regs → guards σ checks = .ok () →
    EvoIR.Machine.jump .ssa f ⟨f.name, b.id, b.insts.length, regs, owner, incoming⟩
      (if (denoteTerm σ condition).getLsbD 0 then choice.yes.target else choice.no.target)
      (if (denoteTerm σ condition).getLsbD 0 then choice.yes.args else choice.no.args) = .ok entered →
    EvoIR.Machine.step .ssa p (privateMachineState f b b.insts.length regs owner nextFrame incoming stack memory) =
      some ⟨.running entered, stack, nextFrame, memory⟩

private theorem private_false_beq_true : (false == true) = false := rfl

private theorem private_true_beq_false : (true == false) = false := rfl

/-- The finite reader checks the actual branch constructor and its tested
operand. Its proof is polymorphic in the real pointer-containing bank and
uses no assumption about a producer-selected argument permutation. -/
def privateBranchChoiceUnder {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ)
    (condition : Term Γ 1) (checks : List (Guard Γ)) :
    Option {choice : PrivateBranchChoice // PrivateBranchDiagram p f b bank condition checks choice} := do
  if function : p.functions.find? (·.name == f.name) = some f then
    if block : f.blocks.find? (·.id == b.id) = some b then
      match term : b.term with
      | .bCond flags cond taken takenArgs untaken untakenArgs => do
        let operand ← flagOperand bank flags
        if present : (bank.lookup flags.id).isSome = true then
          let tested ← privateGuardTestUnder checks condition operand.val cond
          let yes : PrivateBranchArm := ⟨taken, takenArgs⟩
          let no : PrivateBranchArm := ⟨untaken, untakenArgs⟩
          let choice := if tested.val then (⟨yes, no⟩ : PrivateBranchChoice) else ⟨no, yes⟩
          return ⟨choice, by
            intro σ regs owner nextFrame incoming stack memory entered models passed jumped
            have read := private_flag_read bank flags operand.val operand.property present σ regs models
            have truth := tested.property σ passed
            rw [← one_bit_nonzero] at truth
            cases selected : tested.val <;> cases bit : (denoteTerm σ condition).getLsbD 0
            all_goals
              simp only [selected, bit, Bool.false_eq_true, ↓reduceIte,
                Bool.not_false, Bool.not_true] at truth
              apply EvoIR.Machine.Selection.step_branch_flags .ssa p
                (privateMachineState f b b.insts.length regs owner nextFrame incoming stack memory)
                ⟨f.name, b.id, b.insts.length, regs, owner, incoming⟩
                f b flags cond taken takenArgs untaken untakenArgs (operand.val.denote σ) _ entered
                rfl function block rfl term read truth
              simpa only [choice, yes, no, selected, bit, Bool.false_eq_true,
                ↓reduceIte] using jumped⟩
        else none
      | .tb nonzero reg 0 taken takenArgs untaken untakenArgs => do
        let operand ← wordOperand bank reg
        if present : (bank.lookup reg.id).isSome = true then
          let equal ← checkWordUnder checks ⟨1, condition⟩ operand.val
          let yes : PrivateBranchArm := ⟨taken, takenArgs⟩
          let no : PrivateBranchArm := ⟨untaken, untakenArgs⟩
          let choice := if nonzero then (⟨yes, no⟩ : PrivateBranchChoice) else ⟨no, yes⟩
          return ⟨choice, by
            intro σ regs owner nextFrame incoming stack memory entered models passed jumped
            have read := private_word_read bank reg operand.val operand.property present σ regs models
            have sameBit : (operand.val.denote σ).getLsbD 0 = (denoteTerm σ condition).getLsbD 0 := by
              rw [equal.down σ passed]
              simp only [BitVec.getLsbD_setWidth, Nat.zero_lt_succ, decide_true, Bool.true_and]
            cases nonzero <;> cases bit : (denoteTerm σ condition).getLsbD 0
            all_goals
              apply EvoIR.Machine.Selection.step_branch_bit .ssa p
                (privateMachineState f b b.insts.length regs owner nextFrame incoming stack memory)
                ⟨f.name, b.id, b.insts.length, regs, owner, incoming⟩
                f b _ reg 0 taken takenArgs untaken untakenArgs (operand.val.denote σ) entered
                rfl function block rfl term (by decide) read
              simpa only [choice, yes, no, sameBit, bit, beq_self_eq_true,
                private_false_beq_true, private_true_beq_false, Bool.false_eq_true, ↓reduceIte] using jumped⟩
        else none
      | .cb nonzero reg taken takenArgs untaken untakenArgs => do
        let width : {width // reg.class.width = some width} ←
          match found : reg.class.width with
          | none => none
          | some width => some ⟨width, found⟩
        let operand ← wordOperand bank reg
        if present : (bank.lookup reg.id).isSome = true then
          let tested ← zeroTestUnder checks condition operand.val width.val
          let yes : PrivateBranchArm := ⟨taken, takenArgs⟩
          let no : PrivateBranchArm := ⟨untaken, untakenArgs⟩
          let choice := if nonzero == tested.val then (⟨yes, no⟩ : PrivateBranchChoice) else ⟨no, yes⟩
          return ⟨choice, by
            intro σ regs owner nextFrame incoming stack memory entered models passed jumped
            have read := private_word_read bank reg operand.val operand.property present σ regs models
            have truth := tested.property σ passed
            cases selected : tested.val <;> cases nonzero <;> cases bit : (denoteTerm σ condition).getLsbD 0
            all_goals
              apply EvoIR.Machine.Selection.step_branch_zero .ssa p
                (privateMachineState f b b.insts.length regs owner nextFrame incoming stack memory)
                ⟨f.name, b.id, b.insts.length, regs, owner, incoming⟩
                f b _ reg taken takenArgs untaken untakenArgs width.val (operand.val.denote σ) entered
                rfl function block rfl term width.property read
              simpa only [choice, selected, yes, no, truth, bit, Bool.not_false,
                Bool.not_true, Bool.false_eq_true, beq_self_eq_true,
                Bool.true_beq, Bool.false_beq, ↓reduceIte] using jumped⟩
        else none
      | _ => none
    else none
  else none

end EvoIR.Core.Symbolic.MachineSelection
