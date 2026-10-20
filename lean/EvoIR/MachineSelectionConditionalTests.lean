import EvoIR.MachineSelectionConditional

namespace EvoIR.Core.Symbolic.MachineSelection.ConditionalTests

private def Γ : Var → Nat := fun _ => 64
private def input (name : Var) (width : Nat) : Term Γ width :=
  .cast .trunc width (.var name 64 rfl)
private def lhs : Term Γ 32 := input "x" 32
private def count : Term Γ 32 := input "k" 32
private def sourceShift : Word Γ := ⟨32, .shift .shl lhs count⟩
private def targetShift : WordExpr Γ := .normalize .gpr32
  (.binary .lsl .w (.normalize .gpr32 (.input ⟨32, lhs⟩))
    (.normalize .gpr32 (.input ⟨32, count⟩)))
private def checks : List (Guard Γ) := [⟨.invalidShiftCount, .count true lhs count⟩]

example : (checkWord sourceShift targetShift).isNone = true := by decide
example : (checkWordUnder [] sourceShift targetShift).isNone = true := by decide
example : (checkWordUnder checks sourceShift targetShift).isSome = true := by decide

private def wrongCount : List (Guard Γ) :=
  [⟨.invalidShiftCount, .count true lhs (input "j" 32)⟩]
private def weakBound : List (Guard Γ) :=
  [⟨.invalidShiftCount, .count true (input "x" 64) count⟩]

example : (checkWordUnder wrongCount sourceShift targetShift).isNone = true := by decide
example : (checkWordUnder weakBound sourceShift targetShift).isNone = true := by decide

private def sourceCompare : Term Γ 1 := .cmp .ne sourceShift.2 (.const 32 0)
private def targetFlags : FlagExpr Γ := .comparison .w targetShift (.literal 0)

example : (checkFlag sourceCompare targetFlags .ne).isNone = true := by decide
example : (checkFlagUnder checks sourceCompare targetFlags .ne).isSome = true := by decide

private def sourceConstant : Word Γ := ⟨32, .shift .lshr lhs (.const 32 1)⟩
private def targetConstant : WordExpr Γ :=
  .binary .lsr .w (.input ⟨32, lhs⟩) (.literal 1)

example : (checkWordUnder [] sourceConstant targetConstant).isSome = true := by decide

/-- The accepted checker result is used for every valuation satisfying the
executed source guard trace, not only selected concrete shift amounts. -/
theorem checked_variable_shift (σ : Subst Γ) (passed : guards σ checks = .ok ()) :
    targetShift.denote σ = (denoteTerm σ sourceShift.2).setWidth 64 := by
  have accepted : (checkWordUnder checks sourceShift targetShift).isSome = true := by decide
  cases found : checkWordUnder checks sourceShift targetShift with
  | none => simp [found] at accepted
  | some proof => exact proof.down σ passed

private def widened : Term Γ 64 := .cast .zext 64 lhs
private def zeroEqual : Term Γ 1 := .cmp .eq widened (.const 64 0)
private def zeroDifferent : Term Γ 1 := .cmp .ne widened (.const 64 0)
private def testedWord : WordExpr Γ := .input ⟨32, lhs⟩

example : (zeroTestUnder [] zeroEqual testedWord .w).map Subtype.val = some false := by decide
example : (zeroTestUnder [] zeroDifferent testedWord .w).map Subtype.val = some true := by decide
example : (zeroTestUnder [] zeroEqual (.input ⟨32, input "y" 32⟩) .w).isNone := by decide

-- Testing all 64 bits is not a replacement for testing the low 32 bits.
example : (zeroTestUnder [] zeroEqual (.input ⟨64, .var "x" 64 rfl⟩) .x).isNone := by decide
example : (zeroTestUnder [] (.cmp .slt widened (.const 64 0)) testedWord .w).isNone := by decide

end EvoIR.Core.Symbolic.MachineSelection.ConditionalTests
