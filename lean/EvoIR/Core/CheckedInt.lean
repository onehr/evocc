import EvoIR.Sem

/-!
The first V01 primitive bridge, not a source-to-EvoIR result checker.

Source arithmetic uses mathematical signed integers, a representability check,
and a destination representation. The candidate fragment uses the existing
EvoIR total operator and a separate, reason-bearing obligation. These lemmas
relate both successful values and the first overflow diagnostic. No optimizer
rule, known-bits transfer, native decision procedure, or host arithmetic is used.
The later structural checker must prove that an actual fragment has this shape.
-/

namespace EvoIR.Core

inductive SignedArithmetic where
  | add | sub | mul
  deriving DecidableEq, Repr

def SignedArithmetic.math : SignedArithmetic → Int → Int → Int
  | .add => (· + ·)
  | .sub => (· - ·)
  | .mul => (· * ·)

def SignedArithmetic.guard : SignedArithmetic → GuardOp
  | .add => .add
  | .sub => .sub
  | .mul => .mul

def SignedArithmetic.op : SignedArithmetic → BinOp
  | .add => .add
  | .sub => .sub
  | .mul => .mul

abbrev CheckedValue (n : Nat) := Except Nat (BitVec n)

/-- C-HIR signed arithmetic in the checked, non-wrapping profile. -/
def sourceArithmetic (op : SignedArithmetic) {n : Nat}
    (x y : BitVec n) : CheckedValue n :=
  let z := op.math x.toInt y.toInt
  if signedRepresentable n z then .ok (BitVec.ofInt n z) else .error 3004

/-- A total EvoIR operation together with `ub.assume.signed_overflow`.
The obligation is checked before the value can escape to a consumer. -/
def irArithmetic (op : SignedArithmetic) {n : Nat}
    (x y : BitVec n) : CheckedValue n :=
  if denoteOp (.guard op.guard n) (x, y) = 1#1 then
    .ok (denoteOp (.bin op.op n) (x, y))
  else .error 3004

theorem arithmetic_representation (op : SignedArithmetic) {n : Nat}
    (x y : BitVec n) :
    BitVec.ofInt n (op.math x.toInt y.toInt) =
      denoteOp (.bin op.op n) (x, y) := by
  cases op <;> simp [SignedArithmetic.math, SignedArithmetic.op, denoteOp,
    Int.sub_eq_add_neg, BitVec.ofInt_add, BitVec.ofInt_mul,
    BitVec.ofInt_neg, BitVec.sub_eq_add_neg]

