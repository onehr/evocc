import EvoIR.Core.Scalar

/-! Connect the typed source machine's arithmetic to the existing primitive
lowering lemmas. These are still not a whole-function result checker. -/
namespace EvoIR.Core.Source

/-- The source validator's scalar execution contract: a correctly typed value
or one of the admitted checked diagnostics, never a malformed-machine result.
This does not by itself establish local-store or continuation invariants. -/
def ResultSafe (type : IntType) : Result → Prop
  | .ok value => value.type = type
  | .error (.diagnostic code) => code ∈ [3002, 3003, 3004, 3005]
  | .error (.malformed _) => False

theorem arithmetic_result_safe (type : IntType) (value : Int) (constant : Bool) :
    ResultSafe type (arithmeticResult type value constant) := by
  simp only [arithmeticResult]
  split <;> simp [ResultSafe, Value.ofInt]

theorem unary_result_safe (op : Unary) (value : Value) (constant : Bool) :
    ResultSafe (if op == .not then .int else value.type) (unaryValue op value constant) := by
  cases op with
  | neg => exact arithmetic_result_safe value.type (-value.number) constant
  | _ => simp [unaryValue, ResultSafe, Value.boolean, Value.ofNat]

theorem binary_result_safe (op : Binary) (left right : Value) (constant : Bool)
    (admitted : !(op == .shl && left.type.signed) = true) :
    ResultSafe (binaryType op left.type right.type) (binaryValue op left right constant) := by
  cases op with
  | add => exact arithmetic_result_safe _ _ constant
  | sub => exact arithmetic_result_safe _ _ constant
  | mul => exact arithmetic_result_safe _ _ constant
  | div | rem =>
    by_cases zero : (right.cast (usualType left.type right.type)).number = 0
    all_goals by_cases overflow :
      ((usualType left.type right.type).signed = true ∧
        (left.cast (usualType left.type right.type)).number = signedMin (usualType left.type right.type).width) ∧
        (right.cast (usualType left.type right.type)).number = -1
    all_goals simp [binaryType, binaryValue, zero, overflow, ResultSafe, Value.ofInt,
      bind, Except.bind, pure, Except.pure, throw, throwThe, MonadExceptOf.throw]
  | shl | shr =>
    by_cases negative : right.number < 0
    all_goals by_cases high : (left.type.width : Int) ≤ right.number
    all_goals cases signed : left.type.signed
    all_goals simp [binaryType, binaryValue, ResultSafe, negative, high, signed,
      bind, Except.bind, pure, Except.pure, throw, throwThe, MonadExceptOf.throw]
    all_goals simp_all
  | _ => simp [binaryType, binaryValue, ResultSafe, Value.boolean, Value.ofNat,
      pure, Except.pure]

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
