import EvoIR.MachineSelectionNormal

namespace EvoIR.Core.Symbolic.MachineSelection.NormalTests

private def Γ : Var → Nat := fun _ => 64
private def value : Term Γ 64 := .var "x" 64 rfl
private def masked (mask : BitVec 64) : Term Γ 64 := .bin .and value (.const 64 mask)
private def narrowed (width : Nat) : Term Γ 64 := .cast .zext 64 (.cast .trunc width value)

-- The observed compiler form is a 32-bit unsigned view implemented by AND.
-- The same proof must cover the empty and full views, not only that one mask.
example : (checkTerm (masked 0xffffffff) (narrowed 32)).isSome = true := by decide
example : (checkTerm (masked 0) (narrowed 0)).isSome = true := by decide
example : (checkTerm (masked 1) (narrowed 1)).isSome = true := by decide
example : (checkTerm (masked (BitVec.allOnes 64)) (narrowed 64)).isSome = true := by decide
example : (lowMaskWidth (BitVec.ofNat 1 0)).isSome = true := by decide
example : (lowMaskWidth (BitVec.ofNat 0 0)).isSome = true := by decide

-- A missing low bit, a shifted mask and a width mismatch change values.
example : (lowMaskWidth (BitVec.ofNat 64 0xfffffffd)).isNone = true := by decide
example : (checkTerm (masked 0xfffffffe) (narrowed 32)).isNone = true := by decide
example : (checkTerm (masked 0xffffffff) (narrowed 31)).isNone = true := by decide
example : (checkTerm (masked 0x100000000) (narrowed 33)).isNone = true := by decide

end EvoIR.Core.Symbolic.MachineSelection.NormalTests