theorem predicate_bit_true (p : Prop) [Decidable p] :
    (BitVec.ofBool (decide p) = 1#1) ↔ p := by
  change (BitVec.ofBool (decide p) = BitVec.ofBool true) ↔ p
  rw [BitVec.ofBool_eq_iff_eq]
  simp only [decide_eq_true_eq]

theorem checked_arithmetic_lowering (op : SignedArithmetic) {n : Nat}
    (x y : BitVec n) : irArithmetic op x y = sourceArithmetic op x y := by
  have condition : (denoteOp (.guard op.guard n) (x, y) = 1#1) ↔
      signedRepresentable n (op.math x.toInt y.toInt) := by
    cases op <;> exact predicate_bit_true _
  simp only [irArithmetic, sourceArithmetic, condition, arithmetic_representation]

def sourceNeg {n : Nat} (x : BitVec n) : CheckedValue n :=
  if signedRepresentable n (-x.toInt) then
    .ok (BitVec.ofInt n (-x.toInt))
  else .error 3004

def irNeg {n : Nat} (x : BitVec n) : CheckedValue n :=
  irArithmetic .sub (0#n) x

theorem checked_neg_lowering {n : Nat} (x : BitVec n) :
    irNeg x = sourceNeg x := by
  rw [irNeg, checked_arithmetic_lowering]
  simp [sourceArithmetic, sourceNeg, SignedArithmetic.math]

/-- Proposed primitive pair. Operand bindings/order and the surrounding CFG
are deliberately not certified by this type; V01 must check those separately. -/
structure ArithmeticFragment where
  guard : GuardOp
  value : BinOp
  reason : Nat
  deriving DecidableEq, Repr

def checkArithmeticFragment (source : SignedArithmetic)
    (candidate : ArithmeticFragment) : Bool :=
  decide (candidate = ⟨source.guard, source.op, 3004⟩)

def runArithmeticFragment (candidate : ArithmeticFragment) {n : Nat}
    (x y : BitVec n) : CheckedValue n :=
  if denoteOp (.guard candidate.guard n) (x, y) = 1#1 then
    .ok (denoteOp (.bin candidate.value n) (x, y))
  else .error candidate.reason

theorem arithmetic_fragment_checker_sound (source : SignedArithmetic)
    (candidate : ArithmeticFragment)
    (accepted : checkArithmeticFragment source candidate = true)
    {n : Nat} (x y : BitVec n) :
    runArithmeticFragment candidate x y = sourceArithmetic source x y := by
  have same : candidate = ⟨source.guard, source.op, 3004⟩ := by
    simpa [checkArithmeticFragment] using accepted
  subst candidate
  exact checked_arithmetic_lowering source x y

/-- The selected original musl expression, before any CFG/call reasoning. -/
def sourceAbs {n : Nat} (x : BitVec n) : CheckedValue n :=
  if 0 < x.toInt then .ok x else sourceNeg x

def irAbs {n : Nat} (x : BitVec n) : CheckedValue n :=
  if 0 < x.toInt then .ok x else irNeg x

theorem checked_abs_expression {n : Nat} (x : BitVec n) :
    irAbs x = sourceAbs x := by
  simp [irAbs, sourceAbs, checked_neg_lowering]

/-- The target's signed narrowing retains the low destination bits. This is
the integer conversion boundary, not a floating or host-language cast. -/
theorem signed_narrow_representation {w n : Nat} (x : BitVec w) (h : n ≤ w) :
    BitVec.ofInt n x.toInt = BitVec.truncate n x := by
  exact BitVec.signExtend_eq_setWidth_of_le x h

/-- The unchanged Doom expression's mathematical signed conversions and right
shift. As with `sourceAbs`, this is not yet its complete function/CFG semantics. -/
def sourceFixedMul (x y : BitVec 32) : CheckedValue 32 := do
  let product ← sourceArithmetic .mul (BitVec.ofInt 64 x.toInt) (BitVec.ofInt 64 y.toInt)
  return BitVec.ofInt 32 (product.toInt >>> 16)

def irFixedMul (x y : BitVec 32) : CheckedValue 32 := do
  let a := denoteOp (.cast .sext 32 64) x
  let b := denoteOp (.cast .sext 32 64) y
  let product ← irArithmetic .mul a b
  if denoteOp (.count true 64 32) (product, 16#32) = 1#1 then
    return denoteOp (.cast .trunc 64 32) (denoteOp (.shift .ashr 64 32) (product, 16#32))
  else .error 3005

theorem checked_fixedmul_expression (x y : BitVec 32) :
    irFixedMul x y = sourceFixedMul x y := by
  have count (product : BitVec 64) :
      denoteOp (.count true 64 32) (product, 16#32) = 1#1 := by rfl
  have narrowed (product : BitVec 64) :
      BitVec.ofInt 32 (product.toInt >>> 16) =
        denoteOp (.cast .trunc 64 32) (denoteOp (.shift .ashr 64 32) (product, 16#32)) := by
    change BitVec.ofInt 32 (product.toInt >>> 16) = BitVec.truncate 32 (product.sshiftRight 16)
    rw [← BitVec.toInt_sshiftRight]
    exact signed_narrow_representation _ (by decide)
  simp only [irFixedMul, sourceFixedMul, checked_arithmetic_lowering, count, ↓reduceIte]
  change (sourceArithmetic .mul (BitVec.ofInt 64 x.toInt) (BitVec.ofInt 64 y.toInt) >>= fun p =>
      .ok (denoteOp (.cast .trunc 64 32) (denoteOp (.shift .ashr 64 32) (p, 16#32)))) = _
  cases sourceArithmetic .mul (BitVec.ofInt 64 x.toInt) (BitVec.ofInt 64 y.toInt) with
  | error _ => rfl
  | ok product => exact congrArg Except.ok (narrowed product).symm

-- Kernel-reduced boundary tests: deleting the obligation changes behavior.
example : sourceAbs (0x80000000#32) = .error 3004 := by rfl
example : sourceAbs (0x8000000000000000#64) = .error 3004 := by rfl
example : sourceAbs (0xfffffff7#32) = .ok (9#32) := by rfl
example : sourceArithmetic .add (0x7fffffff#32) (1#32) = .error 3004 := by rfl
example : sourceArithmetic .mul (0x100000000#64) (0x100000000#64) = .error 3004 := by rfl
example : sourceFixedMul (98304#32) (131072#32) = .ok (196608#32) := by rfl
example : sourceFixedMul (0xfffe8000#32) (131072#32) = .ok (0xfffd0000#32) := by rfl
example : sourceFixedMul (0x80000000#32) (0x80000000#32) = .ok (0#32) := by rfl
example : checkArithmeticFragment .add ⟨.sub, .add, 3004⟩ = false := by decide
example : checkArithmeticFragment .add ⟨.add, .mul, 3004⟩ = false := by decide
example : checkArithmeticFragment .add ⟨.add, .add, 3002⟩ = false := by decide

end EvoIR.Core
