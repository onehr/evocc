import EvoIR.MachineSelectionTest
import EvoIR.MachineSelectionTermStep
import EvoIR.MachineSelectionConditional

/-!
The conditional branch reader checks the actual terminator and the actual
flags/value operand. It returns both exact successor bindings. Clients use
the same result for source branches and for checked diagnostic chains.
-/
namespace EvoIR.Core.Symbolic.MachineSelection

structure BranchArm (Γ : Var → Nat) (f : EvoIR.Machine.Function) (bank : Bank Γ) where
  target : Nat
  args : List EvoIR.Machine.VReg
  result : JumpResult Γ
  checked : JumpDiagram f bank target args result

structure BranchChoice (Γ : Var → Nat) (f : EvoIR.Machine.Function) (bank : Bank Γ) where
  yes : BranchArm Γ f bank
  no : BranchArm Γ f bank

def BranchUnderDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ)
    (condition : Term Γ 1) (checks : List (Guard Γ)) (choice : BranchChoice Γ f bank) : Prop :=
  ∀ σ owner nextFrame incoming stack memory,
    guards σ checks = .ok () →
    EvoIR.Machine.step .ssa p (machineState σ f b b.insts.length bank owner nextFrame incoming stack memory) =
      some (if (denoteTerm σ condition).getLsbD 0 then
        machineState σ f choice.yes.result.block 0 choice.yes.result.bank owner nextFrame incoming stack memory
      else
        machineState σ f choice.no.result.block 0 choice.no.result.bank owner nextFrame incoming stack memory)

private theorem operand_bits_read {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg)
    (word : WordExpr Γ) (read : ∀ σ,
      EvoIR.Machine.operandBits (EvoIR.Machine.readOperand (bankDenote σ bank)) reg = .ok (word.denote σ))
    (σ : Subst Γ) : EvoIR.Machine.readBits (bankDenote σ bank) reg = .ok (word.denote σ) := by
  have value := word_operand_value bank reg word read σ
  rw [EvoIR.Machine.operandValue_readOperand] at value
  simp only [EvoIR.Machine.readBits, value, bind, Except.bind, pure, Except.pure]

private theorem encoded_nonzero (value : BitVec 1) (width : EvoIR.Machine.Width) :
    ((value.setWidth 64).setWidth width.bits != 0) = value.getLsbD 0 := by
  rw [one_bit_reify value]
  cases value.getLsbD 0 <;> cases width <;> rfl

private def guardTestUnder {Γ : Var → Nat} (checks : List (Guard Γ))
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

