import EvoIR.Core.SymbolicSource

/-! Integer rank/conversion and real instruction-fragment mutation cases. -/
namespace EvoIR.Core.Symbolic.SourceTests

private def widths (_ : Var) : Nat := 32
private def a : CWord widths := ⟨.int, .var "a" 32 rfl⟩
private def b : CWord widths := ⟨.int, .var "b" 32 rfl⟩
private def u : CWord widths := ⟨.uint, .var "b" 32 rfl⟩
private def scope : Scope widths := [(0, .word a.word), (1, .word b.word)]
private def checkedAdd : List IR.Instruction := [
  ⟨[⟨2, .bits 1⟩], .scalar (.guard .add 32), [0, 1]⟩,
  ⟨[], .assume .signedOverflow, [2]⟩,
  ⟨[⟨3, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩]
private def unsignedAdd : List IR.Instruction := [
  ⟨[⟨3, .bits 32⟩], .scalar (.bin .add 32), [0, 1]⟩]

example : checkArithmetic .add a b scope checkedAdd 3 = true := by decide
example : checkArithmetic .add a b scope unsignedAdd 3 = false := by decide
example : checkArithmetic .add a u scope unsignedAdd 3 = true := by decide
example : checkArithmetic .add a u scope checkedAdd 3 = false := by decide
example : checkArithmetic .sub a b scope checkedAdd 3 = false := by decide
example : checkArithmetic .mul a b scope checkedAdd 3 = false := by decide
example : checkArithmetic .add a b scope checkedAdd 0 = false := by decide

private def mixedMul (leftCast rightCast : CastOp) : List IR.Instruction := [
  ⟨[⟨2, .bits 64⟩], .scalar (.cast leftCast 32 64), [0]⟩,
  ⟨[⟨3, .bits 64⟩], .scalar (.cast rightCast 32 64), [1]⟩,
  ⟨[⟨4, .bits 1⟩], .scalar (.guard .mul 64), [2, 3]⟩,
  ⟨[], .assume .signedOverflow, [4]⟩,
  ⟨[⟨5, .bits 64⟩], .scalar (.bin .mul 64), [2, 3]⟩]

example : checkArithmetic .mul (a.cast .long) u scope (mixedMul .sext .zext) 5 = true := by decide
example : checkArithmetic .mul (a.cast .long) u scope (mixedMul .zext .zext) 5 = false := by decide
example : checkArithmetic .mul (a.cast .long) u scope (mixedMul .sext .sext) 5 = false := by decide

-- Identical representation widths do not erase C rank or signedness.
example : (arithmetic .add a u).2.type = .uint := by rfl
example : (a.cast .long).type = .long := by rfl
example : ((a.cast .long).cast .longLong).type = .longLong := by rfl
example : (arithmetic .add (a.cast .long) u).2.type = .long := by rfl
example : (arithmetic .add (a.cast .long) (u.cast .ulong)).2.type = .ulong := by rfl
example : (arithmetic .add (a.cast .longLong) (u.cast .ulong)).2.type = .ulongLong := by rfl

private def six : List Source.IntType := [.int, .uint, .long, .ulong, .longLong, .ulongLong]
private def constant (t : Source.IntType) (bits : Nat) : CWord widths :=
  ⟨t, .const t.width (BitVec.ofNat t.width bits)⟩
private def σ : Subst widths := fun _ => 0#32

-- All 36 source/destination pairs, covering sign/zero extension, truncation
-- and equal-width rank changes; this concrete matrix supplements cast_sound.
example : six.all (fun sourceType => six.all (fun destinationType =>
    [0, 1, 0x80000000, 0xffffffff, 0x8000000000000000, 0xffffffffffffffff].all fun n =>
      decide (((constant sourceType n).cast destinationType).denote σ =
        ((constant sourceType n).denote σ).cast destinationType))) = true := by decide

example : ((constant .int 0xffffffff).cast .ulong).denote σ =
    Source.Value.ofNat .ulong 0xffffffffffffffff := by decide
example : ((constant .uint 0xffffffff).cast .long).denote σ =
    Source.Value.ofNat .long 0xffffffff := by decide

example (x y : BitVec 32) :
    (do let values ← concreteInstructions 1
          (scope.denote (fun name => if name = "a" then x else y) (fun _ => .world)) checkedAdd
        [3].mapM (IR.lookup values)) =
      (do let value ← Source.binaryValue .add ⟨.int, x⟩ ⟨.int, y⟩
          return [sourceValueIR value]) := by
  simpa [a, b, CWord.denote, denoteTerm, Source.arithmeticSourceOp] using arithmetic_fragment_sound
    (fun name => if name = "a" then x else y) (fun _ => .world)
    1 .add a b scope checkedAdd 3 (by decide)

end EvoIR.Core.Symbolic.SourceTests
