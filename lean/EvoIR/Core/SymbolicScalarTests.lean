import EvoIR.Core.SymbolicScalar

/-! Width, operand and predicate mutation cases for symbolic scalar checking.
All reductions are kernel checked, without native decision procedures. -/
namespace EvoIR.Core.Symbolic.Tests

private def widths (_ : Var) : Nat := 32
private def x : Word widths := ⟨32, .var "x" 32 rfl⟩
private def y : Word widths := ⟨32, .var "y" 32 rfl⟩
private def count64 : Word widths := ⟨64, .const 64 (0x100000001#64)⟩

example : (scalar (.bin .add 32) [x, y]).isSome = true := by rfl
example : (scalar (.bin .add 32) [x, count64]).isSome = false := by rfl
example : (scalar (.bin .add 64) [x, y]).isSome = false := by rfl
example : (scalar (.bin .add 32) [x]).isSome = false := by rfl
example : (scalar (.bin .add 32) [x, y, x]).isSome = false := by rfl
example : (scalar (.cast .sext 64 32) [x]).isSome = false := by rfl
example : (scalar (.shift .lshr 32 64) [x, count64]).isSome = true := by rfl
example : (scalar (.shift .lshr 32 32) [x, count64]).isSome = false := by rfl

-- Do not truncate/mask a 64-bit count or erase the predicate's signedness.
example : (scalar (.count true 32 64) [x, count64]).map
    (Word.denote (fun _ => 0#32)) = some (.bits 1 (0#1)) := by rfl
example : sameWord (Γ := widths)
    ⟨1, .count true x.2 y.2⟩ ⟨1, .count false x.2 y.2⟩ = false := by decide

example : sameWord x x = true := by decide
example : sameWord x y = false := by decide
example : sameWord (Γ := widths)
    ⟨32, .bin .sub x.2 y.2⟩ ⟨32, .bin .sub y.2 x.2⟩ = false := by decide
example : sameWord (Γ := widths)
    ⟨1, .cmp .slt x.2 y.2⟩ ⟨1, .cmp .ult x.2 y.2⟩ = false := by decide
example : sameWord (Γ := widths)
    ⟨64, .cast .sext 64 x.2⟩ ⟨64, .cast .zext 64 x.2⟩ = false := by decide
example : sameWord (Γ := widths)
    ⟨32, .const 32 (0#32)⟩ ⟨64, .const 64 (0#64)⟩ = false := by decide

end EvoIR.Core.Symbolic.Tests
