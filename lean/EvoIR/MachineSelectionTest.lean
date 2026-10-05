import EvoIR.MachineSelectionEdges
import EvoIR.MachineSelectionMultiply
import EvoIR.MachineSelectionTermStep
import EvoIR.Core.ConstantBranch

/-! Independent condition matching for actual MIR branches. -/
namespace EvoIR.Core.Symbolic.MachineSelection

def inverseCond : EvoIR.Machine.Cond → EvoIR.Machine.Cond
  | .eq => .ne | .ne => .eq | .lt => .ge | .ge => .lt
  | .le => .gt | .gt => .le | .lo => .hs | .hs => .lo
  | .ls => .hi | .hi => .ls | .vs => .vc | .vc => .vs
  | .cs => .cc | .cc => .cs

theorem condition_inverse (flags : EvoIR.Machine.Flags) (cond : EvoIR.Machine.Cond) :
    EvoIR.Machine.condition flags cond =
      (EvoIR.Machine.condition flags (inverseCond cond)).map Bool.not := by
  cases flags with
  | identity equal => cases equal <;> cases cond <;> rfl
  | arithmetic negative zero carry overflow =>
    cases negative <;> cases zero <;> cases carry <;> cases overflow <;> cases cond <;> rfl
  | integer lhs rhs width =>
    cases cond <;>
      simp [inverseCond, EvoIR.Machine.condition, Except.map, decide_not, bne]
    all_goals apply Bool.eq_iff_iff.mpr
    all_goals simp only [decide_eq_true_eq, Bool.not_eq_true', Bool.eq_false_iff,
      not_decide_eq_true, beq_iff_eq, ne_eq]
    all_goals omega

theorem one_bit_nonzero (value : BitVec 1) : value.getLsbD 0 = (value.toNat != 0) := by
  rw [one_bit_reify value]
  cases value.getLsbD 0 <;> rfl

def checkGuardFlag {Γ : Var → Nat} (source : Term Γ 1) (flags : FlagExpr Γ)
    (cond : EvoIR.Machine.Cond) : Option (PLift (∀ σ,
      EvoIR.Machine.condition (flags.denote σ) cond = .ok ((denoteTerm σ source).getLsbD 0))) :=
  match checked : checkFlag source flags cond with
  | some proof => some proof
  | none => match source, cond with
    | .guard .mul a b, .eq => checkMultiplyGuard a b flags
    | _, _ => none

def GuardTest {Γ : Var → Nat} (source : Term Γ 1) (flags : FlagExpr Γ)
    (cond : EvoIR.Machine.Cond) (passWhen : Bool) : Prop :=
  ∀ σ, EvoIR.Machine.condition (flags.denote σ) cond =
    .ok (if (denoteTerm σ source).toNat != 0 then passWhen else !passWhen)

def guardTest {Γ : Var → Nat} (source : Term Γ 1) (flags : FlagExpr Γ)
    (cond : EvoIR.Machine.Cond) : Option {passWhen : Bool // GuardTest source flags cond passWhen} :=
  match checked : checkGuardFlag source flags cond with
  | some proof => some ⟨true, by
      intro σ
      rw [proof.down σ, one_bit_nonzero]
      cases (denoteTerm σ source).toNat != 0 <;> rfl⟩
  | none => do
    let inverse ← checkGuardFlag source flags (inverseCond cond)
    return ⟨false, by
      intro σ
      rw [condition_inverse, inverse.down σ, one_bit_nonzero]
      cases (denoteTerm σ source).toNat != 0 <;> rfl⟩

theorem flag_operand_read {Γ : Var → Nat} (bank : Bank Γ) (reg : EvoIR.Machine.VReg)
    (flags : FlagExpr Γ) (read : ∀ σ,
      EvoIR.Machine.operandFlags (EvoIR.Machine.readOperand (bankDenote σ bank)) reg =
        .ok (flags.denote σ)) (σ : Subst Γ) :
    EvoIR.Machine.readFlags (bankDenote σ bank) reg = .ok (flags.denote σ) := by
  have good := read σ
  cases found : EvoIR.Machine.readSlot (bankDenote σ bank) reg with
  | none => simp [EvoIR.Machine.operandFlags, EvoIR.Machine.readOperand, found,
      bind, Except.bind] at good
  | some slot =>
    cases slot with
    | value value => simp [EvoIR.Machine.operandFlags, EvoIR.Machine.readOperand, found,
        bind, Except.bind, pure, Except.pure, throw, throwThe, MonadExceptOf.throw] at good
    | flags raw =>
      simp only [EvoIR.Machine.operandFlags, EvoIR.Machine.readOperand, found,
        bind, Except.bind, pure, Except.pure, Except.ok.injEq] at good
      simpa only [EvoIR.Machine.readFlags, found, good]

end EvoIR.Core.Symbolic.MachineSelection
