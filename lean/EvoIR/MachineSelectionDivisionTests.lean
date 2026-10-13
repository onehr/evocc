import EvoIR.MachineSelectionConditional

/-! D06: the signed-division guard is a one-bit OR. The emitted code computes
it as the 32-bit OR of two zero-extended comparison results. The proved lift
of `checkTerm` identifies the two forms. Each refused form below has a
different value for some input. -/
namespace EvoIR.Core.Symbolic.MachineSelection.DivisionTests

private def Γ : Var → Nat := fun _ => 32
private def dividend : Term Γ 32 := .var "dividend" 32 rfl
private def divisor : Term Γ 32 := .var "divisor" 32 rfl

/-- The source guard of `dividend / divisor`. -/
private def sourceGuard (dividend : Term Γ 32) : Word Γ := ⟨1, .guard .divRem dividend divisor⟩

/-- The emitted guard: `cmp w, INT_MIN; cset ne; cmp divisor, -1; cset ne; orr`. -/
private def machineGuard (lhs : WordExpr Γ) (op : EvoIR.Machine.BinOp := .orr)
    (cond : EvoIR.Machine.Cond := .ne) (allOnes : Nat := 4294967295) : WordExpr Γ :=
  .binary op .w
    (.cset (.comparison .w lhs (.literal 2147483648)) .ne)
    (.cset (.comparison .w (.input ⟨32, divisor⟩) (.literal allOnes)) cond)

-- The pair of the retained D06 program, with the constant dividend 12.
example : (checkWord (sourceGuard (.const 32 12)) (machineGuard (.literal 12))).isSome = true := by
  decide
-- The same pair after the earlier nonzero check, as the branch reader checks it.
example : (checkWordUnder [⟨.divisionByZero, .nonzero divisor⟩] (sourceGuard (.const 32 12))
    (machineGuard (.literal 12))).isSome = true := by decide
-- An arbitrary dividend.
example : (checkWord (sourceGuard dividend) (machineGuard (.input ⟨32, dividend⟩))).isSome = true := by
  decide

-- An AND, an inverted comparison and a wrong constant change the value.
example : (checkWord (sourceGuard dividend)
    (machineGuard (.input ⟨32, dividend⟩) .and)).isNone = true := by decide
example : (checkWord (sourceGuard dividend)
    (machineGuard (.input ⟨32, dividend⟩) .orr .eq)).isNone = true := by decide
example : (checkWord (sourceGuard dividend)
    (machineGuard (.input ⟨32, dividend⟩) .orr .ne 4294967294)).isNone = true := by decide

private def bits : Var → Nat := fun _ => 1
private def a : Term bits 1 := .var "a" 1 rfl
private def b : Term bits 1 := .var "b" 1 rfl

/-- The 32-bit form of one bitwise operator over two zero-extended bits. -/
private def wide (op : BinOp) (cast : CastOp := .zext) : Term bits 32 :=
  .bin op (.cast cast 32 a) (.cast cast 32 b)

-- The lift holds for AND, OR and XOR. A `trunc` to a wider width also zero-extends.
example : (checkTerm (.cast .zext 32 (.bin .and a b)) (wide .and)).isSome = true := by decide
example : (checkTerm (.cast .zext 32 (.bin .or a b)) (wide .or)).isSome = true := by decide
example : (checkTerm (.cast .zext 32 (.bin .xor a b)) (wide .xor)).isSome = true := by decide
example : (checkTerm (.cast .zext 32 (.bin .or a b)) (wide .or .trunc)).isSome = true := by decide

-- Sign extension and addition do not commute with the width change.
example : (checkTerm (.cast .zext 32 (.bin .or a b)) (wide .or .sext)).isNone = true := by decide
example : (checkTerm (.cast .zext 32 (.bin .add a b)) (wide .add)).isNone = true := by decide

/-- The accepted check gives the equation for every valuation. -/
theorem guarded_division_word (σ : Subst Γ) :
    (machineGuard (.input ⟨32, dividend⟩)).denote σ =
      (denoteTerm σ (sourceGuard dividend).2).setWidth 64 := by
  have accepted :
      (checkWord (sourceGuard dividend) (machineGuard (.input ⟨32, dividend⟩))).isSome = true := by
    decide
  cases found : checkWord (sourceGuard dividend) (machineGuard (.input ⟨32, dividend⟩)) with
  | none => simp [found] at accepted
  | some proof => exact proof.down σ

#print axioms guarded_division_word

end EvoIR.Core.Symbolic.MachineSelection.DivisionTests
