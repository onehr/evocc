import EvoIR.Core.SymbolicInstruction

/-! Actual IR instruction shapes, SSA freshness, opaque/poison boundaries and
ordered diagnostic mutations. Reductions use only the Lean kernel. -/
namespace EvoIR.Core.Symbolic.InstructionTests

private def widths (_ : Var) : Nat := 32
private def x : Word widths := ⟨32, .var "x" 32 rfl⟩
private def zero : Word widths := ⟨32, .const 32 (0#32)⟩
private def input : Scope widths := [(0, .word x), (8, .opaque 0)]
private def zeroInst : IR.Instruction := ⟨[⟨1, .bits 32⟩], .constant 32 0, []⟩
private def guardInst : IR.Instruction :=
  ⟨[⟨2, .bits 1⟩], .scalar (.guard .sub 32), [1, 0]⟩
private def assumeInst : IR.Instruction := ⟨[], .assume .signedOverflow, [2]⟩
private def subInst : IR.Instruction := ⟨[⟨3, .bits 32⟩], .scalar (.bin .sub 32), [1, 0]⟩
private def code := [zeroInst, guardInst, assumeInst, subInst]
private def expected : List (Guard widths) := [⟨.signedOverflow, .guard .sub zero.2 x.2⟩]
private def result : List (Word widths) := [⟨32, .bin .sub zero.2 x.2⟩]

example : checkFragment input code [3] expected result = true := by decide
-- A deleted obligation, changed reason, missing output or changed operand
-- cannot be excused by equal outputs on the sampled non-overflowing inputs.
example : checkFragment input [zeroInst, guardInst, subInst] [3] expected result = false := by decide
example : checkFragment input [zeroInst, guardInst,
    {assumeInst with op := .assume .divisionByZero}, subInst] [3] expected result = false := by decide
example : checkFragment input code [] expected result = false := by decide
example : checkFragment input code [0] expected result = false := by decide
example : checkFragment input code [99] expected result = false := by decide
example : checkFragment input [zeroInst, guardInst, assumeInst,
    {subInst with operands := [0, 1]}] [3] expected result = false := by decide
example : checkFragment input [zeroInst, guardInst, assumeInst,
    {subInst with op := .scalar (.bin .add 32)}] [3] expected result = false := by decide

-- SSA/result checks include opaque bindings. Reading one cannot turn poison,
-- memory or a token into initialized integer data.
example : (instruction input {zeroInst with results := [⟨0, .bits 32⟩]}).isSome = false := by rfl
example : (instruction input {zeroInst with results := [⟨8, .bits 32⟩]}).isSome = false := by rfl
example : (instruction input {zeroInst with results := [⟨1, .bits 64⟩]}).isSome = false := by rfl
example : (instruction input {zeroInst with results := [⟨1, .world⟩]}).isSome = false := by rfl
example : (instruction input {zeroInst with operands := [0]}).isSome = false := by rfl
example : (instruction input {zeroInst with op := .constant 0 0}).isSome = false := by rfl
example : (instruction input {zeroInst with op := .constant 32 (2^32)}).isSome = false := by rfl
example : (instruction input ⟨[⟨1, .bits 1⟩], .scalar (.nonzero 32), [8]⟩).isSome = false := by rfl
example : (instruction input ⟨[⟨1, .bits 1⟩], .scalar (.count false 32 32), [8, 0]⟩).isSome = false := by rfl
example : (instruction input ⟨[⟨1, .bits 32⟩], .uninitialized (.bits 32), []⟩).isSome = false := by rfl
example : (instruction input ⟨[], .call "f", []⟩).isSome = false := by rfl
example : (instruction input ⟨[], .store, []⟩).isSome = false := by rfl
example : (instruction input ⟨[], .assume .signedOverflow, [0]⟩).isSome = false := by rfl

private def falseGuard (reason : IR.Reason) : Guard widths := ⟨reason, .const 1 (0#1)⟩
private def σ : Subst widths := fun _ => 0#32

-- Two failures expose their order, including division versus remainder.
example : guards σ [falseGuard .divisionByZero, falseGuard .signedOverflow] =
    .error (.diagnostic 3002) := by rfl
example : guards σ [falseGuard .signedOverflow, falseGuard .divisionByZero] =
    .error (.diagnostic 3004) := by rfl
example : guards σ [falseGuard .remainderByZero, falseGuard .divisionByZero] =
    .error (.diagnostic 3003) := by rfl
example : guards σ [falseGuard .invalidShiftCount] = .error (.diagnostic 3005) := by rfl

private def twoFailures (first second : IR.Reason) : List IR.Instruction := [
  ⟨[⟨1, .bits 1⟩], .constant 1 0, []⟩,
  ⟨[], .assume first, [1]⟩, ⟨[], .assume second, [1]⟩]

example : checkFragment (Γ := widths) [] (twoFailures .divisionByZero .signedOverflow) []
    [falseGuard .divisionByZero, falseGuard .signedOverflow] [] = true := by decide
example : checkFragment (Γ := widths) [] (twoFailures .signedOverflow .divisionByZero) []
    [falseGuard .divisionByZero, falseGuard .signedOverflow] [] = false := by decide
example : checkFragment (Γ := widths) [] (twoFailures .divisionByZero .divisionByZero) []
    [falseGuard .divisionByZero, falseGuard .signedOverflow] [] = false := by decide

-- The universal theorem executes the same opaque caller value unchanged;
-- a poison placeholder never becomes a used numeric input.
example (value : BitVec 32) (callerValue : IR.Value) :
    (do let values ← concreteInstructions 1
          (input.denote (fun _ => value) (fun _ => callerValue)) code
        [3].mapM (IR.lookup values)) =
      (do guards (fun _ => value) expected
          return result.map (Word.denote (fun _ => value))) :=
  fragment_sound (fun _ => value) (fun _ => callerValue) 1 input code [3] expected result (by decide)

end EvoIR.Core.Symbolic.InstructionTests
