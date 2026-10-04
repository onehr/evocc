import EvoIR.Core.SymbolicInstruction
import EvoIR.Core.ScalarCorrectness

/-! Typed source values supply independently justified expectations for target
fragments. Source integer rank and signedness are retained even when two ranks
have the same representation width. This is not yet an expression/CFG checker. -/
namespace EvoIR.Core.Symbolic

structure CWord (Γ : Var → Nat) where
  type : Source.IntType
  term : Term Γ type.width

def CWord.denote {Γ : Var → Nat} (σ : Subst Γ) (v : CWord Γ) : Source.Value :=
  ⟨v.type, denoteTerm σ v.term⟩

def CWord.word {Γ : Var → Nat} (v : CWord Γ) : Word Γ := ⟨v.type.width, v.term⟩

def CWord.cast {Γ : Var → Nat} (t : Source.IntType) (v : CWord Γ) : CWord Γ :=
  ⟨t, if same : v.type.width = t.width then same ▸ v.term
    else if v.type.width < t.width then
      .cast (if v.type.signed then .sext else .zext) t.width v.term
    else .cast .trunc t.width v.term⟩

theorem cast_sound {Γ : Var → Nat} (σ : Subst Γ) (t : Source.IntType) (v : CWord Γ) :
    (v.cast t).denote σ = (v.denote σ).cast t := by
  rcases v with ⟨sourceType, term⟩
  cases sourceType <;> cases t <;>
    simp [CWord.cast, CWord.denote, Source.Value.cast, Source.Value.ofInt,
      Source.Value.number, Source.IntType.width, Source.IntType.signed,
      denoteOp]
  all_goals congr 1
  all_goals exact (signed_narrow_representation (n := 32) (denoteTerm σ term) (by decide)).symm

def arithmetic {Γ : Var → Nat} (op : SignedArithmetic) (a b : CWord Γ) :
    List (Guard Γ) × CWord Γ :=
  let t := Source.usualType a.type b.type
  let x := (a.cast t).term
  let y := (b.cast t).term
  (if t.signed then [⟨.signedOverflow, .guard op.guard x y⟩] else [],
    ⟨t, .bin op.op x y⟩)

theorem typed_arithmetic_trace (op : SignedArithmetic) (t : Source.IntType)
    (x y : BitVec t.width) :
    Source.binaryValue (Source.arithmeticSourceOp op) ⟨t, x⟩ ⟨t, y⟩ =
      (if t.signed then
        if (denoteOp (.guard op.guard t.width) (x, y)).toNat == 0 then
          .error (.diagnostic 3004)
        else .ok ⟨t, denoteOp (.bin op.op t.width) (x, y)⟩
      else .ok ⟨t, denoteOp (.bin op.op t.width) (x, y)⟩) := by
  have normal (a b : Source.Value) :
      Source.binaryValue (Source.arithmeticSourceOp op) a b =
        Source.arithmeticResult (Source.usualType a.type b.type)
          (op.math (a.cast (Source.usualType a.type b.type)).number
            (b.cast (Source.usualType a.type b.type)).number) false := by
    cases op <;> rfl
  rw [normal, Source.usualType_self, Source.value_cast_self ⟨t, x⟩,
    Source.value_cast_self ⟨t, y⟩]
  have guardValue : denoteOp (.guard op.guard t.width) (x, y) =
      BitVec.ofBool (decide (signedRepresentable t.width (op.math x.toInt y.toInt))) := by
    cases op <;> rfl
  cases signed : t.signed with
  | true =>
    simp only [Source.arithmeticResult, Source.Value.number, signed,
      ↓reduceIte, Bool.not_false, Bool.true_and, Bool.not_eq_true', decide_eq_false_iff_not,
      guardValue]
    by_cases safe : signedRepresentable t.width (op.math x.toInt y.toInt)
    · simp [safe, Source.Value.ofInt, arithmetic_representation]
    · simp [safe]
  | false =>
    simp only [Source.arithmeticResult, Source.Value.number, signed, Bool.false_eq_true,
      ↓reduceIte, Bool.false_and]
    have representation : BitVec.ofInt t.width (op.math (↑x.toNat) (↑y.toNat)) =
        denoteOp (.bin op.op t.width) (x, y) := by
      cases op <;> simp [SignedArithmetic.math, SignedArithmetic.op, denoteOp,
        Int.sub_eq_add_neg, BitVec.ofInt_add, BitVec.ofInt_mul,
        BitVec.ofInt_neg, BitVec.sub_eq_add_neg]
    exact congrArg Except.ok (congrArg (Source.Value.mk t) representation)

theorem arithmetic_promote (op : SignedArithmetic) (a b : Source.Value) :
    Source.binaryValue (Source.arithmeticSourceOp op) a b =
      Source.binaryValue (Source.arithmeticSourceOp op)
        (a.cast (Source.usualType a.type b.type)) (b.cast (Source.usualType a.type b.type)) := by
  have normal (a b : Source.Value) :
      Source.binaryValue (Source.arithmeticSourceOp op) a b =
        Source.arithmeticResult (Source.usualType a.type b.type)
          (op.math (a.cast (Source.usualType a.type b.type)).number
            (b.cast (Source.usualType a.type b.type)).number) false := by
    cases op <;> rfl
  rw [normal, normal]
  have castType (v : Source.Value) (t : Source.IntType) : (v.cast t).type = t := rfl
  simp only [castType, Source.usualType_self]
  have castAgain (v : Source.Value) (t : Source.IntType) : (v.cast t).cast t = v.cast t :=
    Source.value_cast_self (v.cast t)
  rw [castAgain, castAgain]