/-- Only earlier successful source checks may justify a conditional match.
The current branch is not itself a premise of its own verification. -/
def branchChoiceUnder {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ)
    (condition : Term Γ 1) (checks : List (Guard Γ)) :
    Option {choice : BranchChoice Γ f bank // BranchUnderDiagram p f b bank condition checks choice} := do
  if function : p.functions.find? (·.name == f.name) = some f then
    if block : f.blocks.find? (·.id == b.id) = some b then
      match term : b.term with
      | .bCond flags cond taken takenArgs untaken untakenArgs => do
        let operand ← flagOperand bank flags
        let tested ← guardTestUnder checks condition operand.val cond
        let first ← jumpBank f bank taken takenArgs
        let second ← jumpBank f bank untaken untakenArgs
        let yes : BranchArm Γ f bank := ⟨taken, takenArgs, first.val, first.property⟩
        let no : BranchArm Γ f bank := ⟨untaken, untakenArgs, second.val, second.property⟩
        let choice := if tested.val then (⟨yes, no⟩ : BranchChoice Γ f bank) else ⟨no, yes⟩
        return ⟨choice, by
          intro σ owner nextFrame incoming stack memory passed
          have read := flag_operand_read bank flags operand.val operand.property σ
          have truth := tested.property σ passed
          rw [← one_bit_nonzero] at truth
          have firstJump := first.property.2.2 σ owner incoming b.id b.insts.length
          have secondJump := second.property.2.2 σ owner incoming b.id b.insts.length
          cases selected : tested.val <;> cases bit : (denoteTerm σ condition).getLsbD 0
          all_goals
            simp only [selected, bit, Bool.false_eq_true, ↓reduceIte,
              Bool.not_false, Bool.not_true] at truth
            have step := EvoIR.Machine.Selection.step_branch_flags .ssa p
              (machineState σ f b b.insts.length bank owner nextFrame incoming stack memory)
              ⟨f.name, b.id, b.insts.length, bankDenote σ bank, owner, incoming⟩
              f b flags cond taken takenArgs untaken untakenArgs (operand.val.denote σ) _ _
              rfl function block rfl term read truth
              (by first | exact firstJump | exact secondJump)
            simpa only [choice, yes, no, selected, bit, Bool.false_eq_true,
              ↓reduceIte, machineState, first.property.2.1, second.property.2.1] using step⟩
      | .tb nonzero reg 0 taken takenArgs untaken untakenArgs => do
        let operand ← wordOperand bank reg
        let equal ← checkWordUnder checks ⟨1, condition⟩ operand.val
        let first ← jumpBank f bank taken takenArgs
        let second ← jumpBank f bank untaken untakenArgs
        let yes : BranchArm Γ f bank := ⟨taken, takenArgs, first.val, first.property⟩
        let no : BranchArm Γ f bank := ⟨untaken, untakenArgs, second.val, second.property⟩
        let choice := if nonzero then (⟨yes, no⟩ : BranchChoice Γ f bank) else ⟨no, yes⟩
        return ⟨choice, by
          intro σ owner nextFrame incoming stack memory passed
          have read := operand_bits_read bank reg operand.val operand.property σ
          have sameBit : (operand.val.denote σ).getLsbD 0 = (denoteTerm σ condition).getLsbD 0 := by
            rw [equal.down σ passed]
            simp only [BitVec.getLsbD_setWidth, Nat.zero_lt_succ, decide_true, Bool.true_and]
          have firstJump := first.property.2.2 σ owner incoming b.id b.insts.length
          have secondJump := second.property.2.2 σ owner incoming b.id b.insts.length
          cases nonzero <;> cases bit : (denoteTerm σ condition).getLsbD 0
          all_goals
            have step := EvoIR.Machine.Selection.step_branch_bit .ssa p
              (machineState σ f b b.insts.length bank owner nextFrame incoming stack memory)
              ⟨f.name, b.id, b.insts.length, bankDenote σ bank, owner, incoming⟩
              f b _ reg 0 taken takenArgs untaken untakenArgs (operand.val.denote σ) _
              rfl function block rfl term (by decide) read
              (by simpa only [sameBit, bit, beq_self_eq_true, Bool.false_eq_true,
                ↓reduceIte]
                using (by first | exact firstJump | exact secondJump))
            simpa only [choice, yes, no, bit, Bool.false_eq_true,
              ↓reduceIte, machineState, first.property.2.1, second.property.2.1] using step⟩
      | .cb nonzero reg taken takenArgs untaken untakenArgs => do
        let width : {width // reg.class.width = some width} ←
          match found : reg.class.width with
          | none => none
          | some width => some ⟨width, found⟩
        let operand ← wordOperand bank reg
        let equal ← checkWordUnder checks ⟨1, condition⟩ operand.val
        let first ← jumpBank f bank taken takenArgs
        let second ← jumpBank f bank untaken untakenArgs
        let yes : BranchArm Γ f bank := ⟨taken, takenArgs, first.val, first.property⟩
        let no : BranchArm Γ f bank := ⟨untaken, untakenArgs, second.val, second.property⟩
        let choice := if nonzero then (⟨yes, no⟩ : BranchChoice Γ f bank) else ⟨no, yes⟩
        return ⟨choice, by
          intro σ owner nextFrame incoming stack memory passed
          have read := operand_bits_read bank reg operand.val operand.property σ
          have truth : ((operand.val.denote σ).setWidth width.val.bits != 0) =
              (denoteTerm σ condition).getLsbD 0 := by
            rw [equal.down σ passed]
            exact encoded_nonzero _ _
          have firstJump := first.property.2.2 σ owner incoming b.id b.insts.length
          have secondJump := second.property.2.2 σ owner incoming b.id b.insts.length
          cases nonzero <;> cases bit : (denoteTerm σ condition).getLsbD 0
          all_goals
            have step := EvoIR.Machine.Selection.step_branch_zero .ssa p
              (machineState σ f b b.insts.length bank owner nextFrame incoming stack memory)
              ⟨f.name, b.id, b.insts.length, bankDenote σ bank, owner, incoming⟩
              f b _ reg taken takenArgs untaken untakenArgs width.val (operand.val.denote σ) _
              rfl function block rfl term width.property read
              (by simpa only [truth, bit, beq_self_eq_true, Bool.false_eq_true,
                ↓reduceIte]
                using (by first | exact firstJump | exact secondJump))
            simpa only [choice, yes, no, bit, Bool.false_eq_true,
              ↓reduceIte, machineState, first.property.2.1, second.property.2.1] using step⟩
      | _ => none
    else none
  else none

def BranchDiagram {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ)
    (condition : Term Γ 1) (choice : BranchChoice Γ f bank) : Prop :=
  ∀ σ owner nextFrame incoming stack memory,
    EvoIR.Machine.step .ssa p (machineState σ f b b.insts.length bank owner nextFrame incoming stack memory) =
      some (if (denoteTerm σ condition).getLsbD 0 then
        machineState σ f choice.yes.result.block 0 choice.yes.result.bank owner nextFrame incoming stack memory
      else
        machineState σ f choice.no.result.block 0 choice.no.result.bank owner nextFrame incoming stack memory)

/-- Guard-chain clients with no passed-check environment retain the stronger
unconditional interface. -/
def branchChoice {Γ : Var → Nat} (p : EvoIR.Machine.Program)
    (f : EvoIR.Machine.Function) (b : EvoIR.Machine.Block) (bank : Bank Γ)
    (condition : Term Γ 1) :
    Option {choice : BranchChoice Γ f bank // BranchDiagram p f b bank condition choice} := do
  let result ← branchChoiceUnder p f b bank condition []
  return ⟨result.val, by
    intro σ owner nextFrame incoming stack memory
    exact result.property σ owner nextFrame incoming stack memory rfl⟩

end EvoIR.Core.Symbolic.MachineSelection
