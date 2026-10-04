import EvoIR.Core.Scalar

/-! Connect the typed source machine's arithmetic to the existing primitive
lowering lemmas. These are still not a whole-function result checker. -/
namespace EvoIR.Core.Source

theorem usualType_self (t : IntType) : usualType t t = t := by
  cases t <;> rfl

theorem value_cast_self (v : Value) : v.cast v.type = v := by
  cases v with
  | mk t bits =>
    cases t <;> simp [Value.cast, Value.number, Value.ofInt, IntType.signed]

def liftChecked (t : IntType) (result : CheckedValue t.width) : Result :=
  match result with
  | .ok bits => .ok ⟨t, bits⟩
  | .error code => .error (.diagnostic code)

def arithmeticSourceOp : SignedArithmetic → Binary
  | .add => .add | .sub => .sub | .mul => .mul

theorem typed_checked_arithmetic (op : SignedArithmetic) (t : IntType)
    (signed : t.signed = true) (x y : BitVec t.width) :
    binaryValue (arithmeticSourceOp op) ⟨t, x⟩ ⟨t, y⟩ =
      liftChecked t (sourceArithmetic op x y) := by
  cases t <;> simp_all [IntType.signed]
  all_goals cases op <;>
    simp [arithmeticSourceOp, binaryValue, usualType, IntType.signed, IntType.rank,
      Value.cast, Value.number, arithmeticResult, sourceArithmetic,
      SignedArithmetic.math, liftChecked, Value.ofInt]
  all_goals split <;> simp_all

theorem typed_arithmetic_lowering (op : SignedArithmetic) (t : IntType)
    (signed : t.signed = true) (x y : BitVec t.width) :
    liftChecked t (irArithmetic op x y) =
      binaryValue (arithmeticSourceOp op) ⟨t, x⟩ ⟨t, y⟩ := by
  rw [checked_arithmetic_lowering, typed_checked_arithmetic op t signed]

end EvoIR.Core.Source