theorem arithmetic_sound {Γ : Var → Nat} (σ : Subst Γ)
    (op : SignedArithmetic) (a b : CWord Γ) :
    (do guards σ (arithmetic op a b).1; return (arithmetic op a b).2.denote σ) =
      Source.binaryValue (Source.arithmeticSourceOp op) (a.denote σ) (b.denote σ) := by
  let t := Source.usualType a.type b.type
  rw [arithmetic_promote op (a.denote σ) (b.denote σ)]
  change _ = Source.binaryValue (Source.arithmeticSourceOp op)
    ((a.denote σ).cast t) ((b.denote σ).cast t)
  rw [← cast_sound σ t a, ← cast_sound σ t b]
  change _ = Source.binaryValue (Source.arithmeticSourceOp op)
    ⟨t, denoteTerm σ (a.cast t).term⟩ ⟨t, denoteTerm σ (b.cast t).term⟩
  rw [typed_arithmetic_trace op t]
  simp only [arithmetic, CWord.denote, denoteTerm]
  split <;> simp_all [guards, denoteTerm, t, IR.Reason.code,
    bind, Except.bind, pure, Except.pure]
  split <;> split at * <;> simp_all
  all_goals assumption

def sourceValueIR (value : Source.Value) : IR.Value := .bits value.type.width value.bits

def shift {Γ : Var → Nat} (left : Bool) (a b : CWord Γ) :
    List (Guard Γ) × CWord Γ :=
  ([⟨.invalidShiftCount, .count b.type.signed a.term b.term⟩],
    ⟨a.type, .shift (if left then .shl else if a.type.signed then .ashr else .lshr)
      a.term b.term⟩)

private theorem number_toNat (v : Source.Value) (nonnegative : 0 ≤ v.number) :
    v.number.toNat = v.bits.toNat := by
  unfold Source.Value.number at nonnegative ⊢
  split at *
  · have same := BitVec.toInt_eq_toNat_of_lt
      (BitVec.toInt_pos_iff.mp nonnegative)
    simp [same]
  · simp_all

theorem shift_sound {Γ : Var → Nat} (σ : Subst Γ) (left : Bool) (a b : CWord Γ)
    (admitted : left = false ∨ a.type.signed = false) :
    (do guards σ (shift left a b).1; return (shift left a b).2.denote σ) =
      Source.binaryValue (if left then .shl else .shr) (a.denote σ) (b.denote σ) := by
  have countValue : denoteTerm σ (Term.count b.type.signed a.term b.term) =
      BitVec.ofBool (decide (0 ≤ (b.denote σ).number ∧
        (b.denote σ).number < (a.type.width : Int))) := by
    simp only [denoteTerm, Source.Value.number, CWord.denote]
    cases b.type.signed <;> simp [denoteOp]
  have trace : guards σ (shift left a b).1 =
      if 0 ≤ (b.denote σ).number ∧ (b.denote σ).number < (a.type.width : Int)
      then .ok () else .error (.diagnostic 3005) := by
    simp only [shift, guards, countValue]
    split <;> simp_all [IR.Reason.code]
  rw [trace]
  by_cases valid : 0 ≤ (b.denote σ).number ∧ (b.denote σ).number < (a.type.width : Int)
  · have count := number_toNat (b.denote σ) valid.1
    have lower := Int.not_lt.mpr valid.1
    have upper := Int.not_le.mpr valid.2
    simp only [CWord.denote] at count lower upper
    simp only [valid, bind, Except.bind]
    cases left <;> cases signed : a.type.signed <;>
      simp_all only [Bool.false_eq_true, Bool.true_eq_false, or_false, true_or, ↓reduceIte]
    all_goals simp [Source.binaryValue, lower, upper, count, signed,
      shift, CWord.denote, denoteOp, pure, Except.pure]
    all_goals rfl
  · have invalid : (b.denote σ).number < 0 ∨
        (a.type.width : Int) ≤ (b.denote σ).number := by omega
    simp only [valid, ↓reduceIte, bind, Except.bind]
    simp only [CWord.denote] at invalid
    cases left <;> simp [Source.binaryValue, CWord.denote, invalid,
      throw, throwThe, MonadExceptOf.throw, bind, Except.bind]

/-- A two-sided arithmetic check: the source operation chooses its promotion,
overflow trace and result term; the target is the supplied instruction list.
This does not assume the Rust lowering emitted a prescribed textual spelling. -/
def checkArithmetic {Γ : Var → Nat} (op : SignedArithmetic) (a b : CWord Γ)
    (scope : Scope Γ) (code : List IR.Instruction) (output : Nat) : Bool :=
  checkFragment scope code [output] (arithmetic op a b).1 [(arithmetic op a b).2.word]

theorem arithmetic_fragment_sound {Γ : Var → Nat} (σ : Subst Γ) (ρ : Nat → IR.Value)
    (owner : Nat) (op : SignedArithmetic) (a b : CWord Γ) (scope : Scope Γ)
    (code : List IR.Instruction) (output : Nat)
    (accepted : checkArithmetic op a b scope code output = true) :
    (do let values ← concreteInstructions owner (scope.denote σ ρ) code
        [output].mapM (IR.lookup values)) =
      (do let value ← Source.binaryValue (Source.arithmeticSourceOp op) (a.denote σ) (b.denote σ)
          return [sourceValueIR value]) := by
  rw [fragment_sound σ ρ owner scope code [output]
    (arithmetic op a b).1 [(arithmetic op a b).2.word] accepted]
  rw [← arithmetic_sound σ op a b]
  cases guards σ (arithmetic op a b).1 <;> rfl

end EvoIR.Core.Symbolic
